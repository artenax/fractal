mod account_settings;
mod avatar;
mod content;
mod event_source_dialog;
mod join_room_dialog;
mod media_viewer;
mod notifications;
pub mod room;
mod room_creation;
mod room_list;
mod settings;
mod sidebar;
mod user;
pub mod verification;

use std::{collections::HashSet, fs, time::Duration};

use adw::{prelude::*, subclass::prelude::*};
use futures::StreamExt;
use gettextrs::gettext;
use gtk::{
    self, gdk, gio, glib,
    glib::{clone, signal::SignalHandlerId},
    CompositeTemplate,
};
use log::{debug, error, warn};
use matrix_sdk::{
    config::SyncSettings,
    room::Room as MatrixRoom,
    ruma::{
        api::client::{
            error::ErrorKind,
            filter::{FilterDefinition, LazyLoadOptions, RoomEventFilter, RoomFilter},
            session::logout,
        },
        assign,
        events::{
            direct::DirectEventContent, room::encryption::SyncRoomEncryptionEvent,
            GlobalAccountDataEvent,
        },
        RoomId,
    },
    sync::SyncResponse,
    Client,
};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use tokio::task::JoinHandle;
use url::Url;

use self::{
    account_settings::AccountSettings, content::Content, join_room_dialog::JoinRoomDialog,
    media_viewer::MediaViewer, notifications::Notifications, room_list::RoomList, sidebar::Sidebar,
    verification::VerificationList,
};
pub use self::{
    avatar::{AvatarData, AvatarImage, AvatarUriSource},
    content::verification::SessionVerification,
    room::{Event, Room},
    room_creation::RoomCreation,
    settings::SessionSettings,
    user::{User, UserActions, UserExt},
};
use crate::{
    secret::{self, StoredSession},
    session::sidebar::ItemList,
    spawn, spawn_tokio, toast,
    utils::{
        check_if_reachable,
        matrix::{self, ClientSetupError},
    },
    Window,
};

/// The state of the session.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, glib::Enum)]
#[repr(i32)]
#[enum_type(name = "SessionState")]
pub enum SessionState {
    LoggedOut = -1,
    #[default]
    Init = 0,
    InitialSync = 1,
    Ready = 2,
}

#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "BoxedStoredSession")]
struct BoxedStoredSession(StoredSession);

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::InitializingObject;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/session.ui")]
    pub struct Session {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub overlay: TemplateChild<gtk::Overlay>,
        #[template_child]
        pub leaflet: TemplateChild<adw::Leaflet>,
        #[template_child]
        pub sidebar: TemplateChild<Sidebar>,
        #[template_child]
        pub content: TemplateChild<Content>,
        #[template_child]
        pub media_viewer: TemplateChild<MediaViewer>,
        pub client: OnceCell<Client>,
        pub item_list: OnceCell<ItemList>,
        pub user: OnceCell<User>,
        pub state: Cell<SessionState>,
        pub info: OnceCell<StoredSession>,
        pub sync_tokio_handle: RefCell<Option<JoinHandle<()>>>,
        pub offline_handler_id: RefCell<Option<SignalHandlerId>>,
        pub offline: Cell<bool>,
        pub settings: OnceCell<SessionSettings>,
        pub window_active_handler_id: RefCell<Option<SignalHandlerId>>,
        pub notifications: Notifications,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Session {
        const NAME: &'static str = "Session";
        type Type = super::Session;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("session.close-room", None, move |session, _, _| {
                session.select_room(None);
            });

            klass.install_action(
                "session.show-room",
                Some("s"),
                move |session, _, parameter| {
                    if let Ok(room_id) =
                        <&RoomId>::try_from(&*parameter.unwrap().get::<String>().unwrap())
                    {
                        session.select_room_by_id(room_id);
                    } else {
                        error!("Can't show room because the provided id is invalid");
                    }
                },
            );

            klass.install_action("session.logout", None, move |session, _, _| {
                spawn!(clone!(@weak session => async move {
                    session.logout().await
                }));
            });

            klass.install_action("session.show-content", None, move |session, _, _| {
                session.show_content();
            });

            klass.install_action("session.room-creation", None, move |session, _, _| {
                session.show_room_creation_dialog();
            });

            klass.install_action("session.show-join-room", None, move |widget, _, _| {
                spawn!(clone!(@weak widget => async move {
                    widget.show_join_room_dialog().await;
                }));
            });

            klass.add_binding_action(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                "session.close-room",
                None,
            );

            klass.install_action("session.toggle-room-search", None, move |session, _, _| {
                session.toggle_room_search();
            });

            klass.add_binding_action(
                gdk::Key::k,
                gdk::ModifierType::CONTROL_MASK,
                "session.toggle-room-search",
                None,
            );

            klass.install_action(
                "session.open-account-settings",
                None,
                move |widget, _, _| {
                    widget.open_account_settings();
                },
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Session {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoxed::builder::<BoxedStoredSession>("info")
                        .write_only()
                        .construct_only()
                        .build(),
                    glib::ParamSpecString::builder("session-id")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<ItemList>("item-list")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<User>("user")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("offline")
                        .read_only()
                        .build(),
                    glib::ParamSpecEnum::builder::<SessionState>("state")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "info" => self
                    .info
                    .set(value.get::<BoxedStoredSession>().unwrap().0)
                    .unwrap(),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "session-id" => obj.session_id().to_value(),
                "item-list" => obj.item_list().to_value(),
                "user" => obj.user().to_value(),
                "offline" => obj.is_offline().to_value(),
                "state" => obj.state().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            self.settings
                .set(SessionSettings::new(obj.session_id()))
                .unwrap();

            self.notifications.set_session(Some(&obj));

            self.sidebar.connect_notify_local(
                Some("selected-item"),
                clone!(@weak self as imp => move |_, _| {
                    if imp.sidebar.selected_item().is_none() {
                        imp.leaflet.navigate(adw::NavigationDirection::Back);
                    } else {
                        imp.leaflet.navigate(adw::NavigationDirection::Forward);
                    }
                }),
            );

            let monitor = gio::NetworkMonitor::default();
            let handler_id = monitor.connect_network_changed(clone!(@weak obj => move |_, _| {
                spawn!(clone!(@weak obj => async move {
                    obj.update_offline().await;
                }));
            }));

            self.offline_handler_id.replace(Some(handler_id));

            self.content.connect_notify_local(
                Some("item"),
                clone!(@weak obj => move |_, _| {
                    // When switching to a room, withdraw its notifications.
                    obj.notifications().withdraw_all_for_selected_room();
                }),
            );

            obj.connect_parent_notify(|obj| {
                if let Some(window) = obj.root().and_then(|root| root.downcast::<Window>().ok()) {
                    let handler_id =
                        window.connect_is_active_notify(clone!(@weak obj => move |window| {
                            // When the window becomes active, withdraw the notifications
                            // of the room that is displayed.
                            if window.is_active()
                                && window.current_session_id().as_deref() == Some(obj.session_id())
                            {
                                obj.notifications().withdraw_all_for_selected_room();
                            }
                        }));
                    obj.imp().window_active_handler_id.replace(Some(handler_id));
                }
            });
        }

        fn dispose(&self) {
            let obj = self.obj();

            // Needs to be disconnected or else it may restart the sync
            if let Some(handler_id) = self.offline_handler_id.take() {
                gio::NetworkMonitor::default().disconnect(handler_id);
            }

            if let Some(handle) = self.sync_tokio_handle.take() {
                handle.abort();
            }

            if let Some(handler_id) = self.window_active_handler_id.take() {
                if let Some(window) = obj.root().and_then(|root| root.downcast::<Window>().ok()) {
                    window.disconnect(handler_id);
                }
            }
        }
    }
    impl WidgetImpl for Session {}
    impl BinImpl for Session {}
}

glib::wrapper! {
    pub struct Session(ObjectSubclass<imp::Session>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Session {
    /// Create a new session.
    pub async fn new(homeserver: Url, data: matrix_sdk::Session) -> Result<Self, ClientSetupError> {
        let mut path = glib::user_data_dir();
        path.push(glib::uuid_string_random().as_str());

        let passphrase = thread_rng()
            .sample_iter(Alphanumeric)
            .take(30)
            .map(char::from)
            .collect();

        let stored_session = StoredSession::from_parts(homeserver, path, passphrase, data);

        Self::restore(stored_session).await
    }

    /// Restore a stored session.
    pub async fn restore(stored_session: StoredSession) -> Result<Self, ClientSetupError> {
        let obj = glib::Object::builder::<Self>()
            .property("info", BoxedStoredSession(stored_session.clone()))
            .build();

        let client =
            spawn_tokio!(async move { matrix::client_with_stored_session(stored_session).await })
                .await
                .unwrap()?;

        let imp = obj.imp();
        imp.client.set(client).unwrap();

        let user = User::new(&obj, &obj.info().user_id);
        imp.user.set(user).unwrap();
        obj.notify("user");

        Ok(obj)
    }

    /// The info to store this session.
    pub fn info(&self) -> &StoredSession {
        self.imp().info.get().unwrap()
    }

    /// The unique local ID for this session.
    pub fn session_id(&self) -> &str {
        self.info().id()
    }

    /// The current state of the session.
    pub fn state(&self) -> SessionState {
        self.imp().state.get()
    }

    /// Set the current state of the session.
    fn set_state(&self, state: SessionState) {
        let old_state = self.state();

        if old_state == SessionState::LoggedOut || old_state == state {
            // The session should be dismissed when it has been logged out, so
            // we don't accept anymore state changes.
            return;
        }

        self.imp().state.set(state);
        self.notify("state");
    }

    /// The currently selected room, if any.
    pub fn selected_room(&self) -> Option<Room> {
        self.imp()
            .content
            .item()
            .and_then(|item| item.downcast().ok())
    }

    pub fn select_room(&self, room: Option<Room>) {
        self.imp()
            .sidebar
            .set_selected_item(room.map(|item| item.upcast()));
    }

    pub fn select_item(&self, item: Option<glib::Object>) {
        self.imp().sidebar.set_selected_item(item);
    }

    pub fn select_room_by_id(&self, room_id: &RoomId) {
        if let Some(room) = self.room_list().get(room_id) {
            self.select_room(Some(room));
        } else {
            warn!("A room with id {room_id} couldn't be found");
        }
    }

    fn toggle_room_search(&self) {
        let room_search = self.imp().sidebar.room_search_bar();
        room_search.set_search_mode(!room_search.is_search_mode());
    }

    pub async fn prepare(&self) {
        self.update_user_profile();
        self.update_offline().await;

        self.room_list().load();
        self.setup_direct_room_handler();
        self.setup_room_encrypted_changes();

        self.set_state(SessionState::InitialSync);
        self.sync();

        debug!("A new session was prepared");
    }

    fn sync(&self) {
        if self.state() < SessionState::InitialSync || self.is_offline() {
            return;
        }

        let client = self.client();
        let session_weak: glib::SendWeakRef<Session> = self.downgrade().into();

        let handle = spawn_tokio!(async move {
            // TODO: only create the filter once and reuse it in the future
            let room_event_filter = assign!(RoomEventFilter::default(), {
                lazy_load_options: LazyLoadOptions::Enabled {include_redundant_members: false},
            });
            let filter = assign!(FilterDefinition::default(), {
                room: assign!(RoomFilter::empty(), {
                    include_leave: true,
                    state: room_event_filter,
                }),
            });

            let sync_settings = SyncSettings::new()
                .timeout(Duration::from_secs(30))
                .filter(filter.into());

            let mut sync_stream = Box::pin(client.sync_stream(sync_settings).await);
            while let Some(response) = sync_stream.next().await {
                let session_weak = session_weak.clone();
                let ctx = glib::MainContext::default();
                ctx.spawn(async move {
                    if let Some(session) = session_weak.upgrade() {
                        session.handle_sync_response(response);
                    }
                });
            }
        });

        self.imp().sync_tokio_handle.replace(Some(handle));
    }

    /// Whether this session is verified with cross-signing.
    pub async fn is_verified(&self) -> bool {
        let client = self.client();
        let e2ee_device_handle = spawn_tokio!(async move {
            let user_id = client.user_id().unwrap();
            let device_id = client.device_id().unwrap();
            client.encryption().get_device(user_id, device_id).await
        });

        match e2ee_device_handle.await.unwrap() {
            Ok(Some(device)) => device.is_verified_with_cross_signing(),
            Ok(None) => {
                error!("Could not find this session’s encryption profile");
                false
            }
            Err(error) => {
                error!("Failed to get session’s encryption profile: {error}");
                false
            }
        }
    }

    pub async fn finish_initialization(&self) {
        let obj_weak = glib::SendWeakRef::from(self.downgrade());
        self.client()
            .register_notification_handler(move |notification, _, _| {
                let obj_weak = obj_weak.clone();
                async move {
                    let ctx = glib::MainContext::default();
                    ctx.spawn(async move {
                        spawn!(async move {
                            if let Some(obj) = obj_weak.upgrade() {
                                obj.notifications().show(notification);
                            }
                        });
                    });
                }
            })
            .await;
    }

    /// The current settings for this session.
    pub fn settings(&self) -> &SessionSettings {
        self.imp().settings.get().unwrap()
    }

    pub fn room_list(&self) -> &RoomList {
        self.item_list().room_list()
    }

    pub fn verification_list(&self) -> &VerificationList {
        self.item_list().verification_list()
    }

    /// The list of items in the sidebar.
    pub fn item_list(&self) -> &ItemList {
        self.imp()
            .item_list
            .get_or_init(|| ItemList::new(&RoomList::new(self), &VerificationList::new(self)))
    }

    /// The user of this session.
    pub fn user(&self) -> Option<&User> {
        self.imp().user.get()
    }

    /// Update the profile of this session’s user.
    ///
    /// Fetches the updated profile and updates the local data.
    pub fn update_user_profile(&self) {
        let client = self.client();
        let user = self.user().unwrap().to_owned();

        let handle = spawn_tokio!(async move { client.account().get_profile().await });

        spawn!(glib::PRIORITY_LOW, async move {
            match handle.await.unwrap() {
                Ok(res) => {
                    user.set_display_name(res.displayname);
                    user.set_avatar_url(res.avatar_url);
                }
                Err(error) => error!("Couldn’t fetch account metadata: {error}"),
            }
        });
    }

    pub fn client(&self) -> Client {
        self.imp()
            .client
            .get()
            .expect("The session wasn't prepared")
            .clone()
    }

    /// Whether this session has a connection to the homeserver.
    pub fn is_offline(&self) -> bool {
        self.imp().offline.get()
    }

    async fn update_offline(&self) {
        let imp = self.imp();
        let monitor = gio::NetworkMonitor::default();

        let is_offline = if monitor.is_network_available() {
            if let Some(info) = imp.info.get() {
                !check_if_reachable(&info.homeserver).await
            } else {
                false
            }
        } else {
            true
        };

        if self.is_offline() == is_offline {
            return;
        }

        if is_offline {
            debug!("This session is now offline");
        } else {
            debug!("This session is now online");
        }

        imp.offline.set(is_offline);

        if let Some(handle) = imp.sync_tokio_handle.take() {
            handle.abort();
        }

        // Restart the sync loop when online
        self.sync();

        self.notify("offline");
    }

    pub fn connect_logged_out<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_notify_local(Some("state"), move |obj, _| {
            if obj.state() == SessionState::LoggedOut {
                f(obj);
            }
        })
    }

    pub fn connect_ready<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_notify_local(Some("state"), move |obj, _| {
            if obj.state() == SessionState::Ready {
                f(obj);
            }
        })
    }

    fn handle_sync_response(&self, response: Result<SyncResponse, matrix_sdk::Error>) {
        debug!("Received sync response");
        match response {
            Ok(response) => {
                self.room_list().handle_response_rooms(response.rooms);
                self.verification_list()
                    .handle_response_to_device(response.to_device);

                if self.state() < SessionState::Ready {
                    self.set_state(SessionState::Ready);

                    spawn!(clone!(@weak self as obj => async move {
                        obj.finish_initialization().await;
                    }));
                }
            }
            Err(error) => {
                if let Some(kind) = error.client_api_error_kind() {
                    if matches!(kind, ErrorKind::UnknownToken { .. }) {
                        self.handle_logged_out();
                    }
                }
                error!("Failed to perform sync: {error}");
            }
        }
    }

    /// Returns the parent GtkWindow containing this widget.
    fn parent_window(&self) -> Option<Window> {
        self.root()?.downcast().ok()
    }

    fn open_account_settings(&self) {
        let window = AccountSettings::new(self.parent_window().as_ref(), self);
        window.present();
    }

    fn show_room_creation_dialog(&self) {
        let window = RoomCreation::new(self.parent_window().as_ref(), self);
        window.present();
    }

    async fn show_join_room_dialog(&self) {
        let dialog = JoinRoomDialog::new(self.parent_window().as_ref(), self);
        dialog.present();
    }

    pub async fn logout(&self) {
        let stack = &self.imp().stack;

        debug!("The session is about to be logged out");

        // First stop the verification in progress
        if let Some(session_verification) = stack.child_by_name("session-verification") {
            stack.remove(&session_verification);
        }

        let client = self.client();
        let handle = spawn_tokio!(async move {
            let request = logout::v3::Request::new();
            client.send(request, None).await
        });

        match handle.await.unwrap() {
            Ok(_) => self.cleanup_session().await,
            Err(error) => {
                error!("Couldn’t logout the session: {error}");
                toast!(self, gettext("Failed to logout the session."));
            }
        }
    }

    /// Handle that the session has been logged out.
    ///
    /// This should only be called if the session has been logged out without
    /// `Session::logout`.
    pub fn handle_logged_out(&self) {
        // TODO: Show error screen. See: https://gitlab.gnome.org/GNOME/fractal/-/issues/901

        spawn!(
            glib::PRIORITY_LOW,
            clone!(@strong self as obj => async move {
                obj.cleanup_session().await;
            })
        );
    }

    pub fn handle_paste_action(&self) {
        self.imp().content.handle_paste_action();
    }

    async fn cleanup_session(&self) {
        let imp = self.imp();
        let info = imp.info.get().unwrap();

        self.set_state(SessionState::LoggedOut);

        if let Some(handle) = imp.sync_tokio_handle.take() {
            handle.abort();
        }

        if let Some(settings) = imp.settings.get() {
            settings.delete();
        }

        let session_info = info.clone();
        let handle = spawn_tokio!(async move { secret::remove_session(&session_info).await });
        if let Err(error) = handle.await.unwrap() {
            error!("Failed to remove credentials from SecretService after logout: {error}");
        }

        if let Err(error) = fs::remove_dir_all(info.path.clone()) {
            error!("Failed to remove database after logout: {error}");
        }

        self.notifications().clear();

        debug!("The logged out session was cleaned up");
    }

    /// Show the content of the session
    pub fn show_content(&self) {
        let imp = self.imp();

        imp.stack.set_visible_child(&*imp.overlay);

        if let Some(window) = self.parent_window() {
            window.switch_to_sessions_page();
        }
    }

    /// Show a media event.
    pub fn show_media(&self, event: &Event, source_widget: &impl IsA<gtk::Widget>) {
        let Some(message) = event.message() else {
            error!("Trying to open the media viewer with an event that is not a message");
            return;
        };

        let imp = self.imp();
        imp.media_viewer
            .set_message(&event.room(), event.event_id().unwrap(), message);
        imp.media_viewer.reveal(source_widget);
    }

    fn setup_direct_room_handler(&self) {
        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                let obj_weak = glib::SendWeakRef::from(obj.downgrade());
                obj.client().add_event_handler(
                    move |event: GlobalAccountDataEvent<DirectEventContent>| {
                        let obj_weak = obj_weak.clone();
                        async move {
                            let ctx = glib::MainContext::default();
                            ctx.spawn(async move {
                                spawn!(async move {
                                    if let Some(session) = obj_weak.upgrade() {
                                        let room_ids = event.content.iter().fold(HashSet::new(), |mut acc, (_, rooms)| {
                                            acc.extend(rooms);
                                            acc
                                        });
                                        for room_id in room_ids {
                                            if let Some(room) = session.room_list().get(room_id) {
                                                room.load_category();
                                            }
                                        }
                                    }
                                });
                            });
                        }
                    },
                );
            })
        );
    }

    fn setup_room_encrypted_changes(&self) {
        let session_weak = glib::SendWeakRef::from(self.downgrade());
        let client = self.client();
        spawn_tokio!(async move {
            client.add_event_handler(move |_: SyncRoomEncryptionEvent, matrix_room: MatrixRoom| {
                let session_weak = session_weak.clone();
                async move {
                    let ctx = glib::MainContext::default();
                    ctx.spawn(async move {
                        if let Some(session) = session_weak.upgrade() {
                            if let Some(room) = session.room_list().get(matrix_room.room_id()) {
                                room.set_is_encrypted(true);
                            }
                        }
                    });
                }
            });
        });
    }

    pub fn notifications(&self) -> &Notifications {
        &self.imp().notifications
    }
}

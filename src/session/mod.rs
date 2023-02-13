mod account_settings;
mod avatar;
mod content;
mod event_source_dialog;
mod media_viewer;
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
        api::{
            client::{
                error::{Error as ClientApiError, ErrorBody, ErrorKind},
                filter::{FilterDefinition, LazyLoadOptions, RoomEventFilter, RoomFilter},
                session::logout,
            },
            error::FromHttpResponseError,
        },
        assign,
        events::{
            direct::DirectEventContent, room::encryption::SyncRoomEncryptionEvent,
            GlobalAccountDataEvent,
        },
        matrix_uri::MatrixId,
        MatrixToUri, MatrixUri, OwnedEventId, OwnedRoomId, OwnedRoomOrAliasId, OwnedServerName,
        RoomId, RoomOrAliasId,
    },
    sync::SyncResponse,
    Client, HttpError, RumaApiError,
};
use ruma::{api::client::push::get_notifications::v3::Notification, EventId};
use tokio::task::JoinHandle;

use self::{
    account_settings::AccountSettings,
    content::{verification::SessionVerification, Content},
    media_viewer::MediaViewer,
    room_list::RoomList,
    sidebar::Sidebar,
    verification::VerificationList,
};
pub use self::{
    avatar::Avatar,
    room::{Event, Room},
    room_creation::RoomCreation,
    settings::SessionSettings,
    user::{User, UserActions, UserExt},
};
use crate::{
    application::AppShowRoomPayload,
    secret::{self, StoredSession},
    session::sidebar::ItemList,
    spawn, spawn_tokio, toast,
    utils::{check_if_reachable, matrix::get_event_body},
    Application, Window,
};

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };

    use glib::subclass::{InitializingObject, Signal};
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
        pub is_loaded: Cell<bool>,
        pub prepared: Cell<bool>,
        pub logout_on_dispose: Cell<bool>,
        pub info: OnceCell<StoredSession>,
        pub sync_tokio_handle: RefCell<Option<JoinHandle<()>>>,
        pub offline_handler_id: RefCell<Option<SignalHandlerId>>,
        pub offline: Cell<bool>,
        pub settings: OnceCell<SessionSettings>,
        /// A map of room ID to list of event IDs for which a notification was
        /// sent to the system.
        pub notifications: RefCell<HashMap<OwnedRoomId, Vec<OwnedEventId>>>,
        pub window_active_handler_id: RefCell<Option<SignalHandlerId>>,
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
                    session.imp().logout_on_dispose.set(false);
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

            klass.install_action("session.mark-ready", None, move |session, _, _| {
                spawn!(clone!(@weak session => async move {
                    session.mark_ready().await;
                }));
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Session {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
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
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "session-id" => obj.session_id().to_value(),
                "item-list" => obj.item_list().to_value(),
                "user" => obj.user().to_value(),
                "offline" => obj.is_offline().to_value(),
                _ => unimplemented!(),
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("ready").build(),
                    Signal::builder("logged-out").build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

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
                    obj.withdraw_notifications_for_selected_room();
                }),
            );

            obj.connect_parent_notify(|obj| {
                if let Some(window) = obj.root().and_then(|root| root.downcast::<Window>().ok()) {
                    let handler_id =
                        window.connect_is_active_notify(clone!(@weak obj => move |window| {
                            // When the window becomes active, withdraw the notifications
                            // of the room that is displayed.
                            if window.is_active()
                                && window.current_session_id().as_deref() == obj.session_id()
                            {
                                obj.withdraw_notifications_for_selected_room();
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

            if self.logout_on_dispose.get() {
                glib::MainContext::default().block_on(obj.logout());
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
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    pub fn session_id(&self) -> Option<&str> {
        self.imp().info.get().map(|info| info.id())
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
            warn!("A room with id {} couldn't be found", room_id);
        }
    }

    fn toggle_room_search(&self) {
        let room_search = self.imp().sidebar.room_search_bar();
        room_search.set_search_mode(!room_search.is_search_mode());
    }

    pub async fn prepare(&self, client: Client, session: StoredSession) {
        let imp = self.imp();

        imp.client.set(client).unwrap();

        let user = User::new(self, &session.user_id);
        imp.user.set(user).unwrap();
        self.notify("user");

        self.update_user_profile();

        imp.settings
            .set(SessionSettings::new(session.id()))
            .unwrap();

        imp.info.set(session).unwrap();
        self.update_offline().await;

        self.room_list().load();
        self.setup_direct_room_handler();
        self.setup_room_encrypted_changes();

        self.set_is_prepared(true);
        self.sync();

        debug!("A new session was prepared");
    }

    fn sync(&self) {
        if !self.is_prepared() || self.is_offline() {
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

    async fn create_session_verification(&self) {
        let stack = &self.imp().stack;

        let widget = SessionVerification::new(self);
        stack.add_named(&widget, Some("session-verification"));
        stack.set_visible_child(&widget);
        if let Some(window) = self.parent_window() {
            window.switch_to_sessions_page();
        }
    }

    fn mark_loaded(&self) {
        let client = self.client();
        let user_id = self.user().unwrap().user_id();

        self.imp().is_loaded.set(true);

        let encryption = client.encryption();
        let need_new_identity = spawn_tokio!(async move {
            // If there is an error just assume we don't need a new identity since
            // we will try again during the session verification
            encryption
                .get_user_identity(&user_id)
                .await
                .map_or(false, |identity| identity.is_none())
        });

        spawn!(clone!(@weak self as obj => async move {
            let imp = obj.imp();
            if !obj.has_cross_signing_keys().await {
                if need_new_identity.await.unwrap() {
                    debug!("No E2EE identity found for this user, we need to create a new one…");
                    let encryption = obj.client().encryption();

                    let handle = spawn_tokio!(async move { encryption.bootstrap_cross_signing(None).await });
                    if handle.await.is_ok() {
                        imp.stack.set_visible_child(&*imp.overlay);
                        if let Some(window) = obj.parent_window() {
                            window.switch_to_sessions_page();
                        }
                        return;
                    }
                }

                debug!("The cross-signing keys were not found, we need to verify this session…");
                imp.logout_on_dispose.set(true);
                obj.create_session_verification().await;

                return;
            }

            obj.mark_ready().await;
        }));
    }

    pub async fn mark_ready(&self) {
        let imp = self.imp();

        imp.logout_on_dispose.set(false);

        if let Some(session_verification) = imp.stack.child_by_name("session-verification") {
            imp.stack.remove(&session_verification);
        }

        let obj_weak = glib::SendWeakRef::from(self.downgrade());
        self.client()
            .register_notification_handler(move |notification, _, _| {
                let obj_weak = obj_weak.clone();
                async move {
                    let ctx = glib::MainContext::default();
                    ctx.spawn(async move {
                        spawn!(async move {
                            if let Some(obj) = obj_weak.upgrade() {
                                obj.show_notification(notification);
                            }
                        });
                    });
                }
            })
            .await;

        self.emit_by_name::<()>("ready", &[]);
    }

    fn is_loaded(&self) -> bool {
        self.imp().is_loaded.get()
    }

    fn set_is_prepared(&self, prepared: bool) {
        if self.is_prepared() == prepared {
            return;
        }

        self.imp().prepared.set(prepared);
    }

    fn is_prepared(&self) -> bool {
        self.imp().prepared.get()
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
                Err(error) => error!("Couldn’t fetch account metadata: {}", error),
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
        self.connect_local("logged-out", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();

            f(&obj);

            None
        })
    }

    pub fn connect_ready<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("ready", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();

            f(&obj);

            None
        })
    }

    fn handle_sync_response(&self, response: Result<SyncResponse, matrix_sdk::Error>) {
        debug!("Received sync response");
        match response {
            Ok(response) => {
                self.room_list().handle_response_rooms(response.rooms);
                self.verification_list()
                    .handle_response_to_device(response.to_device_events);

                if !self.is_loaded() {
                    self.mark_loaded();
                }
            }
            Err(error) => {
                if let matrix_sdk::Error::Http(HttpError::Api(FromHttpResponseError::Server(
                    RumaApiError::ClientApi(ClientApiError {
                        body:
                            ErrorBody::Standard {
                                kind: ErrorKind::UnknownToken { .. },
                                ..
                            },
                        ..
                    }),
                ))) = &error
                {
                    self.handle_logged_out();
                }
                error!("Failed to perform sync: {:?}", error);
            }
        }
    }

    /// Returns the parent GtkWindow containing this widget.
    fn parent_window(&self) -> Option<Window> {
        self.root()?.downcast().ok()
    }

    fn open_account_settings(&self) {
        let window = AccountSettings::new(self.parent_window().as_ref(), self);
        window.show();
    }

    fn show_room_creation_dialog(&self) {
        let window = RoomCreation::new(self.parent_window().as_ref(), self);
        window.show();
    }

    async fn show_join_room_dialog(&self) {
        let builder = gtk::Builder::from_resource("/org/gnome/Fractal/join-room-dialog.ui");
        let dialog = builder.object::<adw::MessageDialog>("dialog").unwrap();
        let entry = builder.object::<gtk::Entry>("entry").unwrap();

        entry.connect_changed(clone!(@weak self as obj, @weak dialog => move |entry| {
            let room = parse_room(&entry.text());
            dialog.set_response_enabled("join", room.is_some());

            if room
                .and_then(|(room_id, _)| obj.room_list().find_joined_room(&room_id))
                .is_some()
            {
                dialog.set_response_label("join", &gettext("_View"));
            } else {
                dialog.set_response_label("join", &gettext("_Join"));
            }
        }));

        dialog.set_transient_for(self.parent_window().as_ref());
        if dialog.run_future().await == "join" {
            let (room_id, via) = match parse_room(&entry.text()) {
                Some(room) => room,
                None => return,
            };

            if let Some(room) = self.room_list().find_joined_room(&room_id) {
                self.select_room(Some(room));
            } else {
                self.room_list().join_by_id_or_alias(room_id, via)
            }
        }
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
                error!("Couldn’t logout the session {}", error);
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

        imp.is_loaded.set(false);

        if let Some(handle) = imp.sync_tokio_handle.take() {
            handle.abort();
        }

        if let Some(settings) = imp.settings.get() {
            settings.delete_settings();
        }

        let session_info = info.clone();
        let handle = spawn_tokio!(async move { secret::remove_session(&session_info).await });
        if let Err(error) = handle.await.unwrap() {
            error!(
                "Failed to remove credentials from SecretService after logout: {}",
                error
            );
        }

        if let Err(error) = fs::remove_dir_all(info.path.clone()) {
            error!("Failed to remove database after logout: {}", error);
        }

        self.clear_notifications();

        self.emit_by_name::<()>("logged-out", &[]);

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

    /// Show a media event
    pub fn show_media(&self, event: &Event, source_widget: &impl IsA<gtk::Widget>) {
        let imp = self.imp();
        imp.media_viewer.set_event(Some(event.clone()));
        imp.media_viewer.reveal(source_widget);
    }

    pub async fn cross_signing_status(&self) -> Option<CrossSigningStatus> {
        let encryption = self.client().encryption();

        spawn_tokio!(async move { encryption.cross_signing_status().await })
            .await
            .unwrap()
            .map(|s| CrossSigningStatus {
                has_self_signing: s.has_self_signing,
                has_user_signing: s.has_user_signing,
            })
    }

    pub async fn has_cross_signing_keys(&self) -> bool {
        self.cross_signing_status()
            .await
            .filter(|s| s.has_all_keys())
            .is_some()
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

    /// Ask the system to show the given notification, if applicable.
    ///
    /// The notification won't be shown if the application is active and this
    /// session is displayed.
    fn show_notification(&self, matrix_notification: Notification) {
        // Don't show notifications if they are disabled.
        if !self.settings().notifications_enabled() {
            return;
        }

        let window = self.parent_window().unwrap();

        // Don't show notifications for the current session if the window is active.
        if window.is_active() && window.current_session_id().as_deref() == self.session_id() {
            return;
        }

        let room = match self.room_list().get(&matrix_notification.room_id) {
            Some(room) => room,
            None => {
                warn!(
                    "Could not display notification for missing room {}",
                    matrix_notification.room_id
                );
                return;
            }
        };

        let event = match matrix_notification.event.deserialize() {
            Ok(event) => event,
            Err(error) => {
                warn!(
                    "Could not display notification for unrecognized event in room {}: {error}",
                    matrix_notification.room_id
                );
                return;
            }
        };

        let sender_name = room
            .members()
            .member_by_id(event.sender().to_owned())
            .display_name();

        let body = match get_event_body(&event, &sender_name) {
            Some(body) => body,
            None => {
                debug!("Received notification for event of unexpected type {event:?}",);
                return;
            }
        };

        let session_id = self.session_id().unwrap();
        let room_id = room.room_id();
        let event_id = event.event_id();

        let notification = gio::Notification::new(&room.display_name());
        notification.set_priority(gio::NotificationPriority::High);

        let payload = AppShowRoomPayload {
            session_id: session_id.to_owned(),
            room_id: room_id.to_owned(),
        };

        notification
            .set_default_action_and_target_value("app.show-room", Some(&payload.to_variant()));
        notification.set_body(Some(&body));

        if let Some(icon) = room.avatar().as_notification_icon(self.upcast_ref()) {
            notification.set_icon(&icon);
        }

        let id = notification_id(session_id, room_id, event_id);
        Application::default().send_notification(Some(&id), &notification);

        self.imp()
            .notifications
            .borrow_mut()
            .entry(room_id.to_owned())
            .or_default()
            .push(event_id.to_owned());
    }

    /// Ask the system to remove the known notifications for the currently
    /// selected room.
    ///
    /// Only the notifications that were shown since the application's startup
    /// are known, older ones might still be present.
    fn withdraw_notifications_for_selected_room(&self) {
        let room = match self.selected_room() {
            Some(room) => room,
            None => return,
        };

        let room_id = room.room_id();
        if let Some(notifications) = self.imp().notifications.borrow_mut().remove(room_id) {
            let session_id = self.session_id().unwrap();
            let app = Application::default();

            for event_id in notifications {
                let id = notification_id(session_id, room_id, &event_id);
                app.withdraw_notification(&id);
            }
        }
    }

    /// Ask the system to remove all the known notifications for this session.
    ///
    /// Only the notifications that were shown since the application's startup
    /// are known, older ones might still be present.
    fn clear_notifications(&self) {
        let session_id = self.session_id().unwrap();
        let app = Application::default();

        for (room_id, notifications) in self.imp().notifications.take() {
            for event_id in notifications {
                let id = notification_id(session_id, &room_id, &event_id);
                app.withdraw_notification(&id);
            }
        }
    }
}

fn parse_room(room: &str) -> Option<(OwnedRoomOrAliasId, Vec<OwnedServerName>)> {
    MatrixUri::parse(room)
        .ok()
        .and_then(|uri| match uri.id() {
            MatrixId::Room(room_id) => Some((room_id.clone().into(), uri.via().to_owned())),
            MatrixId::RoomAlias(room_alias) => {
                Some((room_alias.clone().into(), uri.via().to_owned()))
            }
            _ => None,
        })
        .or_else(|| {
            MatrixToUri::parse(room)
                .ok()
                .and_then(|uri| match uri.id() {
                    MatrixId::Room(room_id) => Some((room_id.clone().into(), uri.via().to_owned())),
                    MatrixId::RoomAlias(room_alias) => {
                        Some((room_alias.clone().into(), uri.via().to_owned()))
                    }
                    _ => None,
                })
        })
        .or_else(|| {
            RoomOrAliasId::parse(room)
                .ok()
                .map(|room_id| (room_id, vec![]))
        })
}

fn notification_id(session_id: &str, room_id: &RoomId, event_id: &EventId) -> String {
    format!("{session_id}:{room_id}:{event_id}")
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CrossSigningStatus {
    pub has_self_signing: bool,
    pub has_user_signing: bool,
}

impl CrossSigningStatus {
    /// Whether this status indicates that we have all the keys.
    pub fn has_all_keys(&self) -> bool {
        self.has_self_signing && self.has_user_signing
    }
}

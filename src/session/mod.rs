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
    deserialized_responses::SyncResponse,
    room::Room as MatrixRoom,
    ruma::{
        api::{
            client::{
                error::ErrorKind,
                filter::{FilterDefinition, LazyLoadOptions, RoomEventFilter, RoomFilter},
                session::logout,
            },
            error::{FromHttpResponseError, ServerError},
        },
        assign,
        events::{
            direct::DirectEventContent, room::encryption::SyncRoomEncryptionEvent,
            GlobalAccountDataEvent,
        },
        matrix_uri::MatrixId,
        MatrixToUri, MatrixUri, OwnedRoomOrAliasId, OwnedServerName, RoomId, RoomOrAliasId,
    },
    Client, HttpError, RumaApiError,
};
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
    room::{Room, SupportedEvent},
    room_creation::RoomCreation,
    settings::SessionSettings,
    user::{User, UserActions, UserExt},
};
use crate::{
    secret::{self, StoredSession},
    session::sidebar::ItemList,
    spawn, spawn_tokio, toast,
    utils::check_if_reachable,
    Window,
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::{InitializingObject, Signal};
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/session.ui")]
    pub struct Session {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
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
        pub is_ready: Cell<bool>,
        pub prepared: Cell<bool>,
        pub logout_on_dispose: Cell<bool>,
        pub info: OnceCell<StoredSession>,
        pub sync_tokio_handle: RefCell<Option<JoinHandle<()>>>,
        pub offline_handler_id: RefCell<Option<SignalHandlerId>>,
        pub offline: Cell<bool>,
        pub settings: OnceCell<SessionSettings>,
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
                    session.logout(true).await
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
                    glib::ParamSpecObject::new(
                        "item-list",
                        "Item List",
                        "The list of items in the sidebar",
                        ItemList::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "user",
                        "User",
                        "The user of this session",
                        User::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "offline",
                        "Offline",
                        "Whether this session has a connection to the homeserver",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "item-list" => obj.item_list().to_value(),
                "user" => obj.user().to_value(),
                "offline" => obj.is_offline().to_value(),
                _ => unimplemented!(),
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("ready", &[], <()>::static_type().into()).build(),
                    Signal::builder("logged-out", &[], <()>::static_type().into()).build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.sidebar.connect_notify_local(
                Some("selected-item"),
                clone!(@weak obj => move |_, _| {
                    let priv_ = obj.imp();

                    if priv_.sidebar.selected_item().is_none() {
                        priv_.leaflet.navigate(adw::NavigationDirection::Back);
                    } else {
                        priv_.leaflet.navigate(adw::NavigationDirection::Forward);
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
        }

        fn dispose(&self, obj: &Self::Type) {
            // Needs to be disconnected or else it may restart the sync
            if let Some(handler_id) = self.offline_handler_id.take() {
                gio::NetworkMonitor::default().disconnect(handler_id);
            }

            if let Some(handle) = self.sync_tokio_handle.take() {
                handle.abort();
            }

            if self.logout_on_dispose.get() {
                glib::MainContext::default().block_on(obj.logout(true));
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
        glib::Object::new(&[]).expect("Failed to create Session")
    }

    pub fn session_id(&self) -> &str {
        self.imp().info.get().unwrap().id()
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
        let priv_ = self.imp();

        priv_.client.set(client).unwrap();

        let user = User::new(self, &session.user_id);
        priv_.user.set(user).unwrap();
        self.notify("user");

        self.update_user_profile();

        priv_
            .settings
            .set(SessionSettings::new(session.id()))
            .unwrap();

        priv_.info.set(session).unwrap();
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
            let sync_token = client.sync_token().await;
            if sync_token.is_none() {
                debug!("Proceeding to initial sync…");
            }

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

    fn mark_ready(&self) {
        let client = self.client();
        let user_id = self.user().unwrap().user_id();

        self.imp().is_ready.set(true);

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
            let priv_ = obj.imp();
            if !obj.has_cross_signing_keys().await {
                if need_new_identity.await.unwrap() {
                    debug!("No E2EE identity found for this user, we need to create a new one…");
                    let encryption = obj.client().encryption();

                    let handle = spawn_tokio!(async move { encryption.bootstrap_cross_signing(None).await });
                    if handle.await.is_ok() {
                        priv_.stack.set_visible_child(&*priv_.leaflet);
                        if let Some(window) = obj.parent_window() {
                            window.switch_to_sessions_page();
                        }
                        return;
                    }
                }

                debug!("The cross-signing keys were not found, we need to verify this session…");
                priv_.logout_on_dispose.set(true);
                obj.create_session_verification().await;

                return;
            }

            obj.show_content();
        }));
    }

    fn is_ready(&self) -> bool {
        self.imp().is_ready.get()
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

    pub fn is_offline(&self) -> bool {
        self.imp().offline.get()
    }

    async fn update_offline(&self) {
        let priv_ = self.imp();
        let monitor = gio::NetworkMonitor::default();

        let is_offline = if monitor.is_network_available() {
            if let Some(info) = priv_.info.get() {
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

        priv_.offline.set(is_offline);

        if let Some(handle) = priv_.sync_tokio_handle.take() {
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
                    .handle_response_to_device(response.to_device);

                if !self.is_ready() {
                    self.mark_ready();
                }
            }
            Err(error) => {
                if let matrix_sdk::Error::Http(HttpError::Api(FromHttpResponseError::Server(
                    ServerError::Known(RumaApiError::ClientApi(ref error)),
                ))) = error
                {
                    if let ErrorKind::UnknownToken { soft_logout: _ } = error.kind {
                        self.handle_logged_out();
                    }
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

    pub async fn logout(&self, cleanup: bool) {
        let stack = &self.imp().stack;
        self.emit_by_name::<()>("logged-out", &[]);

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
            Ok(_) => {
                if cleanup {
                    self.cleanup_session().await
                }
            }
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
        self.emit_by_name::<()>("logged-out", &[]);
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
        let priv_ = self.imp();
        let info = priv_.info.get().unwrap();

        priv_.is_ready.set(false);

        if let Some(handle) = priv_.sync_tokio_handle.take() {
            handle.abort();
        }

        if let Some(settings) = priv_.settings.get() {
            settings.delete_settings();
        }

        if let Err(error) = secret::remove_session(info).await {
            error!(
                "Failed to remove credentials from SecretService after logout: {}",
                error
            );
        }

        if let Err(error) = fs::remove_dir_all(info.path.clone()) {
            error!("Failed to remove database after logout: {}", error);
        }

        debug!("The logged out session was cleaned up");
    }

    /// Show the content of the session
    pub fn show_content(&self) {
        let priv_ = self.imp();
        // FIXME: we should actually check if we have now the keys
        spawn!(clone!(@weak self as obj => async move {
            obj.has_cross_signing_keys().await;
        }));
        priv_.stack.set_visible_child(&*priv_.leaflet);
        priv_.logout_on_dispose.set(false);
        if let Some(window) = self.parent_window() {
            window.switch_to_sessions_page();
        }

        if let Some(session_verification) = priv_.stack.child_by_name("session-verification") {
            priv_.stack.remove(&session_verification);
        }

        self.emit_by_name::<()>("ready", &[]);
    }

    /// Show a media event
    pub fn show_media(&self, event: &SupportedEvent) {
        let priv_ = self.imp();
        priv_.media_viewer.set_event(Some(event.clone()));

        priv_.stack.set_visible_child(&*priv_.media_viewer);
    }

    async fn has_cross_signing_keys(&self) -> bool {
        let encryption = self.client().encryption();
        spawn_tokio!(async move {
            if let Some(cross_signing_status) = encryption.cross_signing_status().await {
                debug!("Cross signing keys status: {:?}", cross_signing_status);
                cross_signing_status.has_self_signing && cross_signing_status.has_user_signing
            } else {
                debug!("Session doesn't have needed cross signing keys");
                false
            }
        })
        .await
        .unwrap()
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
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
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

mod account_settings;
mod avatar;
mod content;
mod event_source_dialog;
mod room;
mod room_creation;
mod room_list;
mod sidebar;
mod user;

use self::account_settings::AccountSettings;
pub use self::avatar::Avatar;
use self::content::Content;
pub use self::room::Room;
pub use self::room_creation::RoomCreation;
use self::room_list::RoomList;
use self::sidebar::Sidebar;
pub use self::user::{User, UserExt};

use crate::secret;
use crate::secret::StoredSession;
use crate::utils::do_async;
use crate::Error;
use crate::Window;
use crate::RUNTIME;

use crate::matrix_error::UserFacingMatrixError;
use crate::session::content::ContentType;
use adw::subclass::prelude::BinImpl;
use futures::StreamExt;
use gettextrs::gettext;
use gtk::subclass::prelude::*;
use gtk::{self, prelude::*};
use gtk::{
    gdk, glib, glib::clone, glib::source::SourceId, glib::SyncSender, CompositeTemplate,
    SelectionModel,
};
use log::{debug, error};
use matrix_sdk::ruma::{
    api::client::r0::{
        filter::{FilterDefinition, LazyLoadOptions, RoomEventFilter, RoomFilter},
        session::logout,
    },
    assign,
};
use matrix_sdk::{
    config::{ClientConfig, RequestConfig, SyncSettings},
    deserialized_responses::SyncResponse,
    ruma::api::{
        client::error::ErrorKind,
        error::{FromHttpResponseError, ServerError},
    },
    uuid::Uuid,
    Client, HttpError,
};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use std::fs;
use std::time::Duration;
use tokio::task::JoinHandle;
use url::Url;

mod imp {
    use super::*;
    use glib::subclass::{InitializingObject, Signal};
    use once_cell::{sync::Lazy, unsync::OnceCell};
    use std::cell::{Cell, RefCell};

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/session.ui")]
    pub struct Session {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub content: TemplateChild<adw::Leaflet>,
        #[template_child]
        pub sidebar: TemplateChild<Sidebar>,
        pub client: RefCell<Option<Client>>,
        pub room_list: OnceCell<RoomList>,
        pub user: OnceCell<User>,
        pub selected_room: RefCell<Option<Room>>,
        pub selected_content_type: Cell<ContentType>,
        pub is_ready: Cell<bool>,
        pub info: OnceCell<StoredSession>,
        pub source_id: RefCell<Option<SourceId>>,
        pub sync_tokio_handle: RefCell<Option<JoinHandle<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Session {
        const NAME: &'static str = "Session";
        type Type = super::Session;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("session.close-room", None, move |session, _, _| {
                session.set_selected_room(None);
            });

            klass.install_action("session.logout", None, move |session, _, _| {
                session.logout();
            });

            klass.install_action("session.room-creation", None, move |session, _, _| {
                session.show_room_creation_dialog();
            });

            klass.add_binding_action(
                gdk::keys::constants::Escape,
                gdk::ModifierType::empty(),
                "session.close-room",
                None,
            );

            klass.install_action("session.toggle-room-search", None, move |session, _, _| {
                session.toggle_room_search();
            });

            klass.add_binding_action(
                gdk::keys::constants::k,
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
            Sidebar::static_type();
            Content::static_type();
            obj.init_template();
        }
    }

    impl ObjectImpl for Session {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpec::new_object(
                        "room-list",
                        "Room List",
                        "The list of rooms",
                        RoomList::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpec::new_object(
                        "selected-room",
                        "Selected Room",
                        "The selected room in this session",
                        Room::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpec::new_enum(
                        "selected-content-type",
                        "Selected Content Type",
                        "The current content type selected",
                        ContentType::static_type(),
                        ContentType::default() as i32,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpec::new_object(
                        "user",
                        "User",
                        "The user of this session",
                        User::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "selected-room" => {
                    let selected_room = value.get().unwrap();
                    obj.set_selected_room(selected_room);
                }
                "selected-content-type" => obj.set_selected_content_type(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room-list" => obj.room_list().to_value(),
                "selected-room" => obj.selected_room().to_value(),
                "user" => obj.user().to_value(),
                "selected-content-type" => obj.selected_content_type().to_value(),
                _ => unimplemented!(),
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder(
                        "prepared",
                        &[Option::<Error>::static_type().into()],
                        <()>::static_type().into(),
                    )
                    .build(),
                    Signal::builder("logged-out", &[], <()>::static_type().into()).build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn dispose(&self, _obj: &Self::Type) {
            if let Some(source_id) = self.source_id.take() {
                let _ = glib::Source::remove(source_id);
            }

            if let Some(handle) = self.sync_tokio_handle.take() {
                handle.abort();
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

    pub fn selected_content_type(&self) -> ContentType {
        let priv_ = imp::Session::from_instance(self);
        priv_.selected_content_type.get()
    }

    pub fn set_selected_content_type(&self, selected_type: ContentType) {
        let priv_ = imp::Session::from_instance(self);

        if self.selected_content_type() == selected_type {
            return;
        }

        if selected_type == ContentType::None {
            priv_.content.navigate(adw::NavigationDirection::Back);
        } else {
            priv_.content.navigate(adw::NavigationDirection::Forward);
        }

        priv_.selected_content_type.set(selected_type);

        self.notify("selected-content-type");
    }

    pub fn selected_room(&self) -> Option<Room> {
        let priv_ = imp::Session::from_instance(self);
        priv_.selected_room.borrow().clone()
    }

    pub fn set_selected_room(&self, selected_room: Option<Room>) {
        let priv_ = imp::Session::from_instance(self);

        if self.selected_room() == selected_room {
            return;
        }

        priv_.selected_room.replace(selected_room);

        self.notify("selected-room");
    }

    pub fn login_with_password(&self, homeserver: Url, username: String, password: String) {
        let mut path = glib::user_data_dir();
        path.push(
            &Uuid::new_v4()
                .to_hyphenated()
                .encode_lower(&mut Uuid::encode_buffer()),
        );

        do_async(
            glib::PRIORITY_DEFAULT_IDLE,
            async move {
                let passphrase: String = {
                    let mut rng = thread_rng();
                    (&mut rng)
                        .sample_iter(Alphanumeric)
                        .take(30)
                        .map(char::from)
                        .collect()
                };
                let config = ClientConfig::new()
                    .request_config(RequestConfig::new().retry_limit(2))
                    .passphrase(passphrase.clone())
                    .store_path(path.clone());

                let client = Client::new_with_config(homeserver.clone(), config).unwrap();
                let response = client
                    .login(&username, &password, None, Some("Fractal Next"))
                    .await;
                match response {
                    Ok(response) => Ok((
                        client,
                        StoredSession {
                            homeserver,
                            path,
                            passphrase,
                            access_token: response.access_token,
                            user_id: response.user_id,
                            device_id: response.device_id,
                        },
                    )),
                    Err(error) => {
                        // Remove the store created by Client::new()
                        fs::remove_dir_all(path).unwrap();
                        Err(error)
                    }
                }
            },
            clone!(@weak self as obj => move |result| async move {
                obj.handle_login_result(result, true);
            }),
        );
    }

    fn toggle_room_search(&self) {
        let priv_ = imp::Session::from_instance(self);
        let room_search = priv_.sidebar.room_search_bar();
        room_search.set_search_mode(!room_search.is_search_mode());
    }

    pub fn login_with_previous_session(&self, session: StoredSession) {
        do_async(
            glib::PRIORITY_DEFAULT_IDLE,
            async move {
                let config = ClientConfig::new()
                    .request_config(RequestConfig::new().retry_limit(2))
                    .passphrase(session.passphrase.clone())
                    .store_path(session.path.clone());

                let client = Client::new_with_config(session.homeserver.clone(), config).unwrap();
                client
                    .restore_login(matrix_sdk::Session {
                        user_id: session.user_id.clone(),
                        device_id: session.device_id.clone(),
                        access_token: session.access_token.clone(),
                    })
                    .await
                    .map(|_| (client, session))
            },
            clone!(@weak self as obj => move |result| async move {
                obj.handle_login_result(result, false);
            }),
        );
    }

    fn handle_login_result(
        &self,
        result: Result<(Client, StoredSession), matrix_sdk::Error>,
        store_session: bool,
    ) {
        let priv_ = imp::Session::from_instance(self);
        let error = match result {
            Ok((client, session)) => {
                priv_.client.replace(Some(client.clone()));
                let user = User::new(self, &session.user_id);
                priv_.user.set(user.clone()).unwrap();
                self.notify("user");

                do_async(
                    glib::PRIORITY_LOW,
                    async move {
                        let display_name = client.display_name().await?;
                        let avatar_url = client.avatar_url().await?;
                        Ok((display_name, avatar_url))
                    },
                    move |result: matrix_sdk::Result<_>| async move {
                        match result {
                            Ok((display_name, avatar_url)) => {
                                user.set_display_name(display_name);
                                user.set_avatar_url(avatar_url);
                            }
                            Err(error) => error!("Couldn’t fetch account metadata: {}", error),
                        };
                    },
                );

                if store_session {
                    // TODO: report secret service errors
                    secret::store_session(&session).unwrap();
                }

                priv_.info.set(session).unwrap();

                self.room_list().load();
                self.sync();

                None
            }
            Err(error) => {
                error!("Failed to prepare the session: {}", error);

                let error_string = error.to_user_facing();

                Some(Error::new(move |_| {
                    let error_label = gtk::LabelBuilder::new()
                        .label(&error_string)
                        .wrap(true)
                        .build();
                    Some(error_label.upcast())
                }))
            }
        };

        self.emit_by_name("prepared", &[&error]).unwrap();
    }

    fn sync(&self) {
        let priv_ = imp::Session::from_instance(self);
        let sender = self.create_new_sync_response_sender();
        let client = self.client();
        let handle = RUNTIME.spawn(async move {
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

            // We need to automatically restart the stream because it gets killed on error
            loop {
                let mut sync_stream = Box::pin(client.sync_stream(sync_settings.clone()).await);
                while let Some(response) = sync_stream.next().await {
                    if sender.send(response).is_err() {
                        debug!("Stop syncing because the session was disposed");
                        return;
                    }
                }
            }
        });

        priv_.sync_tokio_handle.replace(Some(handle));
    }

    fn mark_ready(&self) {
        let priv_ = &imp::Session::from_instance(self);
        priv_.stack.set_visible_child(&*priv_.content);
        priv_.is_ready.set(true);
    }

    fn is_ready(&self) -> bool {
        let priv_ = &imp::Session::from_instance(self);
        priv_.is_ready.get()
    }

    pub fn room_list(&self) -> &RoomList {
        let priv_ = &imp::Session::from_instance(self);
        priv_.room_list.get_or_init(|| RoomList::new(self))
    }

    pub fn user(&self) -> Option<&User> {
        let priv_ = &imp::Session::from_instance(self);
        priv_.user.get()
    }

    pub fn client(&self) -> Client {
        let priv_ = &imp::Session::from_instance(self);
        priv_
            .client
            .borrow()
            .clone()
            .expect("The session isn't ready")
    }

    /// Sets up the required channel to receive new room events
    fn create_new_sync_response_sender(
        &self,
    ) -> SyncSender<Result<SyncResponse, matrix_sdk::Error>> {
        let priv_ = imp::Session::from_instance(self);
        let (sender, receiver) = glib::MainContext::sync_channel::<
            Result<SyncResponse, matrix_sdk::Error>,
        >(Default::default(), 100);
        let source_id = receiver.attach(
            None,
            clone!(@weak self as obj => @default-return glib::Continue(false), move |response| {
                obj.handle_sync_response(response);

                glib::Continue(true)
            }),
        );

        priv_.source_id.replace(Some(source_id));

        sender
    }

    /// Connects the prepared signals to the function f given in input
    pub fn connect_prepared<F: Fn(&Self, Option<Error>) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("prepared", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            let err = values[1].get::<Option<Error>>().unwrap();

            f(&obj, err);

            None
        })
        .unwrap()
    }

    pub fn connect_logged_out<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("logged-out", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();

            f(&obj);

            None
        })
        .unwrap()
    }

    fn handle_sync_response(&self, response: Result<SyncResponse, matrix_sdk::Error>) {
        match response {
            Ok(response) => {
                if !self.is_ready() {
                    self.mark_ready();
                }
                self.room_list().handle_response_rooms(response.rooms);
            }
            Err(error) => {
                if let matrix_sdk::Error::Http(HttpError::ClientApi(FromHttpResponseError::Http(
                    ServerError::Known(ref error),
                ))) = error
                {
                    match error.kind {
                        ErrorKind::UnknownToken { soft_logout: _ } => {
                            self.cleanup_session();
                        }
                        _ => {}
                    }
                }
                error!("Failed to perform sync: {:?}", error);
            }
        }
    }

    pub fn set_logged_in_users(&self, sessions_stack_pages: &SelectionModel) {
        let priv_ = &imp::Session::from_instance(self);
        priv_
            .sidebar
            .set_logged_in_users(sessions_stack_pages, self);
    }

    /// Returns the parent GtkWindow containing this widget.
    fn parent_window(&self) -> Option<Window> {
        self.root()?.downcast().ok()
    }

    fn open_account_settings(&self) {
        if let Some(user) = self.user() {
            let window = AccountSettings::new(self.parent_window().as_ref(), user);
            window.show();
        }
    }

    fn show_room_creation_dialog(&self) {
        let window = RoomCreation::new(self.parent_window().as_ref(), self);
        window.show();
    }

    pub fn logout(&self) {
        let client = self.client();

        do_async(
            glib::PRIORITY_DEFAULT_IDLE,
            async move {
                let request = logout::Request::new();
                client.send(request, None).await
            },
            clone!(@weak self as obj => move |result| async move {
                match result {
                    Ok(_) => obj.cleanup_session(),
                    Err(error) => {
                        error!("Couldn’t logout the session {}", error);
                        let error = Error::new(
                                clone!(@weak obj => @default-return None, move |_| {
                                        let label = gtk::Label::new(Some(&gettext("Failed to logout the session.")));

                                        Some(label.upcast())
                                }),
                        );

                        if let Some(window) = obj.parent_window() {
                            window.append_error(&error);
                        }
                    }
                }
            }),
        );
    }

    fn cleanup_session(&self) {
        let priv_ = imp::Session::from_instance(self);
        let info = priv_.info.get().unwrap();

        priv_.is_ready.set(false);

        if let Some(source_id) = priv_.source_id.take() {
            let _ = glib::Source::remove(source_id);
        }

        if let Some(handle) = priv_.sync_tokio_handle.take() {
            handle.abort();
        }

        if let Err(error) = secret::remove_session(info) {
            error!(
                "Failed to remove credentials from SecretService after logout: {}",
                error
            );
        }

        if let Err(error) = fs::remove_dir_all(info.path.clone()) {
            error!("Failed to remove database after logout: {}", error);
        }

        self.emit_by_name("logged-out", &[]).unwrap();
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

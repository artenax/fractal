use std::{fs, path::PathBuf};

use adw::{prelude::*, subclass::prelude::BinImpl};
use gettextrs::gettext;
use gtk::{self, gdk, gio, glib, glib::clone, subclass::prelude::*, CompositeTemplate};
use log::{error, warn};
use matrix_sdk::{
    config::{RequestConfig, StoreConfig},
    ruma::api::client::session::get_login_types::v3::LoginType,
    store::{MigrationConflictStrategy, OpenStoreError, SledStateStore},
    Client, ClientBuildError, StoreError,
};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use ruma::OwnedServerName;
use thiserror::Error;
use url::Url;

mod advanced_dialog;
mod homeserver_page;
mod idp_button;
mod method_page;
mod sso_page;

use self::{
    advanced_dialog::LoginAdvancedDialog, homeserver_page::LoginHomeserverPage,
    method_page::LoginMethodPage, sso_page::LoginSsoPage,
};
use crate::{
    components::SpinnerButton,
    secret::{self, Secret, StoredSession},
    spawn, spawn_tokio, toast,
    user_facing_error::UserFacingError,
    Session, Window,
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{
        subclass::{InitializingObject, Signal},
        SignalHandlerId,
    };
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/login.ui")]
    pub struct Login {
        /// The current created Matrix client and its configuration.
        pub created_client: RefCell<Option<CreatedClient>>,
        #[template_child]
        pub back_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub next_button: TemplateChild<SpinnerButton>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub homeserver_page: TemplateChild<LoginHomeserverPage>,
        #[template_child]
        pub method_page: TemplateChild<LoginMethodPage>,
        #[template_child]
        pub sso_page: TemplateChild<LoginSsoPage>,
        #[template_child]
        pub offline_info_bar: TemplateChild<gtk::InfoBar>,
        #[template_child]
        pub offline_info_bar_label: TemplateChild<gtk::Label>,
        pub prepared_source_id: RefCell<Option<SignalHandlerId>>,
        pub logged_out_source_id: RefCell<Option<SignalHandlerId>>,
        pub ready_source_id: RefCell<Option<SignalHandlerId>>,
        /// Whether auto-discovery is enabled.
        pub autodiscovery: Cell<bool>,
        pub supports_password: Cell<bool>,
        /// The homeserver to log into.
        pub homeserver: RefCell<Option<Url>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Login {
        const NAME: &'static str = "Login";
        type Type = super::Login;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_css_name("login");
            klass.set_accessible_role(gtk::AccessibleRole::Group);

            klass.install_action("login.update-next", None, move |widget, _, _| {
                widget.update_next_state()
            });
            klass.install_action("login.next", None, move |widget, _, _| widget.go_next());
            klass.install_action("login.prev", None, move |widget, _, _| widget.go_previous());
            klass.install_action("login.sso", Some("ms"), move |widget, _, variant| {
                let idp_id = variant.and_then(|v| v.get::<Option<String>>()).flatten();
                spawn!(clone!(@weak widget => async move {
                    widget.login_with_sso(idp_id).await;
                }));
            });
            klass.install_action("login.open-advanced", None, move |widget, _, _| {
                spawn!(clone!(@weak widget => async move {
                    widget.open_advanced_dialog().await;
                }));
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Login {
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder(
                    "new-session",
                    &[Session::static_type().into()],
                    <()>::static_type().into(),
                )
                .build()]
            });
            SIGNALS.as_ref()
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::new(
                        "homeserver",
                        "Homeserver",
                        "The homeserver to log into",
                        None,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "autodiscovery",
                        "Auto-discovery",
                        "Whether auto-discovery is enabled",
                        true,
                        glib::ParamFlags::READWRITE
                            | glib::ParamFlags::CONSTRUCT
                            | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "homeserver" => obj.homeserver_pretty().to_value(),
                "autodiscovery" => obj.autodiscovery().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "autodiscovery" => obj.set_autodiscovery(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            obj.action_set_enabled("login.next", false);

            self.parent_constructed(obj);

            let monitor = gio::NetworkMonitor::default();
            monitor.connect_network_changed(clone!(@weak obj => move |_, _| {
                obj.update_network_state();
            }));

            self.main_stack
                .connect_visible_child_notify(clone!(@weak obj => move |_|
                    obj.update_next_state();
                    obj.focus_default();
                ));

            obj.update_network_state();
        }

        fn dispose(&self, obj: &Self::Type) {
            obj.prune_created_client(true);
        }
    }

    impl WidgetImpl for Login {}

    impl BinImpl for Login {}
}

glib::wrapper! {
    /// A widget handling the login flows.
    pub struct Login(ObjectSubclass<imp::Login>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Login {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create Login")
    }

    fn parent_window(&self) -> Window {
        self.root()
            .and_then(|root| root.downcast().ok())
            .expect("Login needs to have a parent window")
    }

    pub fn created_client(&self) -> Option<CreatedClient> {
        self.imp().created_client.borrow().clone()
    }

    pub fn client(&self) -> Option<Client> {
        self.imp()
            .created_client
            .borrow()
            .as_ref()
            .map(|c| c.client.clone())
    }

    pub fn prune_created_client(&self, clean: bool) {
        if let Some(created_client) = self.imp().created_client.take() {
            if clean {
                if let Err(error) = fs::remove_dir_all(created_client.path) {
                    error!("Failed to remove newly-created database: {}", error);
                }
            }
        }
    }

    async fn set_created_client(&self, created_client: Option<CreatedClient>) {
        let homeserver = if let Some(c) = &created_client {
            Some(c.client.homeserver().await)
        } else {
            None
        };

        self.set_homeserver(homeserver);
        self.imp().created_client.replace(created_client);
    }

    pub fn homeserver(&self) -> Option<Url> {
        self.imp().homeserver.borrow().clone()
    }

    pub fn homeserver_pretty(&self) -> Option<String> {
        let homeserver = self.homeserver();
        homeserver
            .as_ref()
            .and_then(|url| url.as_ref().strip_suffix('/').map(ToOwned::to_owned))
            .or_else(|| homeserver.as_ref().map(ToString::to_string))
    }

    pub fn set_homeserver(&self, homeserver: Option<Url>) {
        let priv_ = self.imp();

        if self.homeserver() == homeserver {
            return;
        }

        priv_.homeserver.replace(homeserver);
        self.notify("homeserver");
    }

    pub fn autodiscovery(&self) -> bool {
        self.imp().autodiscovery.get()
    }

    pub fn set_autodiscovery(&self, autodiscovery: bool) {
        if self.autodiscovery() == autodiscovery {
            return;
        }

        self.imp().autodiscovery.set(autodiscovery);
        self.notify("autodiscovery");
        self.update_next_state();
    }

    fn visible_child(&self) -> String {
        self.imp().main_stack.visible_child_name().unwrap().into()
    }

    fn set_visible_child(&self, visible_child: &str) {
        // Clean up the created client when we come back to the homeserver selection.
        if visible_child == "homeserver" {
            self.prune_created_client(true);
        }

        self.imp().main_stack.set_visible_child_name(visible_child);
    }

    fn update_next_state(&self) {
        let priv_ = self.imp();
        match self.visible_child().as_ref() {
            "homeserver" => {
                self.enable_next_action(priv_.homeserver_page.can_go_next());
                priv_.next_button.set_visible(true);
            }
            "method" => {
                self.enable_next_action(priv_.method_page.can_go_next());
                priv_.next_button.set_visible(true);
            }
            _ => {
                priv_.next_button.set_visible(false);
            }
        }
    }

    fn enable_next_action(&self, enabled: bool) {
        self.action_set_enabled(
            "login.next",
            enabled && gio::NetworkMonitor::default().is_network_available(),
        );
    }

    fn go_next(&self) {
        self.freeze();

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match obj.visible_child().as_ref() {
                    "homeserver" => obj.get_homeserver().await,
                    "method" => obj.login_with_password().await,
                    _ => {}
                }

                obj.unfreeze();
            })
        );
    }

    fn go_previous(&self) {
        match self.visible_child().as_ref() {
            "method" => {
                self.set_visible_child("homeserver");
                self.imp().method_page.clean();
            }
            "sso" => {
                self.set_visible_child(if self.imp().supports_password.get() {
                    "method"
                } else {
                    "homeserver"
                });
            }
            _ => {
                self.activate_action("app.show-greeter", None).unwrap();
            }
        }
    }

    async fn open_advanced_dialog(&self) {
        let dialog = LoginAdvancedDialog::new(self.parent_window().upcast_ref());
        self.bind_property("autodiscovery", &dialog, "autodiscovery")
            .flags(glib::BindingFlags::SYNC_CREATE | glib::BindingFlags::BIDIRECTIONAL)
            .build();
        dialog.run_future().await;
    }

    async fn get_homeserver(&self) {
        let autodiscovery = self.autodiscovery();
        let homeserver_page = &self.imp().homeserver_page;

        let homeserver = if autodiscovery {
            HomeserverOrServerName::ServerName(homeserver_page.server_name().unwrap())
        } else {
            HomeserverOrServerName::Homeserver(homeserver_page.homeserver_url().unwrap())
        };

        let handle = spawn_tokio!(async move {
            CreatedClient::new(&homeserver, autodiscovery, None, None).await
        });

        match handle.await.unwrap() {
            Ok(created_client) => {
                self.set_created_client(Some(created_client)).await;
                self.check_login_types().await;
            }
            Err(error) => {
                if autodiscovery {
                    warn!("Failed to discover homeserver: {}", error);
                } else {
                    warn!("Failed to check homeserver: {}", error);
                }
                toast!(self, error.to_user_facing());

                // Clean up the created client because it's bound to the homeserver.
                self.prune_created_client(true);
            }
        };
    }

    async fn check_login_types(&self) {
        let client = self.client().unwrap();
        let handle = spawn_tokio!(async move { client.get_login_types().await });

        let login_types = match handle.await.unwrap() {
            Ok(res) => res,
            Err(error) => {
                warn!("Failed to get available login types: {error}");
                toast!(self, "Failed to get available login types.");
                return;
            }
        };

        let sso = login_types.flows.iter().find_map(|flow| {
            if let LoginType::Sso(sso) = flow {
                Some(sso)
            } else {
                None
            }
        });

        let has_password = login_types
            .flows
            .iter()
            .any(|flow| matches!(flow, LoginType::Password(_)));

        let priv_ = self.imp();
        priv_.supports_password.replace(has_password);

        if has_password {
            priv_.method_page.update_sso(sso);
            self.show_login_methods();
        } else {
            self.login_with_sso(None).await;
        }
    }

    fn show_login_methods(&self) {
        let priv_ = self.imp();

        let domain_name = if self.autodiscovery() {
            priv_.homeserver_page.server_name().unwrap().to_string()
        } else {
            self.homeserver_pretty().unwrap()
        };
        priv_.method_page.set_domain_name(&domain_name);

        self.set_visible_child("method");
    }

    async fn login_with_password(&self) {
        let priv_ = self.imp();
        let username = priv_.method_page.username();
        let password = priv_.method_page.password();
        let CreatedClient {
            client,
            path,
            passphrase,
        } = self.created_client().unwrap();

        let handle = spawn_tokio!(async move {
            client
                .login_username(&username, &password)
                .initial_device_display_name("Fractal")
                .send()
                .await
        });

        match handle.await.unwrap() {
            Ok(response) => {
                let session_info = StoredSession {
                    homeserver: self.homeserver().unwrap(),
                    user_id: response.user_id,
                    device_id: response.device_id,
                    path,
                    secret: Secret {
                        access_token: response.access_token,
                        passphrase,
                    },
                };
                self.create_session(session_info, true).await;
            }
            Err(error) => {
                warn!("Failed to log in: {error}");
                toast!(self, error.to_user_facing());
            }
        }
    }

    async fn login_with_sso(&self, idp_id: Option<String>) {
        let CreatedClient {
            client,
            path,
            passphrase,
        } = self.created_client().unwrap();

        self.set_visible_child("sso");

        let handle = spawn_tokio!(async move {
            let mut login = client
                .login_sso(|sso_url| async move {
                    let ctx = glib::MainContext::default();
                    ctx.spawn(async move {
                        gtk::show_uri(gtk::Window::NONE, &sso_url, gdk::CURRENT_TIME);
                    });
                    Ok(())
                })
                .initial_device_display_name("Fractal");

            if let Some(idp_id) = idp_id.as_deref() {
                login = login.identity_provider_id(idp_id);
            }

            login.send().await
        });

        match handle.await.unwrap() {
            Ok(response) => {
                let session_info = StoredSession {
                    homeserver: self.homeserver().unwrap(),
                    user_id: response.user_id,
                    device_id: response.device_id,
                    path,
                    secret: Secret {
                        access_token: response.access_token,
                        passphrase,
                    },
                };
                self.create_session(session_info, true).await;
            }
            Err(error) => {
                warn!("Failed to log in: {error}");
                toast!(self, error.to_user_facing());
                self.go_previous();
            }
        }
    }

    pub async fn restore_previous_session(&self, session: StoredSession) {
        let handle = spawn_tokio!(async move {
            let created_client = CreatedClient::new(
                &HomeserverOrServerName::Homeserver(session.homeserver.clone()),
                false,
                Some(session.path.clone()),
                Some(session.secret.passphrase.clone()),
            )
            .await?;

            created_client
                .client
                .restore_login(matrix_sdk::Session {
                    user_id: session.user_id.clone(),
                    device_id: session.device_id.clone(),
                    access_token: session.secret.access_token.clone(),
                    refresh_token: None,
                })
                .await
                .map(|_| (created_client, session))
                .map_err(ClientSetupError::from)
        });

        match handle.await.unwrap() {
            Ok((created_client, session_info)) => {
                self.set_created_client(Some(created_client)).await;
                self.create_session(session_info, false).await;
            }
            Err(error) => {
                warn!("Failed to restore previous login: {error}");
                toast!(self, error.to_user_facing());
            }
        }
    }

    pub async fn create_session(&self, session_info: StoredSession, is_new: bool) {
        let client = self.client().unwrap();
        let session = Session::new();

        if is_new {
            if let Err(error) = secret::store_session(&session_info).await {
                error!("Couldn't store session: {:?}", error);

                self.parent_window().switch_to_error_page(
                    &format!("{}\n\n{}", gettext("Unable to store session"), error),
                    error,
                );
                return;
            }
        };

        session.prepare(client, session_info).await;
        self.emit_by_name::<()>("new-session", &[&session]);
    }

    pub fn clean(&self) {
        let priv_ = self.imp();

        // Clean pages.
        priv_.homeserver_page.clean();
        priv_.method_page.clean();

        // Clean data.
        self.prune_created_client(false);
        self.set_autodiscovery(true);

        // Reinitialize UI.
        priv_.main_stack.set_visible_child_name("homeserver");
        self.unfreeze();
    }

    fn freeze(&self) {
        let priv_ = self.imp();

        self.action_set_enabled("login.next", false);
        priv_.next_button.set_loading(true);
        priv_.main_stack.set_sensitive(false);
    }

    fn unfreeze(&self) {
        let priv_ = self.imp();

        priv_.next_button.set_loading(false);
        priv_.main_stack.set_sensitive(true);
        self.update_next_state();
    }

    pub fn connect_new_session<F: Fn(&Self, Session) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("new-session", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            let session = values[1].get::<Session>().unwrap();

            f(&obj, session);

            None
        })
    }

    pub fn default_widget(&self) -> gtk::Widget {
        self.imp().next_button.get().upcast()
    }

    /// Set focus to the proper widget of the current page.
    pub fn focus_default(&self) {
        let priv_ = self.imp();
        match self.visible_child().as_ref() {
            "homeserver" => {
                priv_.homeserver_page.focus_default();
            }
            "method" => {
                priv_.method_page.focus_default();
            }
            _ => {}
        }
    }

    fn update_network_state(&self) {
        let priv_ = self.imp();
        let monitor = gio::NetworkMonitor::default();

        if !monitor.is_network_available() {
            priv_
                .offline_info_bar_label
                .set_label(&gettext("No network connection"));
            priv_.offline_info_bar.set_revealed(true);
            self.action_set_enabled("login.sso", false);
        } else if monitor.connectivity() < gio::NetworkConnectivity::Full {
            priv_
                .offline_info_bar_label
                .set_label(&gettext("No Internet connection"));
            priv_.offline_info_bar.set_revealed(true);
            self.action_set_enabled("login.sso", true);
        } else {
            priv_.offline_info_bar.set_revealed(false);
            self.action_set_enabled("login.sso", true);
        }

        self.update_next_state();
    }
}

impl Default for Login {
    fn default() -> Self {
        Self::new()
    }
}

/// A homeserver URL or a server name.
pub enum HomeserverOrServerName {
    /// A homeserver URL.
    Homeserver(Url),

    /// A server name.
    ServerName(OwnedServerName),
}

/// All errors that can occur when setting up the Matrix client.
#[derive(Error, Debug)]
pub enum ClientSetupError {
    #[error(transparent)]
    Store(#[from] OpenStoreError),
    #[error(transparent)]
    Client(#[from] ClientBuildError),
    #[error(transparent)]
    Sdk(#[from] matrix_sdk::Error),
}

impl UserFacingError for ClientSetupError {
    fn to_user_facing(self) -> String {
        match self {
            ClientSetupError::Store(err) => err.to_user_facing(),
            ClientSetupError::Client(err) => err.to_user_facing(),
            ClientSetupError::Sdk(err) => err.to_user_facing(),
        }
    }
}

/// A newly created Matrix client and its configuration.
#[derive(Debug, Clone)]
pub struct CreatedClient {
    /// The Matrix client.
    pub client: Client,

    /// The path where the store is located.
    pub path: PathBuf,

    /// The passphrase to decrypt the store.
    pub passphrase: String,
}

impl CreatedClient {
    /// Create a Matrix `Client` for the given homeserver.
    async fn new(
        homeserver: &HomeserverOrServerName,
        use_discovery: bool,
        path: Option<PathBuf>,
        passphrase: Option<String>,
    ) -> Result<CreatedClient, ClientSetupError> {
        let path = path.unwrap_or_else(|| {
            let mut path = glib::user_data_dir();
            path.push(glib::uuid_string_random().as_str());
            path
        });

        let passphrase = passphrase.unwrap_or_else(|| {
            thread_rng()
                .sample_iter(Alphanumeric)
                .take(30)
                .map(char::from)
                .collect()
        });

        let state_store = SledStateStore::builder()
            .path(path.clone())
            .passphrase(passphrase.clone())
            .migration_conflict_strategy(MigrationConflictStrategy::Drop)
            .build()
            .map_err(|err| OpenStoreError::from(StoreError::backend(err)))?;
        let crypto_store = state_store.open_crypto_store()?;
        let store_config = StoreConfig::new()
            .state_store(state_store)
            .crypto_store(crypto_store);

        let builder = match homeserver {
            HomeserverOrServerName::Homeserver(url) => Client::builder().homeserver_url(url),
            HomeserverOrServerName::ServerName(server_name) => {
                Client::builder().server_name(server_name)
            }
        };

        let client = builder
            .store_config(store_config)
            // force_auth option to solve an issue with some servers configuration to require
            // auth for profiles:
            // https://gitlab.gnome.org/GNOME/fractal/-/issues/934
            .request_config(RequestConfig::new().retry_limit(2).force_auth())
            .respect_login_well_known(use_discovery)
            .build()
            .await?;

        Ok(Self {
            client,
            path,
            passphrase,
        })
    }
}

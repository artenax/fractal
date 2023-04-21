use std::{ffi::OsStr, fs, path::PathBuf};

use adw::{prelude::*, subclass::prelude::BinImpl};
use gettextrs::gettext;
use gtk::{self, gio, glib, glib::clone, subclass::prelude::*, CompositeTemplate};
use log::{error, warn};
use matrix_sdk::{
    config::RequestConfig, ruma::api::client::session::get_login_types::v3::LoginType, Client,
    ClientBuildError,
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
    Application, Session, Window,
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{subclass::InitializingObject, SignalHandlerId};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/login.ui")]
    pub struct Login {
        /// The current created Matrix client and its configuration.
        pub created_client: RefCell<Option<CreatedClient>>,
        /// The ID of the session that is currently logging in.
        pub current_session_id: RefCell<Option<String>>,
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
        pub offline_banner: TemplateChild<adw::Banner>,
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
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::builder("homeserver")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("autodiscovery")
                        .default_value(true)
                        .construct()
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "homeserver" => obj.homeserver_pretty().to_value(),
                "autodiscovery" => obj.autodiscovery().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "autodiscovery" => self.obj().set_autodiscovery(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            let obj = self.obj();
            obj.action_set_enabled("login.next", false);

            self.parent_constructed();

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

        fn dispose(&self) {
            self.obj().prune_created_client();
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
        glib::Object::new()
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

    pub fn prune_created_client(&self) {
        if let Some(created_client) = self.imp().created_client.take() {
            if let Err(error) = fs::remove_dir_all(created_client.path) {
                error!("Failed to remove newly-created database: {error}");
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

    /// The ID of the session that is currently logging in.
    ///
    /// This will be set until the session is marked as `ready`.
    pub fn current_session_id(&self) -> Option<String> {
        self.imp().current_session_id.borrow().clone()
    }

    /// Set the ID of the session that is currently logging in.
    pub fn set_current_session_id(&self, session_id: Option<String>) {
        self.imp().current_session_id.replace(session_id);
    }

    /// The homeserver to log into.
    pub fn homeserver(&self) -> Option<Url> {
        self.imp().homeserver.borrow().clone()
    }

    /// The pretty-formatted homeserver to log into.
    pub fn homeserver_pretty(&self) -> Option<String> {
        let homeserver = self.homeserver();
        homeserver
            .as_ref()
            .and_then(|url| url.as_ref().strip_suffix('/').map(ToOwned::to_owned))
            .or_else(|| homeserver.as_ref().map(ToString::to_string))
    }

    /// Set the homeserver to log into.
    pub fn set_homeserver(&self, homeserver: Option<Url>) {
        let imp = self.imp();

        if self.homeserver() == homeserver {
            return;
        }

        imp.homeserver.replace(homeserver);
        self.notify("homeserver");
    }

    /// Whether auto-discovery is enabled.
    pub fn autodiscovery(&self) -> bool {
        self.imp().autodiscovery.get()
    }

    /// Set whether auto-discovery is enabled
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
            self.prune_created_client();
        }

        self.imp().main_stack.set_visible_child_name(visible_child);
    }

    fn update_next_state(&self) {
        let imp = self.imp();
        match self.visible_child().as_ref() {
            "homeserver" => {
                self.enable_next_action(imp.homeserver_page.can_go_next());
                imp.next_button.set_visible(true);
            }
            "method" => {
                self.enable_next_action(imp.method_page.can_go_next());
                imp.next_button.set_visible(true);
            }
            _ => {
                imp.next_button.set_visible(false);
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
                self.parent_window().switch_to_greeter_page();
                self.clean();
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
                let session_id = created_client
                    .path
                    .iter()
                    .next_back()
                    .and_then(OsStr::to_str)
                    .unwrap();
                self.set_current_session_id(Some(session_id.to_owned()));

                self.set_created_client(Some(created_client)).await;
                self.check_login_types().await;
            }
            Err(error) => {
                if autodiscovery {
                    warn!("Failed to discover homeserver: {error}");
                } else {
                    warn!("Failed to check homeserver: {error}");
                }
                toast!(self, error.to_user_facing());

                // Clean up the created client because it's bound to the homeserver.
                self.prune_created_client();
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

        let imp = self.imp();
        imp.supports_password.replace(has_password);

        if has_password {
            imp.method_page.update_sso(sso);
            self.show_login_methods();
        } else {
            self.login_with_sso(None).await;
        }
    }

    fn show_login_methods(&self) {
        let imp = self.imp();

        let domain_name = if self.autodiscovery() {
            imp.homeserver_page.server_name().unwrap().to_string()
        } else {
            self.homeserver_pretty().unwrap()
        };
        imp.method_page.set_domain_name(&domain_name);

        self.set_visible_child("method");
    }

    async fn login_with_password(&self) {
        let imp = self.imp();
        let username = imp.method_page.username();
        let password = imp.method_page.password();
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
                        spawn!(async move {
                            if let Err(error) = gtk::UriLauncher::new(&sso_url)
                                .launch_future(gtk::Window::NONE)
                                .await
                            {
                                error!("Could not launch URI: {error}");
                            }
                        });
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

    /// Restore a matrix client with the current settings.
    ///
    /// This is necessary when going back from a cancelled verification after a
    /// successful login, because we can't reuse a logged-out Matrix client.
    pub fn restore_client(&self) {
        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                obj.get_homeserver().await
            })
        );
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
                .restore_session(matrix_sdk::Session {
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
        let client = self.imp().created_client.take().unwrap().client;
        let session = Session::new();

        if is_new {
            // Save ID of logging in session to GSettings
            let settings = Application::default().settings();
            if let Err(err) = settings.set_string(
                "current-session",
                self.current_session_id().unwrap_or_default().as_str(),
            ) {
                warn!("Failed to save current session: {err}");
            }

            let session_info = session_info.clone();
            let handle = spawn_tokio!(async move { secret::store_session(&session_info).await });

            if let Err(error) = handle.await.unwrap() {
                error!("Couldn't store session: {error}");

                let (message, item) = error.into_parts();
                self.parent_window().switch_to_error_page(
                    &format!("{}\n\n{}", gettext("Unable to store session"), message),
                    item,
                );
                return;
            }

            // Clean the `Login` when the session is ready because we won't need
            // to restore it anymore.
            session.connect_ready(clone!(@weak self as obj => move |_| {
                obj.clean();
            }));
        }

        session.prepare(client, session_info).await;
        self.parent_window().add_session(&session);
    }

    pub fn clean(&self) {
        let imp = self.imp();

        // Clean pages.
        imp.homeserver_page.clean();
        imp.method_page.clean();

        // Clean data.
        self.set_current_session_id(None);
        self.prune_created_client();
        self.set_autodiscovery(true);

        // Reinitialize UI.
        imp.main_stack.set_visible_child_name("homeserver");
        self.unfreeze();
    }

    fn freeze(&self) {
        let imp = self.imp();

        self.action_set_enabled("login.next", false);
        imp.next_button.set_loading(true);
        imp.main_stack.set_sensitive(false);
    }

    fn unfreeze(&self) {
        let imp = self.imp();

        imp.next_button.set_loading(false);
        imp.main_stack.set_sensitive(true);
        self.update_next_state();
    }

    pub fn default_widget(&self) -> gtk::Widget {
        self.imp().next_button.get().upcast()
    }

    /// Set focus to the proper widget of the current page.
    pub fn focus_default(&self) {
        let imp = self.imp();
        match self.visible_child().as_ref() {
            "homeserver" => {
                imp.homeserver_page.focus_default();
            }
            "method" => {
                imp.method_page.focus_default();
            }
            _ => {}
        }
    }

    fn update_network_state(&self) {
        let imp = self.imp();
        let monitor = gio::NetworkMonitor::default();

        if !monitor.is_network_available() {
            imp.offline_banner
                .set_title(&gettext("No network connection"));
            imp.offline_banner.set_revealed(true);
            self.action_set_enabled("login.sso", false);
        } else if monitor.connectivity() < gio::NetworkConnectivity::Full {
            imp.offline_banner
                .set_title(&gettext("No Internet connection"));
            imp.offline_banner.set_revealed(true);
            self.action_set_enabled("login.sso", true);
        } else {
            imp.offline_banner.set_revealed(false);
            self.action_set_enabled("login.sso", true);
        }

        self.update_next_state();
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
    Client(#[from] ClientBuildError),
    #[error(transparent)]
    Sdk(#[from] matrix_sdk::Error),
}

impl UserFacingError for ClientSetupError {
    fn to_user_facing(self) -> String {
        match self {
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
    ///
    /// Returns the `CreatedClient` and its ID.
    async fn new(
        homeserver: &HomeserverOrServerName,
        use_discovery: bool,
        path: Option<PathBuf>,
        passphrase: Option<String>,
    ) -> Result<CreatedClient, ClientBuildError> {
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

        let builder = match homeserver {
            HomeserverOrServerName::Homeserver(url) => Client::builder().homeserver_url(url),
            HomeserverOrServerName::ServerName(server_name) => {
                Client::builder().server_name(server_name)
            }
        };

        let client = builder
            .sqlite_store(&path, Some(&passphrase))
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

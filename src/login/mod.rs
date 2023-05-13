use adw::{prelude::*, subclass::prelude::BinImpl};
use gettextrs::gettext;
use gtk::{self, gio, glib, glib::clone, subclass::prelude::*, CompositeTemplate};
use log::{error, warn};
use matrix_sdk::Client;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use ruma::{
    api::client::session::{get_login_types::v3::LoginType, login},
    OwnedServerName,
};
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
    secret::{self, StoredSession},
    spawn, spawn_tokio, toast,
    user_facing_error::UserFacingError,
    utils::matrix,
    Application, Session, Window, RUNTIME,
};

#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "BoxedLoginTypes")]
pub struct BoxedLoginTypes(Vec<LoginType>);

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{subclass::InitializingObject, SignalHandlerId};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/login.ui")]
    pub struct Login {
        /// The Matrix client.
        pub client: RefCell<Option<Client>>,
        /// The ID of the session that is currently logging in.
        pub current_session_id: RefCell<Option<String>>,
        #[template_child]
        pub back_button: TemplateChild<gtk::Button>,
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
        /// The login types supported by the homeserver.
        pub login_types: RefCell<Vec<LoginType>>,
        /// The domain of the homeserver to log into.
        pub domain: RefCell<Option<OwnedServerName>>,
        /// The URL of the homeserver to log into.
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
                    glib::ParamSpecString::builder("domain").read_only().build(),
                    glib::ParamSpecString::builder("homeserver")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("autodiscovery")
                        .default_value(true)
                        .construct()
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoxed::builder::<BoxedLoginTypes>("login-types")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "domain" => obj.domain().to_value(),
                "homeserver" => obj.homeserver_pretty().to_value(),
                "autodiscovery" => obj.autodiscovery().to_value(),
                "login-types" => BoxedLoginTypes(obj.login_types()).to_value(),
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
                    obj.focus_default();
                ));

            obj.update_network_state();
        }

        fn dispose(&self) {
            self.obj().prune_client();
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

    /// The Matrix client.
    pub fn client(&self) -> Option<Client> {
        self.imp().client.borrow().clone()
    }

    pub fn prune_client(&self) {
        if let Some(client) = self.imp().client.take() {
            // The `Client` needs to access a tokio runtime when it is dropped.
            let guard = RUNTIME.enter();
            RUNTIME.block_on(async move {
                drop(client);
                drop(guard);
            });
        }
    }

    async fn set_client(&self, client: Option<Client>) {
        let homeserver = if let Some(client) = client.clone() {
            Some(
                spawn_tokio!(async move { client.homeserver().await })
                    .await
                    .unwrap(),
            )
        } else {
            None
        };

        self.set_homeserver(homeserver);
        self.imp().client.replace(client);
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

    /// The domain of the homeserver to log into.
    ///
    /// If autodiscovery is enabled, this is the server name, otherwise, this is
    /// the prettified homeserver URL.
    pub fn domain(&self) -> Option<String> {
        if self.autodiscovery() {
            self.imp().domain.borrow().clone().map(Into::into)
        } else {
            self.homeserver_pretty()
        }
    }

    fn set_domain(&self, domain: Option<OwnedServerName>) {
        let imp = self.imp();

        if imp.domain.borrow().as_ref() == domain.as_ref() {
            return;
        }

        imp.domain.replace(domain);
        self.notify("domain");
    }

    /// The URL of the homeserver to log into.
    pub fn homeserver(&self) -> Option<Url> {
        self.imp().homeserver.borrow().clone()
    }

    /// The pretty-formatted URL of the homeserver to log into.
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
        if !self.autodiscovery() {
            self.notify("domain");
        }
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
    }

    /// The login types supported by the homeserver.
    pub fn login_types(&self) -> Vec<LoginType> {
        self.imp().login_types.borrow().clone()
    }

    /// Set the login types supported by the homeserver.
    fn set_login_types(&self, types: Vec<LoginType>) {
        self.imp().login_types.replace(types);
        self.notify("login-types");
    }

    /// Whether the password login type is supported.
    pub fn supports_password(&self) -> bool {
        self.imp()
            .login_types
            .borrow()
            .iter()
            .any(|t| matches!(t, LoginType::Password(_)))
    }

    fn visible_child(&self) -> String {
        self.imp().main_stack.visible_child_name().unwrap().into()
    }

    fn set_visible_child(&self, visible_child: &str) {
        // Clean up the created client when we come back to the homeserver selection.
        if visible_child == "homeserver" {
            self.prune_client();
        }

        self.imp().main_stack.set_visible_child_name(visible_child);
    }

    fn go_previous(&self) {
        match self.visible_child().as_ref() {
            "method" => {
                self.set_visible_child("homeserver");
                self.imp().method_page.clean();
            }
            "sso" => {
                self.set_visible_child(if self.supports_password() {
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

    /// Show the appropriate login screen given the current login types.
    fn show_login_screen(&self) {
        if self.supports_password() {
            self.set_visible_child("method");
        } else {
            spawn!(clone!(@weak self as obj => async move {
                obj.login_with_sso(None).await;
            }));
        }
    }

    /// Log in with the SSO login type.
    async fn login_with_sso(&self, idp_id: Option<String>) {
        self.set_visible_child("sso");
        let client = self.client().unwrap();

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
                                // FIXME: We should forward the error.
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
                self.handle_login_response(response).await;
            }
            Err(error) => {
                warn!("Failed to log in: {error}");
                toast!(self, error.to_user_facing());
                self.go_previous();
            }
        }
    }

    /// Handle the given response after successfully logging in.
    async fn handle_login_response(&self, response: login::v3::Response) {
        let client = self.client().unwrap();
        // The homeserver could have changed with the login response so get it from the
        // Client.
        let homeserver = spawn_tokio!(async move { client.homeserver().await })
            .await
            .unwrap();

        let mut path = glib::user_data_dir();
        path.push(glib::uuid_string_random().as_str());

        let passphrase = thread_rng()
            .sample_iter(Alphanumeric)
            .take(30)
            .map(char::from)
            .collect();

        let session_info = StoredSession::from_parts(homeserver, path, passphrase, response.into());

        let session_info_clone = session_info.clone();
        let handle =
            spawn_tokio!(
                async move { matrix::client_with_stored_session(session_info_clone).await }
            );

        match handle.await.unwrap() {
            Ok(client) => {
                self.create_session(client, session_info, true).await;
            }
            Err(error) => {
                warn!("Failed to create new client: {error}");
                toast!(self, error.to_user_facing());
            }
        }
    }

    /// Restore a matrix client with the current settings.
    ///
    /// This is necessary when going back from a cancelled verification after a
    /// successful login, because we can't reuse a logged-out Matrix client.
    pub fn restore_client(&self) {
        self.imp().homeserver_page.fetch_homeserver_details();
    }

    pub async fn restore_previous_session(&self, session_info: StoredSession) {
        let session_info_clone = session_info.clone();
        let handle =
            spawn_tokio!(
                async move { matrix::client_with_stored_session(session_info_clone).await }
            );

        match handle.await.unwrap() {
            Ok(client) => {
                self.create_session(client, session_info, false).await;
            }
            Err(error) => {
                warn!("Failed to restore previous login: {error}");
                toast!(self, error.to_user_facing());
            }
        }
    }

    pub async fn create_session(&self, client: Client, session_info: StoredSession, is_new: bool) {
        self.prune_client();
        let session = Session::new();

        if is_new {
            // Save ID of logging in session to GSettings
            let settings = Application::default().settings();
            if let Err(err) = settings.set_string("current-session", session_info.id()) {
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
        self.prune_client();
        self.set_autodiscovery(true);
        self.set_login_types(vec![]);
        self.set_domain(None);
        self.set_homeserver(None);

        // Reinitialize UI.
        imp.main_stack.set_visible_child_name("homeserver");
        self.unfreeze();
    }

    /// Freeze the login screen.
    fn freeze(&self) {
        self.imp().main_stack.set_sensitive(false);
    }

    /// Unfreeze the login screen.
    fn unfreeze(&self) {
        self.imp().main_stack.set_sensitive(true);
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
    }
}

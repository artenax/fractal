use adw::{prelude::*, subclass::prelude::BinImpl};
use gettextrs::gettext;
use gtk::{self, gio, glib, glib::clone, subclass::prelude::*, CompositeTemplate};
use log::{error, warn};
use matrix_sdk::Client;
use ruma::{
    api::client::session::{get_login_types::v3::LoginType, login},
    OwnedServerName,
};
use strum::{AsRefStr, EnumString};
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
    prelude::*,
    session::{model::Session, view::SessionVerification},
    spawn, spawn_tokio, toast, Application, Window, RUNTIME,
};

#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "BoxedLoginTypes")]
pub struct BoxedLoginTypes(Vec<LoginType>);

/// A page of the login stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
enum LoginPage {
    /// The homeserver page.
    Homeserver,
    /// The page to select a login method.
    Method,
    /// The page to wait for SSO to be finished.
    Sso,
    /// The loading page.
    Loading,
    /// The session verification stack.
    SessionVerification,
    /// The login is completed.
    Completed,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{subclass::InitializingObject, SignalHandlerId};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/login/mod.ui")]
    pub struct Login {
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
        #[template_child]
        pub done_button: TemplateChild<gtk::Button>,
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
        /// The Matrix client used to log in.
        pub client: RefCell<Option<Client>>,
        /// The session that was just logged in.
        pub session: RefCell<Option<Session>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Login {
        const NAME: &'static str = "Login";
        type Type = super::Login;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);

            klass.set_css_name("login");
            klass.set_accessible_role(gtk::AccessibleRole::Group);

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
            let obj = self.obj();

            obj.drop_client();
            obj.drop_session();
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

#[gtk::template_callbacks]
impl Login {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn parent_window(&self) -> Window {
        self.root()
            .and_downcast()
            .expect("Login needs to have a parent window")
    }

    /// The Matrix client.
    pub async fn client(&self) -> Option<Client> {
        if let Some(client) = self.imp().client.borrow().clone() {
            return Some(client);
        }

        // If the client was dropped, try to recreate it.
        self.imp().homeserver_page.check_homeserver().await;
        if let Some(client) = self.imp().client.borrow().clone() {
            return Some(client);
        }

        None
    }

    /// Set the Matrix client.
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

    /// Drop the Matrix client.
    pub fn drop_client(&self) {
        if let Some(client) = self.imp().client.take() {
            // The `Client` needs to access a tokio runtime when it is dropped.
            let _guard = RUNTIME.enter();
            drop(client);
        }
    }

    /// Drop the session and clean up its data from the system.
    fn drop_session(&self) {
        if let Some(session) = self.imp().session.take() {
            glib::MainContext::default().block_on(async move {
                let _ = session.logout().await;
            });
        }
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

    /// The visible page of the login stack.
    fn visible_child(&self) -> LoginPage {
        self.imp()
            .main_stack
            .visible_child_name()
            .and_then(|s| s.as_str().try_into().ok())
            .unwrap()
    }

    /// Set the visible page of the login stack.
    fn set_visible_child(&self, visible_child: LoginPage) {
        self.imp()
            .main_stack
            .set_visible_child_name(visible_child.as_ref());
    }

    /// The page to go back to for the current login stack page.
    fn previous_page(&self) -> Option<LoginPage> {
        match self.visible_child() {
            LoginPage::Homeserver => None,
            LoginPage::Method => Some(LoginPage::Homeserver),
            LoginPage::Sso | LoginPage::Loading | LoginPage::SessionVerification => {
                if self.supports_password() {
                    Some(LoginPage::Method)
                } else {
                    Some(LoginPage::Homeserver)
                }
            }
            // The go-back button should be deactivated.
            LoginPage::Completed => None,
        }
    }

    /// Go back to the previous step.
    #[template_callback]
    fn go_previous(&self) {
        let session_verification = self.session_verification();
        if let Some(session_verification) = &session_verification {
            if session_verification.go_previous() {
                // The session verification handled the action.
                return;
            }
        }

        let Some(previous_page) = self.previous_page() else {
            self.parent_window().switch_to_greeter_page();
            self.clean();
            return;
        };

        self.set_visible_child(previous_page);

        match previous_page {
            LoginPage::Homeserver => {
                // Drop the client because it is bound to the homeserver.
                self.drop_client();
                // Drop the session because it is bound to the homeserver and account.
                self.drop_session();
                self.imp().method_page.clean();
            }
            LoginPage::Method => {
                // Drop the session because it is bound to the account.
                self.drop_session();
            }
            _ => {}
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
            self.set_visible_child(LoginPage::Method);
        } else {
            spawn!(clone!(@weak self as obj => async move {
                obj.login_with_sso(None).await;
            }));
        }
    }

    /// Log in with the SSO login type.
    async fn login_with_sso(&self, idp_id: Option<String>) {
        self.set_visible_child(LoginPage::Sso);
        let client = self.client().await.unwrap();

        let handle = spawn_tokio!(async move {
            let mut login = client
                .matrix_auth()
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
        let client = self.client().await.unwrap();
        // The homeserver could have changed with the login response so get it from the
        // Client.
        let homeserver = spawn_tokio!(async move { client.homeserver().await })
            .await
            .unwrap();

        match Session::new(homeserver, (&response).into()).await {
            Ok(session) => {
                self.init_session(session).await;
            }
            Err(error) => {
                warn!("Failed to create session: {error}");
                toast!(self, error.to_user_facing());

                self.go_previous();
            }
        }
    }

    pub async fn init_session(&self, session: Session) {
        self.set_visible_child(LoginPage::Loading);
        self.drop_client();
        self.imp().session.replace(Some(session.clone()));

        // Save ID of logging in session to GSettings
        let settings = Application::default().settings();
        if let Err(err) = settings.set_string("current-session", session.session_id()) {
            warn!("Failed to save current session: {err}");
        }

        let session_info = session.info().clone();
        let handle = spawn_tokio!(async move { session_info.store().await });

        if let Err(error) = handle.await.unwrap() {
            error!("Couldn't store session: {error}");

            let (message, item) = error.into_parts();
            self.parent_window().switch_to_error_page(
                &format!("{}\n\n{}", gettext("Unable to store session"), message),
                item,
            );
            return;
        }

        session.connect_ready(clone!(@weak self as obj => move |_| {
            spawn!(clone!(@weak obj => async move {
                obj.check_verification().await;
            }));
        }));
        session.prepare().await;
    }

    /// Check whether the logged in session needs to be verified.
    async fn check_verification(&self) {
        let imp = self.imp();
        let session = imp.session.borrow().clone().unwrap();

        if session.is_verified().await {
            self.finish_login();
            return;
        }

        let stack = &imp.main_stack;
        let widget = SessionVerification::new(self, &session);
        stack.add_named(&widget, Some(LoginPage::SessionVerification.as_ref()));
        stack.set_visible_child(&widget);
    }

    /// Get the session verification, if any.
    fn session_verification(&self) -> Option<SessionVerification> {
        self.imp()
            .main_stack
            .child_by_name(LoginPage::SessionVerification.as_ref())
            .and_downcast()
    }

    /// Show the completed page.
    #[template_callback]
    pub fn show_completed(&self) {
        let imp = self.imp();

        imp.back_button.set_visible(false);
        self.set_visible_child(LoginPage::Completed);
        imp.done_button.grab_focus();
    }

    /// Finish the login process and show the session.
    #[template_callback]
    fn finish_login(&self) {
        let session = self.imp().session.take().unwrap();
        self.parent_window().add_session(&session);

        self.clean();
    }

    /// Reset the login stack.
    pub fn clean(&self) {
        let imp = self.imp();

        // Clean pages.
        imp.homeserver_page.clean();
        imp.method_page.clean();
        if let Some(session_verification) = self.session_verification() {
            imp.main_stack.remove(&session_verification);
        }

        // Clean data.
        self.set_autodiscovery(true);
        self.set_login_types(vec![]);
        self.set_domain(None);
        self.set_homeserver(None);
        self.drop_client();
        self.drop_session();

        // Reinitialize UI.
        self.set_visible_child(LoginPage::Homeserver);
        imp.back_button.set_visible(true);
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
        match self.visible_child() {
            LoginPage::Homeserver => {
                imp.homeserver_page.focus_default();
            }
            LoginPage::Method => {
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

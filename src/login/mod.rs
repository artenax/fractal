use adw::{prelude::*, subclass::prelude::BinImpl};
use gettextrs::gettext;
use gtk::{self, gio, glib, glib::clone, subclass::prelude::*, CompositeTemplate};
use log::{debug, warn};
use matrix_sdk::{
    config::RequestConfig, ruma::api::client::session::get_login_types::v3::LoginType, Client,
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
    components::SpinnerButton, spawn, spawn_tokio, toast, user_facing_error::UserFacingError,
    Session,
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
        pub current_session: RefCell<Option<Session>>,
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
                widget.login_with_sso(idp_id);
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
        let priv_ = imp::Login::from_instance(self);

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
        let priv_ = imp::Login::from_instance(self);
        priv_.main_stack.visible_child_name().unwrap().into()
    }

    fn set_visible_child(&self, visible_child: &str) {
        let priv_ = imp::Login::from_instance(self);
        priv_.main_stack.set_visible_child_name(visible_child);
    }

    fn update_next_state(&self) {
        let priv_ = imp::Login::from_instance(self);
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
        match self.visible_child().as_ref() {
            "homeserver" => {
                if self.autodiscovery() {
                    self.try_autodiscovery();
                } else {
                    self.check_homeserver();
                }
            }
            "method" => self.login_with_password(),
            _ => {}
        }
    }

    fn go_previous(&self) {
        match self.visible_child().as_ref() {
            "method" => self.set_visible_child("homeserver"),
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
        let dialog =
            LoginAdvancedDialog::new(self.root().unwrap().downcast_ref::<gtk::Window>().unwrap());
        self.bind_property("autodiscovery", &dialog, "autodiscovery")
            .flags(glib::BindingFlags::SYNC_CREATE | glib::BindingFlags::BIDIRECTIONAL)
            .build();
        dialog.run_future().await;
    }

    fn try_autodiscovery(&self) {
        let server = self.imp().homeserver_page.server_name().unwrap();

        self.freeze();

        let handle =
            spawn_tokio!(async move { Client::builder().server_name(&server).build().await });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(client) => {
                        let homeserver = client.homeserver().await;
                        obj.set_homeserver(Some(homeserver));
                        obj.check_login_types(client).await;
                    }
                    Err(error) => {
                        warn!("Failed to discover homeserver: {}", error);
                        toast!(obj, error.to_user_facing());
                    }
                };
                obj.unfreeze();
            })
        );
    }

    fn check_homeserver(&self) {
        let homeserver = self.imp().homeserver_page.homeserver_url().unwrap();
        let homeserver_clone = homeserver.clone();

        self.freeze();

        let handle = spawn_tokio!(async move {
            Client::builder()
                .homeserver_url(homeserver_clone)
                .request_config(RequestConfig::new().disable_retry())
                .build()
                .await
        });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(client) => {
                        obj.set_homeserver(Some(homeserver));
                        obj.check_login_types(client).await;
                    }
                    Err(error) => {
                        warn!("Failed to check homeserver: {}", error);
                        toast!(obj, error.to_user_facing());
                    }
                };
                obj.unfreeze();
            })
        );
    }

    async fn check_login_types(&self, client: Client) {
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

        self.imp().supports_password.replace(has_password);

        if has_password {
            self.imp().method_page.update_sso(sso);
            self.show_login_methods();
        } else {
            self.login_with_sso(None);
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

    fn login_with_password(&self) {
        let priv_ = self.imp();
        let homeserver = self.homeserver().unwrap();
        let username = priv_.method_page.username();
        let password = priv_.method_page.password();
        let autodiscovery = self.autodiscovery();

        self.freeze();

        let session = Session::new();
        self.set_handler_for_prepared_session(&session);

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak session => async move {
                session.login_with_password(homeserver, username, password, autodiscovery).await;
            })
        );
        priv_.current_session.replace(Some(session));
    }

    fn login_with_sso(&self, idp_id: Option<String>) {
        let priv_ = imp::Login::from_instance(self);
        let homeserver = self.homeserver().unwrap();
        self.set_visible_child("sso");

        let session = Session::new();
        self.set_handler_for_prepared_session(&session);
        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak session, @weak self as s => async move {
                session.login_with_sso(homeserver, idp_id).await;
                s.set_visible_child("homeserver");
            })
        );
        priv_.current_session.replace(Some(session));
    }

    pub fn clean(&self) {
        let priv_ = self.imp();
        priv_.homeserver_page.clean();
        priv_.method_page.clean();
        self.set_autodiscovery(true);
        priv_.homeserver.take();
        priv_.main_stack.set_visible_child_name("homeserver");
        self.unfreeze();
        self.drop_session_reference();
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

    fn drop_session_reference(&self) {
        let priv_ = self.imp();

        if let Some(session) = priv_.current_session.take() {
            if let Some(id) = priv_.prepared_source_id.take() {
                session.disconnect(id);
            }
            if let Some(id) = priv_.logged_out_source_id.take() {
                session.disconnect(id);
            }
            if let Some(id) = priv_.ready_source_id.take() {
                session.disconnect(id);
            }
        }
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

    fn set_handler_for_prepared_session(&self, session: &Session) {
        let priv_ = self.imp();
        priv_
            .prepared_source_id
            .replace(Some(session.connect_prepared(
                clone!(@weak self as login => move |session, error| {
                    match error {
                        Some(e) => {
                            toast!(login, e);
                            login.unfreeze();
                        },
                        None => {
                            debug!("A new session was prepared");
                            login.emit_by_name::<()>("new-session", &[&session]);
                        }
                    }
                }),
            )));

        priv_.ready_source_id.replace(Some(session.connect_ready(
            clone!(@weak self as login => move |_| {
                login.clean();
            }),
        )));

        priv_
            .logged_out_source_id
            .replace(Some(session.connect_logged_out(
                clone!(@weak self as login => move |_| {
                    login.parent_window().switch_to_login_page();
                    login.drop_session_reference();
                    login.unfreeze();
                }),
            )));
    }

    fn parent_window(&self) -> crate::Window {
        self.root()
            .and_then(|root| root.downcast().ok())
            .expect("Login needs to have a parent window")
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

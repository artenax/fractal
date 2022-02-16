use adw::{prelude::*, subclass::prelude::BinImpl};
use gettextrs::gettext;
use gtk::{self, glib, glib::clone, subclass::prelude::*, CompositeTemplate};
use log::{debug, warn};
use matrix_sdk::{
    config::RequestConfig,
    ruma::{
        api::client::unversioned::get_supported_versions, identifiers::Error as IdentifierError,
        ServerName, UserId,
    },
    Client, Result as MatrixResult,
};
use tokio::task::JoinHandle;
use url::{ParseError, Url};

use crate::{
    components::{SpinnerButton, Toast},
    login_advanced_dialog::LoginAdvancedDialog,
    spawn, spawn_tokio,
    user_facing_error::UserFacingError,
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
    #[template(resource = "/org/gnome/FractalNext/login.ui")]
    pub struct Login {
        pub current_session: RefCell<Option<Session>>,
        #[template_child]
        pub back_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub next_button: TemplateChild<SpinnerButton>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub homeserver_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub homeserver_help: TemplateChild<gtk::Label>,
        #[template_child]
        pub password_title: TemplateChild<gtk::Label>,
        #[template_child]
        pub username_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub password_entry: TemplateChild<gtk::PasswordEntry>,
        pub prepared_source_id: RefCell<Option<SignalHandlerId>>,
        pub logged_out_source_id: RefCell<Option<SignalHandlerId>>,
        pub ready_source_id: RefCell<Option<SignalHandlerId>>,
        /// Whether auto-discovery is enabled.
        pub autodiscovery: Cell<bool>,
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
            klass.install_action("login.next", None, move |widget, _, _| widget.forward());
            klass.install_action("login.prev", None, move |widget, _, _| widget.backward());
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
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT,
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

            self.main_stack
                .connect_visible_child_notify(clone!(@weak obj => move |_|
                    obj.update_next_action()
                ));
            obj.update_next_action();

            self.homeserver_entry
                .connect_changed(clone!(@weak obj => move |_| obj.update_next_action()));
            self.username_entry
                .connect_changed(clone!(@weak obj => move |_| obj.update_next_action()));
            self.password_entry
                .connect_changed(clone!(@weak obj => move |_| obj.update_next_action()));
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

    fn visible_child(&self) -> String {
        let priv_ = imp::Login::from_instance(self);
        priv_.main_stack.visible_child_name().unwrap().into()
    }

    fn set_visible_child(&self, visible_child: &str) {
        let priv_ = imp::Login::from_instance(self);
        priv_.main_stack.set_visible_child_name(visible_child);
    }

    fn update_next_action(&self) {
        let priv_ = imp::Login::from_instance(self);
        match self.visible_child().as_ref() {
            "homeserver" => {
                let homeserver = priv_.homeserver_entry.text();
                let enabled = if self.autodiscovery() {
                    build_server_name(homeserver.as_str()).is_ok()
                } else {
                    build_homeserver_url(homeserver.as_str()).is_ok()
                };
                self.action_set_enabled("login.next", enabled);
                priv_.next_button.set_visible(true);
            }
            "password" => {
                let username_length = priv_.username_entry.text_length();
                let password_length = priv_.password_entry.text().len();
                self.action_set_enabled("login.next", username_length != 0 && password_length != 0);
                priv_.next_button.set_visible(true);
            }
            _ => {
                priv_.next_button.set_visible(false);
            }
        }
    }

    fn forward(&self) {
        match self.visible_child().as_ref() {
            "homeserver" => {
                if self.autodiscovery() {
                    self.try_autodiscovery();
                } else {
                    self.check_homeserver();
                }
            }
            "password" => self.login_with_password(),
            _ => {}
        }
    }

    fn backward(&self) {
        match self.visible_child().as_ref() {
            "password" => self.set_visible_child("homeserver"),
            _ => {
                self.activate_action("app.show-greeter", None).unwrap();
            }
        }
    }

    pub fn autodiscovery(&self) -> bool {
        self.imp().autodiscovery.get()
    }

    fn set_autodiscovery(&self, autodiscovery: bool) {
        let priv_ = self.imp();

        priv_.autodiscovery.set(autodiscovery);
        if autodiscovery {
            priv_
                .homeserver_entry
                .set_placeholder_text(Some(&gettext("Domain Name…")));
            priv_.homeserver_help.set_markup(&gettext(
                "The domain of your Matrix homeserver, for example gnome.org",
            ));
        } else {
            priv_
                .homeserver_entry
                .set_placeholder_text(Some(&gettext("Homeserver URL…")));
            priv_.homeserver_help.set_markup(&gettext("The URL of your Matrix homeserver, for example <span segment=\"word\">https://gnome.modular.im</span>"));
        }
        self.update_next_action();
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
        let server = build_server_name(self.imp().homeserver_entry.text().as_str()).unwrap();
        let mxid = UserId::parse_with_server_name("user", &server).unwrap();

        self.freeze();

        let handle = spawn_tokio!(async move { Client::new_from_user_id(&mxid).await });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(client) => {
                        let homeserver = client.homeserver().await;
                        obj.set_homeserver(Some(homeserver));
                        obj.show_password_page();
                    }
                    Err(error) => {
                        warn!("Failed to discover homeserver: {}", error);
                        obj.parent_window().add_toast(&Toast::new(&error.to_user_facing()));
                    }
                };
                obj.unfreeze();
            })
        );
    }

    fn check_homeserver(&self) {
        let homeserver = build_homeserver_url(self.imp().homeserver_entry.text().as_str()).unwrap();
        let homeserver_clone = homeserver.clone();

        self.freeze();

        let handle: JoinHandle<MatrixResult<_>> = spawn_tokio!(async move {
            let client = Client::new(homeserver_clone).await?;
            Ok(client
                .send(
                    get_supported_versions::Request::new(),
                    Some(RequestConfig::new().disable_retry()),
                )
                .await?)
        });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(_) => {
                        obj.set_homeserver(Some(homeserver));
                        obj.show_password_page();
                    }
                    Err(error) => {
                        warn!("Failed to check homeserver: {}", error);
                        obj.parent_window().add_toast(&Toast::new(&error.to_user_facing()));
                    }
                };
                obj.unfreeze();
            })
        );
    }

    fn show_password_page(&self) {
        let priv_ = self.imp();
        if self.autodiscovery() {
            // Translators: the variable is a domain name, eg. gnome.org.
            priv_.password_title.set_markup(&gettext!(
                "Connecting to {}",
                format!(
                    "<span segment=\"word\">{}</span>",
                    priv_.homeserver_entry.text()
                )
            ));
        } else {
            priv_.password_title.set_markup(&gettext!(
                "Connecting to {}",
                format!(
                    "<span segment=\"word\">{}</span>",
                    self.homeserver_pretty().unwrap()
                )
            ));
        }
        self.set_visible_child("password");
    }

    fn login_with_password(&self) {
        let priv_ = self.imp();
        let homeserver = self.homeserver().unwrap();
        let username = priv_.username_entry.text().to_string();
        let password = priv_.password_entry.text().to_string();

        self.freeze();

        let session = Session::new();
        self.set_handler_for_prepared_session(&session);

        session.login_with_password(homeserver, username, password, self.autodiscovery());
        priv_.current_session.replace(Some(session));
    }

    pub fn clean(&self) {
        let priv_ = self.imp();
        priv_.homeserver_entry.set_text("");
        priv_.username_entry.set_text("");
        priv_.password_entry.set_text("");
        priv_.autodiscovery.set(true);
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
        self.update_next_action();
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

    fn set_handler_for_prepared_session(&self, session: &Session) {
        let priv_ = self.imp();
        priv_
            .prepared_source_id
            .replace(Some(session.connect_prepared(
                clone!(@weak self as login => move |session, error| {
                    match error {
                        Some(e) => {
                            login.parent_window().add_toast(&e);
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
}

impl Default for Login {
    fn default() -> Self {
        Self::new()
    }
}

fn build_server_name(server: &str) -> Result<Box<ServerName>, IdentifierError> {
    let server = server
        .strip_prefix("http://")
        .or_else(|| server.strip_prefix("https://"))
        .unwrap_or(server);
    ServerName::parse(server)
}

fn build_homeserver_url(server: &str) -> Result<Url, ParseError> {
    if server.starts_with("http://") || server.starts_with("https://") {
        Url::parse(server)
    } else {
        Url::parse(&format!("https://{}", server))
    }
}

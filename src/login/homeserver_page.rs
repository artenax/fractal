use adw::{prelude::*, subclass::prelude::BinImpl};
use gettextrs::gettext;
use gtk::{self, glib, glib::clone, subclass::prelude::*, CompositeTemplate};
use matrix_sdk::{
    config::RequestConfig, sanitize_server_name, Client, ClientBuildError, ClientBuilder,
};
use ruma::{api::client::discovery::get_supported_versions, OwnedServerName};
use tracing::warn;
use url::{ParseError, Url};

use super::Login;
use crate::{
    components::SpinnerButton, gettext_f, prelude::*, spawn, spawn_tokio, toast,
    utils::BoundObjectWeakRef,
};

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/login/homeserver_page.ui")]
    pub struct LoginHomeserverPage {
        #[template_child]
        pub homeserver_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub homeserver_help: TemplateChild<gtk::Label>,
        #[template_child]
        pub next_button: TemplateChild<SpinnerButton>,
        /// The parent `Login` object.
        pub login: BoundObjectWeakRef<Login>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LoginHomeserverPage {
        const NAME: &'static str = "LoginHomeserverPage";
        type Type = super::LoginHomeserverPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LoginHomeserverPage {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> =
                Lazy::new(|| vec![glib::ParamSpecObject::builder::<Login>("login").build()]);

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "login" => self.obj().login().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "login" => self.obj().set_login(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            self.login.disconnect_signals();
        }
    }

    impl WidgetImpl for LoginHomeserverPage {}
    impl BinImpl for LoginHomeserverPage {}
}

glib::wrapper! {
    /// The login page to provide the homeserver and login settings.
    pub struct LoginHomeserverPage(ObjectSubclass<imp::LoginHomeserverPage>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl LoginHomeserverPage {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The parent `Login` object.
    pub fn login(&self) -> Option<Login> {
        self.imp().login.obj()
    }

    /// Set the parent `Login` object.
    fn set_login(&self, login: Option<&Login>) {
        let imp = self.imp();

        imp.login.disconnect_signals();

        if let Some(login) = login {
            let handler = login.connect_notify_local(
                Some("autodiscovery"),
                clone!(@weak self as obj => move |_, _| {
                    obj.update_next_state();
                    obj.update_text();
                }),
            );

            imp.login.set(login, vec![handler]);
        }

        self.update_next_state();
        self.update_text();
    }

    /// Update the text of this page according to the current settings.
    fn update_text(&self) {
        let Some(login) = self.login() else {
            return;
        };
        let imp = self.imp();

        if login.autodiscovery() {
            imp.homeserver_entry.set_title(&gettext("Domain Name"));
            imp.homeserver_help.set_markup(&gettext(
                "The domain of your Matrix homeserver, for example gnome.org",
            ));
        } else {
            imp.homeserver_entry.set_title(&gettext("Homeserver URL"));
            imp.homeserver_help.set_markup(&gettext_f(
                // Translators: Do NOT translate the content between '{' and '}', this is a
                // variable name.
                "The URL of your Matrix homeserver, for example {address}",
                &[(
                    "address",
                    "<span segment=\"word\">https://gnome.modular.im</span>",
                )],
            ));
        }
    }

    /// Focus the default widget.
    pub fn focus_default(&self) {
        self.imp().homeserver_entry.grab_focus();
    }

    /// Reset this page.
    pub fn clean(&self) {
        let imp = self.imp();
        imp.homeserver_entry.set_text("");
        imp.next_button.set_loading(false);
        self.update_next_state();
    }

    /// The server name entered by the user, if any.
    pub fn server_name(&self) -> Option<OwnedServerName> {
        sanitize_server_name(self.imp().homeserver_entry.text().as_str()).ok()
    }

    /// The homeserver URL entered by the user, if any.
    pub fn homeserver_url(&self) -> Option<Url> {
        build_homeserver_url(self.imp().homeserver_entry.text().as_str()).ok()
    }

    /// Whether the current state allows to go to the next step.
    fn can_go_next(&self) -> bool {
        let Some(login) = self.login() else {
            return false;
        };
        let homeserver = self.imp().homeserver_entry.text();

        if login.autodiscovery() {
            sanitize_server_name(homeserver.as_str()).is_ok()
                || build_homeserver_url(homeserver.as_str()).is_ok()
        } else {
            build_homeserver_url(homeserver.as_str()).is_ok()
        }
    }

    /// Update the state of the "Next" button.
    #[template_callback]
    fn update_next_state(&self) {
        self.imp().next_button.set_sensitive(self.can_go_next());
    }

    /// Fetch the login details of the homeserver.
    #[template_callback]
    pub fn fetch_homeserver_details(&self) {
        spawn!(clone!(@weak self as obj => async move {
            obj.check_homeserver().await;
        }));
    }

    /// Check if the homeserver that was entered is valid.
    pub async fn check_homeserver(&self) {
        if !self.can_go_next() {
            return;
        }

        let Some(login) = self.login() else {
            return;
        };
        let imp = self.imp();

        imp.next_button.set_loading(true);
        login.freeze();

        let autodiscovery = login.autodiscovery();

        let res = if autodiscovery {
            self.discover_homeserver().await
        } else {
            self.detect_homeserver(self.homeserver_url().unwrap(), false)
                .await
                .map(|c| (c, None))
        };

        match res {
            Ok((client, server_name)) => {
                login.set_domain(server_name);
                login.set_client(Some(client.clone()));

                self.homeserver_login_types(client).await;
            }
            Err(error) => {
                toast!(self, error.to_user_facing());
            }
        };

        imp.next_button.set_loading(false);
        login.unfreeze();
    }

    async fn discover_homeserver(
        &self,
    ) -> Result<(Client, Option<OwnedServerName>), ClientBuildError> {
        let mut discovery_error = None;
        let mut server_error = None;

        // Try to discover the server.
        if let Some(server_name) = self.server_name() {
            let server_name_clone = server_name.clone();
            let handle = spawn_tokio!(async move {
                client_builder()
                    .respect_login_well_known(true)
                    .server_name(&server_name_clone)
                    .build()
                    .await
            });

            match handle.await.unwrap() {
                Ok(client) => return Ok((client, Some(server_name))),
                Err(error) => {
                    discovery_error = Some(error);
                }
            }
        }

        // Check if it is a valid homeserver URL.
        if let Some(homeserver_url) = self.homeserver_url() {
            match self.detect_homeserver(homeserver_url, true).await {
                Ok(client) => return Ok((client, None)),
                Err(error) => {
                    server_error = Some(error);
                }
            }
        }

        match (discovery_error, server_error) {
            (Some(discovery_error), Some(server_error)) => {
                warn!("Failed to discover homeserver. Auto-discovery error: {discovery_error}. Homeserver detection error: {server_error}");
                Err(discovery_error)
            }
            (Some(discovery_error), None) => {
                warn!("Failed to discover homeserver. Auto-discovery error: {discovery_error}");
                Err(discovery_error)
            }
            (None, Some(server_error)) => {
                warn!("Failed to discover homeserver. Homeserver detection error: {server_error}");
                Err(server_error)
            }
            // We should have at least one error at this step.
            _ => unreachable!(),
        }
    }

    async fn detect_homeserver(
        &self,
        url: Url,
        autodiscovery: bool,
    ) -> Result<Client, ClientBuildError> {
        spawn_tokio!(async move {
            let client = client_builder()
                .respect_login_well_known(autodiscovery)
                .homeserver_url(url)
                .build()
                .await?;

            client
                .send(get_supported_versions::Request::new(), None)
                .await?;

            Ok(client)
        })
        .await
        .unwrap()
    }

    /// Fetch the login types supported by the homeserver.
    async fn homeserver_login_types(&self, client: Client) {
        let Some(login) = self.login() else {
            return;
        };

        let handle = spawn_tokio!(async move { client.matrix_auth().get_login_types().await });

        match handle.await.unwrap() {
            Ok(res) => {
                login.set_login_types(res.flows);
                login.show_login_screen();
            }
            Err(error) => {
                warn!("Failed to get available login types: {error}");
                toast!(self, "Failed to get available login types.");

                // Drop the client because it is bound to the homeserver.
                login.drop_client();
            }
        };
    }
}

fn build_homeserver_url(server: &str) -> Result<Url, ParseError> {
    if server.starts_with("http://") || server.starts_with("https://") {
        Url::parse(server)
    } else {
        Url::parse(&format!("https://{server}"))
    }
}

fn client_builder() -> ClientBuilder {
    Client::builder().request_config(RequestConfig::new().retry_limit(2))
}

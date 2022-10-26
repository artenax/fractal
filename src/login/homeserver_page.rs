use adw::{prelude::*, subclass::prelude::BinImpl};
use gettextrs::gettext;
use gtk::{self, glib, glib::clone, subclass::prelude::*, CompositeTemplate};
use ruma::{IdParseError, OwnedServerName, ServerName};
use url::{ParseError, Url};

use crate::gettext_f;

mod imp {
    use std::cell::Cell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/login-homeserver-page.ui")]
    pub struct LoginHomeserverPage {
        #[template_child]
        pub homeserver_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub homeserver_help: TemplateChild<gtk::Label>,
        /// Whether homeserver auto-discovery is enabled.
        pub autodiscovery: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LoginHomeserverPage {
        const NAME: &'static str = "LoginHomeserverPage";
        type Type = super::LoginHomeserverPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LoginHomeserverPage {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> =
                Lazy::new(|| vec![glib::ParamSpecBoolean::builder("autodiscovery").build()]);

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "autodiscovery" => self.obj().autodiscovery().to_value(),
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
            self.parent_constructed();
            let obj = self.obj();

            self.homeserver_entry
                .connect_entry_activated(clone!(@weak obj => move|_| {
                    let _ = obj.activate_action("login.next", None);
                }));
            self.homeserver_entry
                .connect_changed(clone!(@weak obj => move |_| {
                    let _ = obj.activate_action("login.update-next", None);
                }));
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

impl LoginHomeserverPage {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// Whether homeserver auto-discovery is enabled.
    pub fn autodiscovery(&self) -> bool {
        self.imp().autodiscovery.get()
    }

    /// Set whether homeserver auto-discovery is enabled.
    fn set_autodiscovery(&self, autodiscovery: bool) {
        let priv_ = self.imp();

        priv_.autodiscovery.set(autodiscovery);

        if autodiscovery {
            priv_.homeserver_entry.set_title(&gettext("Domain Name"));
            priv_.homeserver_help.set_markup(&gettext(
                "The domain of your Matrix homeserver, for example gnome.org",
            ));
        } else {
            priv_.homeserver_entry.set_title(&gettext("Homeserver URL"));
            priv_.homeserver_help.set_markup(&gettext_f(
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

    /// The server name entered by the user, if any.
    pub fn server_name(&self) -> Option<OwnedServerName> {
        build_server_name(self.imp().homeserver_entry.text().as_str()).ok()
    }

    /// The homeserver URL entered by the user, if any.
    pub fn homeserver_url(&self) -> Option<Url> {
        build_homeserver_url(self.imp().homeserver_entry.text().as_str()).ok()
    }

    pub fn can_go_next(&self) -> bool {
        let homeserver = self.imp().homeserver_entry.text();

        if self.autodiscovery() {
            build_server_name(homeserver.as_str()).is_ok()
        } else {
            build_homeserver_url(homeserver.as_str()).is_ok()
        }
    }

    pub fn focus_default(&self) {
        self.imp().homeserver_entry.grab_focus();
    }

    pub fn clean(&self) {
        self.imp().homeserver_entry.set_text("");
    }
}

impl Default for LoginHomeserverPage {
    fn default() -> Self {
        Self::new()
    }
}

fn build_server_name(server: &str) -> Result<OwnedServerName, IdParseError> {
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

use adw::{prelude::*, subclass::prelude::BinImpl};
use gtk::{self, glib, glib::clone, subclass::prelude::*, CompositeTemplate};
use ruma::api::client::session::get_login_types::v3::SsoLoginType;

use super::idp_button::IdpButton;
use crate::i18n::gettext_f;

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/login-method-page.ui")]
    pub struct LoginMethodPage {
        #[template_child]
        pub title: TemplateChild<gtk::Label>,
        #[template_child]
        pub username_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub password_entry: TemplateChild<adw::PasswordEntryRow>,
        #[template_child]
        pub sso_idp_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub more_sso_option: TemplateChild<gtk::Button>,
        /// The homeserver to log into.
        pub homeserver: RefCell<Option<String>>,
        /// Whether homeserver auto-discovery is enabled.
        pub autodiscovery: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LoginMethodPage {
        const NAME: &'static str = "LoginMethodPage";
        type Type = super::LoginMethodPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LoginMethodPage {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::new(
                        "homeserver",
                        "Homeserver",
                        "The homeserver to log into",
                        None,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "autodiscovery",
                        "Auto-discovery",
                        "Whether homeserver auto-discovery is enabled",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "homeserver" => self.homeserver.borrow().to_value(),
                "autodiscovery" => self.autodiscovery.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "homeserver" => {
                    self.homeserver.replace(value.get().ok());
                }
                "autodiscovery" => self.autodiscovery.set(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            self.username_entry
                .connect_entry_activated(clone!(@weak obj => move|_| {
                    let _ = obj.activate_action("login.next", None);
                }));
            self.username_entry
                .connect_changed(clone!(@weak obj => move |_| {
                    let _ = obj.activate_action("login.update-next", None);
                }));

            self.password_entry
                .connect_entry_activated(clone!(@weak obj => move|_| {
                    let _ = obj.activate_action("login.next", None);
                }));
            self.password_entry
                .connect_changed(clone!(@weak obj => move |_| {
                    let _ = obj.activate_action("login.update-next", None);
                }));
        }
    }

    impl WidgetImpl for LoginMethodPage {}

    impl BinImpl for LoginMethodPage {}
}

glib::wrapper! {
    /// A widget handling the login flows.
    pub struct LoginMethodPage(ObjectSubclass<imp::LoginMethodPage>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl LoginMethodPage {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// The username entered by the user.
    pub fn username(&self) -> String {
        self.imp().username_entry.text().into()
    }

    /// The password entered by the user.
    pub fn password(&self) -> String {
        self.imp().password_entry.text().into()
    }

    /// Set the domain name to show in the title.
    pub fn set_domain_name(&self, domain_name: &str) {
        self.imp().title.set_markup(&gettext_f(
            // Translators: Do NOT translate the content between '{' and '}', this is a variable
            // name.
            "Connecting to {domain_name}",
            &[(
                "domain_name",
                &format!("<span segment=\"word\">{}</span>", domain_name),
            )],
        ))
    }

    pub fn update_sso(&self, login_types: Option<&SsoLoginType>) {
        let priv_ = self.imp();

        let login_types = match login_types {
            Some(t) => t,
            None => {
                priv_.sso_idp_box.hide();
                priv_.more_sso_option.hide();
                return;
            }
        };

        self.clean_idp_box();

        let mut has_unknown_methods = false;
        let mut has_known_methods = false;

        for provider in &login_types.identity_providers {
            let btn = IdpButton::new_from_identity_provider(provider);

            if let Some(btn) = btn {
                priv_.sso_idp_box.append(&btn);
                has_known_methods = true;
            } else {
                has_unknown_methods = true;
            }
        }

        priv_.sso_idp_box.set_visible(has_known_methods);
        priv_.more_sso_option.set_visible(has_unknown_methods);
    }

    pub fn can_go_next(&self) -> bool {
        let priv_ = self.imp();
        let username_length = priv_.username_entry.text().len();
        let password_length = priv_.password_entry.text().len();
        username_length != 0 && password_length != 0
    }

    pub fn clean(&self) {
        let priv_ = self.imp();
        priv_.username_entry.set_text("");
        priv_.password_entry.set_text("");

        self.clean_idp_box();
    }

    /// Empty the identity providers box.
    pub fn clean_idp_box(&self) {
        let priv_ = self.imp();

        let mut child = priv_.sso_idp_box.first_child();
        while child.is_some() {
            priv_.sso_idp_box.remove(&child.unwrap());
            child = priv_.sso_idp_box.first_child();
        }
    }

    pub fn focus_default(&self) {
        self.imp().username_entry.grab_focus();
    }
}

impl Default for LoginMethodPage {
    fn default() -> Self {
        Self::new()
    }
}

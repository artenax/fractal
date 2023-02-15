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
                    glib::ParamSpecString::builder("homeserver").build(),
                    glib::ParamSpecBoolean::builder("autodiscovery").build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "homeserver" => obj.homeserver().to_value(),
                "autodiscovery" => obj.autodiscovery().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "homeserver" => {
                    obj.set_homeserver(value.get().unwrap());
                }
                "autodiscovery" => obj.set_autodiscovery(value.get().unwrap()),
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
        glib::Object::new()
    }

    /// The homeserver to log into.
    pub fn homeserver(&self) -> Option<String> {
        self.imp().homeserver.borrow().clone()
    }

    /// Set the homeserver to log into.
    pub fn set_homeserver(&self, homeserver: Option<String>) {
        self.imp().homeserver.replace(homeserver);
    }

    /// Whether homeserver auto-discovery is enabled.
    pub fn autodiscovery(&self) -> bool {
        self.imp().autodiscovery.get()
    }

    /// Set whether homeserver auto-discovery is enabled.
    pub fn set_autodiscovery(&self, autodiscovery: bool) {
        self.imp().autodiscovery.set(autodiscovery)
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
                &format!("<span segment=\"word\">{domain_name}</span>"),
            )],
        ))
    }

    pub fn update_sso(&self, login_types: Option<&SsoLoginType>) {
        let imp = self.imp();

        let login_types = match login_types {
            Some(t) => t,
            None => {
                imp.sso_idp_box.hide();
                imp.more_sso_option.hide();
                return;
            }
        };

        self.clean_idp_box();

        let mut has_unknown_methods = false;
        let mut has_known_methods = false;

        for provider in &login_types.identity_providers {
            let btn = IdpButton::new_from_identity_provider(provider);

            if let Some(btn) = btn {
                imp.sso_idp_box.append(&btn);
                has_known_methods = true;
            } else {
                has_unknown_methods = true;
            }
        }

        imp.sso_idp_box.set_visible(has_known_methods);
        imp.more_sso_option.set_visible(has_unknown_methods);
    }

    pub fn can_go_next(&self) -> bool {
        let imp = self.imp();
        let username_length = imp.username_entry.text().len();
        let password_length = imp.password_entry.text().len();
        username_length != 0 && password_length != 0
    }

    pub fn clean(&self) {
        let imp = self.imp();
        imp.username_entry.set_text("");
        imp.password_entry.set_text("");

        self.clean_idp_box();
    }

    /// Empty the identity providers box.
    pub fn clean_idp_box(&self) {
        let imp = self.imp();

        let mut child = imp.sso_idp_box.first_child();
        while child.is_some() {
            imp.sso_idp_box.remove(&child.unwrap());
            child = imp.sso_idp_box.first_child();
        }
    }

    pub fn focus_default(&self) {
        self.imp().username_entry.grab_focus();
    }
}

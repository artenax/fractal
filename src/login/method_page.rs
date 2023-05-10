use adw::{prelude::*, subclass::prelude::BinImpl};
use gtk::{self, glib, glib::clone, subclass::prelude::*, CompositeTemplate};
use log::warn;
use ruma::api::client::session::get_login_types::v3::LoginType;

use super::{idp_button::IdpButton, Login};
use crate::{
    components::SpinnerButton, gettext_f, spawn, spawn_tokio, toast,
    user_facing_error::UserFacingError, utils::BoundObjectWeakRef,
};

mod imp {
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
        #[template_child]
        pub next_button: TemplateChild<SpinnerButton>,
        /// The parent `Login` object.
        pub login: BoundObjectWeakRef<Login>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LoginMethodPage {
        const NAME: &'static str = "LoginMethodPage";
        type Type = super::LoginMethodPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LoginMethodPage {
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

    impl WidgetImpl for LoginMethodPage {}
    impl BinImpl for LoginMethodPage {}
}

glib::wrapper! {
    /// The login page allowing to login via password or to choose a SSO provider.
    pub struct LoginMethodPage(ObjectSubclass<imp::LoginMethodPage>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl LoginMethodPage {
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
            let domain_handler = login.connect_notify_local(
                Some("domain"),
                clone!(@weak self as obj => move |_, _| {
                    obj.update_domain_name();
                }),
            );
            let login_types_handler = login.connect_notify_local(
                Some("login-types"),
                clone!(@weak self as obj => move |_, _| {
                    obj.update_sso();
                }),
            );

            imp.login
                .set(login, vec![domain_handler, login_types_handler]);
        }

        self.update_domain_name();
        self.update_sso();
        self.update_next_state();
    }

    /// The username entered by the user.
    pub fn username(&self) -> String {
        self.imp().username_entry.text().into()
    }

    /// The password entered by the user.
    pub fn password(&self) -> String {
        self.imp().password_entry.text().into()
    }

    /// Update the domain name displayed in the title.
    pub fn update_domain_name(&self) {
        let Some(login) = self.login() else {
            return;
        };
        let Some(domain) = login.domain() else {
            return;
        };

        self.imp().title.set_markup(&gettext_f(
            // Translators: Do NOT translate the content between '{' and '}', this is a variable
            // name.
            "Log in to {domain_name}",
            &[(
                "domain_name",
                &format!("<span segment=\"word\">{domain}</span>"),
            )],
        ))
    }

    /// Update the SSO group.
    pub fn update_sso(&self) {
        let Some(login) = self.login() else {
            return;
        };
        let imp = self.imp();

        let login_types = login.login_types();
        let sso_login = match login_types.into_iter().find_map(|t| match t {
            LoginType::Sso(sso) => Some(sso),
            _ => None,
        }) {
            Some(sso) => sso,
            None => {
                imp.sso_idp_box.set_visible(false);
                imp.more_sso_option.set_visible(false);
                return;
            }
        };

        self.clean_idp_box();

        let mut has_unknown_methods = false;
        let mut has_known_methods = false;

        for provider in &sso_login.identity_providers {
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

    /// Whether the current state allows to login with a password.
    pub fn can_login_with_password(&self) -> bool {
        let username_length = self.username().len();
        let password_length = self.password().len();
        username_length != 0 && password_length != 0
    }

    /// Update the state of the "Next" button.
    #[template_callback]
    fn update_next_state(&self) {
        self.imp()
            .next_button
            .set_sensitive(self.can_login_with_password());
    }

    /// Login with the password login type.
    #[template_callback]
    fn login_with_password(&self) {
        if !self.can_login_with_password() {
            return;
        }

        spawn!(clone!(@weak self as obj => async move {
            obj.login_with_password_inner().await;
        }));
    }

    async fn login_with_password_inner(&self) {
        let Some(login) = self.login() else {
            return;
        };
        let imp = self.imp();

        imp.next_button.set_loading(true);
        login.freeze();

        let username = self.username();
        let password = self.password();

        let client = login.client().unwrap();
        let handle = spawn_tokio!(async move {
            client
                .login_username(&username, &password)
                .initial_device_display_name("Fractal")
                .send()
                .await
        });

        match handle.await.unwrap() {
            Ok(response) => {
                login.handle_login_response(response).await;
            }
            Err(error) => {
                warn!("Failed to log in: {error}");
                toast!(self, error.to_user_facing());
            }
        }

        imp.next_button.set_loading(false);
        login.freeze();
    }

    /// Reset this page.
    pub fn clean(&self) {
        let imp = self.imp();
        imp.username_entry.set_text("");
        imp.password_entry.set_text("");
        imp.next_button.set_loading(false);
        self.update_next_state();
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

    /// Focus the default widget.
    pub fn focus_default(&self) {
        self.imp().username_entry.grab_focus();
    }
}

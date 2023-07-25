use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    glib::{self, clone},
    CompositeTemplate,
};
use matrix_sdk::ruma::{
    api::client::{account::change_password, error::ErrorKind},
    assign,
};
use tracing::error;

use crate::{
    components::{AuthDialog, AuthError, SpinnerButton},
    session::model::Session,
    spawn, toast,
    utils::matrix::validate_password,
};

mod imp {
    use glib::{subclass::InitializingObject, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/account_settings/user_page/change_password_subpage.ui"
    )]
    pub struct ChangePasswordSubpage {
        pub session: WeakRef<Session>,
        #[template_child]
        pub password: TemplateChild<adw::PasswordEntryRow>,
        #[template_child]
        pub password_progress: TemplateChild<gtk::LevelBar>,
        #[template_child]
        pub password_error_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub password_error: TemplateChild<gtk::Label>,
        #[template_child]
        pub confirm_password: TemplateChild<adw::PasswordEntryRow>,
        #[template_child]
        pub confirm_password_error_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub confirm_password_error: TemplateChild<gtk::Label>,
        #[template_child]
        pub button: TemplateChild<SpinnerButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChangePasswordSubpage {
        const NAME: &'static str = "ChangePasswordSubpage";
        type Type = super::ChangePasswordSubpage;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ChangePasswordSubpage {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> =
                Lazy::new(|| vec![glib::ParamSpecObject::builder::<Session>("session").build()]);

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "session" => self.obj().set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session" => self.obj().session().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            self.password_progress.set_min_value(0.0);
            self.password_progress.set_max_value(5.0);
            self.password_progress
                .add_offset_value(gtk::LEVEL_BAR_OFFSET_LOW, 1.0);
            self.password_progress.add_offset_value("step2", 2.0);
            self.password_progress.add_offset_value("step3", 3.0);
            self.password_progress
                .add_offset_value(gtk::LEVEL_BAR_OFFSET_HIGH, 4.0);
            self.password_progress
                .add_offset_value(gtk::LEVEL_BAR_OFFSET_FULL, 5.0);

            self.password.connect_changed(clone!(@weak obj => move|_| {
                obj.validate_password();
            }));

            self.confirm_password
                .connect_changed(clone!(@weak obj => move|_| {
                    obj.validate_password_confirmation();
                }));
        }
    }

    impl WidgetImpl for ChangePasswordSubpage {}
    impl BoxImpl for ChangePasswordSubpage {}
}

glib::wrapper! {
    /// Account settings page about the user and the session.
    pub struct ChangePasswordSubpage(ObjectSubclass<imp::ChangePasswordSubpage>)
        @extends gtk::Widget, gtk::Box, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl ChangePasswordSubpage {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Set the current session.
    pub fn set_session(&self, session: Option<Session>) {
        self.imp().session.set(session.as_ref());
    }

    fn validate_password(&self) {
        let imp = self.imp();
        let entry = &imp.password;
        let progress = &imp.password_progress;
        let revealer = &imp.password_error_revealer;
        let label = &imp.password_error;
        let password = entry.text();

        if password.is_empty() {
            revealer.set_reveal_child(false);
            entry.remove_css_class("success");
            entry.remove_css_class("warning");
            progress.set_value(0.0);
            progress.remove_css_class("success");
            progress.remove_css_class("warning");
            self.update_button();
            return;
        }

        let validity = validate_password(&password);

        progress.set_value(validity.progress as f64 / 20.0);
        if validity.progress == 100 {
            revealer.set_reveal_child(false);
            entry.add_css_class("success");
            entry.remove_css_class("warning");
            progress.add_css_class("success");
            progress.remove_css_class("warning");
        } else {
            entry.remove_css_class("success");
            entry.add_css_class("warning");
            progress.remove_css_class("success");
            progress.add_css_class("warning");
            if !validity.has_length {
                label.set_label(&gettext("Password must be at least 8 characters long"));
            } else if !validity.has_lowercase {
                label.set_label(&gettext(
                    "Password must have at least one lower-case letter",
                ));
            } else if !validity.has_uppercase {
                label.set_label(&gettext(
                    "Password must have at least one upper-case letter",
                ));
            } else if !validity.has_number {
                label.set_label(&gettext("Password must have at least one digit"));
            } else if !validity.has_symbol {
                label.set_label(&gettext("Password must have at least one symbol"));
            }
            revealer.set_reveal_child(true);
        }

        self.update_button();
    }

    fn validate_password_confirmation(&self) {
        let imp = self.imp();
        let entry = &imp.confirm_password;
        let revealer = &imp.confirm_password_error_revealer;
        let label = &imp.confirm_password_error;
        let password = imp.password.text();
        let confirmation = entry.text();

        if confirmation.is_empty() {
            revealer.set_reveal_child(false);
            entry.remove_css_class("success");
            entry.remove_css_class("warning");
            return;
        }

        if password == confirmation {
            revealer.set_reveal_child(false);
            entry.add_css_class("success");
            entry.remove_css_class("warning");
        } else {
            entry.remove_css_class("success");
            entry.add_css_class("warning");
            label.set_label(&gettext("Passwords do not match"));
            revealer.set_reveal_child(true);
        }
        self.update_button();
    }

    fn update_button(&self) {
        self.imp().button.set_sensitive(self.can_change_password());
    }

    fn can_change_password(&self) -> bool {
        let imp = self.imp();
        let password = imp.password.text();
        let confirmation = imp.confirm_password.text();

        validate_password(&password).progress == 100 && password == confirmation
    }

    #[template_callback]
    fn handle_proceed(&self) {
        spawn!(clone!(@weak self as obj => async move {
            obj.change_password().await;
        }));
    }

    async fn change_password(&self) {
        if !self.can_change_password() {
            return;
        }

        let imp = self.imp();
        let password = imp.password.text();

        imp.button.set_loading(true);
        imp.password.set_sensitive(false);
        imp.confirm_password.set_sensitive(false);

        let session = self.session().unwrap();
        let dialog = AuthDialog::new(self.root().and_downcast_ref::<gtk::Window>(), &session);

        let result = dialog
            .authenticate(move |client, auth| {
                let password = password.clone();
                async move {
                    let request =
                        assign!(change_password::v3::Request::new(password.into()), { auth });
                    client.send(request, None).await.map_err(Into::into)
                }
            })
            .await;

        match result {
            Ok(_) => {
                toast!(self, gettext("Password changed successfully"));
                imp.password.set_text("");
                imp.confirm_password.set_text("");
                self.activate_action("win.close-subpage", None).unwrap();
            }
            Err(error) => match error {
                AuthError::UserCancelled => {}
                AuthError::ServerResponse(error)
                    if matches!(error.client_api_error_kind(), Some(ErrorKind::WeakPassword)) =>
                {
                    error!("Weak password: {error}");
                    toast!(self, gettext("Password rejected for being too weak"));
                }
                _ => {
                    error!("Failed to change the password: {error:?}");
                    toast!(self, gettext("Could not change password"));
                }
            },
        }
        imp.button.set_loading(false);
        imp.password.set_sensitive(true);
        imp.confirm_password.set_sensitive(true);
    }
}

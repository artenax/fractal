use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    glib::{self, clone},
    CompositeTemplate,
};
use log::error;
use matrix_sdk::{
    ruma::{
        api::{
            client::{account::change_password, error::ErrorKind},
            error::{FromHttpResponseError, ServerError},
        },
        assign,
    },
    Error as MatrixError, HttpError, RumaApiError,
};

use crate::{
    components::{AuthDialog, AuthError, PasswordEntryRow, SpinnerButton},
    session::Session,
    spawn, toast,
    utils::validate_password,
};

mod imp {
    use glib::{subclass::InitializingObject, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/account-settings-change-password-subpage.ui")]
    pub struct ChangePasswordSubpage {
        pub session: WeakRef<Session>,
        #[template_child]
        pub password: TemplateChild<PasswordEntryRow>,
        #[template_child]
        pub confirm_password: TemplateChild<PasswordEntryRow>,
        #[template_child]
        pub button: TemplateChild<SpinnerButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChangePasswordSubpage {
        const NAME: &'static str = "ChangePasswordSubpage";
        type Type = super::ChangePasswordSubpage;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            PasswordEntryRow::static_type();
            SpinnerButton::static_type();
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ChangePasswordSubpage {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "session",
                    "Session",
                    "The session",
                    Session::static_type(),
                    glib::ParamFlags::READWRITE,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "session" => obj.set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session" => obj.session().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.password.define_progress_steps(&[
                &gtk::LEVEL_BAR_OFFSET_LOW,
                "step2",
                "step3",
                &gtk::LEVEL_BAR_OFFSET_HIGH,
                &gtk::LEVEL_BAR_OFFSET_FULL,
            ]);
            self.password
                .connect_focused(clone!(@weak obj => move |entry, focused| {
                    if focused {
                        entry.set_progress_visible(true);
                        obj.validate_password();
                    } else {
                        entry.remove_css_class("warning");
                        entry.remove_css_class("success");
                        if entry.text().is_empty() {
                            entry.set_progress_visible(false);
                        }
                    }
                }));
            self.password
                .connect_activated(clone!(@weak obj => move|_| {
                    spawn!(
                        clone!(@weak obj => async move {
                            obj.change_password().await;
                        })
                    );
                }));
            self.password.connect_changed(clone!(@weak obj => move|_| {
                obj.validate_password();
            }));

            self.confirm_password
                .connect_focused(clone!(@weak obj => move |entry, focused| {
                    if focused {
                        obj.validate_password_confirmation();
                    } else {
                        entry.remove_css_class("warning");
                        entry.remove_css_class("success");
                    }
                }));
            self.confirm_password
                .connect_activated(clone!(@weak obj => move|_| {
                    spawn!(
                        clone!(@weak obj => async move {
                            obj.change_password().await;
                        })
                    );
                }));
            self.confirm_password
                .connect_changed(clone!(@weak obj => move|_| {
                    obj.validate_password_confirmation();
                }));

            self.button.connect_clicked(clone!(@weak obj => move|_| {
                spawn!(
                    clone!(@weak obj => async move {
                        obj.change_password().await;
                    })
                );
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

impl ChangePasswordSubpage {
    pub fn new(session: &Session) -> Self {
        glib::Object::new(&[("session", session)]).expect("Failed to create ChangePasswordSubpage")
    }

    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    pub fn set_session(&self, session: Option<Session>) {
        self.imp().session.set(session.as_ref());
    }

    fn validate_password(&self) {
        let entry = &self.imp().password;
        let password = entry.text();

        if password.is_empty() {
            entry.set_hint("");
            entry.remove_css_class("success");
            entry.remove_css_class("warning");
            entry.set_progress_value(0.0);
            self.update_button();
            return;
        }

        let validity = validate_password(&password);

        entry.set_progress_value(validity.progress as f64 / 20.0);
        if validity.progress == 100 {
            entry.set_hint("");
            entry.add_css_class("success");
            entry.remove_css_class("warning");
        } else {
            entry.remove_css_class("success");
            entry.add_css_class("warning");
            if !validity.has_length {
                entry.set_hint(&gettext("Password must be at least 8 characters long"));
            } else if !validity.has_lowercase {
                entry.set_hint(&gettext(
                    "Password must have at least one lower-case letter",
                ));
            } else if !validity.has_uppercase {
                entry.set_hint(&gettext(
                    "Password must have at least one upper-case letter",
                ));
            } else if !validity.has_number {
                entry.set_hint(&gettext("Password must have at least one digit"));
            } else if !validity.has_symbol {
                entry.set_hint(&gettext("Password must have at least one symbol"));
            }
        }
        self.update_button();
    }

    fn validate_password_confirmation(&self) {
        let priv_ = self.imp();
        let entry = &priv_.confirm_password;
        let password = priv_.password.text();
        let confirmation = entry.text();

        if confirmation.is_empty() {
            entry.set_hint("");
            entry.remove_css_class("success");
            entry.remove_css_class("warning");
            return;
        }

        if password == confirmation {
            entry.set_hint("");
            entry.add_css_class("success");
            entry.remove_css_class("warning");
        } else {
            entry.remove_css_class("success");
            entry.add_css_class("warning");
            entry.set_hint(&gettext("Passwords do not match"));
        }
        self.update_button();
    }

    fn update_button(&self) {
        self.imp().button.set_sensitive(self.can_change_password());
    }

    fn can_change_password(&self) -> bool {
        let priv_ = self.imp();
        let password = priv_.password.text();
        let confirmation = priv_.confirm_password.text();

        validate_password(&password).progress == 100 && password == confirmation
    }

    async fn change_password(&self) {
        if !self.can_change_password() {
            return;
        }

        let priv_ = self.imp();
        let password = priv_.password.text();

        priv_.button.set_loading(true);
        priv_.password.set_entry_sensitive(false);
        priv_.confirm_password.set_entry_sensitive(false);

        let session = self.session().unwrap();
        let dialog = AuthDialog::new(
            self.root()
                .as_ref()
                .and_then(|root| root.downcast_ref::<gtk::Window>()),
            &session,
        );

        let result = dialog
            .authenticate(move |client, auth_data| {
                let password = password.clone();
                async move {
                    if let Some(auth) = auth_data {
                        let auth = Some(auth.as_matrix_auth_data());
                        let request =
                            assign!(change_password::v3::Request::new(&password), { auth });
                        client.send(request, None).await.map_err(Into::into)
                    } else {
                        let request = change_password::v3::Request::new(&password);
                        client.send(request, None).await.map_err(Into::into)
                    }
                }
            })
            .await;

        match result {
            Ok(_) => {
                toast!(self, gettext("Password changed successfully"));
                priv_.password.set_text("");
                priv_.confirm_password.set_text("");
                self.activate_action("win.close-subpage", None).unwrap();
            }
            Err(err) => match err {
                AuthError::UserCancelled => {}
                AuthError::ServerResponse(error)
                    if matches!(error.as_ref(), MatrixError::Http(HttpError::Api(
                    FromHttpResponseError::Server(ServerError::Known(RumaApiError::ClientApi(
                        error,
                    ))),
                )) if error.kind == ErrorKind::WeakPassword) =>
                {
                    error!("Weak password: {:?}", error);
                    toast!(self, gettext("Password rejected for being too weak"));
                }
                _ => {
                    error!("Failed to change the password: {:?}", err);
                    toast!(self, gettext("Could not change password"));
                }
            },
        }
        priv_.button.set_loading(false);
        priv_.password.set_entry_sensitive(true);
        priv_.confirm_password.set_entry_sensitive(true);
    }
}

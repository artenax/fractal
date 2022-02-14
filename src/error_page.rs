use adw::subclass::prelude::BinImpl;
use gettextrs::gettext;
use gtk::{self, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};
use log::error;

use crate::{components::Toast, secret, secret::SecretError, spawn, window::Window};

pub enum ErrorSubpage {
    SecretErrorSession,
    SecretErrorOther,
}

impl AsRef<str> for ErrorSubpage {
    fn as_ref(&self) -> &str {
        match self {
            Self::SecretErrorSession => "secret-error-session",
            Self::SecretErrorOther => "secret-error-other",
        }
    }
}

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/error-page.ui")]
    pub struct ErrorPage {
        #[template_child]
        pub page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        pub secret_error: RefCell<Option<SecretError>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ErrorPage {
        const NAME: &'static str = "ErrorPage";
        type Type = super::ErrorPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Group);
            klass.install_action(
                "error-page.remove-secret-error-session",
                None,
                |obj, _, _| {
                    spawn!(clone!(@weak obj => async move {
                        obj.remove_secret_error_session().await;
                    }));
                },
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ErrorPage {
        fn constructed(&self, obj: &Self::Type) {
            obj.action_set_enabled("error-page.remove-secret-error-session", false);
        }
    }

    impl WidgetImpl for ErrorPage {}

    impl BinImpl for ErrorPage {}
}

glib::wrapper! {
    pub struct ErrorPage(ObjectSubclass<imp::ErrorPage>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl ErrorPage {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create ErrorPage")
    }

    pub fn display_secret_error(&self, message: &str, error: SecretError) {
        let priv_ = self.imp();
        self.action_set_enabled(
            "error-page.remove-secret-error-session",
            matches!(error, SecretError::CorruptSession(_)),
        );
        priv_.page.set_description(Some(message));
        priv_
            .stack
            .set_visible_child_name(error.error_subpage().as_ref());
        priv_.secret_error.replace(Some(error));
    }

    async fn remove_secret_error_session(&self) {
        if let Some(SecretError::CorruptSession((_, item))) = self.imp().secret_error.take() {
            match secret::remove_item(&item).await {
                Ok(_) => {
                    self.action_set_enabled("error-page.remove-secret-error-session", false);
                    if let Some(window) = self
                        .root()
                        .as_ref()
                        .and_then(|root| root.downcast_ref::<Window>())
                    {
                        window.add_toast(&Toast::new(&gettext("Session removed successfully.")));
                        window.restore_sessions().await;
                    }
                }
                Err(err) => {
                    error!("Could not remove session from secret storage: {:?}", err);
                    if let Some(window) = self
                        .root()
                        .as_ref()
                        .and_then(|root| root.downcast_ref::<Window>())
                    {
                        window.add_toast(&Toast::new(&gettext(
                            "Could not remove session from secret storage",
                        )));
                    }
                }
            }
        }
    }
}

impl Default for ErrorPage {
    fn default() -> Self {
        Self::new()
    }
}

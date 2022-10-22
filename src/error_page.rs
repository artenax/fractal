use adw::subclass::prelude::BinImpl;
use gettextrs::gettext;
use gtk::{self, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};
use log::error;

use crate::{spawn, toast, window::Window};

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
    #[template(resource = "/org/gnome/Fractal/error-page.ui")]
    pub struct ErrorPage {
        #[template_child]
        pub page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        pub secret_item: RefCell<Option<oo7::Item>>,
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

    pub fn display_secret_error(&self, message: &str, item: Option<oo7::Item>) {
        let priv_ = self.imp();
        self.action_set_enabled("error-page.remove-secret-error-session", item.is_some());
        priv_.page.set_description(Some(message));

        let error_subpage = if item.is_some() {
            ErrorSubpage::SecretErrorSession
        } else {
            ErrorSubpage::SecretErrorOther
        };

        priv_.stack.set_visible_child_name(error_subpage.as_ref());
        priv_.secret_item.replace(item);
    }

    async fn remove_secret_error_session(&self) {
        if let Some(item) = self.imp().secret_item.take() {
            match item.delete().await {
                Ok(_) => {
                    self.action_set_enabled("error-page.remove-secret-error-session", false);
                    if let Some(window) = self
                        .root()
                        .as_ref()
                        .and_then(|root| root.downcast_ref::<Window>())
                    {
                        toast!(self, gettext("Session removed successfully."));
                        window.restore_sessions().await;
                    }
                }
                Err(err) => {
                    error!("Could not remove session from secret storage: {:?}", err);
                    toast!(
                        self,
                        gettext("Could not remove session from secret storage")
                    );
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

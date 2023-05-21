use adw::{prelude::*, subclass::prelude::*};
use gtk::{
    glib::{self, clone},
    CompositeTemplate,
};

use crate::{components::SpinnerButton, session::model::Session, spawn, toast};

mod imp {
    use glib::{subclass::InitializingObject, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/account_settings/user_page/log_out_subpage.ui"
    )]
    pub struct LogOutSubpage {
        pub session: WeakRef<Session>,
        #[template_child]
        pub logout_button: TemplateChild<SpinnerButton>,
        #[template_child]
        pub make_backup_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LogOutSubpage {
        const NAME: &'static str = "LogOutSubpage";
        type Type = super::LogOutSubpage;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LogOutSubpage {
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
    }

    impl WidgetImpl for LogOutSubpage {}
    impl BoxImpl for LogOutSubpage {}
}

glib::wrapper! {
    /// Account settings page about the user and the session.
    pub struct LogOutSubpage(ObjectSubclass<imp::LogOutSubpage>)
        @extends gtk::Widget, gtk::Box, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl LogOutSubpage {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Set the current session.
    pub fn set_session(&self, session: Option<Session>) {
        if let Some(session) = session {
            self.imp().session.set(Some(&session));
        }
    }

    #[template_callback]
    fn logout_button_clicked_cb(&self) {
        let imp = self.imp();
        let logout_button = imp.logout_button.get();
        let make_backup_button = imp.make_backup_button.get();
        let session = self.session().unwrap();

        logout_button.set_loading(true);
        make_backup_button.set_sensitive(false);

        spawn!(
            clone!(@weak self as obj, @weak logout_button, @weak make_backup_button, @weak session => async move {
                if let Err(error) = session.logout().await {
                    toast!(obj, error);
                }

                logout_button.set_loading(false);
                make_backup_button.set_sensitive(true);
            })
        );
    }
}

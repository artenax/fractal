use adw::{prelude::*, subclass::prelude::BinImpl};
use gtk::{self, glib, subclass::prelude::*, CompositeTemplate};

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/login-sso-page.ui")]
    pub struct LoginSsoPage {}

    #[glib::object_subclass]
    impl ObjectSubclass for LoginSsoPage {
        const NAME: &'static str = "LoginSsoPage";
        type Type = super::LoginSsoPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LoginSsoPage {}

    impl WidgetImpl for LoginSsoPage {}

    impl BinImpl for LoginSsoPage {}
}

glib::wrapper! {
    /// A widget handling the login flows.
    pub struct LoginSsoPage(ObjectSubclass<imp::LoginSsoPage>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl LoginSsoPage {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }
}

impl Default for LoginSsoPage {
    fn default() -> Self {
        Self::new()
    }
}

use adw::subclass::prelude::BinImpl;
use gtk::{self, glib, prelude::*, subclass::prelude::*, CompositeTemplate};

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/greeter.ui")]
    pub struct Greeter {
        #[template_child]
        pub back_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub login_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Greeter {
        const NAME: &'static str = "Greeter";
        type Type = super::Greeter;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Greeter {}

    impl WidgetImpl for Greeter {}

    impl BinImpl for Greeter {}
}

glib::wrapper! {
    pub struct Greeter(ObjectSubclass<imp::Greeter>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Greeter {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create Greeter")
    }

    pub fn default_widget(&self) -> gtk::Widget {
        self.imp().login_button.get().upcast()
    }
}

impl Default for Greeter {
    fn default() -> Self {
        Self::new()
    }
}

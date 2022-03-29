use adw::subclass::prelude::BinImpl;
use gtk::{self, glib, prelude::*, subclass::prelude::*, CompositeTemplate};

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/add-account-row.ui")]
    pub struct AddAccountRow;

    #[glib::object_subclass]
    impl ObjectSubclass for AddAccountRow {
        const NAME: &'static str = "AddAccountRow";
        type Type = super::AddAccountRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AddAccountRow {}
    impl WidgetImpl for AddAccountRow {}
    impl BinImpl for AddAccountRow {}
}

glib::wrapper! {
    pub struct AddAccountRow(ObjectSubclass<imp::AddAccountRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl AddAccountRow {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create AddAccountRow")
    }
}

impl Default for AddAccountRow {
    fn default() -> Self {
        Self::new()
    }
}

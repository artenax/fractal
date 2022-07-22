use adw::subclass::prelude::*;
use gtk::glib;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct CustomEntry {}

    #[glib::object_subclass]
    impl ObjectSubclass for CustomEntry {
        const NAME: &'static str = "CustomEntry";
        type Type = super::CustomEntry;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("entry");
        }
    }

    impl ObjectImpl for CustomEntry {}
    impl WidgetImpl for CustomEntry {}
    impl BinImpl for CustomEntry {}
}

glib::wrapper! {
    /// Wrapper object acting as an entry.
    ///
    /// Wrap your custom widgets with CustomEntry to get stock entry styling and
    /// behavior for free.
    pub struct CustomEntry(ObjectSubclass<imp::CustomEntry>)
        @extends gtk::Widget, adw::Bin;
}

impl CustomEntry {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create CustomEntry")
    }
}

impl Default for CustomEntry {
    fn default() -> Self {
        Self::new()
    }
}

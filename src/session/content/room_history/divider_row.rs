use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-divider-row.ui")]
    pub struct DividerRow {
        #[template_child]
        pub label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DividerRow {
        const NAME: &'static str = "ContentDividerRow";
        type Type = super::DividerRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DividerRow {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecString::builder("label")
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "label" => self.obj().set_label(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "label" => self.obj().label().to_value(),
                _ => unimplemented!(),
            }
        }
    }
    impl WidgetImpl for DividerRow {}
    impl BinImpl for DividerRow {}
}

glib::wrapper! {
    pub struct DividerRow(ObjectSubclass<imp::DividerRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl DividerRow {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    pub fn with_label(label: String) -> Self {
        glib::Object::builder().property("label", &label).build()
    }

    /// The label of this divider.
    pub fn set_label(&self, label: &str) {
        self.imp().label.set_text(label);
        self.notify("label");
    }

    /// Set the label of this divider.
    pub fn label(&self) -> String {
        self.imp().label.text().as_str().to_owned()
    }
}

impl Default for DividerRow {
    fn default() -> Self {
        Self::new()
    }
}

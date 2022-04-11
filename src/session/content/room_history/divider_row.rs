use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

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
                vec![glib::ParamSpecString::new(
                    "label",
                    "Label",
                    "The label for this divider",
                    None,
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
                "label" => {
                    let label = value.get().unwrap();
                    obj.set_label(label);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "label" => obj.label().to_value(),
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
        glib::Object::new(&[]).expect("Failed to create DividerRow")
    }

    pub fn with_label(label: String) -> Self {
        glib::Object::new(&[("label", &label)]).expect("Failed to create DividerRow")
    }

    pub fn set_label(&self, label: &str) {
        self.imp().label.set_text(label);
    }

    pub fn label(&self) -> String {
        self.imp().label.text().as_str().to_owned()
    }
}

impl Default for DividerRow {
    fn default() -> Self {
        Self::new()
    }
}

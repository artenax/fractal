use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

pub mod imp {
    use std::cell::Cell;

    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-markdown-popover.ui")]
    pub struct MarkdownPopover {
        pub markdown_enabled: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MarkdownPopover {
        const NAME: &'static str = "MarkdownPopover";
        type Type = super::MarkdownPopover;
        type ParentType = gtk::Popover;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Dialog);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MarkdownPopover {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> =
                Lazy::new(|| vec![glib::ParamSpecBoolean::builder("markdown-enabled").build()]);

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "markdown-enabled" => self.obj().set_markdown_enabled(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "markdown-enabled" => self.obj().markdown_enabled().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for MarkdownPopover {}
    impl PopoverImpl for MarkdownPopover {}
}

glib::wrapper! {
    pub struct MarkdownPopover(ObjectSubclass<imp::MarkdownPopover>)
        @extends gtk::Widget, gtk::Popover, @implements gtk::Accessible;
}

impl MarkdownPopover {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// Whether outgoing messages should be interpreted as markdown.
    pub fn markdown_enabled(&self) -> bool {
        self.imp().markdown_enabled.get()
    }

    /// Set whether outgoing messages should be interpreted as markdown.
    pub fn set_markdown_enabled(&self, enabled: bool) {
        self.imp().markdown_enabled.set(enabled)
    }
}

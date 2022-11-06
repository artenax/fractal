use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};

use super::ContentFormat;

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-message-file.ui")]
    pub struct MessageFile {
        /// The filename of the file
        pub filename: RefCell<Option<String>>,
        /// Whether this file should be displayed in a compact format.
        pub compact: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MessageFile {
        const NAME: &'static str = "ContentMessageFile";
        type Type = super::MessageFile;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MessageFile {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::builder("filename")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("compact")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "filename" => obj.set_filename(value.get().unwrap()),
                "compact" => obj.set_compact(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "filename" => obj.filename().to_value(),
                "compact" => obj.compact().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for MessageFile {}

    impl BinImpl for MessageFile {}
}

glib::wrapper! {
    /// A widget displaying an interface to download or open the content of a file message.
    pub struct MessageFile(ObjectSubclass<imp::MessageFile>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl MessageFile {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// Set the filename of the file.
    pub fn set_filename(&self, filename: Option<String>) {
        let imp = self.imp();

        let name = filename.filter(|name| !name.is_empty());

        if name.as_ref() == imp.filename.borrow().as_ref() {
            return;
        }

        imp.filename.replace(name);
        self.notify("filename");
    }

    /// The filename of the file.
    pub fn filename(&self) -> Option<String> {
        self.imp().filename.borrow().to_owned()
    }

    /// Set whether this file should be displayed in a compact format.
    pub fn set_compact(&self, compact: bool) {
        if self.compact() == compact {
            return;
        }

        self.imp().compact.set(compact);
        self.notify("compact");
    }

    /// Whether this file should be displayed in a compact format.
    pub fn compact(&self) -> bool {
        self.imp().compact.get()
    }

    pub fn set_format(&self, format: ContentFormat) {
        self.set_compact(matches!(
            format,
            ContentFormat::Compact | ContentFormat::Ellipsized
        ));
    }
}

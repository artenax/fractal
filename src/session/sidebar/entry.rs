use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::session::sidebar::EntryType;

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct Entry {
        pub type_: Cell<EntryType>,
        pub icon_name: RefCell<Option<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Entry {
        const NAME: &'static str = "Entry";
        type Type = super::Entry;
    }

    impl ObjectImpl for Entry {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecEnum::new(
                        "type",
                        "Type",
                        "The type of this category",
                        EntryType::static_type(),
                        EntryType::default() as i32,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecString::new(
                        "display-name",
                        "Display Name",
                        "The display name of this Entry",
                        None,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecString::new(
                        "icon-name",
                        "Icon Name",
                        "The icon name used for this Entry",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "type" => {
                    self.type_.set(value.get().unwrap());
                }
                "icon-name" => {
                    let _ = self.icon_name.replace(value.get().unwrap());
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "type" => obj.type_().to_value(),
                "display-name" => obj.type_().to_string().to_value(),
                "icon-name" => obj.icon_name().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// A top-level row in the sidebar without children.
    ///
    /// Entry is supposed to be used in a TreeListModel, but as it does not have
    /// any children, implementing the ListModel interface is not required.
    pub struct Entry(ObjectSubclass<imp::Entry>);
}

impl Entry {
    pub fn new(type_: EntryType) -> Self {
        glib::Object::new(&[("type", &type_)]).expect("Failed to create Entry")
    }

    pub fn type_(&self) -> EntryType {
        self.imp().type_.get()
    }

    pub fn icon_name(&self) -> Option<&str> {
        match self.type_() {
            EntryType::Explore => Some("explore-symbolic"),
            EntryType::Forget => Some("user-trash-symbolic"),
        }
    }
}

use gtk::{glib, prelude::*, subclass::prelude::*};

use super::{CategoryType, EntryType, SidebarItem, SidebarItemExt, SidebarItemImpl};

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
        type ParentType = SidebarItem;
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

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
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

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "type" => obj.type_().to_value(),
                "display-name" => obj.type_().to_string().to_value(),
                "icon-name" => obj.icon_name().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl SidebarItemImpl for Entry {
        fn update_visibility(&self, for_category: CategoryType) {
            let obj = self.obj();

            match obj.type_() {
                EntryType::Explore => obj.set_visible(true),
                EntryType::Forget => obj.set_visible(for_category == CategoryType::Left),
            }
        }
    }
}

glib::wrapper! {
    /// A top-level row in the sidebar without children.
    ///
    /// Entry is supposed to be used in a TreeListModel, but as it does not have
    /// any children, implementing the ListModel interface is not required.
    pub struct Entry(ObjectSubclass<imp::Entry>) @extends SidebarItem;
}

impl Entry {
    pub fn new(type_: EntryType) -> Self {
        glib::Object::builder().property("type", &type_).build()
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

use gtk::{glib, prelude::*, subclass::prelude::*};

use super::{CategoryType, EntryType, SidebarItem, SidebarItemExt, SidebarItemImpl};

mod imp {
    use std::cell::Cell;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Entry {
        pub type_: Cell<EntryType>,
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
                    glib::ParamSpecEnum::builder::<EntryType>("type")
                        .construct_only()
                        .build(),
                    glib::ParamSpecString::builder("display-name")
                        .read_only()
                        .build(),
                    glib::ParamSpecString::builder("icon-name")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "type" => {
                    self.type_.set(value.get().unwrap());
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "type" => obj.type_().to_value(),
                "display-name" => obj.display_name().to_value(),
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

    /// The type of this entry.
    pub fn type_(&self) -> EntryType {
        self.imp().type_.get()
    }

    /// The display name of this entry.
    pub fn display_name(&self) -> String {
        self.type_().to_string()
    }

    /// The icon name used for this entry.
    pub fn icon_name(&self) -> Option<&str> {
        match self.type_() {
            EntryType::Explore => Some("explore-symbolic"),
            EntryType::Forget => Some("user-trash-symbolic"),
        }
    }
}

use gtk::{gio, glib, prelude::*, subclass::prelude::*};

use super::Member;

mod imp {
    use std::cell::{Cell, RefCell};

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug)]
    pub struct TypingList {
        /// The list of members currently typing.
        pub members: RefCell<Vec<Member>>,

        /// Whether this list is empty.
        pub is_empty: Cell<bool>,
    }

    impl Default for TypingList {
        fn default() -> Self {
            Self {
                members: Default::default(),
                is_empty: Cell::new(true),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TypingList {
        const NAME: &'static str = "TypingList";
        type Type = super::TypingList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for TypingList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecBoolean::builder("is-empty")
                    .default_value(true)
                    .read_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "is-empty" => self.obj().is_empty().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for TypingList {
        fn item_type(&self) -> glib::Type {
            Member::static_type()
        }

        fn n_items(&self) -> u32 {
            self.members.borrow().len() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.members
                .borrow()
                .get(position as usize)
                .map(|member| member.clone().upcast())
        }
    }
}

glib::wrapper! {
    /// List of members that are currently typing.
    pub struct TypingList(ObjectSubclass<imp::TypingList>)
        @implements gio::ListModel;
}

impl TypingList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn members(&self) -> Vec<Member> {
        self.imp().members.borrow().clone()
    }

    /// Set whether the list is empty.
    fn set_is_empty(&self, empty: bool) {
        self.imp().is_empty.set(empty);
        self.notify("is-empty");
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.imp().is_empty.get()
    }

    pub fn update(&self, new_members: Vec<Member>) {
        let prev_is_empty = self.is_empty();

        if new_members.is_empty() {
            if !prev_is_empty {
                self.set_is_empty(true);
            }

            return;
        }

        let (removed, added) = {
            let mut members = self.imp().members.borrow_mut();
            let removed = members.len() as u32;
            let added = new_members.len() as u32;
            *members = new_members;
            (removed, added)
        };

        self.items_changed(0, removed, added);

        if prev_is_empty {
            self.set_is_empty(false);
        }
    }
}

impl Default for TypingList {
    fn default() -> Self {
        Self::new()
    }
}

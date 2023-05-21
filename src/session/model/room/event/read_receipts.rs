use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use indexmap::IndexMap;
use ruma::{events::receipt::Receipt, OwnedUserId};

use crate::{
    prelude::*,
    session::model::{Member, Room},
};

mod imp {
    use std::cell::RefCell;

    use glib::WeakRef;
    use indexmap::IndexSet;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ReadReceipts {
        /// The room containing the parent `Event`.
        pub room: WeakRef<Room>,

        /// The list of members with a read receipt.
        pub members: RefCell<IndexSet<Member>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ReadReceipts {
        const NAME: &'static str = "ReadReceipts";
        type Type = super::ReadReceipts;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for ReadReceipts {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecBoolean::builder("is-empty")
                    .read_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "is-empty" => obj.is_empty().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for ReadReceipts {
        fn item_type(&self) -> glib::Type {
            Member::static_type()
        }

        fn n_items(&self) -> u32 {
            self.members.borrow().len() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.members
                .borrow()
                .get_index(position as usize)
                .map(|member| member.clone().upcast())
        }
    }
}

glib::wrapper! {
    /// List of all read receipts for an event. Implements `ListModel`.
    ///
    /// Receipts are sorted in "insertion order".
    pub struct ReadReceipts(ObjectSubclass<imp::ReadReceipts>)
        @implements gio::ListModel;
}

impl ReadReceipts {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The room containing the parent event.
    pub fn room(&self) -> Room {
        self.imp().room.upgrade().unwrap()
    }

    /// Set the room containing the parent event.
    pub fn set_room(&self, room: &Room) {
        self.imp().room.set(Some(room));
    }

    /// Whether this is empty.
    pub fn is_empty(&self) -> bool {
        self.imp().members.borrow().is_empty()
    }

    /// Update the read receipts list with the given receipts.
    pub fn update(&self, new_read_receipts: IndexMap<OwnedUserId, Receipt>) {
        let was_empty = self.is_empty();
        let members = &self.imp().members;

        {
            let old_members = members.borrow();

            if old_members.len() == new_read_receipts.len()
                && new_read_receipts
                    .keys()
                    .zip(old_members.iter())
                    .all(|(new_user_id, old_member)| *new_user_id == old_member.user_id())
            {
                return;
            }
        }

        let mut members = members.borrow_mut();
        let room = self.room();
        let prev_len = members.len();
        let new_len = new_read_receipts.len();

        *members = new_read_receipts
            .into_iter()
            .map(|(user_id, _)| room.members().member_by_id(user_id))
            .collect();

        // We can't have the borrow active when items_changed is emitted because that
        // will probably cause reads of the members field.
        std::mem::drop(members);

        self.items_changed(0, prev_len as u32, new_len as u32);

        if was_empty != self.is_empty() {
            self.notify("is-empty");
        }
    }

    /// Removes all receipts.
    pub fn clear(&self) {
        let len = {
            let mut members = self.imp().members.borrow_mut();
            let len = members.len();
            members.clear();

            len
        };

        self.items_changed(0, len as u32, 0);

        self.notify("is-empty");
    }
}

impl Default for ReadReceipts {
    fn default() -> Self {
        Self::new()
    }
}

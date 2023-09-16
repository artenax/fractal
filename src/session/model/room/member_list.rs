use gtk::{
    gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use indexmap::{map::Entry, IndexMap};
use matrix_sdk::{
    ruma::{
        events::{room::member::RoomMemberEventContent, OriginalSyncStateEvent},
        OwnedUserId, UserId,
    },
    RoomMemberships,
};
use tracing::error;

use super::{Member, Membership, Room};
use crate::{spawn, spawn_tokio};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::object::WeakRef;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct MemberList {
        /// The list of known members.
        pub members: RefCell<IndexMap<OwnedUserId, Member>>,
        /// The room these members belong to.
        pub room: WeakRef<Room>,
        /// Whether all the members are loaded.
        pub is_loaded: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MemberList {
        const NAME: &'static str = "MemberList";
        type Type = super::MemberList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for MemberList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Room>("room")
                        .construct_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("is-loaded")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "room" => self.obj().set_room(&value.get().ok().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "room" => obj.room().to_value(),
                "is-loaded" => obj.is_loaded().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for MemberList {
        fn item_type(&self) -> glib::Type {
            Member::static_type()
        }
        fn n_items(&self) -> u32 {
            self.members.borrow().len() as u32
        }
        fn item(&self, position: u32) -> Option<glib::Object> {
            let members = self.members.borrow();

            members
                .get_index(position as usize)
                .map(|(_user_id, member)| member.clone().upcast())
        }
    }
}

glib::wrapper! {
    /// List of all Members in a room. Implements ListModel.
    ///
    /// Members are sorted in "insertion order", not anything useful.
    pub struct MemberList(ObjectSubclass<imp::MemberList>)
        @implements gio::ListModel;
}

impl MemberList {
    pub fn new(room: &Room) -> Self {
        glib::Object::builder().property("room", room).build()
    }

    /// The room containing these members.
    pub fn room(&self) -> Room {
        self.imp().room.upgrade().unwrap()
    }

    fn set_room(&self, room: &Room) {
        self.imp().room.set(Some(room));
        self.notify("room");

        spawn!(
            glib::Priority::LOW,
            clone!(@weak self as obj => async move {
                obj.load().await;
            })
        );
    }

    /// Whether this list is fully loaded.
    pub fn is_loaded(&self) -> bool {
        self.imp().is_loaded.get()
    }

    /// Set whether this list is fully loaded.
    fn set_is_loaded(&self, is_loaded: bool) {
        if self.is_loaded() == is_loaded {
            return;
        }

        self.imp().is_loaded.set(is_loaded);
        self.notify("is-loaded");
    }

    pub fn reload(&self) {
        self.set_is_loaded(false);

        spawn!(clone!(@weak self as obj => async move {
            obj.load().await;
        }));
    }

    /// Load this list.
    async fn load(&self) {
        if self.is_loaded() {
            return;
        }

        self.set_is_loaded(true);

        let room = self.room();
        let matrix_room = room.matrix_room();
        let handle = spawn_tokio!(async move {
            let mut memberships = RoomMemberships::all();
            memberships.remove(RoomMemberships::LEAVE);

            matrix_room.members(memberships).await
        });

        // FIXME: We should retry to load the room members if the request failed
        match handle.await.unwrap() {
            Ok(members) => {
                // Add all members needed to display room events.
                self.update_from_room_members(&members);
                return;
            }
            Err(error) => {
                self.set_is_loaded(false);
                error!(%error, "Failed to load room members from server");
            }
        }

        // Use the members already received by the SDK as a fallback.
        let matrix_room = room.matrix_room();
        let handle = spawn_tokio!(async move {
            let mut memberships = RoomMemberships::all();
            memberships.remove(RoomMemberships::LEAVE);

            matrix_room.members_no_sync(memberships).await
        });

        match handle.await.unwrap() {
            Ok(members) => {
                // Add all members needed to display room events.
                self.update_from_room_members(&members);
            }
            Err(error) => {
                error!(%error, "Failed to load room members from store");
            }
        }
    }

    /// Updates members with the given RoomMember values.
    ///
    /// If some of the values do not correspond to existing members, new members
    /// are created.
    fn update_from_room_members(&self, new_members: &[matrix_sdk::room::RoomMember]) {
        let imp = self.imp();
        let mut members = imp.members.borrow_mut();
        let prev_len = members.len();
        for member in new_members {
            if let Entry::Vacant(entry) = members.entry(member.user_id().into()) {
                entry.insert(Member::new(&self.room(), member.user_id()));
            }
        }
        let num_members_added = members.len().saturating_sub(prev_len);

        // We can't have the mut borrow active when members are updated or items_changed
        // is emitted because that will probably cause reads of the members
        // field.
        std::mem::drop(members);

        {
            let members = imp.members.borrow();
            for room_member in new_members {
                if let Some(member) = members.get(room_member.user_id()) {
                    member.update_from_room_member(room_member);
                }
            }
        }

        if num_members_added > 0 {
            // IndexMap preserves insertion order, so all the new items will be at the end.
            self.items_changed(prev_len as u32, 0, num_members_added as u32);
        }
    }

    /// Returns the member with the given ID.
    ///
    /// Creates a new member first if there is no member with the given ID.
    pub fn get_or_create(&self, user_id: OwnedUserId) -> Member {
        let mut members = self.imp().members.borrow_mut();
        let mut was_member_added = false;
        let prev_len = members.len();
        let member = members
            .entry(user_id)
            .or_insert_with_key(|user_id| {
                was_member_added = true;
                Member::new(&self.room(), user_id)
            })
            .clone();

        // We can't have the borrow active when items_changed is emitted because that
        // will probably cause reads of the members field.
        std::mem::drop(members);
        if was_member_added {
            // IndexMap preserves insertion order so the new member will be at the end.
            self.items_changed(prev_len as u32, 0, 1);
        }

        member
    }

    /// Updates a room member based on the room member state event.
    ///
    /// Creates a new member first if there is no member matching the given
    /// event.
    pub(super) fn update_member_for_member_event(
        &self,
        event: &OriginalSyncStateEvent<RoomMemberEventContent>,
    ) {
        self.get_or_create(event.state_key.to_owned())
            .update_from_member_event(event);
    }

    /// Returns the Membership of a given UserId.
    ///
    /// If the user has no Membership, Membership::Leave will be returned
    pub fn get_membership(&self, user_id: &UserId) -> Membership {
        self.imp()
            .members
            .borrow()
            .get(user_id)
            .map_or_else(|| Membership::Leave, |member| member.membership())
    }
}

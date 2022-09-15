use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use indexmap::{map::Entry, IndexMap};
use matrix_sdk::ruma::{
    events::{room::member::RoomMemberEventContent, OriginalSyncStateEvent},
    OwnedUserId, UserId,
};

use crate::session::room::{Member, Membership, Room};

mod imp {
    use std::cell::RefCell;

    use glib::object::WeakRef;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct MemberList {
        pub members: RefCell<IndexMap<OwnedUserId, Member>>,
        pub room: OnceCell<WeakRef<Room>>,
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
                vec![glib::ParamSpecObject::new(
                    "room",
                    "Room",
                    "The associated room",
                    Room::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                )]
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
                "room" => self
                    .room
                    .set(value.get::<Room>().unwrap().downgrade())
                    .unwrap(),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room" => obj.room().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for MemberList {
        fn item_type(&self, _list_model: &Self::Type) -> glib::Type {
            Member::static_type()
        }
        fn n_items(&self, _list_model: &Self::Type) -> u32 {
            self.members.borrow().len() as u32
        }
        fn item(&self, _list_model: &Self::Type, position: u32) -> Option<glib::Object> {
            let members = self.members.borrow();

            members
                .get_index(position as usize)
                .map(|(_user_id, member)| member.clone().upcast::<glib::Object>())
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
        glib::Object::new(&[("room", room)]).expect("Failed to create MemberList")
    }

    pub fn room(&self) -> Room {
        self.imp().room.get().unwrap().upgrade().unwrap()
    }

    /// Updates members with the given RoomMember values.
    ///
    /// If some of the values do not correspond to existing members, new members
    /// are created.
    pub fn update_from_room_members(&self, new_members: &[matrix_sdk::room::RoomMember]) {
        let priv_ = self.imp();
        let mut members = priv_.members.borrow_mut();
        let prev_len = members.len();
        for member in new_members {
            if let Entry::Vacant(entry) = members.entry(member.user_id().into()) {
                entry.insert(Member::new(&self.room(), member.user_id()));
            }
        }
        let num_members_added = members.len().saturating_sub(prev_len);

        // We can't have the borrow active when members are updated or items_changed is
        // emitted because that will probably cause reads of the members field.
        std::mem::drop(members);

        {
            let members = priv_.members.borrow();
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
    pub fn member_by_id(&self, user_id: OwnedUserId) -> Member {
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
    pub fn update_member_for_member_event(
        &self,
        event: &OriginalSyncStateEvent<RoomMemberEventContent>,
    ) {
        self.member_by_id(event.state_key.to_owned())
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

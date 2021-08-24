use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use matrix_sdk::ruma::events::room::member::MemberEventContent;
use matrix_sdk::ruma::events::{StrippedStateEvent, SyncStateEvent};
use matrix_sdk::ruma::identifiers::{MxcUri, UserId};
use matrix_sdk::RoomMember;

use crate::prelude::*;
use crate::session::{Room, User};

mod imp {
    use super::*;
    use once_cell::sync::Lazy;
    use std::cell::Cell;

    #[derive(Debug, Default)]
    pub struct Member {
        pub power_level: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Member {
        const NAME: &'static str = "Member";
        type Type = super::Member;
        type ParentType = User;
    }

    impl ObjectImpl for Member {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpec::new_uint(
                    "power-level",
                    "Power level",
                    "Power level of the member in its room.",
                    0,
                    100,
                    0,
                    glib::ParamFlags::READABLE | glib::ParamFlags::EXPLICIT_NOTIFY,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "power-level" => obj.power_level().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// A User in the context of a given room.
    pub struct Member(ObjectSubclass<imp::Member>) @extends User;
}

impl Member {
    pub fn new(room: &Room, user_id: &UserId) -> Self {
        let session = room.session();
        glib::Object::new(&[("session", &session), ("user-id", &user_id.as_str())])
            .expect("Failed to create Member")
    }

    pub fn power_level(&self) -> u32 {
        let priv_ = imp::Member::from_instance(self);
        priv_.power_level.get()
    }

    fn set_power_level(&self, power_level: u32) {
        if self.power_level() == power_level {
            return;
        }
        let priv_ = imp::Member::from_instance(self);
        priv_.power_level.replace(power_level);
        self.notify("power-level");
    }

    /// Update the user based on the the room member state event
    pub fn update_from_room_member(&self, member: &RoomMember) {
        if member.user_id() != self.user_id() {
            log::error!("Tried Member update from RoomMember with wrong user ID.");
            return;
        };

        self.set_display_name(member.display_name().map(String::from));
        self.avatar().set_url(member.avatar_url().cloned());
        self.set_power_level(member.power_level().clamp(0, 100) as u32);
    }

    /// Update the user based on the the room member state event
    pub fn update_from_member_event(&self, event: &impl MemberEvent) {
        if event.sender() != self.user_id() {
            log::error!("Tried Member update from MemberEvent with wrong user ID.");
            return;
        };

        self.set_display_name(event.display_name());
        self.avatar().set_url(event.avatar_url());
    }
}

pub trait MemberEvent {
    fn sender(&self) -> &UserId;
    fn content(&self) -> &MemberEventContent;

    fn avatar_url(&self) -> Option<MxcUri> {
        self.content().avatar_url.clone()
    }

    fn display_name(&self) -> Option<String> {
        match &self.content().displayname {
            Some(display_name) => Some(display_name.clone()),
            None => self
                .content()
                .third_party_invite
                .as_ref()
                .map(|i| i.display_name.clone()),
        }
    }
}

impl MemberEvent for SyncStateEvent<MemberEventContent> {
    fn sender(&self) -> &UserId {
        &self.sender
    }
    fn content(&self) -> &MemberEventContent {
        &self.content
    }
}
impl MemberEvent for StrippedStateEvent<MemberEventContent> {
    fn sender(&self) -> &UserId {
        &self.sender
    }
    fn content(&self) -> &MemberEventContent {
        &self.content
    }
}
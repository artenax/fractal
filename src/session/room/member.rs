use gettextrs::gettext;
use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk::{
    room::RoomMember,
    ruma::{
        events::{
            room::member::{MembershipState, RoomMemberEventContent},
            OriginalSyncStateEvent, StrippedStateEvent,
        },
        OwnedMxcUri, UserId,
    },
};

use crate::{
    prelude::*,
    session::{
        room::{
            power_levels::{PowerLevel, POWER_LEVEL_MAX, POWER_LEVEL_MIN},
            MemberRole,
        },
        Room, User,
    },
};

#[derive(Debug, Default, Hash, Eq, PartialEq, Clone, Copy, glib::Enum, glib::Variant)]
#[variant_enum(repr)]
#[repr(u32)]
#[enum_type(name = "Membership")]
pub enum Membership {
    #[default]
    Leave = 0,
    Join = 1,
    Invite = 2,
    Ban = 3,
    Knock = 4,
    Custom = 5,
}

impl std::fmt::Display for Membership {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Membership::Leave => gettext("Left"),
            Membership::Join => gettext("Joined"),
            Membership::Invite => gettext("Invited"),
            Membership::Ban => gettext("Banned"),
            Membership::Knock => gettext("Knocked"),
            Membership::Custom => gettext("Custom"),
        };
        f.write_str(&label)
    }
}

impl From<&MembershipState> for Membership {
    fn from(state: &MembershipState) -> Self {
        match state {
            MembershipState::Leave => Membership::Leave,
            MembershipState::Join => Membership::Join,
            MembershipState::Invite => Membership::Invite,
            MembershipState::Ban => Membership::Ban,
            MembershipState::Knock => Membership::Knock,
            _ => Membership::Custom,
        }
    }
}

impl From<MembershipState> for Membership {
    fn from(state: MembershipState) -> Self {
        Membership::from(&state)
    }
}

mod imp {
    use std::cell::Cell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Member {
        pub power_level: Cell<PowerLevel>,
        pub membership: Cell<Membership>,
        /// The timestamp of the latest activity of this member.
        pub latest_activity: Cell<u64>,
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
                vec![
                    glib::ParamSpecInt64::builder("power-level")
                        .minimum(POWER_LEVEL_MIN)
                        .maximum(POWER_LEVEL_MAX)
                        .read_only()
                        .build(),
                    glib::ParamSpecEnum::builder::<Membership>("membership")
                        .read_only()
                        .build(),
                    glib::ParamSpecUInt64::builder("latest-activity")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "latest-activity" => self.obj().set_latest_activity(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "power-level" => obj.power_level().to_value(),
                "membership" => obj.membership().to_value(),
                "latest-activity" => obj.latest_activity().to_value(),
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
        glib::Object::builder()
            .property("session", &session)
            .property("user-id", user_id.as_str())
            .build()
    }

    /// The power level of the member.
    pub fn power_level(&self) -> PowerLevel {
        self.imp().power_level.get()
    }

    /// Set the power level of the member.
    fn set_power_level(&self, power_level: PowerLevel) {
        if self.power_level() == power_level {
            return;
        }
        self.imp().power_level.replace(power_level);
        self.notify("power-level");
    }

    pub fn role(&self) -> MemberRole {
        self.power_level().into()
    }

    pub fn is_admin(&self) -> bool {
        self.role().is_admin()
    }

    pub fn is_mod(&self) -> bool {
        self.role().is_mod()
    }

    pub fn is_peasant(&self) -> bool {
        self.role().is_peasant()
    }

    /// This member's membership state.
    pub fn membership(&self) -> Membership {
        let imp = self.imp();
        imp.membership.get()
    }

    /// Set this member's membership state.
    fn set_membership(&self, membership: Membership) {
        if self.membership() == membership {
            return;
        }
        let imp = self.imp();
        imp.membership.replace(membership);
        self.notify("membership");
    }

    /// The timestamp of the latest activity of this member.
    pub fn latest_activity(&self) -> u64 {
        self.imp().latest_activity.get()
    }

    /// Set the timestamp of the latest activity of this member.
    pub fn set_latest_activity(&self, activity: u64) {
        if self.latest_activity() >= activity {
            return;
        }

        self.imp().latest_activity.set(activity);
        self.notify("latest-activity");
    }

    /// Update the user based on the room member.
    pub fn update_from_room_member(&self, member: &RoomMember) {
        if member.user_id() != &*self.user_id() {
            log::error!("Tried Member update from RoomMember with wrong user ID.");
            return;
        };

        self.set_display_name(member.display_name().map(String::from));
        self.avatar_data()
            .set_url(member.avatar_url().map(std::borrow::ToOwned::to_owned));
        self.set_power_level(member.power_level());
        self.set_membership(member.membership().into());
    }

    /// Update the user based on the room member state event
    pub fn update_from_member_event(&self, event: &impl MemberEvent) {
        if event.state_key() != &*self.user_id() {
            log::error!("Tried Member update from MemberEvent with wrong user ID.");
            return;
        };

        self.set_display_name(event.display_name());
        self.avatar_data().set_url(event.avatar_url());
        self.set_membership((&event.content().membership).into());

        let session = self.session();
        if let Some(user) = session.user() {
            if user.user_id() == self.user_id() {
                session.update_user_profile();
            }
        }
    }
}

pub trait MemberEvent {
    fn sender(&self) -> &UserId;
    fn content(&self) -> &RoomMemberEventContent;
    fn state_key(&self) -> &UserId;

    fn avatar_url(&self) -> Option<OwnedMxcUri> {
        self.content().avatar_url.to_owned()
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

impl MemberEvent for OriginalSyncStateEvent<RoomMemberEventContent> {
    fn sender(&self) -> &UserId {
        &self.sender
    }
    fn content(&self) -> &RoomMemberEventContent {
        &self.content
    }
    fn state_key(&self) -> &UserId {
        &self.state_key
    }
}
impl MemberEvent for StrippedStateEvent<RoomMemberEventContent> {
    fn sender(&self) -> &UserId {
        &self.sender
    }
    fn content(&self) -> &RoomMemberEventContent {
        &self.content
    }
    fn state_key(&self) -> &UserId {
        &self.state_key
    }
}

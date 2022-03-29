use gtk::{glib, glib::closure, prelude::*, subclass::prelude::*};
use matrix_sdk::ruma::events::{
    room::power_levels::RoomPowerLevelsEventContent, RoomEventType, SyncStateEvent,
};

use crate::session::room::Member;

#[derive(Clone, Debug, Default, glib::Boxed)]
#[boxed_type(name = "BoxedPowerLevelsEventContent")]
pub struct BoxedPowerLevelsEventContent(RoomPowerLevelsEventContent);

/// Power level of a user.
///
/// Is usually in the range (0..=100), but can be any JS integer.
pub type PowerLevel = i64;
// Same value as MAX_SAFE_INT from js_int.
pub const POWER_LEVEL_MAX: i64 = 0x001F_FFFF_FFFF_FFFF;
pub const POWER_LEVEL_MIN: i64 = -POWER_LEVEL_MAX;

mod imp {
    use std::cell::RefCell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct PowerLevels {
        pub content: RefCell<BoxedPowerLevelsEventContent>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PowerLevels {
        const NAME: &'static str = "PowerLevels";
        type Type = super::PowerLevels;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for PowerLevels {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecBoxed::new(
                    "power-levels",
                    "Power levels",
                    "Ruma struct containing all power level information of a room",
                    BoxedPowerLevelsEventContent::static_type(),
                    glib::ParamFlags::READABLE,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "power-levels" => obj.power_levels().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct PowerLevels(ObjectSubclass<imp::PowerLevels>);
}

impl PowerLevels {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create PowerLevels")
    }

    pub fn power_levels(&self) -> BoxedPowerLevelsEventContent {
        self.imp().content.borrow().clone()
    }

    /// Returns the power level minimally required to perform the given action.
    pub fn min_level_for_room_action(&self, room_action: &RoomAction) -> PowerLevel {
        let content = self.imp().content.borrow();
        min_level_for_room_action(&content.0, room_action)
    }

    /// Creates an expression that is true when the user is allowed the given
    /// action.
    pub fn new_allowed_expr(
        &self,
        member: &Member,
        room_action: RoomAction,
    ) -> gtk::ClosureExpression {
        gtk::ClosureExpression::new::<bool, _, _>(
            &[
                member.property_expression("power-level"),
                self.property_expression("power-levels"),
            ],
            closure!(|_: Option<glib::Object>,
                      power_level: PowerLevel,
                      content: BoxedPowerLevelsEventContent| {
                power_level >= min_level_for_room_action(&content.0, &room_action)
            }),
        )
    }

    /// Updates the power levels from the given event.
    pub fn update_from_event(&self, event: SyncStateEvent<RoomPowerLevelsEventContent>) {
        let content = BoxedPowerLevelsEventContent(event.content);
        self.imp().content.replace(content);
        self.notify("power-levels");
    }
}

impl Default for PowerLevels {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the power level minimally required to perform the given action.
fn min_level_for_room_action(
    content: &RoomPowerLevelsEventContent,
    room_action: &RoomAction,
) -> PowerLevel {
    match room_action {
        RoomAction::Ban => content.ban,
        RoomAction::Invite => content.invite,
        RoomAction::Kick => content.kick,
        RoomAction::Redact => content.redact,
        RoomAction::RoomNotification => content.notifications.room,
        RoomAction::StateEvent(event_type) => *content
            .events
            .get(event_type)
            .unwrap_or(&content.state_default),
        RoomAction::MessageLikeEvent(event_type) => *content
            .events
            .get(event_type)
            .unwrap_or(&content.events_default),
    }
    .into()
}

/// Actions that require different power levels to perform them.
pub enum RoomAction {
    Ban,
    Invite,
    Kick,
    Redact,
    RoomNotification,
    StateEvent(RoomEventType),
    MessageLikeEvent(RoomEventType),
}

use gtk::{glib, glib::closure_local, prelude::*, subclass::prelude::*};
use ruma::{
    events::{
        room::power_levels::{PowerLevelAction, RoomPowerLevels, RoomPowerLevelsEventContent},
        OriginalSyncStateEvent,
    },
    OwnedUserId, UserId,
};

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
    }

    impl ObjectImpl for PowerLevels {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoxed::builder::<BoxedPowerLevelsEventContent>("power-levels")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "power-levels" => self.obj().power_levels().to_value(),
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
        glib::Object::new()
    }

    /// The source of the power levels information.
    pub fn power_levels(&self) -> BoxedPowerLevelsEventContent {
        self.imp().content.borrow().clone()
    }

    /// Returns whether the member with the given user ID is allowed to do the
    /// given action.
    pub fn member_is_allowed_to(&self, user_id: &UserId, room_action: PowerLevelAction) -> bool {
        let content = self.imp().content.borrow().0.clone();
        RoomPowerLevels::from(content).user_can_do(user_id, room_action)
    }

    /// Creates an expression that is true when the member with the given user
    /// ID is allowed to do the given action.
    pub fn member_is_allowed_to_expr(
        &self,
        user_id: OwnedUserId,
        room_action: PowerLevelAction,
    ) -> gtk::ClosureExpression {
        gtk::ClosureExpression::new::<bool>(
            &[self.property_expression("power-levels")],
            closure_local!(
                move |_: Option<glib::Object>, content: BoxedPowerLevelsEventContent| {
                    RoomPowerLevels::from(content.0).user_can_do(&user_id, room_action.clone())
                }
            ),
        )
    }

    /// Updates the power levels from the given event.
    pub fn update_from_event(&self, event: OriginalSyncStateEvent<RoomPowerLevelsEventContent>) {
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

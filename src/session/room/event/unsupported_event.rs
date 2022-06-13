use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk::{deserialized_responses::SyncRoomEvent, ruma::events::RoomEventType};

use super::{BoxedSyncRoomEvent, Event, EventImpl};
use crate::session::room::{
    timeline::{TimelineItem, TimelineItemImpl},
    Room,
};

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct UnsupportedEvent {}

    #[glib::object_subclass]
    impl ObjectSubclass for UnsupportedEvent {
        const NAME: &'static str = "RoomUnsupportedEvent";
        type Type = super::UnsupportedEvent;
        type ParentType = Event;
    }

    impl ObjectImpl for UnsupportedEvent {}

    impl TimelineItemImpl for UnsupportedEvent {}

    impl EventImpl for UnsupportedEvent {}
}

glib::wrapper! {
    /// GObject representation of an unsupported Matrix room event.
    pub struct UnsupportedEvent(ObjectSubclass<imp::UnsupportedEvent>) @extends TimelineItem, Event;
}

impl UnsupportedEvent {
    /// Construct an `UnsupportedEvent` from the given pure event and room.
    pub fn new(pure_event: SyncRoomEvent, room: &Room) -> Self {
        let pure_event = BoxedSyncRoomEvent(pure_event);
        glib::Object::new(&[("pure-event", &pure_event), ("room", room)])
            .expect("Failed to create UnsupportedEvent")
    }

    /// The type of this `UnsupportedEvent`, if the field is found.
    pub fn event_type(&self) -> Option<RoomEventType> {
        self.upcast_ref::<Event>()
            .imp()
            .pure_event
            .borrow()
            .as_ref()
            .unwrap()
            .event
            .get_field::<RoomEventType>("type")
            .ok()
            .flatten()
    }
}

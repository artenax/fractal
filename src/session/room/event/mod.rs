use gtk::{glib, prelude::*, subclass::prelude::*};
use log::warn;
use matrix_sdk::{
    deserialized_responses::SyncRoomEvent,
    ruma::{MilliSecondsSinceUnixEpoch, OwnedEventId, OwnedUserId},
};

use super::{
    timeline::{TimelineItem, TimelineItemImpl},
    Member, Room,
};

mod supported_event;
mod unsupported_event;

pub use supported_event::SupportedEvent;
pub use unsupported_event::UnsupportedEvent;

#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "BoxedSyncRoomEvent")]
pub struct BoxedSyncRoomEvent(SyncRoomEvent);

mod imp {
    use std::cell::RefCell;

    use glib::{object::WeakRef, Class};
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[repr(C)]
    pub struct EventClass {
        pub parent_class: Class<TimelineItem>,
        pub source: fn(&super::Event) -> String,
        pub event_id: fn(&super::Event) -> Option<OwnedEventId>,
        pub sender_id: fn(&super::Event) -> Option<OwnedUserId>,
        pub origin_server_ts: fn(&super::Event) -> Option<MilliSecondsSinceUnixEpoch>,
    }

    unsafe impl ClassStruct for EventClass {
        type Type = Event;
    }

    pub(super) fn event_source(this: &super::Event) -> String {
        let klass = this.class();
        (klass.as_ref().source)(this)
    }

    pub(super) fn event_event_id(this: &super::Event) -> Option<OwnedEventId> {
        let klass = this.class();
        (klass.as_ref().event_id)(this)
    }

    pub(super) fn event_sender_id(this: &super::Event) -> Option<OwnedUserId> {
        let klass = this.class();
        (klass.as_ref().sender_id)(this)
    }

    pub(super) fn event_origin_server_ts(
        this: &super::Event,
    ) -> Option<MilliSecondsSinceUnixEpoch> {
        let klass = this.class();
        (klass.as_ref().origin_server_ts)(this)
    }

    #[derive(Debug, Default)]
    pub struct Event {
        /// The SDK event containing encryption information and the serialized
        /// event as `Raw`.
        pub pure_event: RefCell<Option<SyncRoomEvent>>,

        /// The room containing this `Event`.
        pub room: OnceCell<WeakRef<Room>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Event {
        const NAME: &'static str = "RoomEvent";
        const ABSTRACT: bool = true;
        type Type = super::Event;
        type ParentType = TimelineItem;
        type Class = EventClass;
    }

    impl ObjectImpl for Event {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoxed::new(
                        "pure-event",
                        "Pure Event",
                        "The pure Matrix event of this Event",
                        BoxedSyncRoomEvent::static_type(),
                        glib::ParamFlags::WRITABLE,
                    ),
                    glib::ParamSpecString::new(
                        "source",
                        "Source",
                        "The JSON source of this Event",
                        None,
                        glib::ParamFlags::READABLE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecObject::new(
                        "room",
                        "Room",
                        "The room containing this Event",
                        Room::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecString::new(
                        "time",
                        "Time",
                        "The locally formatted time of this Matrix event",
                        None,
                        glib::ParamFlags::READABLE,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "pure-event" => {
                    let event = value.get::<BoxedSyncRoomEvent>().unwrap();
                    obj.set_pure_event(event.0);
                }
                "room" => {
                    self.room
                        .set(value.get::<Room>().unwrap().downgrade())
                        .unwrap();
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "source" => obj.source().to_value(),
                "room" => obj.room().to_value(),
                "time" => obj.time().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl TimelineItemImpl for Event {
        fn event_sender(&self, obj: &Self::Type) -> Option<Member> {
            Some(obj.room().members().member_by_id(obj.sender_id()?))
        }

        fn selectable(&self, _obj: &Self::Type) -> bool {
            true
        }
    }
}

glib::wrapper! {
    /// GObject representation of a Matrix room event.
    pub struct Event(ObjectSubclass<imp::Event>) @extends TimelineItem;
}

impl Event {
    /// Create an `Event` with the given pure SDK event and room.
    ///
    /// Constructs the proper subtype according to the event.
    pub fn new(pure_event: SyncRoomEvent, room: &Room) -> Self {
        SupportedEvent::try_from_event(pure_event.clone(), room)
            .map(|event| event.upcast())
            .unwrap_or_else(|_| {
                warn!("Failed to deserialize event: {:?}", pure_event);
                UnsupportedEvent::new(pure_event, room).upcast()
            })
    }
}

/// Public trait containing implemented methods for everything that derives from
/// `Event`.
///
/// To override the behavior of these methods, override the corresponding method
/// of `EventImpl`.
pub trait EventExt: 'static {
    /// The `Room` where this `Event` was sent.
    fn room(&self) -> Room;

    /// The pure SDK event of this `Event`.
    fn pure_event(&self) -> SyncRoomEvent;

    /// Set the pure SDK event of this `Event`.
    fn set_pure_event(&self, pure_event: SyncRoomEvent);

    /// The source JSON of this `Event`.
    fn original_source(&self) -> String;

    /// The source JSON displayed for this `Event`.
    ///
    /// Defaults to the `original_source`.
    fn source(&self) -> String;

    /// The event ID of this `Event`, if it was found.
    fn event_id(&self) -> Option<OwnedEventId>;

    /// The user ID of the sender of this `Event`, if it was found.
    fn sender_id(&self) -> Option<OwnedUserId>;

    /// The timestamp on the origin server when this `Event` was sent as
    /// `MilliSecondsSinceUnixEpoch`, if it was found.
    fn origin_server_ts(&self) -> Option<MilliSecondsSinceUnixEpoch>;

    /// The timestamp on the origin server when this `Event` was sent as
    /// `glib::DateTime`.
    ///
    /// This is computed from the `origin_server_ts`.
    fn timestamp(&self) -> Option<glib::DateTime> {
        glib::DateTime::from_unix_utc(self.origin_server_ts()?.as_secs().into())
            .and_then(|t| t.to_local())
            .ok()
    }

    /// The formatted time when this `Event` was sent.
    ///
    /// This is computed from the `origin_server_ts`.
    fn time(&self) -> Option<String> {
        let datetime = self.timestamp()?;

        // FIXME Is there a cleaner to find out if we should use 24h format?
        let local_time = datetime.format("%X").unwrap().as_str().to_ascii_lowercase();

        let time = if local_time.ends_with("am") || local_time.ends_with("pm") {
            // Use 12h time format (AM/PM)
            datetime.format("%l∶%M %p").unwrap().to_string()
        } else {
            // Use 24 time format
            datetime.format("%R").unwrap().to_string()
        };
        Some(time)
    }

    fn connect_pure_event_notify<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId;
}

impl<O: IsA<Event>> EventExt for O {
    fn room(&self) -> Room {
        self.upcast_ref()
            .imp()
            .room
            .get()
            .unwrap()
            .upgrade()
            .unwrap()
    }

    fn pure_event(&self) -> SyncRoomEvent {
        self.upcast_ref().imp().pure_event.borrow().clone().unwrap()
    }

    fn set_pure_event(&self, pure_event: SyncRoomEvent) {
        let priv_ = self.upcast_ref().imp();
        priv_.pure_event.replace(Some(pure_event));

        self.notify("pure-event");
        self.notify("source");
    }

    fn original_source(&self) -> String {
        let pure_event = self.upcast_ref().imp().pure_event.borrow();
        let raw = pure_event.as_ref().unwrap().event.json().get();

        // We have to convert it to a Value, because a RawValue cannot be
        // pretty-printed.
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(raw) {
            serde_json::to_string_pretty(&json).unwrap()
        } else {
            raw.to_owned()
        }
    }

    fn source(&self) -> String {
        imp::event_source(self.upcast_ref())
    }

    fn event_id(&self) -> Option<OwnedEventId> {
        imp::event_event_id(self.upcast_ref())
    }

    fn sender_id(&self) -> Option<OwnedUserId> {
        imp::event_sender_id(self.upcast_ref())
    }

    fn origin_server_ts(&self) -> Option<MilliSecondsSinceUnixEpoch> {
        imp::event_origin_server_ts(self.upcast_ref())
    }

    fn connect_pure_event_notify<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_notify_local(Some("pure-event"), move |this, _| {
            f(this);
        })
    }
}

/// Public trait that must be implemented for everything that derives from
/// `Event`.
///
/// Overriding a method from this trait overrides also its behavior in
/// `EventExt`.
pub trait EventImpl: ObjectImpl {
    fn source(&self, obj: &Self::Type) -> String {
        obj.dynamic_cast_ref::<Event>()
            .map(|event| event.original_source())
            .unwrap_or_default()
    }

    fn event_id(&self, obj: &Self::Type) -> Option<OwnedEventId> {
        obj.dynamic_cast_ref::<Event>().and_then(|event| {
            event
                .imp()
                .pure_event
                .borrow()
                .as_ref()
                .unwrap()
                .event
                .get_field::<OwnedEventId>("event_id")
                .ok()
                .flatten()
        })
    }

    fn sender_id(&self, obj: &Self::Type) -> Option<OwnedUserId> {
        obj.dynamic_cast_ref::<Event>().and_then(|event| {
            event
                .imp()
                .pure_event
                .borrow()
                .as_ref()
                .unwrap()
                .event
                .get_field::<OwnedUserId>("sender")
                .ok()
                .flatten()
        })
    }

    fn origin_server_ts(&self, obj: &Self::Type) -> Option<MilliSecondsSinceUnixEpoch> {
        obj.dynamic_cast_ref::<Event>().and_then(|event| {
            event
                .imp()
                .pure_event
                .borrow()
                .as_ref()
                .unwrap()
                .event
                .get_field::<MilliSecondsSinceUnixEpoch>("origin_server_ts")
                .ok()
                .flatten()
        })
    }
}

// Make `Event` subclassable.
unsafe impl<T> IsSubclassable<T> for Event
where
    T: TimelineItemImpl + EventImpl,
    T::Type: IsA<TimelineItem> + IsA<Event>,
{
    fn class_init(class: &mut glib::Class<Self>) {
        Self::parent_class_init::<T>(class.upcast_ref_mut());

        let klass = class.as_mut();

        klass.source = source_trampoline::<T>;
        klass.event_id = event_id_trampoline::<T>;
        klass.sender_id = sender_id_trampoline::<T>;
        klass.origin_server_ts = origin_server_ts_trampoline::<T>;
    }
}

// Virtual method implementation trampolines.
fn source_trampoline<T>(this: &Event) -> String
where
    T: ObjectSubclass + EventImpl,
    T::Type: IsA<Event>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().source(this)
}

fn event_id_trampoline<T>(this: &Event) -> Option<OwnedEventId>
where
    T: ObjectSubclass + EventImpl,
    T::Type: IsA<Event>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().event_id(this)
}

fn sender_id_trampoline<T>(this: &Event) -> Option<OwnedUserId>
where
    T: ObjectSubclass + EventImpl,
    T::Type: IsA<Event>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().sender_id(this)
}

fn origin_server_ts_trampoline<T>(this: &Event) -> Option<MilliSecondsSinceUnixEpoch>
where
    T: ObjectSubclass + EventImpl,
    T::Type: IsA<Event>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().origin_server_ts(this)
}

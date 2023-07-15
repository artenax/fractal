use std::fmt;

use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk_ui::timeline::{
    AnyOtherFullStateEventContent, Error as TimelineError, EventTimelineItem, RepliedToEvent,
    TimelineDetails, TimelineItemContent,
};
use ruma::{
    events::{room::message::MessageType, AnySyncTimelineEvent},
    serde::Raw,
    MilliSecondsSinceUnixEpoch, OwnedEventId, OwnedTransactionId, OwnedUserId,
};

mod reaction_group;
mod reaction_list;
mod read_receipts;

pub use self::{
    reaction_group::ReactionGroup, reaction_list::ReactionList, read_receipts::ReadReceipts,
};
use super::{
    timeline::{TimelineItem, TimelineItemImpl},
    Member, Room,
};
use crate::{spawn_tokio, utils::matrix::get_media_content};

/// The unique key to identify an event in a room.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EventKey {
    /// This is the local echo of the event, the key is its transaction ID.
    TransactionId(OwnedTransactionId),

    /// This is the remote echo of the event, the key is its event ID.
    EventId(OwnedEventId),
}

impl fmt::Display for EventKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventKey::TransactionId(txn_id) => write!(f, "transaction_id:{txn_id}"),
            EventKey::EventId(event_id) => write!(f, "event_id:{event_id}"),
        }
    }
}

#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "BoxedEventTimelineItem")]
pub struct BoxedEventTimelineItem(EventTimelineItem);

mod imp {
    use std::cell::RefCell;

    use glib::object::WeakRef;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Event {
        /// The underlying SDK timeline item.
        pub item: RefCell<Option<EventTimelineItem>>,

        /// The room containing this `Event`.
        pub room: WeakRef<Room>,

        /// The reactions on this event.
        pub reactions: ReactionList,

        /// The read receipts on this event.
        pub read_receipts: ReadReceipts,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Event {
        const NAME: &'static str = "RoomEvent";
        type Type = super::Event;
        type ParentType = TimelineItem;
    }

    impl ObjectImpl for Event {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoxed::builder::<BoxedEventTimelineItem>("item")
                        .write_only()
                        .build(),
                    glib::ParamSpecString::builder("source").read_only().build(),
                    glib::ParamSpecObject::builder::<Room>("room")
                        .construct_only()
                        .build(),
                    glib::ParamSpecString::builder("time").read_only().build(),
                    glib::ParamSpecObject::builder::<ReactionList>("reactions")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("is-edited")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("is-highlighted")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "item" => {
                    let item = value.get::<BoxedEventTimelineItem>().unwrap();
                    obj.set_item(item.0);
                }
                "room" => {
                    obj.set_room(value.get().unwrap());
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "source" => obj.source().to_value(),
                "room" => obj.room().to_value(),
                "time" => obj.time().to_value(),
                "reactions" => obj.reactions().to_value(),
                "is-edited" => obj.is_edited().to_value(),
                "is-highlighted" => obj.is_highlighted().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl TimelineItemImpl for Event {
        fn id(&self) -> String {
            format!("Event::{}", self.obj().key())
        }

        fn can_hide_header(&self) -> bool {
            match self.obj().content() {
                TimelineItemContent::Message(message) => {
                    matches!(
                        message.msgtype(),
                        MessageType::Audio(_)
                            | MessageType::File(_)
                            | MessageType::Image(_)
                            | MessageType::Location(_)
                            | MessageType::Notice(_)
                            | MessageType::Text(_)
                            | MessageType::Video(_)
                    )
                }
                TimelineItemContent::Sticker(_) => true,
                _ => false,
            }
        }

        fn event_sender(&self) -> Option<Member> {
            Some(self.obj().sender())
        }

        fn selectable(&self) -> bool {
            true
        }
    }
}

glib::wrapper! {
    /// GObject representation of a Matrix room event.
    pub struct Event(ObjectSubclass<imp::Event>) @extends TimelineItem;
}

impl Event {
    /// Create a new `Event` with the given SDK timeline item.
    pub fn new(item: EventTimelineItem, room: &Room) -> Self {
        let item = BoxedEventTimelineItem(item);
        glib::Object::builder()
            .property("item", &item)
            .property("room", room)
            .build()
    }

    /// Try to update this `Event` with the given SDK timeline item.
    ///
    /// Returns `true` if the update succeeded.
    pub fn try_update_with(&self, item: &EventTimelineItem) -> bool {
        match &self.key() {
            EventKey::TransactionId(txn_id)
                if item.is_local_echo() && item.transaction_id() == Some(txn_id) =>
            {
                self.set_item(item.clone());
                return true;
            }
            EventKey::EventId(event_id)
                if !item.is_local_echo() && item.event_id() == Some(event_id) =>
            {
                self.set_item(item.clone());
                return true;
            }
            _ => {}
        }

        false
    }

    /// The room that contains this `Event`.
    pub fn room(&self) -> Room {
        self.imp().room.upgrade().unwrap()
    }

    /// Set the room that contains this `Event`.
    fn set_room(&self, room: Room) {
        let imp = self.imp();
        imp.room.set(Some(&room));
        imp.reactions
            .set_user(room.session().user().unwrap().clone());
        imp.read_receipts.set_room(&room);
    }

    /// The underlying SDK timeline item of this `Event`.
    pub fn item(&self) -> EventTimelineItem {
        self.imp().item.borrow().clone().unwrap()
    }

    /// Set the underlying SDK timeline item of this `Event`.
    pub fn set_item(&self, item: EventTimelineItem) {
        let was_edited = self.is_edited();
        let was_highlighted = self.is_highlighted();
        let imp = self.imp();

        imp.reactions.update(item.reactions().clone());
        imp.read_receipts.update(item.read_receipts().clone());
        imp.item.replace(Some(item));

        self.notify("source");
        if self.is_edited() != was_edited {
            self.notify("is-edited");
        }
        if self.is_highlighted() != was_highlighted {
            self.notify("is-highlighted");
        }
    }

    /// The raw JSON source for this `Event`, if it has been echoed back
    /// by the server.
    pub fn raw(&self) -> Option<Raw<AnySyncTimelineEvent>> {
        self.imp()
            .item
            .borrow()
            .as_ref()
            .unwrap()
            .original_json()
            .cloned()
    }

    /// The pretty-formatted JSON source for this `Event`, if it has
    /// been echoed back by the server.
    pub fn source(&self) -> Option<String> {
        self.imp()
            .item
            .borrow()
            .as_ref()
            .unwrap()
            .original_json()
            .map(|raw| {
                // We have to convert it to a Value, because a RawValue cannot be
                // pretty-printed.
                let json = serde_json::to_value(raw).unwrap();

                serde_json::to_string_pretty(&json).unwrap()
            })
    }

    /// The unique of this `Event` in the timeline.
    pub fn key(&self) -> EventKey {
        let item_ref = self.imp().item.borrow();
        let item = item_ref.as_ref().unwrap();
        if item.is_local_echo() {
            EventKey::TransactionId(item.transaction_id().unwrap().to_owned())
        } else {
            EventKey::EventId(item.event_id().unwrap().to_owned())
        }
    }

    /// The event ID of this `Event`, if it has been received from the server.
    pub fn event_id(&self) -> Option<OwnedEventId> {
        match self.key() {
            EventKey::EventId(event_id) => Some(event_id),
            _ => None,
        }
    }

    /// The transaction ID of this `Event`, if it is still pending.
    pub fn transaction_id(&self) -> Option<OwnedTransactionId> {
        match self.key() {
            EventKey::TransactionId(txn_id) => Some(txn_id),
            _ => None,
        }
    }

    /// The user ID of the sender of this `Event`.
    pub fn sender_id(&self) -> OwnedUserId {
        self.imp()
            .item
            .borrow()
            .as_ref()
            .unwrap()
            .sender()
            .to_owned()
    }

    /// The sender of this `Event`.
    pub fn sender(&self) -> Member {
        self.room().members().get_or_create(self.sender_id())
    }

    /// The timestamp of this `Event` as the number of milliseconds
    /// since Unix Epoch, if it has been echoed back by the server.
    ///
    /// Otherwise it's the local time when this event was created.
    pub fn origin_server_ts(&self) -> MilliSecondsSinceUnixEpoch {
        self.imp().item.borrow().as_ref().unwrap().timestamp()
    }

    /// The timestamp of this `Event` as a `u64`, if it has been echoed back by
    /// the server.
    ///
    /// Otherwise it's the local time when this event was created.
    pub fn origin_server_ts_u64(&self) -> u64 {
        self.origin_server_ts().get().into()
    }

    /// The timestamp of this `Event`.
    pub fn timestamp(&self) -> glib::DateTime {
        let ts = self.origin_server_ts();

        glib::DateTime::from_unix_utc(ts.as_secs().into())
            .and_then(|t| t.to_local())
            .unwrap()
    }

    /// The formatted time of this `Event`.
    pub fn time(&self) -> String {
        let datetime = self.timestamp();

        // FIXME Is there a cleaner way to know whether the locale uses 12 or 24 hour
        // format?
        let local_time = datetime.format("%X").unwrap().as_str().to_ascii_lowercase();

        if local_time.ends_with("am") || local_time.ends_with("pm") {
            // Use 12h time format (AM/PM)
            datetime.format("%l∶%M %p").unwrap().to_string()
        } else {
            // Use 24 time format
            datetime.format("%R").unwrap().to_string()
        }
    }

    /// The content to display for this `Event`.
    pub fn content(&self) -> TimelineItemContent {
        self.imp().item.borrow().as_ref().unwrap().content().clone()
    }

    /// The message of this `Event`, if any.
    pub fn message(&self) -> Option<MessageType> {
        match self.imp().item.borrow().as_ref().unwrap().content() {
            TimelineItemContent::Message(msg) => Some(msg.msgtype().clone()),
            _ => None,
        }
    }

    /// Whether this `Event` was edited.
    pub fn is_edited(&self) -> bool {
        let item_ref = self.imp().item.borrow();
        let Some(item) = item_ref.as_ref() else {
            return false;
        };

        match item.content() {
            TimelineItemContent::Message(msg) => msg.is_edited(),
            _ => false,
        }
    }

    /// Whether this `Event` should be highlighted.
    pub fn is_highlighted(&self) -> bool {
        let item_ref = self.imp().item.borrow();
        let Some(item) = item_ref.as_ref() else {
            return false;
        };

        item.is_highlighted()
    }

    /// The reactions to this event.
    pub fn reactions(&self) -> &ReactionList {
        &self.imp().reactions
    }

    /// The read receipts on this event.
    pub fn read_receipts(&self) -> &ReadReceipts {
        &self.imp().read_receipts
    }

    /// Get the ID of the event this `Event` replies to, if any.
    pub fn reply_to_id(&self) -> Option<OwnedEventId> {
        match self.imp().item.borrow().as_ref().unwrap().content() {
            TimelineItemContent::Message(message) => {
                message.in_reply_to().map(|d| d.event_id.clone())
            }
            _ => None,
        }
    }

    /// Whether this `Event` is a reply to another event.
    pub fn is_reply(&self) -> bool {
        self.reply_to_id().is_some()
    }

    /// Get the details of the event this `Event` replies to, if any.
    ///
    /// Returns `None(_)` if this event is not a reply.
    pub fn reply_to_event_content(&self) -> Option<TimelineDetails<Box<RepliedToEvent>>> {
        match self.imp().item.borrow().as_ref().unwrap().content() {
            TimelineItemContent::Message(message) => message.in_reply_to().map(|d| d.event.clone()),
            _ => None,
        }
    }

    /// Fetch missing details for this event.
    ///
    /// This is a no-op if called for a local event.
    pub async fn fetch_missing_details(&self) -> Result<(), TimelineError> {
        let Some(event_id) = self.event_id() else {
            return Ok(());
        };

        let timeline = self.room().timeline().matrix_timeline();
        spawn_tokio!(async move { timeline.fetch_details_for_event(&event_id).await })
            .await
            .unwrap()
    }

    /// Fetch the content of the media message in this `Event`.
    ///
    /// Compatible events:
    ///
    /// - File message (`MessageType::File`).
    /// - Image message (`MessageType::Image`).
    /// - Video message (`MessageType::Video`).
    /// - Audio message (`MessageType::Audio`).
    ///
    /// Returns `Ok((filename, binary_content))` on success.
    ///
    /// Returns `Err` if an error occurred while fetching the content. Panics on
    /// an incompatible event.
    pub async fn get_media_content(&self) -> Result<(String, Vec<u8>), matrix_sdk::Error> {
        let TimelineItemContent::Message(message) = self.content() else {
            panic!("Trying to get the media content of an event of incompatible type");
        };

        let client = self.room().session().client();
        get_media_content(client, message.msgtype().clone()).await
    }

    /// Whether this `Event` is considered a message.
    pub fn is_message(&self) -> bool {
        matches!(
            self.content(),
            TimelineItemContent::Message(_) | TimelineItemContent::Sticker(_)
        )
    }

    /// Whether this `Event` can count as an unread message.
    ///
    /// This follows the algorithm in [MSC2654], excluding events that we don't
    /// show in the timeline.
    ///
    /// [MSC2654]: https://github.com/matrix-org/matrix-spec-proposals/pull/2654
    pub fn counts_as_unread(&self) -> bool {
        count_as_unread(self.imp().item.borrow().as_ref().unwrap().content())
    }

    /// Listen to changes of the source of this `TimelineEvent`.
    pub fn connect_source_notify<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_notify_local(Some("source"), move |this, _| {
            f(this);
        })
    }
}

/// Whether the given event can count as an unread message.
///
/// This follows the algorithm in [MSC2654], excluding events that we don't
/// show in the timeline.
///
/// [MSC2654]: https://github.com/matrix-org/matrix-spec-proposals/pull/2654
pub fn count_as_unread(content: &TimelineItemContent) -> bool {
    match content {
        TimelineItemContent::Message(message) => {
            !matches!(message.msgtype(), MessageType::Notice(_))
        }
        TimelineItemContent::Sticker(_) => true,
        TimelineItemContent::OtherState(state) => matches!(
            state.content(),
            AnyOtherFullStateEventContent::RoomTombstone(_)
        ),
        _ => false,
    }
}

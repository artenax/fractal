use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk::{
    media::MediaEventContent,
    room::timeline::{
        AnyOtherFullStateEventContent, EventTimelineItem, RepliedToEvent, TimelineDetails,
        TimelineItemContent,
    },
    ruma::{
        events::room::message::MessageType, MilliSecondsSinceUnixEpoch, OwnedEventId, OwnedUserId,
    },
    Error as MatrixError,
};
use ruma::{events::AnySyncTimelineEvent, serde::Raw, OwnedTransactionId};

mod event_actions;

pub use self::event_actions::{EventActions, EventTexture};
use super::{
    timeline::{TimelineItem, TimelineItemImpl},
    Member, ReactionList, Room,
};
use crate::{
    spawn_tokio,
    utils::media::{filename_for_mime, media_type_uid},
};

/// The unique key to identify an event in a room.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EventKey {
    /// This is the local echo of the event, the key is its transaction ID.
    TransactionId(OwnedTransactionId),

    /// This is the remote echo of the event, the key is its event ID.
    EventId(OwnedEventId),
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

        pub reactions: ReactionList,
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
                _ => unimplemented!(),
            }
        }
    }

    impl TimelineItemImpl for Event {
        fn is_visible(&self) -> bool {
            match self.obj().content() {
                TimelineItemContent::Message(message) => matches!(
                    message.msgtype(),
                    MessageType::Audio(_)
                        | MessageType::Emote(_)
                        | MessageType::File(_)
                        | MessageType::Image(_)
                        | MessageType::Location(_)
                        | MessageType::Notice(_)
                        | MessageType::ServerNotice(_)
                        | MessageType::Text(_)
                        | MessageType::Video(_)
                        | MessageType::VerificationRequest(_)
                ),
                TimelineItemContent::Sticker(_) => true,
                TimelineItemContent::UnableToDecrypt(_) => true,
                TimelineItemContent::MembershipChange(_) => true,
                TimelineItemContent::ProfileChange(_) => true,
                TimelineItemContent::OtherState(state) => matches!(
                    state.content(),
                    AnyOtherFullStateEventContent::RoomCreate(_)
                        | AnyOtherFullStateEventContent::RoomEncryption(_)
                        | AnyOtherFullStateEventContent::RoomThirdPartyInvite(_)
                        | AnyOtherFullStateEventContent::RoomTombstone(_)
                ),
                _ => false,
            }
        }

        fn activatable(&self) -> bool {
            match self.obj().content() {
                // The event can be activated to open the media viewer if it's an image or a video.
                TimelineItemContent::Message(message) => {
                    matches!(
                        message.msgtype(),
                        MessageType::Image(_) | MessageType::Video(_)
                    )
                }
                _ => false,
            }
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
        match self.key() {
            EventKey::TransactionId(txn_id) => match item {
                EventTimelineItem::Local(local_event) if local_event.transaction_id == txn_id => {
                    self.set_item(item.clone());
                    return true;
                }
                _ => {}
            },
            EventKey::EventId(event_id) => match item {
                EventTimelineItem::Remote(remote_event) if remote_event.event_id == event_id => {
                    self.set_item(item.clone());
                    return true;
                }
                _ => {}
            },
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
    }

    /// The underlying SDK timeline item of this `Event`.
    pub fn item(&self) -> EventTimelineItem {
        self.imp().item.borrow().clone().unwrap()
    }

    /// Set the underlying SDK timeline item of this `Event`.
    pub fn set_item(&self, item: EventTimelineItem) {
        let imp = self.imp();

        imp.reactions.update(
            item.as_remote()
                .map(|i| i.reactions().clone())
                .unwrap_or_default(),
        );
        imp.item.replace(Some(item));

        self.notify("activatable");
        self.notify("source");
    }

    /// The raw JSON source for this `Event`, if it has been echoed back
    /// by the server.
    pub fn raw(&self) -> Option<Raw<AnySyncTimelineEvent>> {
        self.imp().item.borrow().as_ref().unwrap().raw().cloned()
    }

    /// The pretty-formatted JSON source for this `Event`, if it has
    /// been echoed back by the server.
    pub fn source(&self) -> Option<String> {
        self.imp().item.borrow().as_ref().unwrap().raw().map(|raw| {
            // We have to convert it to a Value, because a RawValue cannot be
            // pretty-printed.
            let json = serde_json::to_value(raw).unwrap();

            serde_json::to_string_pretty(&json).unwrap()
        })
    }

    /// The unique of this `Event` in the timeline.
    pub fn key(&self) -> EventKey {
        match self.imp().item.borrow().as_ref().unwrap() {
            EventTimelineItem::Local(event) => {
                EventKey::TransactionId(event.transaction_id.clone())
            }
            EventTimelineItem::Remote(event) => EventKey::EventId(event.event_id.clone()),
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
        self.room().members().member_by_id(self.sender_id())
    }

    /// The timestamp of this `Event` as the number of milliseconds
    /// since Unix Epoch, if it has been echoed back by the server.
    ///
    /// Otherwise it's the local time when this event was created.
    pub fn origin_server_ts(&self) -> MilliSecondsSinceUnixEpoch {
        self.imp().item.borrow().as_ref().unwrap().timestamp()
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

    /// The reactions to this event.
    pub fn reactions(&self) -> &ReactionList {
        &self.imp().reactions
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
            TimelineItemContent::Message(message) => {
                message.in_reply_to().map(|d| d.details.clone())
            }
            _ => None,
        }
    }

    /// Fetch missing details for this event.
    ///
    /// This is a no-op if called for a local event.
    pub async fn fetch_missing_details(&self) -> Result<(), MatrixError> {
        let Some(event_id) = self.event_id() else {
            return Ok(())
        };

        let timeline = self.room().timeline().matrix_timeline();
        spawn_tokio!(async move { timeline.fetch_event_details(&event_id).await })
            .await
            .unwrap()
    }

    /// Fetch the content of the media message in this `SupportedEvent`.
    ///
    /// Compatible events:
    ///
    /// - File message (`MessageType::File`).
    /// - Image message (`MessageType::Image`).
    /// - Video message (`MessageType::Video`).
    /// - Audio message (`MessageType::Audio`).
    ///
    /// Returns `Ok((uid, filename, binary_content))` on success. `uid` is a
    /// unique identifier for this media.
    ///
    /// Returns `Err` if an error occurred while fetching the content. Panics on
    /// an incompatible event.
    pub async fn get_media_content(&self) -> Result<(String, String, Vec<u8>), matrix_sdk::Error> {
        if let TimelineItemContent::Message(message) = self.content() {
            let media = self.room().session().client().media();
            match message.msgtype() {
                MessageType::File(content) => {
                    let content = content.clone();
                    let uid = media_type_uid(content.source());
                    let filename = content
                        .filename
                        .as_ref()
                        .filter(|name| !name.is_empty())
                        .or(Some(&content.body))
                        .filter(|name| !name.is_empty())
                        .cloned()
                        .unwrap_or_else(|| {
                            filename_for_mime(
                                content
                                    .info
                                    .as_ref()
                                    .and_then(|info| info.mimetype.as_deref()),
                                None,
                            )
                        });
                    let handle = spawn_tokio!(async move { media.get_file(content, true).await });
                    let data = handle.await.unwrap()?.unwrap();
                    return Ok((uid, filename, data));
                }
                MessageType::Image(content) => {
                    let content = content.clone();
                    let uid = media_type_uid(content.source());
                    let filename = if content.body.is_empty() {
                        filename_for_mime(
                            content
                                .info
                                .as_ref()
                                .and_then(|info| info.mimetype.as_deref()),
                            Some(mime::IMAGE),
                        )
                    } else {
                        content.body.clone()
                    };
                    let handle = spawn_tokio!(async move { media.get_file(content, true).await });
                    let data = handle.await.unwrap()?.unwrap();
                    return Ok((uid, filename, data));
                }
                MessageType::Video(content) => {
                    let content = content.clone();
                    let uid = media_type_uid(content.source());
                    let filename = if content.body.is_empty() {
                        filename_for_mime(
                            content
                                .info
                                .as_ref()
                                .and_then(|info| info.mimetype.as_deref()),
                            Some(mime::VIDEO),
                        )
                    } else {
                        content.body.clone()
                    };
                    let handle = spawn_tokio!(async move { media.get_file(content, true).await });
                    let data = handle.await.unwrap()?.unwrap();
                    return Ok((uid, filename, data));
                }
                MessageType::Audio(content) => {
                    let content = content.clone();
                    let uid = media_type_uid(content.source());
                    let filename = if content.body.is_empty() {
                        filename_for_mime(
                            content
                                .info
                                .as_ref()
                                .and_then(|info| info.mimetype.as_deref()),
                            Some(mime::AUDIO),
                        )
                    } else {
                        content.body.clone()
                    };
                    let handle = spawn_tokio!(async move { media.get_file(content, true).await });
                    let data = handle.await.unwrap()?.unwrap();
                    return Ok((uid, filename, data));
                }
                _ => {}
            };
        };

        panic!("Trying to get the media content of an event of incompatible type");
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

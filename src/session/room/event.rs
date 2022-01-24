use std::sync::Arc;

use gtk::{
    glib,
    glib::{clone, DateTime},
    prelude::*,
    subclass::prelude::*,
};
use log::warn;
use matrix_sdk::{
    deserialized_responses::SyncRoomEvent,
    media::MediaEventContent,
    ruma::{
        events::{
            room::message::{MessageType, Relation},
            AnyMessageEventContent, AnySyncMessageEvent, AnySyncRoomEvent, AnySyncStateEvent,
            Unsigned,
        },
        identifiers::{EventId, UserId},
        MilliSecondsSinceUnixEpoch,
    },
    Error as MatrixError,
};

use crate::{
    session::{
        room::{Member, ReactionList},
        Room,
    },
    spawn_tokio,
    utils::{filename_for_mime, media_type_uid},
};

#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "BoxedSyncRoomEvent")]
pub struct BoxedSyncRoomEvent(SyncRoomEvent);

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{object::WeakRef, SignalHandlerId};
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct Event {
        /// The deserialized matrix event
        pub event: RefCell<Option<AnySyncRoomEvent>>,
        /// The SDK event containing encryption information and the serialized
        /// event as `Raw`
        pub pure_event: RefCell<Option<SyncRoomEvent>>,
        /// Events that replace this one, in the order they arrive.
        pub replacing_events: RefCell<Vec<super::Event>>,
        pub reactions: ReactionList,
        pub source_changed_handler: RefCell<Option<SignalHandlerId>>,
        pub show_header: Cell<bool>,
        pub room: OnceCell<WeakRef<Room>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Event {
        const NAME: &'static str = "RoomEvent";
        type Type = super::Event;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for Event {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoxed::new(
                        "event",
                        "event",
                        "The matrix event of this Event",
                        BoxedSyncRoomEvent::static_type(),
                        glib::ParamFlags::WRITABLE,
                    ),
                    glib::ParamSpecString::new(
                        "source",
                        "Source",
                        "The source (JSON) of this Event",
                        None,
                        glib::ParamFlags::READABLE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecBoolean::new(
                        "show-header",
                        "Show Header",
                        "Whether this event should show a header. This does nothing if this event doesn’t have a header. ",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "can-hide-header",
                        "Can hide header",
                        "Whether this event is allowed to hide it's header or not.",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "sender",
                        "Sender",
                        "The sender of this matrix event",
                        Member::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "room",
                        "Room",
                        "The room containing this event",
                        Room::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecString::new(
                        "time",
                        "Time",
                        "The locally formatted time of this matrix event",
                        None,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "can-view-media",
                        "Can View Media",
                        "Whether this is a media event that can be viewed",
                        false,
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
                "event" => {
                    let event = value.get::<BoxedSyncRoomEvent>().unwrap();
                    obj.set_matrix_pure_event(event.0);
                }
                "show-header" => {
                    let show_header = value.get().unwrap();
                    let _ = obj.set_show_header(show_header);
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
                "sender" => obj.sender().to_value(),
                "room" => obj.room().to_value(),
                "show-header" => obj.show_header().to_value(),
                "can-hide-header" => obj.can_hide_header().to_value(),
                "time" => obj.time().to_value(),
                "can-view-media" => obj.can_view_media().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// GObject representation of a Matrix room event.
    pub struct Event(ObjectSubclass<imp::Event>);
}

// TODO:
// - [ ] implement operations for events: forward, reply, delete...

impl Event {
    pub fn new(event: SyncRoomEvent, room: &Room) -> Self {
        let event = BoxedSyncRoomEvent(event);
        glib::Object::new(&[("event", &event), ("room", room)]).expect("Failed to create Event")
    }

    pub fn sender(&self) -> Member {
        self.room().members().member_by_id(self.matrix_sender())
    }

    pub fn room(&self) -> Room {
        self.imp().room.get().unwrap().upgrade().unwrap()
    }

    /// Get the matrix event
    ///
    /// If the `SyncRoomEvent` couldn't be deserialized this is `None`
    pub fn matrix_event(&self) -> Option<AnySyncRoomEvent> {
        self.imp().event.borrow().clone()
    }

    pub fn matrix_pure_event(&self) -> SyncRoomEvent {
        self.imp().pure_event.borrow().clone().unwrap()
    }

    pub fn set_matrix_pure_event(&self, event: SyncRoomEvent) {
        let priv_ = self.imp();

        if let Ok(deserialized) = event.event.deserialize() {
            priv_.event.replace(Some(deserialized));
        } else {
            warn!("Failed to deserialize event: {:?}", event);
        }

        priv_.pure_event.replace(Some(event));

        self.notify("event");
        self.notify("can-view-media");
    }

    pub fn matrix_sender(&self) -> Arc<UserId> {
        let priv_ = self.imp();

        if let Some(event) = priv_.event.borrow().as_ref() {
            event.sender().into()
        } else {
            priv_
                .pure_event
                .borrow()
                .as_ref()
                .unwrap()
                .event
                .get_field::<Arc<UserId>>("sender")
                .unwrap()
                .unwrap()
        }
    }

    pub fn matrix_event_id(&self) -> Box<EventId> {
        let priv_ = self.imp();

        if let Some(event) = priv_.event.borrow().as_ref() {
            event.event_id().to_owned()
        } else {
            priv_
                .pure_event
                .borrow()
                .as_ref()
                .unwrap()
                .event
                .get_field::<Box<EventId>>("event_id")
                .unwrap()
                .unwrap()
        }
    }

    pub fn matrix_transaction_id(&self) -> Option<String> {
        self.imp()
            .pure_event
            .borrow()
            .as_ref()
            .unwrap()
            .event
            .get_field::<Unsigned>("unsigned")
            .ok()
            .and_then(|opt| opt)
            .and_then(|unsigned| unsigned.transaction_id)
    }

    /// The pretty-formatted JSON of this matrix event.
    pub fn original_source(&self) -> String {
        // We have to convert it to a Value, because a RawValue cannot be
        // pretty-printed.
        let json: serde_json::Value = serde_json::from_str(
            self.imp()
                .pure_event
                .borrow()
                .as_ref()
                .unwrap()
                .event
                .json()
                .get(),
        )
        .unwrap();

        serde_json::to_string_pretty(&json).unwrap()
    }

    /// The pretty-formatted JSON used for this matrix event.
    ///
    /// If this matrix event has been replaced, returns the replacing `Event`'s
    /// source.
    pub fn source(&self) -> String {
        self.replacement()
            .map(|replacement| replacement.source())
            .unwrap_or_else(|| self.original_source())
    }

    pub fn timestamp(&self) -> DateTime {
        let priv_ = self.imp();
        let ts = if let Some(event) = priv_.event.borrow().as_ref() {
            event.origin_server_ts().as_secs()
        } else {
            priv_
                .pure_event
                .borrow()
                .as_ref()
                .unwrap()
                .event
                .get_field::<MilliSecondsSinceUnixEpoch>("origin_server_ts")
                .unwrap()
                .unwrap()
                .as_secs()
        };

        DateTime::from_unix_utc(ts.into())
            .and_then(|t| t.to_local())
            .unwrap()
    }

    pub fn time(&self) -> String {
        let datetime = self.timestamp();

        // FIXME Is there a cleaner way to do that?
        let local_time = datetime.format("%X").unwrap().as_str().to_ascii_lowercase();

        if local_time.ends_with("am") || local_time.ends_with("pm") {
            // Use 12h time format (AM/PM)
            datetime.format("%l∶%M %p").unwrap().to_string()
        } else {
            // Use 24 time format
            datetime.format("%R").unwrap().to_string()
        }
    }

    /// Find the related event if any
    pub fn related_matrix_event(&self) -> Option<Box<EventId>> {
        match self.imp().event.borrow().as_ref()? {
            AnySyncRoomEvent::Message(ref message) => match message {
                AnySyncMessageEvent::RoomRedaction(event) => Some(event.redacts.clone()),
                _ => match message.content() {
                    AnyMessageEventContent::Reaction(event) => Some(event.relates_to.event_id),
                    AnyMessageEventContent::RoomMessage(event) => match event.relates_to {
                        Some(relates_to) => match relates_to {
                            // TODO: Figure out Relation::Annotation(), Relation::Reference() but
                            // they are pre-specs for now See: https://github.com/uhoreg/matrix-doc/blob/aggregations-reactions/proposals/2677-reactions.md
                            Relation::Reply { in_reply_to } => Some(in_reply_to.event_id),
                            Relation::Replacement(replacement) => Some(replacement.event_id),
                            _ => None,
                        },
                        _ => None,
                    },
                    // TODO: RoomEncrypted needs https://github.com/ruma/ruma/issues/502
                    _ => None,
                },
            },
            _ => None,
        }
    }

    /// Whether this event is hidden from the user or displayed in the room
    /// history.
    pub fn is_hidden_event(&self) -> bool {
        let priv_ = self.imp();

        if self.related_matrix_event().is_some() {
            if let Some(AnySyncRoomEvent::Message(message)) = priv_.event.borrow().as_ref() {
                if let AnyMessageEventContent::RoomMessage(content) = message.content() {
                    if let Some(Relation::Reply { in_reply_to: _ }) = content.relates_to {
                        return false;
                    }
                }
            }
            return true;
        }

        let event = priv_.event.borrow();

        // List of all events to be hidden.
        match event.as_ref() {
            Some(AnySyncRoomEvent::Message(message)) => matches!(
                message,
                AnySyncMessageEvent::CallAnswer(_)
                    | AnySyncMessageEvent::CallInvite(_)
                    | AnySyncMessageEvent::CallHangup(_)
                    | AnySyncMessageEvent::CallCandidates(_)
                    | AnySyncMessageEvent::KeyVerificationReady(_)
                    | AnySyncMessageEvent::KeyVerificationStart(_)
                    | AnySyncMessageEvent::KeyVerificationCancel(_)
                    | AnySyncMessageEvent::KeyVerificationAccept(_)
                    | AnySyncMessageEvent::KeyVerificationKey(_)
                    | AnySyncMessageEvent::KeyVerificationMac(_)
                    | AnySyncMessageEvent::KeyVerificationDone(_)
                    | AnySyncMessageEvent::RoomMessageFeedback(_)
                    | AnySyncMessageEvent::RoomRedaction(_)
            ),
            Some(AnySyncRoomEvent::State(state)) => matches!(
                state,
                AnySyncStateEvent::PolicyRuleRoom(_)
                    | AnySyncStateEvent::PolicyRuleServer(_)
                    | AnySyncStateEvent::PolicyRuleUser(_)
                    | AnySyncStateEvent::RoomAliases(_)
                    | AnySyncStateEvent::RoomAvatar(_)
                    | AnySyncStateEvent::RoomCanonicalAlias(_)
                    | AnySyncStateEvent::RoomEncryption(_)
                    | AnySyncStateEvent::RoomJoinRules(_)
                    | AnySyncStateEvent::RoomName(_)
                    | AnySyncStateEvent::RoomPinnedEvents(_)
                    | AnySyncStateEvent::RoomPowerLevels(_)
                    | AnySyncStateEvent::RoomServerAcl(_)
                    | AnySyncStateEvent::RoomTopic(_)
                    | AnySyncStateEvent::SpaceParent(_)
                    | AnySyncStateEvent::SpaceChild(_)
            ),
            Some(AnySyncRoomEvent::RedactedMessage(_)) => true,
            Some(AnySyncRoomEvent::RedactedState(_)) => true,
            _ => false,
        }
    }

    pub fn set_show_header(&self, visible: bool) {
        let priv_ = self.imp();
        if priv_.show_header.get() == visible {
            return;
        }
        priv_.show_header.set(visible);
        self.notify("show-header");
    }

    pub fn show_header(&self) -> bool {
        self.imp().show_header.get()
    }

    /// The content of this message.
    ///
    /// Returns `None` if this is not a message.
    pub fn message_content(&self) -> Option<AnyMessageEventContent> {
        match self.matrix_event() {
            Some(AnySyncRoomEvent::Message(message)) => Some(message.content()),
            _ => None,
        }
    }

    pub fn can_hide_header(&self) -> bool {
        match self.message_content() {
            Some(AnyMessageEventContent::RoomMessage(message)) => {
                matches!(
                    message.msgtype,
                    MessageType::Audio(_)
                        | MessageType::File(_)
                        | MessageType::Image(_)
                        | MessageType::Location(_)
                        | MessageType::Notice(_)
                        | MessageType::Text(_)
                        | MessageType::Video(_)
                )
            }
            Some(AnyMessageEventContent::Sticker(_)) => true,
            _ => false,
        }
    }

    /// Whether this is a replacing `Event`.
    ///
    /// Replacing matrix events are:
    ///
    /// - `RoomRedaction`
    /// - `RoomMessage` with `Relation::Replacement`
    pub fn is_replacing_event(&self) -> bool {
        match self.matrix_event() {
            Some(AnySyncRoomEvent::Message(AnySyncMessageEvent::RoomMessage(message))) => {
                matches!(message.content.relates_to, Some(Relation::Replacement(_)))
            }
            Some(AnySyncRoomEvent::Message(AnySyncMessageEvent::RoomRedaction(_))) => true,
            _ => false,
        }
    }

    pub fn prepend_replacing_events(&self, events: Vec<Event>) {
        let priv_ = self.imp();
        priv_.replacing_events.borrow_mut().splice(..0, events);
        if self.redacted() {
            priv_.reactions.clear();
        }
    }

    pub fn append_replacing_events(&self, events: Vec<Event>) {
        let priv_ = self.imp();
        let old_replacement = self.replacement();

        priv_.replacing_events.borrow_mut().extend(events);

        let new_replacement = self.replacement();

        // Update the signal handler to the new replacement
        if new_replacement != old_replacement {
            if let Some(replacement) = old_replacement {
                if let Some(source_changed_handler) = priv_.source_changed_handler.take() {
                    replacement.disconnect(source_changed_handler);
                }
            }

            // If the replacing event's content changed, this content changed too.
            if let Some(replacement) = new_replacement {
                priv_
                    .source_changed_handler
                    .replace(Some(replacement.connect_notify_local(
                        Some("source"),
                        clone!(@weak self as obj => move |_, _| {
                            obj.notify("source");
                        }),
                    )));
            }
            if self.redacted() {
                priv_.reactions.clear();
            }
            self.notify("source");
        }
    }

    pub fn replacing_events(&self) -> Vec<Event> {
        self.imp().replacing_events.borrow().clone()
    }

    /// The `Event` that replaces this one, if any.
    ///
    /// If this matrix event has been redacted or replaced, returns the
    /// corresponding `Event`, otherwise returns `None`.
    pub fn replacement(&self) -> Option<Event> {
        self.replacing_events()
            .iter()
            .rev()
            .find(|event| event.is_replacing_event() && !event.redacted())
            .cloned()
    }

    /// Whether this matrix event has been redacted.
    pub fn redacted(&self) -> bool {
        self.replacement()
            .filter(|event| {
                matches!(
                    event.matrix_event(),
                    Some(AnySyncRoomEvent::Message(
                        AnySyncMessageEvent::RoomRedaction(_)
                    ))
                )
            })
            .is_some()
    }

    /// Whether this is a reaction.
    pub fn is_reaction(&self) -> bool {
        matches!(
            self.message_content(),
            Some(AnyMessageEventContent::Reaction(_))
        )
    }

    /// The reactions for this event.
    pub fn reactions(&self) -> &ReactionList {
        &self.imp().reactions
    }

    /// Add reactions to this event.
    pub fn add_reactions(&self, reactions: Vec<Event>) {
        if !self.redacted() {
            self.imp().reactions.add_reactions(reactions);
        }
    }

    /// The content of this matrix event.
    pub fn original_content(&self) -> Option<AnyMessageEventContent> {
        match self.matrix_event()? {
            AnySyncRoomEvent::Message(message) => Some(message.content()),
            _ => None,
        }
    }

    /// The content to display for this `Event`.
    ///
    /// If this matrix event has been replaced, returns the replacing `Event`'s
    /// content.
    pub fn content(&self) -> Option<AnyMessageEventContent> {
        self.replacement()
            .and_then(|replacement| replacement.content())
            .or_else(|| self.original_content())
    }

    pub fn connect_show_header_notify<F: Fn(&Self, &glib::ParamSpec) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_notify_local(Some("show-header"), f)
    }

    /// The content of a media message.
    ///
    /// Compatible events:
    ///
    /// - File message (`MessageType::File`).
    /// - Image message (`MessageType::Image`).
    /// - Video message (`MessageType::Video`).
    ///
    /// Returns `Ok((uid, filename, binary_content))` on success, `Err` if an
    /// error occurred while fetching the content. Panics on an incompatible
    /// event. `uid` is a unique identifier for this media.
    pub async fn get_media_content(&self) -> Result<(String, String, Vec<u8>), matrix_sdk::Error> {
        if let AnyMessageEventContent::RoomMessage(content) = self.message_content().unwrap() {
            let client = self.room().session().client();
            match content.msgtype {
                MessageType::File(content) => {
                    let uid = media_type_uid(content.file());
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
                    let handle = spawn_tokio!(async move { client.get_file(content, true).await });
                    let data = handle.await.unwrap()?.unwrap();
                    return Ok((uid, filename, data));
                }
                MessageType::Image(content) => {
                    let uid = media_type_uid(content.file());
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
                    let handle = spawn_tokio!(async move { client.get_file(content, true).await });
                    let data = handle.await.unwrap()?.unwrap();
                    return Ok((uid, filename, data));
                }
                MessageType::Video(content) => {
                    let uid = media_type_uid(content.file());
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
                    let handle = spawn_tokio!(async move { client.get_file(content, true).await });
                    let data = handle.await.unwrap()?.unwrap();
                    return Ok((uid, filename, data));
                }
                _ => {}
            };
        };

        panic!("Trying to get the media content of an event of incompatible type");
    }

    /// Whether this is a media event that can be viewed.
    pub fn can_view_media(&self) -> bool {
        match self.message_content() {
            Some(AnyMessageEventContent::RoomMessage(message)) => {
                matches!(
                    message.msgtype,
                    MessageType::Image(_) | MessageType::Video(_)
                )
            }
            _ => false,
        }
    }

    /// Get the id of the event this `Event` replies to, if any.
    pub fn reply_to_id(&self) -> Option<Box<EventId>> {
        match self.original_content()? {
            AnyMessageEventContent::RoomMessage(message) => {
                if let Some(Relation::Reply { in_reply_to }) = message.relates_to {
                    Some(in_reply_to.event_id)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Whether this `Event` is a reply to another event.
    pub fn is_reply(&self) -> bool {
        self.reply_to_id().is_some()
    }

    /// Get the `Event` this `Event` replies to, if any.
    ///
    /// Returns `Ok(None)` if this event is not a reply.
    pub async fn reply_to_event(&self) -> Result<Option<Event>, MatrixError> {
        let related_event_id = match self.reply_to_id() {
            Some(related_event_id) => related_event_id,
            None => {
                return Ok(None);
            }
        };
        let event = self
            .room()
            .timeline()
            .fetch_event_by_id(&related_event_id)
            .await?;
        Ok(Some(event))
    }
}

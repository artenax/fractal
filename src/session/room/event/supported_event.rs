use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*};
use log::warn;
use matrix_sdk::{
    deserialized_responses::SyncRoomEvent,
    media::MediaEventContent,
    ruma::{
        events::{
            room::{
                encrypted::OriginalSyncRoomEncryptedEvent,
                message::{MessageType, Relation},
                redaction::SyncRoomRedactionEvent,
            },
            AnyMessageLikeEventContent, AnySyncMessageLikeEvent, AnySyncRoomEvent,
            AnySyncStateEvent, SyncMessageLikeEvent, SyncStateEvent,
        },
        serde::Raw,
        MilliSecondsSinceUnixEpoch, OwnedEventId, OwnedTransactionId, OwnedUserId,
    },
    Error as MatrixError,
};
use serde_json::Error as JsonError;

use super::{BoxedSyncRoomEvent, Event, EventImpl};
use crate::{
    prelude::*,
    session::room::{
        timeline::{TimelineItem, TimelineItemImpl},
        Member, ReactionList, Room,
    },
    spawn, spawn_tokio,
    utils::{filename_for_mime, media_type_uid},
};

#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "BoxedAnySyncRoomEvent")]
pub struct BoxedAnySyncRoomEvent(AnySyncRoomEvent);

mod imp {
    use std::cell::RefCell;

    use glib::SignalHandlerId;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct SupportedEvent {
        /// The deserialized Matrix event.
        pub matrix_event: RefCell<Option<AnySyncRoomEvent>>,
        /// Events that replace this one, in the order they arrive.
        pub replacing_events: RefCell<Vec<super::SupportedEvent>>,
        pub reactions: ReactionList,
        pub keys_handle: RefCell<Option<SignalHandlerId>>,
        pub source_changed_handler: RefCell<Option<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SupportedEvent {
        const NAME: &'static str = "RoomSupportedEvent";
        type Type = super::SupportedEvent;
        type ParentType = Event;
    }

    impl ObjectImpl for SupportedEvent {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoxed::new(
                        "matrix-event",
                        "Matrix Event",
                        "The deserialized Matrix event of this Event",
                        BoxedAnySyncRoomEvent::static_type(),
                        glib::ParamFlags::WRITABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "reactions",
                        "Reactions",
                        "The reactions related to this event",
                        ReactionList::static_type(),
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
                "matrix-event" => {
                    let matrix_event = value.get::<BoxedAnySyncRoomEvent>().unwrap();
                    obj.set_matrix_event(matrix_event.0);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "reactions" => obj.reactions().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl TimelineItemImpl for SupportedEvent {
        fn activatable(&self, obj: &Self::Type) -> bool {
            match obj.original_content() {
                // The event can be activated to open the media viewer if it's an image or a video.
                Some(AnyMessageLikeEventContent::RoomMessage(message)) => {
                    matches!(
                        message.msgtype,
                        MessageType::Image(_) | MessageType::Video(_)
                    )
                }
                _ => false,
            }
        }

        fn can_hide_header(&self, obj: &Self::Type) -> bool {
            match obj.original_content() {
                Some(AnyMessageLikeEventContent::RoomMessage(message)) => {
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
                Some(AnyMessageLikeEventContent::Sticker(_)) => true,
                _ => false,
            }
        }

        fn event_sender(&self, obj: &Self::Type) -> Option<Member> {
            Some(obj.sender())
        }
    }

    impl EventImpl for SupportedEvent {
        fn source(&self, obj: &Self::Type) -> String {
            obj.replacement()
                .map(|replacement| replacement.source())
                .unwrap_or_else(|| obj.original_source())
        }

        fn origin_server_ts(&self, _obj: &Self::Type) -> Option<MilliSecondsSinceUnixEpoch> {
            Some(
                self.matrix_event
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .origin_server_ts(),
            )
        }
    }
}

glib::wrapper! {
    /// GObject representation of a supported Matrix room event.
    pub struct SupportedEvent(ObjectSubclass<imp::SupportedEvent>) @extends TimelineItem, Event;
}

// TODO:
// - [ ] implement operations for events: forward, reply, edit...

impl SupportedEvent {
    /// Try to construct a new `SupportedEvent` with the given pure event and
    /// room.
    ///
    /// Returns an error if the pure event fails to deserialize.
    pub fn try_from_event(pure_event: SyncRoomEvent, room: &Room) -> Result<Self, JsonError> {
        let matrix_event = BoxedAnySyncRoomEvent(pure_event.event.deserialize()?);
        let pure_event = BoxedSyncRoomEvent(pure_event);
        Ok(glib::Object::new(&[
            ("pure-event", &pure_event),
            ("matrix-event", &matrix_event),
            ("room", room),
        ])
        .expect("Failed to create SupportedEvent"))
    }

    /// Set the deserialized Matrix event of this `SupportedEvent`.
    fn set_matrix_event(&self, matrix_event: AnySyncRoomEvent) {
        if let AnySyncRoomEvent::MessageLike(AnySyncMessageLikeEvent::RoomEncrypted(
            SyncMessageLikeEvent::Original(_),
        )) = matrix_event
        {
            spawn!(clone!(@weak self as obj => async move {
                obj.try_to_decrypt(obj.pure_event().event.cast()).await;
            }));
        }

        self.imp().matrix_event.replace(Some(matrix_event));
        self.notify("activatable");
    }

    /// The deserialized Matrix event of this `SupportedEvent`.
    pub fn matrix_event(&self) -> AnySyncRoomEvent {
        self.imp().matrix_event.borrow().clone().unwrap()
    }

    /// Try to decrypt this `SupportedEvent` with the current room keys.
    ///
    /// If decryption fails, it will be retried everytime we receive new room
    /// keys.
    pub async fn try_to_decrypt(&self, event: Raw<OriginalSyncRoomEncryptedEvent>) {
        let priv_ = self.imp();
        let room = self.room().matrix_room();
        let handle = spawn_tokio!(async move { room.decrypt_event(&event).await });

        match handle.await.unwrap() {
            Ok(decrypted) => {
                if let Some(keys_handle) = priv_.keys_handle.take() {
                    self.room().disconnect(keys_handle);
                }
                let pure_event = SyncRoomEvent::from(decrypted);
                let matrix_event = pure_event.event.deserialize().unwrap();
                self.set_pure_event(pure_event);
                self.set_matrix_event(matrix_event);
            }
            Err(error) => {
                warn!("Failed to decrypt event: {}", error);
                if priv_.keys_handle.borrow().is_none() {
                    let handle = self.room().connect_new_encryption_keys(
                        clone!(@weak self as obj => move |_| {
                            // Try to decrypt the event again
                            obj.set_matrix_event(obj.matrix_event());
                        }),
                    );

                    priv_.keys_handle.replace(Some(handle));
                }
            }
        }
    }

    /// The event ID of this `SupportedEvent`.
    pub fn event_id(&self) -> OwnedEventId {
        self.imp()
            .matrix_event
            .borrow()
            .as_ref()
            .unwrap()
            .event_id()
            .to_owned()
    }

    /// The user ID of the sender of this `SupportedEvent`.
    pub fn sender_id(&self) -> OwnedUserId {
        self.imp()
            .matrix_event
            .borrow()
            .as_ref()
            .unwrap()
            .sender()
            .to_owned()
    }

    /// The room member that sent this `SupportedEvent`.
    pub fn sender(&self) -> Member {
        self.room().members().member_by_id(self.sender_id())
    }

    /// The transaction ID of this `SupportedEvent`, if any.
    ///
    /// This is the random string sent with the event, if it was sent from this
    /// session.
    pub fn transaction_id(&self) -> Option<OwnedTransactionId> {
        self.imp()
            .matrix_event
            .borrow()
            .as_ref()
            .unwrap()
            .transaction_id()
            .map(|txn_id| txn_id.to_owned())
    }

    /// The ID of the event this `SupportedEvent` relates to, if any.
    pub fn related_event_id(&self) -> Option<OwnedEventId> {
        match self.imp().matrix_event.borrow().as_ref()? {
            AnySyncRoomEvent::MessageLike(ref message) => match message {
                AnySyncMessageLikeEvent::RoomRedaction(SyncRoomRedactionEvent::Original(event)) => {
                    Some(event.redacts.clone())
                }
                AnySyncMessageLikeEvent::Reaction(SyncMessageLikeEvent::Original(event)) => {
                    Some(event.content.relates_to.event_id.clone())
                }
                AnySyncMessageLikeEvent::RoomMessage(SyncMessageLikeEvent::Original(event)) => {
                    match &event.content.relates_to {
                        Some(relates_to) => match relates_to {
                            Relation::Reply { in_reply_to } => Some(in_reply_to.event_id.clone()),
                            Relation::Replacement(replacement) => {
                                Some(replacement.event_id.clone())
                            }
                            _ => None,
                        },
                        _ => None,
                    }
                }
                // TODO: RoomEncrypted needs https://github.com/ruma/ruma/issues/502
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether this `SupportedEvent` replaces another one.
    ///
    /// Replacing Matrix events are:
    ///
    /// - `RoomRedaction`
    /// - `RoomMessage` with `Relation::Replacement`
    pub fn is_replacing_event(&self) -> bool {
        match self.imp().matrix_event.borrow().as_ref().unwrap() {
            AnySyncRoomEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
                SyncMessageLikeEvent::Original(message),
            )) => {
                matches!(message.content.relates_to, Some(Relation::Replacement(_)))
            }
            AnySyncRoomEvent::MessageLike(AnySyncMessageLikeEvent::RoomRedaction(_)) => true,
            _ => false,
        }
    }

    /// Prepend the given events to the list of replacing events.
    pub fn prepend_replacing_events(&self, events: Vec<SupportedEvent>) {
        let priv_ = self.imp();
        priv_.replacing_events.borrow_mut().splice(..0, events);
        if self.redacted() {
            priv_.reactions.clear();
        }
    }

    /// Append the given events to the list of replacing events.
    pub fn append_replacing_events(&self, events: Vec<SupportedEvent>) {
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

    /// The replacing events of this `SupportedEvent`, in the order of the
    /// timeline.
    pub fn replacing_events(&self) -> Vec<SupportedEvent> {
        self.imp().replacing_events.borrow().clone()
    }

    /// The event that replaces this `SupportedEvent`, if any.
    pub fn replacement(&self) -> Option<SupportedEvent> {
        self.replacing_events()
            .iter()
            .rev()
            .find(|event| event.is_replacing_event() && !event.redacted())
            .cloned()
    }

    /// Whether this `SupportedEvent` has been redacted.
    pub fn redacted(&self) -> bool {
        self.replacement()
            .filter(|event| {
                matches!(
                    event.matrix_event(),
                    AnySyncRoomEvent::MessageLike(AnySyncMessageLikeEvent::RoomRedaction(_))
                )
            })
            .is_some()
    }

    /// Whether this `SupportedEvent` is a reaction.
    pub fn is_reaction(&self) -> bool {
        matches!(
            self.original_content(),
            Some(AnyMessageLikeEventContent::Reaction(_))
        )
    }

    /// The reactions for this `SupportedEvent`.
    pub fn reactions(&self) -> &ReactionList {
        &self.imp().reactions
    }

    /// Add reactions to this `SupportedEvent`.
    pub fn add_reactions(&self, reactions: Vec<SupportedEvent>) {
        if !self.redacted() {
            self.imp().reactions.add_reactions(reactions);
        }
    }

    /// The content of this `SupportedEvent`, if this is a message-like event.
    pub fn original_content(&self) -> Option<AnyMessageLikeEventContent> {
        match self.matrix_event() {
            AnySyncRoomEvent::MessageLike(message) => message.original_content(),
            _ => None,
        }
    }

    /// The content to display for this `SupportedEvent`, if this is a
    /// message-like event.
    ///
    /// If this event has been replaced, returns the replacing
    /// `SupportedEvent`'s content.
    pub fn content(&self) -> Option<AnyMessageLikeEventContent> {
        self.replacement()
            .and_then(|replacement| replacement.content())
            .or_else(|| self.original_content())
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
        if let AnyMessageLikeEventContent::RoomMessage(content) = self.original_content().unwrap() {
            let client = self.room().session().client();
            match content.msgtype {
                MessageType::File(content) => {
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
                    let handle = spawn_tokio!(async move { client.get_file(content, true).await });
                    let data = handle.await.unwrap()?.unwrap();
                    return Ok((uid, filename, data));
                }
                MessageType::Image(content) => {
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
                    let handle = spawn_tokio!(async move { client.get_file(content, true).await });
                    let data = handle.await.unwrap()?.unwrap();
                    return Ok((uid, filename, data));
                }
                MessageType::Video(content) => {
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
                    let handle = spawn_tokio!(async move { client.get_file(content, true).await });
                    let data = handle.await.unwrap()?.unwrap();
                    return Ok((uid, filename, data));
                }
                MessageType::Audio(content) => {
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
                    let handle = spawn_tokio!(async move { client.get_file(content, true).await });
                    let data = handle.await.unwrap()?.unwrap();
                    return Ok((uid, filename, data));
                }
                _ => {}
            };
        };

        panic!("Trying to get the media content of an event of incompatible type");
    }

    /// Get the ID of the event this `SupportedEvent` replies to, if any.
    pub fn reply_to_id(&self) -> Option<OwnedEventId> {
        match self.original_content()? {
            AnyMessageLikeEventContent::RoomMessage(message) => {
                if let Some(Relation::Reply { in_reply_to }) = message.relates_to {
                    Some(in_reply_to.event_id)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Whether this `SupportedEvent` is a reply to another event.
    pub fn is_reply(&self) -> bool {
        self.reply_to_id().is_some()
    }

    /// Get the `Event` this `SupportedEvent` replies to, if any.
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

    /// Whether this `SupportedEvent` is hidden from the user or displayed in
    /// the room history.
    pub fn is_hidden_event(&self) -> bool {
        let priv_ = self.imp();

        if self.related_event_id().is_some() {
            if let Some(AnySyncRoomEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
                SyncMessageLikeEvent::Original(message),
            ))) = priv_.matrix_event.borrow().as_ref()
            {
                if let Some(Relation::Reply { in_reply_to: _ }) = message.content.relates_to {
                    return false;
                }
            }
            return true;
        }

        // List of all events to be shown.
        match priv_.matrix_event.borrow().as_ref() {
            Some(AnySyncRoomEvent::MessageLike(message)) => !matches!(
                message,
                AnySyncMessageLikeEvent::RoomMessage(SyncMessageLikeEvent::Original(_))
                    | AnySyncMessageLikeEvent::RoomEncrypted(SyncMessageLikeEvent::Original(_))
                    | AnySyncMessageLikeEvent::Sticker(SyncMessageLikeEvent::Original(_))
            ),
            Some(AnySyncRoomEvent::State(state)) => !matches!(
                state,
                AnySyncStateEvent::RoomCreate(SyncStateEvent::Original(_))
                    | AnySyncStateEvent::RoomMember(SyncStateEvent::Original(_))
                    | AnySyncStateEvent::RoomThirdPartyInvite(SyncStateEvent::Original(_))
                    | AnySyncStateEvent::RoomTombstone(SyncStateEvent::Original(_))
            ),
            _ => true,
        }
    }
}
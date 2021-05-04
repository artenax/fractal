use chrono::{offset::Local, DateTime};
use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk::{
    events::{
        room::message::MessageType, room::message::Relation, AnyMessageEvent,
        AnyMessageEventContent, AnyRedactedMessageEvent, AnyRedactedStateEvent, AnyRoomEvent,
        AnyStateEvent,
    },
    identifiers::{EventId, UserId},
};

use crate::fn_event;
use crate::session::User;

#[derive(Clone, Debug, glib::GBoxed)]
#[gboxed(type_name = "BoxedAnyRoomEvent")]
pub struct BoxedAnyRoomEvent(AnyRoomEvent);

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use once_cell::sync::{Lazy, OnceCell};
    use std::cell::{Cell, RefCell};

    #[derive(Debug, Default)]
    pub struct Event {
        pub event: OnceCell<AnyRoomEvent>,
        pub relates_to: RefCell<Vec<super::Event>>,
        pub show_header: Cell<bool>,
        pub sender: OnceCell<User>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Event {
        const NAME: &'static str = "RoomEvent";
        type Type = super::Event;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for Event {
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder("relates-to-changed", &[], <()>::static_type().into()).build()]
            });
            SIGNALS.as_ref()
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpec::new_boxed(
                        "event",
                        "event",
                        "The matrix event of this Event",
                        BoxedAnyRoomEvent::static_type(),
                        glib::ParamFlags::WRITABLE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpec::new_boolean(
                        "show-header",
                        "Show Header",
                        "Whether this event should show a header or not. This does do nothing if this event doesn't have a header. ",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpec::new_boolean(
                        "can-hide-header",
                        "Can hide header",
                        "Whether this event is allowed to hide it's header or not.",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpec::new_object(
                        "sender",
                        "Sender",
                        "The sender of this matrix event",
                        User::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
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
                    let event = value.get::<BoxedAnyRoomEvent>().unwrap();
                    self.event.set(event.0).unwrap();
                }
                "show-header" => {
                    let show_header = value.get().unwrap();
                    let _ = obj.set_show_header(show_header);
                }
                "sender" => {
                    let sender = value.get().unwrap();
                    if let Some(sender) = sender {
                        let _ = self.sender.set(sender).unwrap();
                    }
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "sender" => self.sender.get().to_value(),
                "show-header" => obj.show_header().to_value(),
                "can-hide-header" => obj.can_hide_header().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct Event(ObjectSubclass<imp::Event>);
}

// TODO:
// - [ ] implement operations for events: forward, reply, delete...

/// This is the GObject represatation of a matrix room event
impl Event {
    pub fn new(event: &AnyRoomEvent, sender: &User) -> Self {
        let event = BoxedAnyRoomEvent(event.to_owned());
        glib::Object::new(&[("event", &event), ("sender", &sender)])
            .expect("Failed to create Event")
    }

    pub fn sender(&self) -> &User {
        let priv_ = imp::Event::from_instance(&self);
        priv_.sender.get().unwrap()
    }

    pub fn matrix_event(&self) -> &AnyRoomEvent {
        let priv_ = imp::Event::from_instance(&self);
        priv_.event.get().unwrap()
    }

    pub fn matrix_sender(&self) -> &UserId {
        let priv_ = imp::Event::from_instance(&self);
        let event = priv_.event.get().unwrap();
        fn_event!(event, sender)
    }

    pub fn matrix_event_id(&self) -> &EventId {
        let priv_ = imp::Event::from_instance(&self);
        let event = priv_.event.get().unwrap();
        fn_event!(event, event_id)
    }

    pub fn timestamp(&self) -> DateTime<Local> {
        let priv_ = imp::Event::from_instance(&self);
        let event = priv_.event.get().unwrap();
        fn_event!(event, origin_server_ts).clone().into()
    }

    /// Find the related event if any
    pub fn related_matrix_event(&self) -> Option<EventId> {
        match self.matrix_event() {
            AnyRoomEvent::Message(ref message) => match message {
                AnyMessageEvent::RoomRedaction(event) => Some(event.redacts.clone()),
                _ => match message.content() {
                    AnyMessageEventContent::Reaction(event) => Some(event.relation.event_id),
                    AnyMessageEventContent::RoomMessage(event) => match event.relates_to {
                        Some(relates_to) => match relates_to {
                            // TODO: Figure out Relation::Annotation(), Relation::Reference() but they are pre-specs for now
                            // See: https://github.com/uhoreg/matrix-doc/blob/aggregations-reactions/proposals/2677-reactions.md
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

    /// Whether this event is hidden from the user or displayed in the room history.
    pub fn is_hidden_event(&self) -> bool {
        if self.related_matrix_event().is_some() {
            return true;
        }

        match self.matrix_event() {
            AnyRoomEvent::Message(message) => match message {
                AnyMessageEvent::CallAnswer(_) => true,
                AnyMessageEvent::CallInvite(_) => true,
                AnyMessageEvent::CallHangup(_) => true,
                AnyMessageEvent::CallCandidates(_) => true,
                AnyMessageEvent::KeyVerificationReady(_) => true,
                AnyMessageEvent::KeyVerificationStart(_) => true,
                AnyMessageEvent::KeyVerificationCancel(_) => true,
                AnyMessageEvent::KeyVerificationAccept(_) => true,
                AnyMessageEvent::KeyVerificationKey(_) => true,
                AnyMessageEvent::KeyVerificationMac(_) => true,
                AnyMessageEvent::KeyVerificationDone(_) => true,
                AnyMessageEvent::RoomEncrypted(_) => true,
                AnyMessageEvent::RoomMessageFeedback(_) => true,
                AnyMessageEvent::RoomRedaction(_) => true,
                AnyMessageEvent::Sticker(_) => true,
                AnyMessageEvent::Custom(_) => true,
                _ => false,
            },
            AnyRoomEvent::State(state) => match state {
                AnyStateEvent::PolicyRuleRoom(_) => true,
                AnyStateEvent::PolicyRuleServer(_) => true,
                AnyStateEvent::PolicyRuleUser(_) => true,
                AnyStateEvent::RoomAliases(_) => true,
                AnyStateEvent::RoomAvatar(_) => true,
                AnyStateEvent::RoomCanonicalAlias(_) => true,
                AnyStateEvent::RoomEncryption(_) => true,
                AnyStateEvent::RoomJoinRules(_) => true,
                AnyStateEvent::RoomName(_) => true,
                AnyStateEvent::RoomPinnedEvents(_) => true,
                AnyStateEvent::RoomPowerLevels(_) => true,
                AnyStateEvent::RoomServerAcl(_) => true,
                AnyStateEvent::RoomTopic(_) => true,
                AnyStateEvent::Custom(_) => true,
                _ => false,
            },
            AnyRoomEvent::RedactedMessage(message) => match message {
                AnyRedactedMessageEvent::CallAnswer(_) => true,
                AnyRedactedMessageEvent::CallInvite(_) => true,
                AnyRedactedMessageEvent::CallHangup(_) => true,
                AnyRedactedMessageEvent::CallCandidates(_) => true,
                AnyRedactedMessageEvent::KeyVerificationReady(_) => true,
                AnyRedactedMessageEvent::KeyVerificationStart(_) => true,
                AnyRedactedMessageEvent::KeyVerificationCancel(_) => true,
                AnyRedactedMessageEvent::KeyVerificationAccept(_) => true,
                AnyRedactedMessageEvent::KeyVerificationKey(_) => true,
                AnyRedactedMessageEvent::KeyVerificationMac(_) => true,
                AnyRedactedMessageEvent::KeyVerificationDone(_) => true,
                AnyRedactedMessageEvent::RoomEncrypted(_) => true,
                AnyRedactedMessageEvent::RoomMessageFeedback(_) => true,
                AnyRedactedMessageEvent::RoomRedaction(_) => true,
                AnyRedactedMessageEvent::Sticker(_) => true,
                AnyRedactedMessageEvent::Custom(_) => true,
                _ => false,
            },
            AnyRoomEvent::RedactedState(state) => match state {
                AnyRedactedStateEvent::PolicyRuleRoom(_) => true,
                AnyRedactedStateEvent::PolicyRuleServer(_) => true,
                AnyRedactedStateEvent::PolicyRuleUser(_) => true,
                AnyRedactedStateEvent::RoomAliases(_) => true,
                AnyRedactedStateEvent::RoomAvatar(_) => true,
                AnyRedactedStateEvent::RoomCanonicalAlias(_) => true,
                AnyRedactedStateEvent::RoomEncryption(_) => true,
                AnyRedactedStateEvent::RoomJoinRules(_) => true,
                AnyRedactedStateEvent::RoomName(_) => true,
                AnyRedactedStateEvent::RoomPinnedEvents(_) => true,
                AnyRedactedStateEvent::RoomPowerLevels(_) => true,
                AnyRedactedStateEvent::RoomServerAcl(_) => true,
                AnyRedactedStateEvent::RoomTopic(_) => true,
                AnyRedactedStateEvent::Custom(_) => true,
                _ => false,
            },
        }
    }

    pub fn set_show_header(&self, visible: bool) {
        let priv_ = imp::Event::from_instance(&self);
        if priv_.show_header.get() == visible {
            return;
        }
        priv_.show_header.set(visible);
        self.notify("show-header");
    }

    pub fn show_header(&self) -> bool {
        let priv_ = imp::Event::from_instance(&self);

        priv_.show_header.get()
    }

    pub fn can_hide_header(&self) -> bool {
        let priv_ = imp::Event::from_instance(&self);
        match priv_.event.get().unwrap() {
            AnyRoomEvent::Message(ref message) => match message.content() {
                AnyMessageEventContent::RoomMessage(message) => match message.msgtype {
                    MessageType::Audio(_) => true,
                    MessageType::File(_) => true,
                    MessageType::Image(_) => true,
                    MessageType::Location(_) => true,
                    MessageType::Notice(_) => true,
                    MessageType::Text(_) => true,
                    MessageType::Video(_) => true,
                    _ => false,
                },
                _ => false,
            },
            _ => false,
        }
    }

    pub fn add_relates_to(&self, events: Vec<Event>) {
        let priv_ = imp::Event::from_instance(&self);
        priv_.relates_to.borrow_mut().extend(events);
        self.emit_by_name("relates-to-changed", &[]).unwrap();
    }

    pub fn relates_to(&self) -> Vec<Event> {
        let priv_ = imp::Event::from_instance(&self);
        priv_.relates_to.borrow().clone()
    }

    pub fn connect_relates_to_changed<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("relates-to-changed", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();

            f(&obj);

            None
        })
        .unwrap()
    }

    pub fn connect_show_header_notify<F: Fn(&Self, &glib::ParamSpec) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_notify_local(Some("show-header"), f)
    }
}
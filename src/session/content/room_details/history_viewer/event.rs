use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk::deserialized_responses::TimelineEvent;
use ruma::events::{AnyMessageLikeEventContent, AnySyncTimelineEvent};

use crate::session::Room;

#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "BoxedAnySyncTimelineEvent")]
pub struct BoxedAnySyncTimelineEvent(pub AnySyncTimelineEvent);

mod imp {
    use glib::{Properties, WeakRef};
    use once_cell::unsync::OnceCell;

    use super::*;

    #[derive(Debug, Properties, Default)]
    #[properties(wrapper_type = super::HistoryViewerEvent)]
    pub struct HistoryViewerEvent {
        #[property(get)]
        pub(super) matrix_event: OnceCell<BoxedAnySyncTimelineEvent>,
        #[property(get)]
        pub(super) room: WeakRef<Room>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for HistoryViewerEvent {
        const NAME: &'static str = "HistoryViewerEvent";
        type Type = super::HistoryViewerEvent;
    }

    impl ObjectImpl for HistoryViewerEvent {
        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }

        fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            self.derived_set_property(id, value, pspec)
        }

        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            self.derived_property(id, pspec)
        }
    }
}

glib::wrapper! {
    pub struct HistoryViewerEvent(ObjectSubclass<imp::HistoryViewerEvent>);
}

impl HistoryViewerEvent {
    pub fn try_new(event: TimelineEvent, room: &Room) -> Option<Self> {
        if let Ok(matrix_event) = event.event.deserialize() {
            let obj: Self = glib::Object::new();
            obj.imp()
                .matrix_event
                .set(BoxedAnySyncTimelineEvent(matrix_event.into()))
                .unwrap();
            obj.imp().room.set(Some(room));
            Some(obj)
        } else {
            None
        }
    }

    pub fn original_content(&self) -> Option<AnyMessageLikeEventContent> {
        match self.matrix_event().0 {
            AnySyncTimelineEvent::MessageLike(message) => message.original_content(),
            _ => None,
        }
    }
}

use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk::deserialized_responses::TimelineEvent;
use ruma::events::{room::message::MessageType, AnyMessageLikeEventContent, AnySyncTimelineEvent};

use crate::{session::model::Room, spawn_tokio, utils::media::filename_for_mime};

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

    pub async fn get_file_content(&self) -> Result<(String, Vec<u8>), matrix_sdk::Error> {
        if let AnyMessageLikeEventContent::RoomMessage(content) = self.original_content().unwrap() {
            let media = self.room().unwrap().session().client().media();

            if let MessageType::File(content) = content.msgtype {
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
                return Ok((filename, data));
            }
        }

        panic!("Trying to get the content of an event of incompatible type");
    }
}

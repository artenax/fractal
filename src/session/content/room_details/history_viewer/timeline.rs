use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use log::error;
use matrix_sdk::{
    room::MessagesOptions,
    ruma::{
        api::client::filter::{RoomEventFilter, UrlFilter},
        assign,
        events::{room::message::MessageType, AnyMessageLikeEventContent, MessageLikeEventType},
        uint,
    },
};

use crate::{
    session::{
        content::room_details::history_viewer::HistoryViewerEvent, room::TimelineState, Room,
    },
    spawn_tokio,
};

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "ContentHistoryViewerTimelineFilter")]
pub enum TimelineFilter {
    #[default]
    Media,
    Files,
    Audio,
}

mod imp {
    use std::{
        cell::{Cell, RefCell},
        sync::Arc,
    };

    use futures::lock::Mutex;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct Timeline {
        pub room: OnceCell<Room>,
        pub state: Cell<TimelineState>,
        pub filter: Cell<TimelineFilter>,
        pub list: RefCell<Vec<HistoryViewerEvent>>,
        pub last_token: Arc<Mutex<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Timeline {
        const NAME: &'static str = "ContentHistoryViewerTimeline";
        type Type = super::Timeline;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for Timeline {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Room>("room")
                        .construct_only()
                        .build(),
                    glib::ParamSpecEnum::builder::<TimelineState>("state")
                        .read_only()
                        .build(),
                    glib::ParamSpecEnum::builder::<TimelineFilter>("filter")
                        .construct_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "room" => self.obj().set_room(value.get().unwrap()),
                "filter" => self.filter.set(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "room" => obj.room().to_value(),
                "state" => obj.state().to_value(),
                "filter" => obj.filter().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for Timeline {
        fn item_type(&self) -> glib::Type {
            HistoryViewerEvent::static_type()
        }

        fn n_items(&self) -> u32 {
            self.list.borrow().len() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            let list = self.list.borrow();
            list.get(position as usize)
                .map(|o| o.clone().upcast::<glib::Object>())
        }
    }
}

glib::wrapper! {
    pub struct Timeline(ObjectSubclass<imp::Timeline>)
        @implements gio::ListModel;
}

impl Timeline {
    pub fn new(room: &Room, filter: TimelineFilter) -> Self {
        glib::Object::builder()
            .property("room", room)
            .property("filter", filter)
            .build()
    }

    pub async fn load(&self) -> bool {
        let imp = self.imp();

        if matches!(
            self.state(),
            TimelineState::Loading | TimelineState::Complete
        ) {
            return false;
        }

        self.set_state(TimelineState::Loading);

        let matrix_room = self.room().matrix_room();
        let last_token = imp.last_token.clone();
        let handle: tokio::task::JoinHandle<matrix_sdk::Result<_>> = spawn_tokio!(async move {
            let last_token = last_token.lock().await;
            let filter_types = vec![MessageLikeEventType::RoomMessage.to_string()];
            let filter = assign!(RoomEventFilter::default(), {
                types: Some(filter_types),
                url_filter: Some(UrlFilter::EventsWithUrl),
            });
            let options = assign!(MessagesOptions::backward().from(&**last_token), {
                limit: uint!(20),
                filter,
            });

            matrix_room.messages(options).await
        });

        match handle.await.unwrap() {
            Ok(events) => match events.end {
                Some(end_token) => {
                    *imp.last_token.lock().await = end_token;

                    let events: Vec<HistoryViewerEvent> = events
                        .chunk
                        .into_iter()
                        .filter_map(|event| {
                            let event = HistoryViewerEvent::try_new(event, self.room())?;

                            match event.original_content() {
                                Some(AnyMessageLikeEventContent::RoomMessage(content)) => {
                                    match self.filter() {
                                        TimelineFilter::Media
                                            if matches!(content.msgtype, MessageType::Image(_))
                                                || matches!(
                                                    content.msgtype,
                                                    MessageType::Video(_)
                                                ) =>
                                        {
                                            Some(event)
                                        }
                                        TimelineFilter::Files
                                            if matches!(content.msgtype, MessageType::File(_)) =>
                                        {
                                            Some(event)
                                        }
                                        TimelineFilter::Audio
                                            if matches!(content.msgtype, MessageType::Audio(_)) =>
                                        {
                                            Some(event)
                                        }
                                        _ => None,
                                    }
                                }
                                _ => None,
                            }
                        })
                        .collect();

                    self.append(events);

                    self.set_state(TimelineState::Ready);
                    true
                }
                None => {
                    self.set_state(TimelineState::Complete);
                    false
                }
            },
            Err(error) => {
                error!("Failed to load events: {}", error);
                self.set_state(TimelineState::Error);
                false
            }
        }
    }

    fn append(&self, batch: Vec<HistoryViewerEvent>) {
        let imp = self.imp();

        if batch.is_empty() {
            return;
        }

        let added = batch.len();
        let index = {
            let mut list = imp.list.borrow_mut();
            let index = list.len();

            // Extend the size of the list so that rust doesn't need to reallocate memory
            // multiple times
            list.reserve(batch.len());

            for event in batch {
                list.push(event.upcast());
            }

            index
        };

        self.items_changed(index as u32, 0, added as u32);
    }

    fn set_room(&self, room: Room) {
        self.imp().room.set(room).unwrap();
    }

    pub fn room(&self) -> &Room {
        self.imp().room.get().unwrap()
    }

    fn set_state(&self, state: TimelineState) {
        if state == self.state() {
            return;
        }

        self.imp().state.set(state);
        self.notify("state");
    }

    pub fn state(&self) -> TimelineState {
        self.imp().state.get()
    }

    pub fn filter(&self) -> TimelineFilter {
        self.imp().filter.get()
    }
}

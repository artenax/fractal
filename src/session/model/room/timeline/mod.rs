mod timeline_item;
mod virtual_item;

use std::{collections::HashMap, sync::Arc};

use eyeball_im::VectorDiff;
use futures_util::StreamExt;
use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use matrix_sdk::Error as MatrixError;
use matrix_sdk_ui::timeline::{
    BackPaginationStatus, PaginationOptions, RoomExt, Timeline as SdkTimeline,
    TimelineItem as SdkTimelineItem,
};
use ruma::{
    events::{
        room::message::MessageType, AnySyncMessageLikeEvent, AnySyncStateEvent,
        AnySyncTimelineEvent, SyncMessageLikeEvent,
    },
    OwnedEventId,
};
use tracing::{error, warn};

pub use self::{
    timeline_item::{TimelineItem, TimelineItemExt, TimelineItemImpl},
    virtual_item::{VirtualItem, VirtualItemKind},
};
use super::{Event, EventKey, Room};
use crate::{spawn, spawn_tokio};

#[derive(Debug, Default, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "TimelineState")]
pub enum TimelineState {
    #[default]
    Initial,
    Loading,
    Ready,
    Error,
    Complete,
}

const MAX_BATCH_SIZE: u16 = 20;

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::object::WeakRef;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug)]
    pub struct Timeline {
        pub room: WeakRef<Room>,
        /// The underlying SDK timeline.
        pub timeline: OnceCell<Arc<SdkTimeline>>,
        /// Items added at the start of the timeline.
        pub start_items: gio::ListStore,
        /// Items provided by the SDK timeline.
        pub sdk_items: gio::ListStore,
        /// Items added at the end of the timeline.
        pub end_items: gio::ListStore,
        /// The `GListModel` containing all the timeline items.
        pub items: gtk::FlattenListModel,
        /// A Hashmap linking `EventKey` to corresponding `Event`
        pub event_map: RefCell<HashMap<EventKey, Event>>,
        pub state: Cell<TimelineState>,
        /// Whether this timeline has a typing row.
        pub has_typing: Cell<bool>,
    }

    impl Default for Timeline {
        fn default() -> Self {
            let start_items = gio::ListStore::new(TimelineItem::static_type());
            let sdk_items = gio::ListStore::new(TimelineItem::static_type());
            let end_items = gio::ListStore::new(TimelineItem::static_type());

            let model_list = gio::ListStore::new(gio::ListModel::static_type());
            model_list.append(&start_items);
            model_list.append(&sdk_items);
            model_list.append(&end_items);

            Self {
                room: Default::default(),
                timeline: Default::default(),
                start_items,
                sdk_items,
                end_items,
                items: gtk::FlattenListModel::new(Some(model_list)),
                event_map: Default::default(),
                state: Default::default(),
                has_typing: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Timeline {
        const NAME: &'static str = "Timeline";
        type Type = super::Timeline;
    }

    impl ObjectImpl for Timeline {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Room>("room")
                        .construct_only()
                        .build(),
                    glib::ParamSpecObject::builder::<gio::ListModel>("items")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("empty").read_only().build(),
                    glib::ParamSpecEnum::builder::<TimelineState>("state")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "room" => self.obj().set_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "room" => obj.room().to_value(),
                "items" => obj.items().to_value(),
                "empty" => obj.is_empty().to_value(),
                "state" => obj.state().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// All loaded items in a room.
    ///
    /// There is no strict message ordering enforced by the Timeline; items
    /// will be appended/prepended to existing items in the order they are
    /// received by the server.
    pub struct Timeline(ObjectSubclass<imp::Timeline>);
}

impl Timeline {
    pub fn new(room: &Room) -> Self {
        glib::Object::builder().property("room", room).build()
    }

    /// The `GListModel` containing the timeline items.
    pub fn items(&self) -> &gio::ListModel {
        self.imp().items.upcast_ref()
    }

    /// Update this `Timeline` with the given diff.
    fn update(&self, diff: VectorDiff<Arc<SdkTimelineItem>>) {
        let imp = self.imp();
        let sdk_items = &imp.sdk_items;
        let room = self.room();
        let was_empty = self.is_empty();

        match diff {
            VectorDiff::Append { values } => {
                let new_list = values
                    .into_iter()
                    .map(|item| self.create_item(&item))
                    .collect::<Vec<_>>();

                // Try to update the latest unread message.
                room.update_latest_unread(
                    new_list.iter().filter_map(|i| i.downcast_ref::<Event>()),
                );

                let pos = sdk_items.n_items();
                let added = new_list.len() as u32;

                sdk_items.extend_from_slice(&new_list);
                self.update_items_headers(pos, added.max(1));
            }
            VectorDiff::Clear => {
                self.clear();
            }
            VectorDiff::PushFront { value } => {
                let item = self.create_item(&value);

                // Try to update the latest unread message.
                if let Some(event) = item.downcast_ref::<Event>() {
                    room.update_latest_unread([event]);
                }

                sdk_items.insert(0, &item);
                self.update_items_headers(0, 1);
            }
            VectorDiff::PushBack { value } => {
                let item = self.create_item(&value);

                // Try to update the latest unread message.
                if let Some(event) = item.downcast_ref::<Event>() {
                    room.update_latest_unread([event]);
                }

                let pos = sdk_items.n_items();
                sdk_items.append(&item);
                self.update_items_headers(pos, 1);
            }
            VectorDiff::PopFront => {
                let item = sdk_items.item(0).and_downcast().unwrap();
                self.remove_item(&item);

                sdk_items.remove(0);
                self.update_items_headers(0, 1);
            }
            VectorDiff::PopBack => {
                let pos = sdk_items.n_items() - 1;
                let item = sdk_items.item(pos).and_downcast().unwrap();
                self.remove_item(&item);

                sdk_items.remove(pos);
            }
            VectorDiff::Insert { index, value } => {
                let pos = index as u32;
                let item = self.create_item(&value);

                // Try to update the latest unread message.
                if let Some(event) = item.downcast_ref::<Event>() {
                    room.update_latest_unread([event]);
                }

                sdk_items.insert(pos, &item);
                self.update_items_headers(pos, 1);
            }
            VectorDiff::Set { index, value } => {
                let pos = index as u32;
                let prev_item = sdk_items.item(pos).and_downcast::<TimelineItem>().unwrap();

                let item = if !prev_item.try_update_with(&value) {
                    self.remove_item(&prev_item);
                    let item = self.create_item(&value);

                    sdk_items.splice(pos, 1, &[item.clone()]);

                    item
                } else {
                    prev_item
                };

                // Try to update the latest unread message.
                if let Some(event) = item.downcast_ref::<Event>() {
                    room.update_latest_unread([event]);
                }

                // The item's header visibility might have changed.
                self.update_items_headers(pos, 1);
            }
            VectorDiff::Remove { index } => {
                let pos = index as u32;
                let item = sdk_items.item(pos).and_downcast().unwrap();
                self.remove_item(&item);

                sdk_items.remove(pos);
                self.update_items_headers(pos, 1);
            }
            VectorDiff::Reset { values } => {
                let new_list = values
                    .into_iter()
                    .map(|item| self.create_item(&item))
                    .collect::<Vec<_>>();

                // Try to update the latest unread message.
                room.update_latest_unread(
                    new_list.iter().filter_map(|i| i.downcast_ref::<Event>()),
                );

                let removed = sdk_items.n_items();
                let added = new_list.len() as u32;

                sdk_items.splice(0, removed, &new_list);
                self.update_items_headers(0, added.max(1));
            }
        }

        if self.is_empty() != was_empty {
            self.notify("empty");
        }
    }

    /// Update `nb` items' headers starting at `pos`.
    fn update_items_headers(&self, pos: u32, nb: u32) {
        let sdk_items = &self.imp().sdk_items;

        let mut previous_sender = if pos > 0 {
            sdk_items
                .item(pos - 1)
                .and_downcast::<TimelineItem>()
                .filter(|item| item.can_hide_header())
                .and_then(|item| item.event_sender())
        } else {
            None
        };

        // Update the headers of changed events plus the first event after them.
        for current_pos in pos..pos + nb + 1 {
            let Some(current) = sdk_items.item(current_pos).and_downcast::<TimelineItem>() else {
                break;
            };

            let current_sender = current.event_sender();

            if !current.can_hide_header() {
                current.set_show_header(false);
                previous_sender = None;
            } else if current_sender != previous_sender {
                current.set_show_header(true);
                previous_sender = current_sender;
            } else {
                current.set_show_header(false);
            }
        }
    }

    /// Create a `TimelineItem` in this `Timeline` from the given SDK timeline
    /// item.
    fn create_item(&self, item: &SdkTimelineItem) -> TimelineItem {
        let item = TimelineItem::new(item, &self.room());

        if let Some(event) = item.downcast_ref::<Event>() {
            self.imp()
                .event_map
                .borrow_mut()
                .insert(event.key(), event.clone());
        }

        item
    }

    /// Remove the given item from this `Timeline`.
    fn remove_item(&self, item: &TimelineItem) {
        if let Some(event) = item.downcast_ref::<Event>() {
            self.imp().event_map.borrow_mut().remove(&event.key());
        }
    }

    /// Load events at the start of the timeline.
    pub async fn load(&self) {
        let state = self.state();
        if matches!(
            state,
            TimelineState::Initial | TimelineState::Loading | TimelineState::Complete
        ) {
            // We don't want to load twice at the same time, and it's useless to try to load
            // more history before the timeline is ready or when we reached the
            // start.
            return;
        }

        self.set_state(TimelineState::Loading);

        let matrix_timeline = self.matrix_timeline();
        let handle = spawn_tokio!(async move {
            matrix_timeline
                .paginate_backwards(PaginationOptions::until_num_items(
                    MAX_BATCH_SIZE,
                    MAX_BATCH_SIZE,
                ))
                .await
        });

        if let Err(error) = handle.await.unwrap() {
            error!("Failed to load timeline: {error}");
            self.set_state(TimelineState::Error);
        }
    }

    fn clear(&self) {
        let imp = self.imp();

        imp.sdk_items.remove_all();
        imp.event_map.take();
    }

    /// Get the event with the given key from this `Timeline`.
    ///
    /// Use this method if you are sure the event has already been received.
    /// Otherwise use `fetch_event_by_id`.
    pub fn event_by_key(&self, key: &EventKey) -> Option<Event> {
        self.imp().event_map.borrow().get(key).cloned()
    }

    /// Get the position of the event with the given key in this `Timeline`.
    pub fn find_event_position(&self, key: &EventKey) -> Option<usize> {
        for (pos, item) in self.items().iter::<TimelineItem>().enumerate() {
            let Ok(item) = item else {
                break;
            };

            if let Some(event) = item.downcast_ref::<Event>() {
                if event.key() == *key {
                    return Some(pos);
                }
            }
        }

        None
    }

    /// Fetch the event with the given id.
    ///
    /// If the event can't be found locally, a request will be made to the
    /// homeserver.
    ///
    /// Use this method if you are not sure the event has already been received.
    /// Otherwise use `event_by_id`.
    pub async fn fetch_event_by_id(
        &self,
        event_id: OwnedEventId,
    ) -> Result<AnySyncTimelineEvent, MatrixError> {
        if let Some(event) = self.event_by_key(&EventKey::EventId(event_id.clone())) {
            event.raw().unwrap().deserialize().map_err(Into::into)
        } else {
            let room = self.room();
            let matrix_room = room.matrix_room();
            let event_id_clone = event_id.clone();
            let handle =
                spawn_tokio!(async move { matrix_room.event(event_id_clone.as_ref()).await });
            match handle.await.unwrap() {
                Ok(room_event) => room_event.event.deserialize_as().map_err(Into::into),
                Err(error) => {
                    // TODO: Retry on connection error?
                    warn!("Could not fetch event {event_id}: {error}");
                    Err(error)
                }
            }
        }
    }

    /// Set the room containing this timeline.
    fn set_room(&self, room: Option<Room>) {
        self.imp().room.set(room.as_ref());

        if let Some(room) = room {
            room.typing_list().connect_items_changed(
                clone!(@weak self as obj => move |list, _, _, _| {
                    if !list.is_empty() {
                        obj.add_typing_row();
                    }
                }),
            );
        }

        spawn!(clone!(@weak self as obj => async move {
            obj.setup_timeline().await;
        }));
    }

    /// Setup the underlying SDK timeline.
    async fn setup_timeline(&self) {
        let room = self.room();
        let room_id = room.room_id().to_owned();
        let matrix_room = room.matrix_room();

        let matrix_timeline = spawn_tokio!(async move {
            Arc::new(
                matrix_room
                    .timeline_builder()
                    .event_filter(|any| match any {
                        AnySyncTimelineEvent::MessageLike(msg) => match msg {
                            AnySyncMessageLikeEvent::RoomMessage(
                                SyncMessageLikeEvent::Original(ev),
                            ) => {
                                matches!(
                                    ev.content.msgtype,
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
                                )
                            }
                            AnySyncMessageLikeEvent::Sticker(SyncMessageLikeEvent::Original(_))
                            | AnySyncMessageLikeEvent::RoomEncrypted(
                                SyncMessageLikeEvent::Original(_),
                            ) => true,
                            _ => false,
                        },
                        AnySyncTimelineEvent::State(state) => matches!(
                            state,
                            AnySyncStateEvent::RoomMember(_)
                                | AnySyncStateEvent::RoomCreate(_)
                                | AnySyncStateEvent::RoomEncryption(_)
                                | AnySyncStateEvent::RoomThirdPartyInvite(_)
                                | AnySyncStateEvent::RoomTombstone(_)
                        ),
                    })
                    .add_failed_to_parse(false)
                    .build()
                    .await,
            )
        })
        .await
        .unwrap();

        self.imp().timeline.set(matrix_timeline.clone()).unwrap();

        let (mut sender, mut receiver) = futures_channel::mpsc::channel(100);
        let (values, timeline_stream) = matrix_timeline.subscribe().await;

        if !values.is_empty() {
            self.update(VectorDiff::Append { values });
        }

        let fut = timeline_stream.for_each(move |diff| {
            if let Err(error) = sender.try_send(diff) {
                error!("Error sending diff from timeline for room {room_id}: {error}");
                panic!();
            }

            async {}
        });
        spawn_tokio!(fut);

        self.set_state(TimelineState::Ready);

        spawn!(clone!(@weak self as obj => async move {
            obj.setup_back_pagination_status().await;
        }));

        while let Some(diff) = receiver.next().await {
            self.update(diff);
        }
    }

    /// Setup the back-pagination status.
    async fn setup_back_pagination_status(&self) {
        let room_id = self.room().room_id().to_owned();
        let matrix_timeline = self.matrix_timeline();

        let (mut sender, mut receiver) = futures_channel::mpsc::channel(8);
        let stream = matrix_timeline.back_pagination_status();

        let fut = stream.for_each(move |status| {
            if let Err(error) = sender.try_send(status) {
                error!("Error sending back-pagination status for room {room_id}: {error}");
                panic!();
            }

            async {}
        });
        spawn_tokio!(fut);

        while let Some(status) = receiver.next().await {
            match status {
                BackPaginationStatus::Idle => self.set_state(TimelineState::Ready),
                BackPaginationStatus::Paginating => self.set_state(TimelineState::Loading),
                BackPaginationStatus::TimelineStartReached => {
                    self.set_state(TimelineState::Complete)
                }
            }
        }
    }

    /// The room containing this timeline.
    pub fn room(&self) -> Room {
        self.imp().room.upgrade().unwrap()
    }

    /// The underlying SDK timeline.
    pub fn matrix_timeline(&self) -> Arc<SdkTimeline> {
        self.imp().timeline.get().unwrap().clone()
    }

    fn set_state(&self, state: TimelineState) {
        let imp = self.imp();
        let prev_state = self.state();

        if state == prev_state {
            return;
        }

        imp.state.set(state);

        let start_items = &imp.start_items;
        let removed = start_items.n_items();

        match state {
            TimelineState::Loading => start_items.splice(0, removed, &[VirtualItem::spinner()]),
            TimelineState::Complete => {
                start_items.splice(0, removed, &[VirtualItem::timeline_start()])
            }
            _ => start_items.remove_all(),
        }

        self.notify("state");
    }

    /// The state of the timeline.
    pub fn state(&self) -> TimelineState {
        self.imp().state.get()
    }

    /// Whether the timeline is empty.
    pub fn is_empty(&self) -> bool {
        self.imp().sdk_items.n_items() == 0
    }

    fn has_typing_row(&self) -> bool {
        self.imp().end_items.n_items() > 0
    }

    fn add_typing_row(&self) {
        if self.has_typing_row() {
            return;
        }

        self.imp().end_items.append(&VirtualItem::typing());
    }

    pub fn remove_empty_typing_row(&self) {
        if !self.has_typing_row() || !self.room().typing_list().is_empty() {
            return;
        }

        self.imp().end_items.remove_all();
    }
}

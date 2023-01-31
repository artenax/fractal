mod timeline_day_divider;
mod timeline_item;
mod timeline_new_messages_divider;
mod timeline_placeholder;

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use futures::StreamExt;
use futures_signals::signal_vec::{SignalVecExt, VecDiff};
use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use log::{error, warn};
use matrix_sdk::{
    room::timeline::{PaginationOptions, Timeline as SdkTimeline, TimelineItem as SdkTimelineItem},
    ruma::OwnedEventId,
    Error as MatrixError,
};
use ruma::events::AnySyncTimelineEvent;
pub use timeline_day_divider::TimelineDayDivider;
pub use timeline_item::{TimelineItem, TimelineItemExt, TimelineItemImpl};
pub use timeline_new_messages_divider::TimelineNewMessagesDivider;
pub use timeline_placeholder::{PlaceholderKind, TimelinePlaceholder};

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

    #[derive(Debug, Default)]
    pub struct Timeline {
        pub room: WeakRef<Room>,
        /// The underlying SDK timeline.
        pub timeline: OnceCell<Arc<SdkTimeline>>,
        /// All events shown in the room history
        pub list: RefCell<VecDeque<TimelineItem>>,
        /// A Hashmap linking `EventKey` to corresponding `Event`
        pub event_map: RefCell<HashMap<EventKey, Event>>,
        pub state: Cell<TimelineState>,
        /// Whether this timeline has a typing row.
        pub has_typing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Timeline {
        const NAME: &'static str = "Timeline";
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
                    glib::ParamSpecBoolean::builder("empty").read_only().build(),
                    glib::ParamSpecEnum::builder("state", TimelineState::default())
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
                "empty" => obj.is_empty().to_value(),
                "state" => obj.state().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for Timeline {
        fn item_type(&self) -> glib::Type {
            TimelineItem::static_type()
        }

        fn n_items(&self) -> u32 {
            let mut len = self.obj().n_items_in_list();

            if self.has_typing.get() {
                len += 1;
            }

            len
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            if self.has_typing.get() && position == self.obj().n_items() {
                return Some(TimelinePlaceholder::typing().upcast());
            }

            self.list
                .borrow()
                .iter()
                .filter(|item| item.is_visible())
                .nth(position as usize)
                .map(|o| o.clone().upcast())
        }
    }
}

glib::wrapper! {
    /// List of all loaded items in a room. Implements ListModel.
    ///
    /// There is no strict message ordering enforced by the Timeline; items
    /// will be appended/prepended to existing items in the order they are
    /// received by the server.
    pub struct Timeline(ObjectSubclass<imp::Timeline>)
        @implements gio::ListModel;
}

impl Timeline {
    pub fn new(room: &Room) -> Self {
        glib::Object::builder().property("room", &room).build()
    }

    /// The number of visible items in the list.
    ///
    /// This is like `n_items` without items not in the list (e.g. the typing
    /// indicator).
    fn n_items_in_list(&self) -> u32 {
        self.imp()
            .list
            .borrow()
            .iter()
            .filter(|item| item.is_visible())
            .count() as u32
    }

    fn items_changed(&self, position: u32, removed: u32, added: u32) {
        // Update the header for events that are allowed to hide the header
        {
            let position = position as usize;
            let added = added as usize;
            let list = self.imp().list.borrow();

            let mut previous_sender = if position > 0 {
                list.get(position - 1)
                    .filter(|item| item.can_hide_header())
                    .and_then(|item| item.event_sender())
            } else {
                None
            };

            for current in list.range(position..position + added) {
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

            // Update the events after the new events
            for next in list.range((position + added)..) {
                // After an event with non hideable header the visibility for headers will be
                // correct
                if !next.can_hide_header() {
                    break;
                }

                // Once the sender changes we can be sure that the visibility for headers will
                // be correct
                if next.event_sender() != previous_sender {
                    next.set_show_header(true);
                    break;
                }

                // The `next` has the same sender as the `current`, therefore we don't show the
                // header and we need to check the event after `next`
                next.set_show_header(false);
            }
        }

        self.notify("empty");

        self.upcast_ref::<gio::ListModel>()
            .items_changed(position, removed, added);
    }

    /// Update this `Timeline` with the given diff.
    fn update(&self, diff: VecDiff<Arc<SdkTimelineItem>>) {
        let imp = self.imp();
        let list = &imp.list;

        match diff {
            VecDiff::Replace { values } => {
                let removed = self.n_items_in_list();
                let new_list = values
                    .into_iter()
                    .map(|item| self.create_item(&item))
                    .collect::<VecDeque<_>>();
                let added = new_list.iter().filter(|item| item.is_visible()).count();

                *list.borrow_mut() = new_list;

                self.items_changed(0, removed, added as u32);
            }
            VecDiff::InsertAt { index, value } => {
                let item = self.create_item(&value);
                let visible = item.is_visible();
                list.borrow_mut().insert(index, item);

                if visible {
                    self.items_changed(index as u32, 0, 1);
                }
            }
            VecDiff::UpdateAt { index, value } => {
                let prev_item = list.borrow()[index].clone();
                let pos = self.find_item_position(&prev_item);

                let changed = if !prev_item.try_update_with(&value) {
                    self.remove_item(&prev_item);
                    list.borrow_mut()[index] = self.create_item(&value);

                    true
                } else {
                    false
                };

                let new_item = list.borrow()[index].clone();

                if let Some(pos) = pos {
                    let pos = pos as u32;

                    if !new_item.is_visible() {
                        // The item was visible but is not anymore, remove it.
                        self.items_changed(pos, 1, 0);
                    } else if changed {
                        // The item is still visible but has changed.
                        self.items_changed(pos, 1, 1);
                    }
                } else if new_item.is_visible() {
                    // The item is now visible.
                    let pos = self.find_item_position(&new_item).unwrap();
                    self.items_changed(pos as u32, 0, 1);
                }
            }
            VecDiff::Push { value } => {
                let visible = {
                    let mut list = list.borrow_mut();
                    let item = self.create_item(&value);
                    let visible = item.is_visible();
                    list.push_back(item);
                    visible
                };

                if visible {
                    self.items_changed(self.n_items_in_list() - 1, 0, 1);
                }
            }
            VecDiff::RemoveAt { index } => {
                let item = list.borrow().get(index).unwrap().clone();
                let pos = self.find_item_position(&item);
                let item = list.borrow_mut().remove(index).unwrap();
                self.remove_item(&item);

                if let Some(pos) = pos {
                    self.items_changed(pos as u32, 1, 0);
                }
            }
            VecDiff::Move {
                old_index,
                new_index,
            } => {
                let item = list.borrow_mut().remove(old_index).unwrap();
                let visible = item.is_visible();
                if visible {
                    self.items_changed(old_index as u32, 1, 0);
                }

                list.borrow_mut().insert(new_index, item);
                if visible {
                    self.items_changed(new_index as u32, 0, 1);
                }
            }
            VecDiff::Pop {} => {
                let visible = {
                    let mut list = list.borrow_mut();
                    let item = list.pop_back().unwrap();
                    self.remove_item(&item);
                    item.is_visible()
                };

                if visible {
                    self.items_changed(self.n_items_in_list(), 1, 0);
                }
            }
            VecDiff::Clear {} => {
                self.clear();
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
        } else if item
            .downcast_ref::<TimelinePlaceholder>()
            .filter(|item| item.kind() == PlaceholderKind::Spinner)
            .is_some()
            && self.state() == TimelineState::Loading
        {
            self.set_state(TimelineState::Ready)
        }
    }

    /// Load events at the start of the timeline.
    pub async fn load(&self) {
        if matches!(
            self.state(),
            TimelineState::Loading | TimelineState::Complete
        ) {
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

        let count = self.n_items_in_list();
        imp.list.take();
        imp.event_map.take();
        self.set_state(TimelineState::Initial);

        self.notify("empty");
        self.upcast_ref::<gio::ListModel>()
            .items_changed(0, count, 0);
    }

    /// Get the event with the given key from this `Timeline`.
    ///
    /// Use this method if you are sure the event has already been received.
    /// Otherwise use `fetch_event_by_id`.
    pub fn event_by_key(&self, key: &EventKey) -> Option<Event> {
        self.imp().event_map.borrow().get(key).cloned()
    }

    /// Get the position of the given item in this `Timeline`.
    pub fn find_item_position(&self, item: &TimelineItem) -> Option<usize> {
        self.imp()
            .list
            .borrow()
            .iter()
            .filter(|item| item.is_visible())
            .enumerate()
            .find_map(|(pos, list_item)| (item == list_item).then_some(pos))
    }

    /// Get the position of the event with the given key in this `Timeline`.
    pub fn find_event_position(&self, key: &EventKey) -> Option<usize> {
        self.imp()
            .list
            .borrow()
            .iter()
            .filter(|item| item.is_visible())
            .enumerate()
            .find_map(|(pos, item)| {
                item.downcast_ref::<Event>()
                    .filter(|event| event.key() == *key)
                    .map(|_| pos)
            })
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
                    warn!("Could not fetch event {}: {}", event_id, error);
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

        let matrix_timeline = spawn_tokio!(async move { Arc::new(matrix_room.timeline().await) })
            .await
            .unwrap();

        self.imp().timeline.set(matrix_timeline.clone()).unwrap();

        let (mut sender, mut receiver) = futures::channel::mpsc::channel(100);
        let fut = matrix_timeline.signal().for_each(move |diff| {
            if let Err(error) = sender.try_send(diff) {
                error!("Error sending diff from timeline for room {room_id}: {error}");
                panic!();
            }

            async {}
        });
        spawn_tokio!(fut);

        while let Some(diff) = receiver.next().await {
            self.update(diff)
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

        if state == self.state() {
            return;
        }

        imp.state.set(state);

        self.notify("state");
    }

    /// The state of the timeline.
    pub fn state(&self) -> TimelineState {
        self.imp().state.get()
    }

    /// Whether the timeline is empty.
    pub fn is_empty(&self) -> bool {
        self.n_items() == 0
    }

    fn has_typing_row(&self) -> bool {
        self.imp().has_typing.get()
    }

    fn add_typing_row(&self) {
        if self.has_typing_row() {
            return;
        }

        let pos = self.n_items();
        self.imp().has_typing.set(true);
        self.upcast_ref::<gio::ListModel>().items_changed(pos, 0, 1);
    }

    pub fn remove_empty_typing_row(&self) {
        if !self.has_typing_row() || !self.room().typing_list().is_empty() {
            return;
        }

        let pos = self.n_items() - 1;
        self.imp().has_typing.set(false);
        self.upcast_ref::<gio::ListModel>().items_changed(pos, 1, 0);
    }
}

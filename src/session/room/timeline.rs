use std::{
    collections::{HashMap, HashSet, VecDeque},
    pin::Pin,
    sync::Arc,
};

use futures::{lock::Mutex, pin_mut, Stream, StreamExt};
use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use log::{error, warn};
use matrix_sdk::{
    deserialized_responses::SyncRoomEvent,
    ruma::{
        events::{room::message::MessageType, AnySyncMessageEvent, AnySyncRoomEvent},
        identifiers::{EventId, TransactionId},
    },
    Error as MatrixError,
};
use tokio::task::JoinHandle;

use crate::{
    session::{
        room::{Event, Item, ItemType, Room},
        user::UserExt,
        verification::{IdentityVerification, VERIFICATION_CREATION_TIMEOUT},
    },
    spawn_tokio,
};

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "TimelineState")]
pub enum TimelineState {
    Initial,
    Loading,
    Ready,
    Error,
    Complete,
}

impl Default for TimelineState {
    fn default() -> Self {
        TimelineState::Initial
    }
}

const MAX_BATCH_SIZE: usize = 20;
type BackwardStream =
    Pin<Box<dyn Stream<Item = Vec<matrix_sdk::Result<SyncRoomEvent>>> + 'static + Send>>;

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::object::WeakRef;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct Timeline {
        pub room: OnceCell<WeakRef<Room>>,
        /// A store to keep track of related events that aren't known
        pub relates_to_events: RefCell<HashMap<Box<EventId>, Vec<Box<EventId>>>>,
        /// All events shown in the room history
        pub list: RefCell<VecDeque<Item>>,
        /// A Hashmap linking `EventId` to corresponding `Event`
        pub event_map: RefCell<HashMap<Box<EventId>, Event>>,
        /// Maps the temporary `EventId` of the pending Event to the real
        /// `EventId`
        pub pending_events: RefCell<HashMap<Box<TransactionId>, Box<EventId>>>,
        /// A Hashset of `EventId`s that where just redacted.
        pub redacted_events: RefCell<HashSet<Box<EventId>>>,
        pub state: Cell<TimelineState>,
        /// The most recent verification request event
        pub verification: RefCell<Option<IdentityVerification>>,
        pub backward_stream: Arc<Mutex<Option<BackwardStream>>>,
        pub forward_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Timeline {
        const NAME: &'static str = "Timeline";
        type Type = super::Timeline;
        type ParentType = glib::Object;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for Timeline {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::new(
                        "room",
                        "Room",
                        "The Room containing this timeline",
                        Room::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecBoolean::new(
                        "empty",
                        "Empty",
                        "Whether the timeline is empty",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecEnum::new(
                        "state",
                        "State",
                        "The state the timeline is in",
                        TimelineState::static_type(),
                        TimelineState::default() as i32,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "verification",
                        "Verification",
                        "The most recent active verification for a user in this timeline",
                        IdentityVerification::static_type(),
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
                "room" => {
                    let room = value.get::<Room>().unwrap();
                    obj.set_room(room);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room" => obj.room().to_value(),
                "empty" => obj.is_empty().to_value(),
                "state" => obj.state().to_value(),
                "verification" => obj.verification().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for Timeline {
        fn item_type(&self, _list_model: &Self::Type) -> glib::Type {
            Item::static_type()
        }
        fn n_items(&self, _list_model: &Self::Type) -> u32 {
            self.list.borrow().len() as u32
        }
        fn item(&self, _list_model: &Self::Type, position: u32) -> Option<glib::Object> {
            let list = self.list.borrow();

            list.get(position as usize)
                .map(|o| o.clone().upcast::<glib::Object>())
        }
    }
}

glib::wrapper! {
    /// List of all loaded Events in a room. Implements ListModel.
    ///
    /// There is no strict message ordering enforced by the Timeline; events
    /// will be appended/prepended to existing events in the order they are
    /// received by the server.
    ///
    /// This struct additionally keeps track of pending events that have yet to
    /// get an event ID assigned from the server.
    pub struct Timeline(ObjectSubclass<imp::Timeline>)
        @implements gio::ListModel;
}

// TODO:
// - [ ] Add and handle AnyEphemeralRoomEvent this includes read recipes
// - [ ] Add new message divider
impl Timeline {
    pub fn new(room: &Room) -> Self {
        glib::Object::new(&[("room", &room)]).expect("Failed to create Timeline")
    }

    fn items_changed(&self, position: u32, removed: u32, added: u32) {
        let priv_ = self.imp();

        let last_new_message_date;

        // Insert date divider, this needs to happen before updating the position and
        // headers
        let added = {
            let position = position as usize;
            let added = added as usize;
            let mut list = priv_.list.borrow_mut();

            let mut previous_timestamp = if position > 0 {
                list.get(position - 1)
                    .and_then(|item| item.event_timestamp())
            } else {
                None
            };
            let mut divider: Vec<(usize, Item)> = vec![];
            let mut index = position;
            for current in list.range(position..position + added) {
                if let Some(current_timestamp) = current.event_timestamp() {
                    if Some(current_timestamp.ymd()) != previous_timestamp.as_ref().map(|t| t.ymd())
                    {
                        divider.push((index, Item::for_day_divider(current_timestamp.clone())));
                        previous_timestamp = Some(current_timestamp);
                    }
                }
                index += 1;
            }

            let divider_len = divider.len();
            last_new_message_date = divider.last().and_then(|item| match item.1.type_() {
                ItemType::DayDivider(date) => Some(date.clone()),
                _ => None,
            });
            for (added, (position, date)) in divider.into_iter().enumerate() {
                list.insert(position + added, date);
            }

            (added + divider_len) as u32
        };

        // Remove first day divider if a new one is added earlier with the same day
        let removed = {
            let mut list = priv_.list.borrow_mut();
            if let Some(ItemType::DayDivider(date)) = list
                .get(position as usize + added as usize)
                .map(|item| item.type_())
            {
                if Some(date.ymd()) == last_new_message_date.as_ref().map(|date| date.ymd()) {
                    list.remove(position as usize + added as usize);
                    removed + 1
                } else {
                    removed
                }
            } else {
                removed
            }
        };

        // Update the header for events that are allowed to hide the header
        {
            let position = position as usize;
            let added = added as usize;
            let list = priv_.list.borrow();

            let mut previous_sender = if position > 0 {
                list.get(position - 1)
                    .filter(|event| event.can_hide_header())
                    .and_then(|event| event.matrix_sender())
            } else {
                None
            };

            for current in list.range(position..position + added) {
                let current_sender = current.matrix_sender();

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
                // After an event with non hiddable header the visibility for headers will be
                // correct
                if !next.can_hide_header() {
                    break;
                }

                // Once the sender changes we can be sure that the visibility for headers will
                // be correct
                if next.matrix_sender() != previous_sender {
                    next.set_show_header(true);
                    break;
                }

                // The `next` has the same sender as the `current`, therefore we don't show the
                // header and we need to check the event after `next`
                next.set_show_header(false);
            }
        }

        // Add relations to event
        {
            let list = priv_.list.borrow();
            let mut relates_to_events = priv_.relates_to_events.borrow_mut();
            let mut redacted_events = priv_.redacted_events.borrow_mut();

            for event in list
                .range(position as usize..(position + added) as usize)
                .filter_map(|item| item.event())
            {
                if let Some(relates_to) = relates_to_events.remove(&event.matrix_event_id()) {
                    let mut replacing_events: Vec<Event> = vec![];
                    let mut reactions: Vec<Event> = vec![];

                    for relation_event_id in relates_to {
                        let relation = self
                            .event_by_id(&relation_event_id)
                            .expect("Previously known event has disappeared");

                        if relation.is_replacing_event() {
                            replacing_events.push(relation);
                        } else if relation.is_reaction() {
                            reactions.push(relation);
                        }
                    }

                    if position != 0 || event.replacing_events().is_empty() {
                        event.append_replacing_events(replacing_events);
                    } else {
                        event.prepend_replacing_events(replacing_events);
                    }
                    event.add_reactions(reactions);

                    if event.redacted() {
                        redacted_events.insert(event.matrix_event_id());
                    }
                }
            }
        }

        self.notify("empty");

        self.upcast_ref::<gio::ListModel>()
            .items_changed(position, removed, added);

        self.remove_redacted_events();
    }

    fn remove_redacted_events(&self) {
        let priv_ = self.imp();
        let mut redacted_events_pos = Vec::with_capacity(priv_.redacted_events.borrow().len());

        // Find redacted events in the list
        {
            let mut redacted_events = priv_.redacted_events.borrow_mut();
            let list = priv_.list.borrow();
            let mut i = list.len();
            let mut list = list.iter();

            while let Some(item) = list.next_back() {
                if let ItemType::Event(event) = item.type_() {
                    if redacted_events.remove(&event.matrix_event_id()) {
                        redacted_events_pos.push(i - 1);
                    }
                    if redacted_events.is_empty() {
                        break;
                    }
                }
                i -= 1;
            }
        }

        let mut redacted_events_pos = &mut redacted_events_pos[..];
        // Sort positions to start from the end so positions are still valid
        // and to group calls to `items_changed`.
        redacted_events_pos.sort_unstable_by(|a, b| b.partial_cmp(a).unwrap());
        while let Some(pos) = redacted_events_pos.first() {
            let mut pos = pos.to_owned();
            let mut removed = 1;

            {
                let mut list = priv_.list.borrow_mut();
                list.remove(pos);

                // Remove all consecutive previous redacted events.
                while let Some(next_pos) = redacted_events_pos.get(removed) {
                    if next_pos == &(pos - 1) {
                        pos -= 1;
                        removed += 1;
                        list.remove(pos);
                    } else {
                        break;
                    }
                }
                redacted_events_pos = &mut redacted_events_pos[removed..];

                // Remove the day divider before this event if it's not useful anymore.
                let day_divider_before = pos >= 1
                    && matches!(
                        list.get(pos - 1).map(|item| item.type_()),
                        Some(ItemType::DayDivider(_))
                    );
                let after = pos == list.len()
                    || matches!(
                        list.get(pos).map(|item| item.type_()),
                        Some(ItemType::DayDivider(_))
                    );

                if day_divider_before && after {
                    pos -= 1;
                    removed += 1;
                    list.remove(pos);
                }
            }

            self.upcast_ref::<gio::ListModel>()
                .items_changed(pos as u32, removed as u32, 0);
        }
    }

    fn add_hidden_events(&self, events: Vec<Event>, at_front: bool) {
        let priv_ = self.imp();
        let mut relates_to_events = priv_.relates_to_events.borrow_mut();

        // Group events by related event
        let mut new_relations: HashMap<Box<EventId>, Vec<Event>> = HashMap::new();
        for event in events {
            if let Some(relates_to) = relates_to_events.remove(&event.matrix_event_id()) {
                let mut replacing_events: Vec<Event> = vec![];
                let mut reactions: Vec<Event> = vec![];

                for relation_event_id in relates_to {
                    let relation = self
                        .event_by_id(&relation_event_id)
                        .expect("Previously known event has disappeared");

                    if relation.is_replacing_event() {
                        replacing_events.push(relation);
                    } else if relation.is_reaction() {
                        reactions.push(relation);
                    }
                }

                if !at_front || event.replacing_events().is_empty() {
                    event.append_replacing_events(replacing_events);
                } else {
                    event.prepend_replacing_events(replacing_events);
                }
                event.add_reactions(reactions);
            }

            if let Some(relates_to_event) = event.related_matrix_event() {
                let relations = new_relations.entry(relates_to_event).or_default();
                relations.push(event);
            }
        }

        // Handle new relations
        let mut redacted_events = priv_.redacted_events.borrow_mut();
        for (relates_to_event_id, new_relations) in new_relations {
            if let Some(relates_to_event) = self.event_by_id(&relates_to_event_id) {
                // Get the relations in relates_to_event otherwise they will be added in
                // in items_changed and they might not be added at the right place.
                let mut relations: Vec<Event> = relates_to_events
                    .remove(&relates_to_event.matrix_event_id())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|event_id| {
                        self.event_by_id(&event_id)
                            .expect("Previously known event has disappeared")
                    })
                    .collect();

                if at_front {
                    relations.splice(..0, new_relations);
                } else {
                    relations.extend(new_relations);
                }

                let mut replacing_events: Vec<Event> = vec![];
                let mut reactions: Vec<Event> = vec![];

                for relation in relations {
                    if relation.is_replacing_event() {
                        replacing_events.push(relation);
                    } else if relation.is_reaction() {
                        reactions.push(relation);
                    }
                }

                if !at_front || relates_to_event.replacing_events().is_empty() {
                    relates_to_event.append_replacing_events(replacing_events);
                } else {
                    relates_to_event.prepend_replacing_events(replacing_events);
                }
                relates_to_event.add_reactions(reactions);

                if relates_to_event.redacted() {
                    redacted_events.insert(relates_to_event.matrix_event_id());
                }
            } else {
                // Store the new event if the `related_to` event isn't known, we will update the
                // `relates_to` once the `related_to` event is added to the list
                let relates_to_event = relates_to_events.entry(relates_to_event_id).or_default();

                let relations_ids: Vec<Box<EventId>> = new_relations
                    .iter()
                    .map(|event| event.matrix_event_id())
                    .collect();
                if at_front {
                    relates_to_event.splice(..0, relations_ids);
                } else {
                    relates_to_event.extend(relations_ids);
                }
            }
        }
    }

    /// Load the timeline
    /// This function should also be called to load more events
    /// Returns `true` when messages where successfully added
    pub async fn load(&self) -> bool {
        let priv_ = self.imp();

        if matches!(
            self.state(),
            TimelineState::Loading | TimelineState::Complete
        ) {
            return false;
        }

        self.set_state(TimelineState::Loading);
        self.add_loading_spinner();

        let matrix_room = self.room().matrix_room();
        let timeline_weak = self.downgrade().into();
        let backward_stream = priv_.backward_stream.clone();
        let forward_handle = priv_.forward_handle.clone();

        let handle: tokio::task::JoinHandle<matrix_sdk::Result<_>> = spawn_tokio!(async move {
            let mut backward_stream_guard = backward_stream.lock().await;
            let mut forward_handle_guard = forward_handle.lock().await;
            if backward_stream_guard.is_none() {
                let (forward_stream, backward_stream) = matrix_room.timeline().await?;

                let forward_handle = tokio::spawn(async move {
                    handle_forward_stream(timeline_weak, forward_stream).await;
                });

                if let Some(old_forward_handle) = forward_handle_guard.replace(forward_handle) {
                    old_forward_handle.abort();
                }

                backward_stream_guard
                    .replace(Box::pin(backward_stream.ready_chunks(MAX_BATCH_SIZE)));
            }

            Ok(backward_stream_guard.as_mut().unwrap().next().await)
        });

        match handle.await.unwrap() {
            Ok(Some(events)) => {
                let events: Vec<Event> = events
                    .into_iter()
                    .filter_map(|event| match event {
                        Ok(event) => Some(event),
                        Err(error) => {
                            error!("Failed to load messages: {}", error);
                            None
                        }
                    })
                    .map(|event| Event::new(event, &self.room()))
                    .collect();

                self.remove_loading_spinner();
                if events.is_empty() {
                    self.set_state(TimelineState::Error);
                    return false;
                }
                self.set_state(TimelineState::Ready);
                self.prepend(events);
                true
            }
            Ok(None) => {
                self.remove_loading_spinner();
                self.set_state(TimelineState::Complete);
                false
            }
            Err(error) => {
                error!("Failed to load timeline: {}", error);
                self.set_state(TimelineState::Error);
                self.remove_loading_spinner();
                false
            }
        }
    }

    async fn clear(&self) {
        let priv_ = self.imp();
        // Remove backward stream so that we create new streams
        let mut backward_stream = priv_.backward_stream.lock().await;
        backward_stream.take();

        let mut forward_handle = priv_.forward_handle.lock().await;
        if let Some(forward_handle) = forward_handle.take() {
            forward_handle.abort();
        }

        let length = priv_.list.borrow().len();
        priv_.relates_to_events.replace(HashMap::new());
        priv_.list.replace(VecDeque::new());
        priv_.event_map.replace(HashMap::new());
        priv_.pending_events.replace(HashMap::new());
        priv_.redacted_events.replace(HashSet::new());
        self.set_state(TimelineState::Initial);

        self.notify("empty");
        self.upcast_ref::<gio::ListModel>()
            .items_changed(0, length as u32, 0);
    }

    /// Append the new events
    pub fn append(&self, batch: Vec<Event>) {
        let priv_ = self.imp();

        if batch.is_empty() {
            return;
        }
        let mut added = batch.len();

        let index = {
            let index = {
                let mut list = priv_.list.borrow_mut();
                // Extend the size of the list so that rust doesn't need to reallocate memory
                // multiple times
                list.reserve(batch.len());

                list.len()
            };

            let mut pending_events = priv_.pending_events.borrow_mut();
            let mut hidden_events: Vec<Event> = vec![];

            for event in batch.into_iter() {
                let event_id = event.matrix_event_id();

                self.handle_verification(&event);

                if let Some(pending_id) = event
                    .matrix_transaction_id()
                    .and_then(|txn_id| pending_events.remove(&txn_id))
                {
                    let mut event_map = priv_.event_map.borrow_mut();

                    if let Some(pending_event) = event_map.remove(&pending_id) {
                        pending_event.set_matrix_pure_event(event.matrix_pure_event());
                        event_map.insert(event_id, pending_event);
                    };
                    added -= 1;
                } else {
                    priv_
                        .event_map
                        .borrow_mut()
                        .insert(event_id.to_owned(), event.clone());
                    if event.is_hidden_event() {
                        hidden_events.push(event);
                        added -= 1;
                    } else {
                        priv_.list.borrow_mut().push_back(Item::for_event(event));
                    }
                }
            }

            self.add_hidden_events(hidden_events, false);

            index
        };

        self.items_changed(index as u32, 0, added as u32);
    }

    /// Append an event that wasn't yet fully sent and received via a sync
    pub fn append_pending(&self, txn_id: &TransactionId, event: Event) {
        let priv_ = self.imp();

        priv_
            .event_map
            .borrow_mut()
            .insert(event.matrix_event_id(), event.clone());

        priv_
            .pending_events
            .borrow_mut()
            .insert(txn_id.to_owned(), event.matrix_event_id());

        let index = {
            let mut list = priv_.list.borrow_mut();
            let index = list.len();

            if event.is_hidden_event() {
                self.add_hidden_events(vec![event], false);
                None
            } else {
                list.push_back(Item::for_event(event));
                Some(index)
            }
        };

        if let Some(index) = index {
            self.items_changed(index as u32, 0, 1);
        }
    }

    /// Get the event with the given id from the local store.
    ///
    /// Use this method if you are sure the event has already been received.
    /// Otherwise use `fetch_event_by_id`.
    pub fn event_by_id(&self, event_id: &EventId) -> Option<Event> {
        self.imp().event_map.borrow().get(event_id).cloned()
    }

    /// Fetch the event with the given id.
    ///
    /// If the event can't be found locally, a request will be made to the
    /// homeserver.
    ///
    /// Use this method if you are not sure the event has already been received.
    /// Otherwise use `event_by_id`.
    pub async fn fetch_event_by_id(&self, event_id: &EventId) -> Result<Event, MatrixError> {
        if let Some(event) = self.event_by_id(event_id) {
            Ok(event)
        } else {
            let room = self.room();
            let matrix_room = room.matrix_room();
            let event_id_clone = event_id.to_owned();
            let handle =
                spawn_tokio!(async move { matrix_room.event(event_id_clone.as_ref()).await });
            match handle.await.unwrap() {
                Ok(room_event) => Ok(Event::new(room_event.into(), &room)),
                Err(error) => {
                    // TODO: Retry on connection error?
                    warn!("Could not fetch event {}: {}", event_id, error);
                    Err(error)
                }
            }
        }
    }

    /// Prepends a batch of events
    pub fn prepend(&self, batch: Vec<Event>) {
        let priv_ = self.imp();
        let mut added = batch.len();

        {
            let mut hidden_events: Vec<Event> = vec![];
            // Extend the size of the list so that rust doesn't need to reallocate memory
            // multiple times
            priv_.list.borrow_mut().reserve(added);

            for event in batch {
                self.handle_verification(&event);

                priv_
                    .event_map
                    .borrow_mut()
                    .insert(event.matrix_event_id(), event.clone());

                if event.is_hidden_event() {
                    hidden_events.push(event);
                    added -= 1;
                } else {
                    priv_.list.borrow_mut().push_front(Item::for_event(event));
                }
            }
            self.add_hidden_events(hidden_events, true);
        }

        self.items_changed(0, 0, added as u32);
    }

    fn set_room(&self, room: Room) {
        self.imp().room.set(room.downgrade()).unwrap();
    }

    pub fn room(&self) -> Room {
        self.imp().room.get().unwrap().upgrade().unwrap()
    }

    fn set_state(&self, state: TimelineState) {
        let priv_ = self.imp();

        if state == self.state() {
            return;
        }

        priv_.state.set(state);

        self.notify("state");
    }

    // The state of the timeline
    pub fn state(&self) -> TimelineState {
        self.imp().state.get()
    }

    pub fn is_empty(&self) -> bool {
        let priv_ = self.imp();
        priv_.list.borrow().is_empty()
            || (priv_.list.borrow().len() == 1 && self.state() == TimelineState::Loading)
    }

    fn add_loading_spinner(&self) {
        self.imp()
            .list
            .borrow_mut()
            .push_front(Item::for_loading_spinner());
        self.upcast_ref::<gio::ListModel>().items_changed(0, 0, 1);
    }

    fn remove_loading_spinner(&self) {
        self.imp().list.borrow_mut().pop_front();
        self.upcast_ref::<gio::ListModel>().items_changed(0, 1, 0);
    }

    fn set_verification(&self, verification: IdentityVerification) {
        self.imp().verification.replace(Some(verification));
        self.notify("verification");
    }

    pub fn verification(&self) -> Option<IdentityVerification> {
        self.imp().verification.borrow().clone()
    }

    fn handle_verification(&self, event: &Event) {
        let message = if let Some(AnySyncRoomEvent::Message(message)) = event.matrix_event() {
            message
        } else {
            return;
        };

        let session = self.room().session();
        let verification_list = session.verification_list();

        let request = match message {
            AnySyncMessageEvent::RoomMessage(message) => {
                if let MessageType::VerificationRequest(request) = message.content.msgtype {
                    // Ignore request that are too old
                    if let Some(time) = message.origin_server_ts.to_system_time() {
                        if let Ok(duration) = time.elapsed() {
                            if duration > VERIFICATION_CREATION_TIMEOUT {
                                return;
                            }
                        } else {
                            warn!("Ignoring verification request because it was sent in the future. The system time of the server or the local machine is probably wrong.");
                            return;
                        }
                    } else {
                        return;
                    }

                    let user = session.user().unwrap();

                    let user_to_verify = if *request.to == *user.user_id() {
                        // The request was sent by another user to verify us
                        event.sender()
                    } else if *message.sender == *user.user_id() {
                        // The request was sent by us to verify another user
                        self.room().members().member_by_id(request.to.into())
                    } else {
                        // Ignore the request when it doesn't verify us or wasn't set by us
                        return;
                    };

                    // Ignore the request when we have a newer one
                    let previous_verification = self.verification();
                    if !(previous_verification.is_none()
                        || &event.timestamp() > previous_verification.unwrap().start_time())
                    {
                        return;
                    }

                    let request = if let Some(request) = verification_list
                        .get_by_id(&user_to_verify.user_id(), &event.matrix_event_id())
                    {
                        request
                    } else {
                        let request = IdentityVerification::for_flow_id(
                            event.matrix_event_id().as_str(),
                            &session,
                            &user_to_verify.upcast(),
                            &event.timestamp(),
                        );

                        verification_list.add(request.clone());
                        request
                    };

                    self.set_verification(request);
                }

                return;
            }
            AnySyncMessageEvent::KeyVerificationReady(e) => {
                verification_list.get_by_id(&e.sender, &e.content.relates_to.event_id)
            }
            AnySyncMessageEvent::KeyVerificationStart(e) => {
                verification_list.get_by_id(&e.sender, &e.content.relates_to.event_id)
            }
            AnySyncMessageEvent::KeyVerificationCancel(e) => {
                verification_list.get_by_id(&e.sender, &e.content.relates_to.event_id)
            }
            AnySyncMessageEvent::KeyVerificationAccept(e) => {
                verification_list.get_by_id(&e.sender, &e.content.relates_to.event_id)
            }
            AnySyncMessageEvent::KeyVerificationKey(e) => {
                verification_list.get_by_id(&e.sender, &e.content.relates_to.event_id)
            }
            AnySyncMessageEvent::KeyVerificationMac(e) => {
                verification_list.get_by_id(&e.sender, &e.content.relates_to.event_id)
            }
            AnySyncMessageEvent::KeyVerificationDone(e) => {
                verification_list.get_by_id(&e.sender, &e.content.relates_to.event_id)
            }
            _ => {
                return;
            }
        };

        if let Some(request) = request {
            request.notify_state();
        }
    }
}

async fn handle_forward_stream(
    timeline: glib::SendWeakRef<Timeline>,
    stream: impl Stream<Item = SyncRoomEvent>,
) {
    let stream = stream.ready_chunks(MAX_BATCH_SIZE);
    pin_mut!(stream);

    while let Some(events) = stream.next().await {
        let timeline = timeline.clone();
        let (sender, receiver) = futures::channel::oneshot::channel();
        let ctx = glib::MainContext::default();
        ctx.spawn(async move {
            let result = if let Some(timeline) = timeline.upgrade() {
                timeline.append(
                    events
                        .into_iter()
                        .map(|event| Event::new(event, &timeline.room()))
                        .collect(),
                );

                true
            } else {
                false
            };
            sender.send(result).unwrap();
        });

        if !receiver.await.unwrap() {
            break;
        }
    }

    let ctx = glib::MainContext::default();
    ctx.spawn(async move {
        crate::spawn!(async move {
            if let Some(timeline) = timeline.upgrade() {
                timeline.clear().await;
            };
        });
    });
}

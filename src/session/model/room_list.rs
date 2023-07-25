use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
};

use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use indexmap::map::IndexMap;
use log::error;
use matrix_sdk::{
    ruma::{OwnedRoomId, OwnedRoomOrAliasId, OwnedServerName, RoomAliasId, RoomId, RoomOrAliasId},
    sync::Rooms as ResponseRooms,
};

use crate::{
    gettext_f,
    session::model::{Room, Session},
    spawn_tokio,
};

mod imp {
    use std::cell::RefCell;

    use glib::{object::WeakRef, subclass::Signal};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct RoomList {
        pub list: RefCell<IndexMap<OwnedRoomId, Room>>,
        pub pending_rooms: RefCell<HashSet<OwnedRoomOrAliasId>>,
        pub tombstoned_rooms: RefCell<HashSet<OwnedRoomId>>,
        pub session: WeakRef<Session>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoomList {
        const NAME: &'static str = "RoomList";
        type Type = super::RoomList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for RoomList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Session>("session")
                    .construct_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "session" => self.session.set(value.get().ok().as_ref()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session" => self.obj().session().to_value(),
                _ => unimplemented!(),
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> =
                Lazy::new(|| vec![Signal::builder("pending-rooms-changed").build()]);
            SIGNALS.as_ref()
        }
    }

    impl ListModelImpl for RoomList {
        fn item_type(&self) -> glib::Type {
            Room::static_type()
        }
        fn n_items(&self) -> u32 {
            self.list.borrow().len() as u32
        }
        fn item(&self, position: u32) -> Option<glib::Object> {
            self.list
                .borrow()
                .get_index(position as usize)
                .map(|(_, v)| v.upcast_ref::<glib::Object>())
                .cloned()
        }
    }
}

glib::wrapper! {
    /// List of all joined rooms of the user.
    ///
    /// This is the parent ListModel of the sidebar from which all other models
    /// are derived.
    ///
    /// The `RoomList` also takes care of all so called *pending rooms*, i.e.
    /// rooms the user requested to join, but received no response from the
    /// server yet.
    pub struct RoomList(ObjectSubclass<imp::RoomList>)
        @implements gio::ListModel;
}

impl RoomList {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// The current session.
    pub fn session(&self) -> Session {
        self.imp().session.upgrade().unwrap()
    }

    pub fn is_pending_room(&self, identifier: &RoomOrAliasId) -> bool {
        self.imp().pending_rooms.borrow().contains(identifier)
    }

    fn pending_rooms_remove(&self, identifier: &RoomOrAliasId) {
        self.imp().pending_rooms.borrow_mut().remove(identifier);
        self.emit_by_name::<()>("pending-rooms-changed", &[]);
    }

    fn pending_rooms_insert(&self, identifier: OwnedRoomOrAliasId) {
        self.imp().pending_rooms.borrow_mut().insert(identifier);
        self.emit_by_name::<()>("pending-rooms-changed", &[]);
    }

    fn pending_rooms_replace_or_remove(&self, identifier: &RoomOrAliasId, room_id: &RoomId) {
        {
            let mut pending_rooms = self.imp().pending_rooms.borrow_mut();
            pending_rooms.remove(identifier);
            if !self.contains_key(room_id) {
                pending_rooms.insert(room_id.to_owned().into());
            }
        }
        self.emit_by_name::<()>("pending-rooms-changed", &[]);
    }

    /// Get the room with the given room ID, if any.
    pub fn get(&self, room_id: &RoomId) -> Option<Room> {
        self.imp().list.borrow().get(room_id).cloned()
    }

    /// Get the room with the given identifier, if any.
    pub fn get_by_identifier(&self, identifier: RoomIdentifier) -> Option<Room> {
        match identifier {
            RoomIdentifier::Id(room_id) => self.get(room_id),
            RoomIdentifier::Alias(room_alias) => self
                .imp()
                .list
                .borrow()
                .values()
                .find(|room| room.matrix_room().canonical_alias().as_deref() == Some(room_alias))
                .cloned(),
        }
    }

    /// Waits till the Room becomes available
    pub async fn get_wait(&self, room_id: &RoomId) -> Option<Room> {
        if let Some(room) = self.get(room_id) {
            Some(room)
        } else {
            let (sender, receiver) = futures::channel::oneshot::channel();

            let room_id = room_id.to_owned();
            let sender = Cell::new(Some(sender));
            // FIXME: add a timeout
            let handler_id = self.connect_items_changed(move |obj, _, _, _| {
                if let Some(room) = obj.get(&room_id) {
                    if let Some(sender) = sender.take() {
                        sender.send(Some(room)).unwrap();
                    }
                }
            });

            let room = receiver.await.unwrap();
            self.disconnect(handler_id);
            room
        }
    }

    pub fn contains_key(&self, room_id: &RoomId) -> bool {
        self.imp().list.borrow().contains_key(room_id)
    }

    pub fn remove(&self, room_id: &RoomId) {
        let imp = self.imp();

        let removed = {
            let mut list = imp.list.borrow_mut();

            list.shift_remove_full(room_id)
        };

        imp.tombstoned_rooms.borrow_mut().remove(room_id);

        if let Some((position, ..)) = removed {
            self.items_changed(position as u32, 1, 0);
        }
    }

    fn items_added(&self, added: usize) {
        let position = {
            let imp = self.imp();
            let list = imp.list.borrow();

            let position = list.len().saturating_sub(added);

            for (_room_id, room) in list.iter().skip(position) {
                room.connect_room_forgotten(clone!(@weak self as obj => move |room| {
                    obj.remove(room.room_id());
                }));
            }

            let mut to_remove = Vec::new();
            for room_id in imp.tombstoned_rooms.borrow().iter() {
                if let Some(room) = list.get(room_id) {
                    if room.update_outdated() {
                        to_remove.push(room_id.to_owned());
                    }
                } else {
                    to_remove.push(room_id.to_owned());
                }
            }

            if !to_remove.is_empty() {
                let mut tombstoned_rooms = imp.tombstoned_rooms.borrow_mut();
                for room_id in to_remove {
                    tombstoned_rooms.remove(&room_id);
                }
            }

            position
        };

        self.items_changed(position as u32, 0, added as u32);
    }

    /// Loads the state from the `Store`.
    ///
    /// Note that the `Store` currently doesn't store all events, therefore, we
    /// aren't really loading much via this function.
    pub fn load(&self) {
        let session = self.session();
        let client = session.client();
        let matrix_rooms = client.rooms();
        let added = matrix_rooms.len();

        if added > 0 {
            {
                let mut added = Vec::with_capacity(matrix_rooms.len());
                for matrix_room in matrix_rooms {
                    let room_id = matrix_room.room_id().to_owned();
                    let room = Room::new(&session, &room_id);
                    added.push((room_id, room));
                }
                self.imp().list.borrow_mut().extend(added);
            }

            self.items_added(added);
        }
    }

    pub fn handle_response_rooms(&self, rooms: ResponseRooms) {
        let session = self.session();

        let mut new_rooms = HashMap::new();

        for (room_id, left_room) in rooms.leave {
            let room = match self.get(&room_id) {
                Some(room) => room,
                None => new_rooms
                    .entry(room_id.clone())
                    .or_insert_with_key(|room_id| Room::new(&session, room_id))
                    .clone(),
            };

            self.pending_rooms_remove((*room_id).into());
            room.update_matrix_room();
            room.handle_left_response(left_room);
        }

        for (room_id, joined_room) in rooms.join {
            let room = match self.get(&room_id) {
                Some(room) => room,
                None => new_rooms
                    .entry(room_id.clone())
                    .or_insert_with_key(|room_id| Room::new(&session, room_id))
                    .clone(),
            };

            self.pending_rooms_remove((*room_id).into());
            room.update_matrix_room();
            room.handle_joined_response(joined_room);
        }

        for (room_id, _invited_room) in rooms.invite {
            let room = match self.get(&room_id) {
                Some(room) => room,
                None => new_rooms
                    .entry(room_id.clone())
                    .or_insert_with_key(|room_id| Room::new(&session, room_id))
                    .clone(),
            };

            self.pending_rooms_remove((*room_id).into());
            room.update_matrix_room();
        }

        if !new_rooms.is_empty() {
            let added = new_rooms.len();
            self.imp().list.borrow_mut().extend(new_rooms);
            self.items_added(added);
        }
    }

    /// Join the room with the given identifier.
    pub async fn join_by_id_or_alias(
        &self,
        identifier: OwnedRoomOrAliasId,
        via: Vec<OwnedServerName>,
    ) -> Result<(), String> {
        let client = self.session().client();
        let identifier_clone = identifier.clone();

        self.pending_rooms_insert(identifier.clone());

        let handle = spawn_tokio!(async move {
            client
                .join_room_by_id_or_alias(&identifier_clone, &via)
                .await
        });

        match handle.await.unwrap() {
            Ok(matrix_room) => {
                self.pending_rooms_replace_or_remove(&identifier, matrix_room.room_id());
                Ok(())
            }
            Err(error) => {
                self.pending_rooms_remove(&identifier);
                error!("Joining room {identifier} failed: {error}");

                let error = gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    "Failed to join room {room_name}. Try again later.",
                    &[("room_name", identifier.as_str())],
                );

                Err(error)
            }
        }
    }

    pub fn connect_pending_rooms_changed<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("pending-rooms-changed", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();

            f(&obj);

            None
        })
    }

    /// Get the room with the given identifier, if it is joined.
    pub fn joined_room(&self, identifier: RoomIdentifier) -> Option<Room> {
        self.get_by_identifier(identifier)
            .filter(|room| room.is_joined())
    }

    /// Add a room that was tombstoned but for which we haven't joined the
    /// successor yet.
    pub fn add_tombstoned_room(&self, room_id: OwnedRoomId) {
        self.imp().tombstoned_rooms.borrow_mut().insert(room_id);
    }
}

/// A unique identifier for a room.
#[derive(Debug, Clone, Copy)]
pub enum RoomIdentifier<'a> {
    /// A room ID.
    Id(&'a RoomId),
    /// A room alias.
    Alias(&'a RoomAliasId),
}

impl<'a> From<&'a RoomId> for RoomIdentifier<'a> {
    fn from(value: &'a RoomId) -> Self {
        Self::Id(value)
    }
}

impl<'a> From<&'a RoomAliasId> for RoomIdentifier<'a> {
    fn from(value: &'a RoomAliasId) -> Self {
        Self::Alias(value)
    }
}

impl<'a> From<&'a RoomOrAliasId> for RoomIdentifier<'a> {
    fn from(value: &'a RoomOrAliasId) -> Self {
        if value.is_room_id() {
            RoomIdentifier::Id(value.as_str().try_into().unwrap())
        } else {
            RoomIdentifier::Alias(value.as_str().try_into().unwrap())
        }
    }
}

impl<'a> From<RoomIdentifier<'a>> for &'a RoomOrAliasId {
    fn from(value: RoomIdentifier<'a>) -> Self {
        match value {
            RoomIdentifier::Id(id) => id.into(),
            RoomIdentifier::Alias(alias) => alias.into(),
        }
    }
}

impl<'a> From<RoomIdentifier<'a>> for OwnedRoomOrAliasId {
    fn from(value: RoomIdentifier<'a>) -> Self {
        match value {
            RoomIdentifier::Id(id) => id.to_owned().into(),
            RoomIdentifier::Alias(alias) => alias.to_owned().into(),
        }
    }
}

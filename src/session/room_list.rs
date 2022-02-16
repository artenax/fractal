use std::{cell::Cell, collections::HashSet};

use gettextrs::gettext;
use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use indexmap::map::IndexMap;
use log::error;
use matrix_sdk::{
    deserialized_responses::Rooms as ResponseRooms,
    ruma::identifiers::{RoomId, RoomOrAliasId},
};

use crate::{
    components::Toast,
    session::{room::Room, Session},
    spawn, spawn_tokio,
};

mod imp {
    use std::cell::RefCell;

    use glib::{object::WeakRef, subclass::Signal};
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct RoomList {
        pub list: RefCell<IndexMap<Box<RoomId>, Room>>,
        pub pending_rooms: RefCell<HashSet<Box<RoomOrAliasId>>>,
        pub session: OnceCell<WeakRef<Session>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoomList {
        const NAME: &'static str = "RoomList";
        type Type = super::RoomList;
        type ParentType = glib::Object;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for RoomList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "session",
                    "Session",
                    "The session",
                    Session::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "session" => self
                    .session
                    .set(value.get::<Session>().unwrap().downgrade())
                    .unwrap(),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session" => obj.session().to_value(),
                _ => unimplemented!(),
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("pending-rooms-changed", &[], <()>::static_type().into())
                        .build(),
                ]
            });
            SIGNALS.as_ref()
        }
    }

    impl ListModelImpl for RoomList {
        fn item_type(&self, _list_model: &Self::Type) -> glib::Type {
            Room::static_type()
        }
        fn n_items(&self, _list_model: &Self::Type) -> u32 {
            self.list.borrow().len() as u32
        }
        fn item(&self, _list_model: &Self::Type, position: u32) -> Option<glib::Object> {
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
    /// are derived. If a room is updated in an order-relevant manner, use
    /// `room.emit_by_name::<()>("order-changed", &[])` to fix the sorting.
    ///
    /// The `RoomList` also takes care of all so called *pending rooms*, i.e.
    /// rooms the user requested to join, but received no response from the
    /// server yet.
    pub struct RoomList(ObjectSubclass<imp::RoomList>)
        @implements gio::ListModel;
}

impl RoomList {
    pub fn new(session: &Session) -> Self {
        glib::Object::new(&[("session", session)]).expect("Failed to create RoomList")
    }

    pub fn session(&self) -> Session {
        self.imp().session.get().unwrap().upgrade().unwrap()
    }

    pub fn is_pending_room(&self, identifier: &RoomOrAliasId) -> bool {
        self.imp().pending_rooms.borrow().contains(identifier)
    }

    fn pending_rooms_remove(&self, identifier: &RoomOrAliasId) {
        self.imp().pending_rooms.borrow_mut().remove(identifier);
        self.emit_by_name::<()>("pending-rooms-changed", &[]);
    }

    fn pending_rooms_insert(&self, identifier: Box<RoomOrAliasId>) {
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

    pub fn get(&self, room_id: &RoomId) -> Option<Room> {
        self.imp().list.borrow().get(room_id).cloned()
    }

    /// Waits till the Room becomes available
    pub async fn get_wait(&self, room_id: Box<RoomId>) -> Option<Room> {
        if let Some(room) = self.imp().list.borrow().get(&*room_id) {
            Some(room.clone())
        } else {
            let (sender, receiver) = futures::channel::oneshot::channel();

            let sender = Cell::new(Some(sender));
            // FIXME: add a timeout
            let handler_id = self.connect_items_changed(move |obj, _, _, _| {
                if let Some(room) = obj.get(&*room_id) {
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

    fn get_full(&self, room_id: &RoomId) -> Option<(usize, Box<RoomId>, Room)> {
        self.imp()
            .list
            .borrow()
            .get_full(room_id)
            .map(|(pos, room_id, room)| (pos, room_id.clone(), room.clone()))
    }

    pub fn contains_key(&self, room_id: &RoomId) -> bool {
        self.imp().list.borrow().contains_key(room_id)
    }

    pub fn remove(&self, room_id: &RoomId) {
        let removed = {
            let mut list = self.imp().list.borrow_mut();

            list.shift_remove_full(room_id)
        };

        if let Some((position, ..)) = removed {
            self.items_changed(position as u32, 1, 0);
        }
    }

    fn items_added(&self, added: usize) {
        let list = self.imp().list.borrow();

        let position = list.len() - added;

        for (_room_id, room) in list.iter().skip(position) {
            room.connect_order_changed(clone!(@weak self as obj => move |room| {
                if let Some((position, _, _)) = obj.get_full(room.room_id()) {
                    obj.items_changed(position as u32, 1, 1);
                }
            }));
            room.connect_room_forgotten(clone!(@weak self as obj => move |room| {
                obj.remove(room.room_id());
            }));
        }

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
                let mut list = self.imp().list.borrow_mut();
                for matrix_room in matrix_rooms {
                    let room_id = matrix_room.room_id().to_owned();
                    let room = Room::new(&session, &room_id);
                    list.insert(room_id, room);
                }
            }

            self.items_added(added);
        }
    }

    pub fn handle_response_rooms(&self, rooms: ResponseRooms) {
        let priv_ = self.imp();
        let session = self.session();

        let mut added = 0;

        for (room_id, left_room) in rooms.leave {
            let room = priv_
                .list
                .borrow_mut()
                .entry(room_id.clone())
                .or_insert_with(|| {
                    added += 1;
                    Room::new(&session, &room_id)
                })
                .clone();

            self.pending_rooms_remove((&*room_id).into());
            room.handle_left_response(left_room);
        }

        for (room_id, joined_room) in rooms.join {
            let room = priv_
                .list
                .borrow_mut()
                .entry(room_id.clone())
                .or_insert_with(|| {
                    added += 1;
                    Room::new(&session, &room_id)
                })
                .clone();

            self.pending_rooms_remove((&*room_id).into());
            room.handle_joined_response(joined_room);
        }

        for (room_id, invited_room) in rooms.invite {
            let room = priv_
                .list
                .borrow_mut()
                .entry(room_id.clone())
                .or_insert_with(|| {
                    added += 1;
                    Room::new(&session, &room_id)
                })
                .clone();

            self.pending_rooms_remove((&*room_id).into());
            room.handle_invited_response(invited_room);
        }

        if added > 0 {
            self.items_added(added);
        }
    }

    pub fn join_by_id_or_alias(&self, identifier: Box<RoomOrAliasId>) {
        let client = self.session().client();
        let identifier_clone = identifier.clone();

        self.pending_rooms_insert(identifier.clone());

        let handle = spawn_tokio!(async move {
            client
                .join_room_by_id_or_alias(&identifier_clone, &[])
                .await
        });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(response) => obj.pending_rooms_replace_or_remove(&identifier, response.room_id.as_ref()),
                    Err(error) => {
                        obj.pending_rooms_remove(&identifier);
                        error!("Joining room {} failed: {}", identifier, error);
                        let error = Toast::new(
                            clone!(@strong obj => move |_| {
                                    let error_message = gettext!(
                                        "Failed to join room {}. Try again later.", identifier
                                    );
                                    let error_label = gtk::Label::builder().label(&error_message).wrap(true).build();
                                    Some(error_label.upcast())
                            }),
                        );

                        if let Some(window) = obj.session().parent_window() {
                            window.append_error(&error);
                        }
                    }
                }
            })
        );
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
}

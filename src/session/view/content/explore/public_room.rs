use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*};
use matrix_sdk::ruma::directory::PublicRoomsChunk;

use crate::session::model::{AvatarData, AvatarImage, AvatarUriSource, Room, RoomList};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::signal::SignalHandlerId;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct PublicRoom {
        pub room_list: OnceCell<RoomList>,
        pub matrix_public_room: OnceCell<PublicRoomsChunk>,
        pub avatar_data: OnceCell<AvatarData>,
        pub room: OnceCell<Room>,
        pub is_pending: Cell<bool>,
        pub room_handler: RefCell<Option<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PublicRoom {
        const NAME: &'static str = "PublicRoom";
        type Type = super::PublicRoom;
    }

    impl ObjectImpl for PublicRoom {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<RoomList>("room-list")
                        .construct_only()
                        .build(),
                    glib::ParamSpecObject::builder::<Room>("room")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("pending")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<AvatarData>("avatar-data")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "room-list" => self.room_list.set(value.get().unwrap()).unwrap(),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "room-list" => obj.room_list().to_value(),
                "avatar-data" => obj.avatar_data().to_value(),
                "room" => obj.room().to_value(),
                "pending" => obj.is_pending().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            self.avatar_data
                .set(AvatarData::new(AvatarImage::new(
                    &obj.room_list().session(),
                    None,
                    AvatarUriSource::Room,
                )))
                .unwrap();

            obj.room_list()
                .connect_pending_rooms_changed(clone!(@weak obj => move |_| {
                    if let Some(matrix_public_room) = obj.matrix_public_room() {
                        obj.set_pending(obj.room_list().session()
                            .room_list()
                            .is_pending_room((*matrix_public_room.room_id).into()));
                    }
                }));
        }

        fn dispose(&self) {
            if let Some(handler_id) = self.room_handler.take() {
                self.obj().room_list().disconnect(handler_id);
            }
        }
    }
}

glib::wrapper! {
    pub struct PublicRoom(ObjectSubclass<imp::PublicRoom>);
}

impl PublicRoom {
    pub fn new(room_list: &RoomList) -> Self {
        glib::Object::builder()
            .property("room-list", room_list)
            .build()
    }

    /// The list of rooms in this session.
    pub fn room_list(&self) -> &RoomList {
        self.imp().room_list.get().unwrap()
    }

    /// The [`AvatarData`] of this room.
    pub fn avatar_data(&self) -> &AvatarData {
        self.imp().avatar_data.get().unwrap()
    }

    /// The `Room` object for this room, if the user is already a member of this
    /// room.
    pub fn room(&self) -> Option<&Room> {
        self.imp().room.get()
    }

    /// Set the `Room` object for this room.
    fn set_room(&self, room: Room) {
        self.imp().room.set(room).unwrap();
        self.notify("room");
    }

    /// Set whether this room is pending.
    fn set_pending(&self, is_pending: bool) {
        if self.is_pending() == is_pending {
            return;
        }

        self.imp().is_pending.set(is_pending);
        self.notify("pending");
    }

    /// Whether the room is pending.
    ///
    /// A room is pending when the user clicked to join it.
    pub fn is_pending(&self) -> bool {
        self.imp().is_pending.get()
    }

    pub fn set_matrix_public_room(&self, room: PublicRoomsChunk) {
        let imp = self.imp();

        let display_name = room.name.clone().map(Into::into);
        self.avatar_data().set_display_name(display_name);
        self.avatar_data().image().set_uri(room.avatar_url.clone());

        if let Some(room) = self.room_list().get(&room.room_id) {
            self.set_room(room);
        } else {
            let room_id = room.room_id.clone();
            let handler_id = self.room_list().connect_items_changed(
                clone!(@weak self as obj => move |room_list, _, _, _| {
                    if let Some(room) = room_list.get(&room_id) {
                        if let Some(handler_id) = obj.imp().room_handler.take() {
                            obj.set_room(room);
                            room_list.disconnect(handler_id);
                        }
                    }
                }),
            );

            imp.room_handler.replace(Some(handler_id));
        }

        self.set_pending(self.room_list().is_pending_room((*room.room_id).into()));

        imp.matrix_public_room.set(room).unwrap();
    }

    pub fn matrix_public_room(&self) -> Option<&PublicRoomsChunk> {
        self.imp().matrix_public_room.get()
    }
}

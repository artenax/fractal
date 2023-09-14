use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use matrix_sdk::ruma::{
    api::client::directory::get_public_rooms_filtered::v3::{
        Request as PublicRoomsRequest, Response as PublicRoomsResponse,
    },
    assign,
    directory::{Filter, RoomNetwork},
    uint, ServerName,
};
use ruma::directory::RoomTypeFilter;
use tracing::error;

use super::{PublicRoom, Server};
use crate::{session::model::Session, spawn, spawn_tokio};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::object::WeakRef;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct PublicRoomList {
        pub list: RefCell<Vec<PublicRoom>>,
        pub search_term: RefCell<Option<String>>,
        pub network: RefCell<Option<String>>,
        pub server: RefCell<Option<String>>,
        pub next_batch: RefCell<Option<String>>,
        pub loading: Cell<bool>,
        pub request_sent: Cell<bool>,
        pub total_room_count_estimate: Cell<Option<u64>>,
        pub session: WeakRef<Session>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PublicRoomList {
        const NAME: &'static str = "PublicRoomList";
        type Type = super::PublicRoomList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for PublicRoomList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Session>("session")
                        .construct_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("loading")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("empty").read_only().build(),
                    glib::ParamSpecBoolean::builder("complete")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "session" => self.obj().set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.session().to_value(),
                "loading" => obj.loading().to_value(),
                "empty" => obj.empty().to_value(),
                "complete" => obj.complete().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for PublicRoomList {
        fn item_type(&self) -> glib::Type {
            PublicRoom::static_type()
        }
        fn n_items(&self) -> u32 {
            self.list.borrow().len() as u32
        }
        fn item(&self, position: u32) -> Option<glib::Object> {
            self.list
                .borrow()
                .get(position as usize)
                .map(glib::object::Cast::upcast_ref::<glib::Object>)
                .cloned()
        }
    }
}

glib::wrapper! {
    pub struct PublicRoomList(ObjectSubclass<imp::PublicRoomList>)
        @implements gio::ListModel;
}

impl PublicRoomList {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Set the current session.
    fn set_session(&self, session: Option<Session>) {
        if session == self.session() {
            return;
        }

        self.imp().session.set(session.as_ref());
        self.notify("session");
    }

    /// Whether the list is loading.
    pub fn loading(&self) -> bool {
        self.request_sent() && self.imp().list.borrow().is_empty()
    }

    /// Whether the list is empty.
    pub fn empty(&self) -> bool {
        !self.request_sent() && self.imp().list.borrow().is_empty()
    }

    /// Whether all results for the current search were loaded.
    pub fn complete(&self) -> bool {
        self.imp().next_batch.borrow().is_none()
    }

    fn request_sent(&self) -> bool {
        self.imp().request_sent.get()
    }

    fn set_request_sent(&self, request_sent: bool) {
        self.imp().request_sent.set(request_sent);

        self.notify("loading");
        self.notify("empty");
        self.notify("complete");
    }

    pub fn search(&self, search_term: Option<String>, server: Server) {
        let imp = self.imp();
        let network = Some(server.network());
        let server = server.server();

        if imp.search_term.borrow().as_ref() == search_term.as_ref()
            && imp.server.borrow().as_deref() == server
            && imp.network.borrow().as_deref() == network
        {
            return;
        }

        imp.search_term.replace(search_term);
        imp.server.replace(server.map(ToOwned::to_owned));
        imp.network.replace(network.map(ToOwned::to_owned));
        self.load_public_rooms(true);
    }

    fn handle_public_rooms_response(&self, response: PublicRoomsResponse) {
        let imp = self.imp();
        let session = self.session().unwrap();
        let room_list = session.room_list();

        imp.next_batch.replace(response.next_batch.to_owned());
        imp.total_room_count_estimate
            .replace(response.total_room_count_estimate.map(Into::into));

        let (position, removed, added) = {
            let mut list = imp.list.borrow_mut();
            let position = list.len();
            let added = response.chunk.len();
            let mut new_rooms = response
                .chunk
                .into_iter()
                .map(|matrix_room| {
                    let room = PublicRoom::new(room_list);
                    room.set_matrix_public_room(matrix_room);
                    room
                })
                .collect();

            let empty_row = list.pop().unwrap_or_else(|| PublicRoom::new(room_list));
            list.append(&mut new_rooms);

            if !self.complete() {
                list.push(empty_row);
                if position == 0 {
                    (position, 0, added + 1)
                } else {
                    (position - 1, 0, added)
                }
            } else if position == 0 {
                (position, 0, added)
            } else {
                (position - 1, 1, added)
            }
        };

        if added > 0 {
            self.items_changed(position as u32, removed as u32, added as u32);
        }
        self.set_request_sent(false);
    }

    fn is_valid_response(
        &self,
        search_term: Option<String>,
        server: Option<String>,
        network: Option<String>,
    ) -> bool {
        let imp = self.imp();
        imp.search_term.borrow().as_ref() == search_term.as_ref()
            && imp.server.borrow().as_ref() == server.as_ref()
            && imp.network.borrow().as_ref() == network.as_ref()
    }

    pub fn load_public_rooms(&self, clear: bool) {
        let imp = self.imp();

        if self.request_sent() && !clear {
            return;
        }

        if clear {
            // Clear the previous list
            let removed = imp.list.borrow().len();
            imp.list.borrow_mut().clear();
            let _ = imp.next_batch.take();
            self.items_changed(0, removed as u32, 0);
        }

        self.set_request_sent(true);

        let next_batch = imp.next_batch.borrow().clone();

        if next_batch.is_none() && !clear {
            return;
        }

        let client = self.session().unwrap().client();
        let search_term = imp.search_term.borrow().to_owned();
        let server = imp.server.borrow().to_owned();
        let network = imp.network.borrow().to_owned();
        let current_search_term = search_term.clone();
        let current_server = server.clone();
        let current_network = network.clone();

        let handle = spawn_tokio!(async move {
            let room_network = match network.as_deref() {
                Some("matrix") => RoomNetwork::Matrix,
                Some("all") => RoomNetwork::All,
                Some(custom) => RoomNetwork::ThirdParty(custom.to_owned()),
                _ => RoomNetwork::default(),
            };
            let server = server.and_then(|server| ServerName::parse(server).ok());

            let request = assign!(PublicRoomsRequest::new(), {
                limit: Some(uint!(20)),
                since: next_batch,
                room_network,
                server,
                filter: assign!(
                    Filter::new(),
                    { generic_search_term: search_term, room_types: vec![RoomTypeFilter::Default] }
                ),
            });
            client.public_rooms_filtered(request).await
        });

        spawn!(
            glib::Priority::DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                // If the search term changed we ignore the response
                if obj.is_valid_response(current_search_term, current_server, current_network) {
                    match handle.await.unwrap() {
                     Ok(response) => obj.handle_public_rooms_response(response),
                     Err(error) => {
                        obj.set_request_sent(false);
                        error!("Error loading public rooms: {error}")
                     },
                    }
                }
            })
        );
    }
}

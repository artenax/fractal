use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use log::error;
use matrix_sdk::ruma::{
    api::client::directory::get_public_rooms_filtered::v3::{
        Request as PublicRoomsRequest, Response as PublicRoomsResponse,
    },
    assign,
    directory::{Filter, RoomNetwork},
    uint, ServerName,
};

use crate::{
    session::{content::explore::PublicRoom, Session},
    spawn, spawn_tokio,
};

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
                    glib::ParamSpecObject::new(
                        "session",
                        "Session",
                        "The session",
                        Session::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecBoolean::new(
                        "loading",
                        "Loading",
                        "Whether a response is loaded",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "empty",
                        "Empty",
                        "Whether any matching rooms are found",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "complete",
                        "Complete",
                        "Whether all search results are loaded",
                        false,
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
                "session" => obj.set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
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
        fn item_type(&self, _list_model: &Self::Type) -> glib::Type {
            PublicRoom::static_type()
        }
        fn n_items(&self, _list_model: &Self::Type) -> u32 {
            self.list.borrow().len() as u32
        }
        fn item(&self, _list_model: &Self::Type, position: u32) -> Option<glib::Object> {
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
        glib::Object::new(&[("session", session)]).expect("Failed to create PublicRoomList")
    }

    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    pub fn set_session(&self, session: Option<Session>) {
        if session == self.session() {
            return;
        }

        self.imp().session.set(session.as_ref());
        self.notify("session");
    }

    pub fn loading(&self) -> bool {
        self.request_sent() && self.imp().list.borrow().is_empty()
    }

    pub fn empty(&self) -> bool {
        !self.request_sent() && self.imp().list.borrow().is_empty()
    }

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

    pub fn search(
        &self,
        search_term: Option<String>,
        server: Option<String>,
        network: Option<String>,
    ) {
        let priv_ = self.imp();

        if priv_.search_term.borrow().as_ref() == search_term.as_ref()
            && priv_.server.borrow().as_ref() == server.as_ref()
            && priv_.network.borrow().as_ref() == network.as_ref()
        {
            return;
        }

        priv_.search_term.replace(search_term);
        priv_.server.replace(server);
        priv_.network.replace(network);
        self.load_public_rooms(true);
    }

    fn handle_public_rooms_response(&self, response: PublicRoomsResponse) {
        let priv_ = self.imp();
        let session = self.session().unwrap();
        let room_list = session.room_list();

        priv_.next_batch.replace(response.next_batch.to_owned());
        priv_
            .total_room_count_estimate
            .replace(response.total_room_count_estimate.map(Into::into));

        let (position, removed, added) = {
            let mut list = priv_.list.borrow_mut();
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
        let priv_ = self.imp();
        priv_.search_term.borrow().as_ref() == search_term.as_ref()
            && priv_.server.borrow().as_ref() == server.as_ref()
            && priv_.network.borrow().as_ref() == network.as_ref()
    }

    pub fn load_public_rooms(&self, clear: bool) {
        let priv_ = self.imp();

        if self.request_sent() && !clear {
            return;
        }

        if clear {
            // Clear the previous list
            let removed = priv_.list.borrow().len();
            priv_.list.borrow_mut().clear();
            let _ = priv_.next_batch.take();
            self.items_changed(0, removed as u32, 0);
        }

        self.set_request_sent(true);

        let next_batch = priv_.next_batch.borrow().clone();

        if next_batch.is_none() && !clear {
            return;
        }

        let client = self.session().unwrap().client();
        let search_term = priv_.search_term.borrow().to_owned();
        let server = priv_.server.borrow().to_owned();
        let network = priv_.network.borrow().to_owned();
        let current_search_term = search_term.clone();
        let current_server = server.clone();
        let current_network = network.clone();

        let handle = spawn_tokio!(async move {
            let room_network = match network.as_deref() {
                Some("matrix") => RoomNetwork::Matrix,
                Some("all") => RoomNetwork::All,
                Some(custom) => RoomNetwork::ThirdParty(custom),
                _ => RoomNetwork::default(),
            };
            let server = server
                .as_deref()
                .and_then(|server| <&ServerName>::try_from(server).ok());

            let request = assign!(PublicRoomsRequest::new(), {
              limit: Some(uint!(20)),
              since: next_batch.as_deref(),
              room_network,
              server,
              filter: assign!(Filter::new(), { generic_search_term: search_term.as_deref() }),
            });
            client.public_rooms_filtered(request).await
        });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                // If the search term changed we ignore the response
                if obj.is_valid_response(current_search_term, current_server, current_network) {
                    match handle.await.unwrap() {
                     Ok(response) => obj.handle_public_rooms_response(response),
                     Err(error) => {
                        obj.set_request_sent(false);
                        error!("Error loading public rooms: {}", error)
                     },
                    }
                }
            })
        );
    }
}

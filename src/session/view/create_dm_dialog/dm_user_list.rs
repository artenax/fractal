use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use matrix_sdk::ruma::{api::client::user_directory::search_users, OwnedUserId, UserId};
use tracing::{debug, error};

use super::DmUser;
use crate::{
    prelude::*,
    session::model::{Member, Room, Session},
    spawn, spawn_tokio,
};

#[derive(Debug, Default, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "ContentDmUserListState")]
pub enum DmUserListState {
    #[default]
    Initial = 0,
    Loading = 1,
    NoMatching = 2,
    Matching = 3,
    Error = 4,
}

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };

    use futures_util::future::AbortHandle;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct DmUserList {
        pub list: RefCell<Vec<DmUser>>,
        pub session: glib::WeakRef<Session>,
        pub state: Cell<DmUserListState>,
        pub search_term: RefCell<Option<String>>,
        pub abort_handle: RefCell<Option<AbortHandle>>,
        pub dm_rooms: RefCell<HashMap<OwnedUserId, Vec<glib::WeakRef<Room>>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DmUserList {
        const NAME: &'static str = "DmUserList";
        type Type = super::DmUserList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for DmUserList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Session>("session")
                        .construct_only()
                        .build(),
                    glib::ParamSpecString::builder("search-term")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecEnum::builder::<DmUserListState>("state")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "session" => self.session.set(value.get().unwrap()),
                "search-term" => self.obj().set_search_term(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.session().to_value(),
                "search-term" => obj.search_term().to_value(),
                "state" => obj.state().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for DmUserList {
        fn item_type(&self) -> glib::Type {
            DmUser::static_type()
        }
        fn n_items(&self) -> u32 {
            self.list.borrow().len() as u32
        }
        fn item(&self, position: u32) -> Option<glib::Object> {
            self.list
                .borrow()
                .get(position as usize)
                .cloned()
                .and_upcast()
        }
    }
}

glib::wrapper! {
    /// List of users matching the `search term`.
    pub struct DmUserList(ObjectSubclass<imp::DmUserList>)
        @implements gio::ListModel;
}

impl DmUserList {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// The session this list refers to.
    pub fn session(&self) -> Session {
        self.imp().session.upgrade().unwrap()
    }

    /// Set the search term.
    pub fn set_search_term(&self, search_term: Option<String>) {
        let imp = self.imp();
        let search_term = search_term.filter(|s| !s.is_empty());

        if search_term.as_ref() == imp.search_term.borrow().as_ref() {
            return;
        }

        imp.search_term.replace(search_term);

        spawn!(clone!(@weak self as obj => async move {
            obj.search_users().await;
        }));

        self.notify("search_term");
    }

    /// The search term.
    fn search_term(&self) -> Option<String> {
        self.imp().search_term.borrow().clone()
    }

    /// Set the state of the list.
    fn set_state(&self, state: DmUserListState) {
        let imp = self.imp();

        if state == self.state() {
            return;
        }

        imp.state.set(state);
        self.notify("state");
    }

    /// The state of the list.
    pub fn state(&self) -> DmUserListState {
        self.imp().state.get()
    }

    fn set_list(&self, users: Vec<DmUser>) {
        let added = users.len();

        let prev_users = self.imp().list.replace(users);

        self.items_changed(0, prev_users.len() as u32, added as u32);
    }

    fn clear_list(&self) {
        self.set_list(Vec::new());
    }

    async fn search_users(&self) {
        let session = self.session();
        let client = session.client();
        let Some(search_term) = self.search_term() else {
            self.set_state(DmUserListState::Initial);
            return;
        };

        self.set_state(DmUserListState::Loading);
        self.clear_list();

        let search_term_clone = search_term.clone();
        let handle = spawn_tokio!(async move { client.search_users(&search_term_clone, 20).await });

        let (future, handle) = futures_util::future::abortable(handle);

        if let Some(abort_handle) = self.imp().abort_handle.replace(Some(handle)) {
            abort_handle.abort();
        }

        let response = if let Ok(result) = future.await {
            result.unwrap()
        } else {
            return;
        };

        if Some(&search_term) != self.search_term().as_ref() {
            return;
        }

        match response {
            Ok(mut response) => {
                let mut add_custom = false;
                // If the search term looks like an UserId and is not already in the response,
                // insert it.
                if let Ok(user_id) = UserId::parse(&search_term) {
                    if !response.results.iter().any(|item| item.user_id == user_id) {
                        let user = search_users::v3::User::new(user_id);
                        response.results.insert(0, user);
                        add_custom = true;
                    }
                }

                self.load_dm_rooms().await;
                let own_user_id = session.user().unwrap().user_id();
                let dm_rooms = self.imp().dm_rooms.borrow().clone();

                let mut users: Vec<DmUser> = vec![];
                for item in response.results.into_iter() {
                    let other_user_id = &item.user_id;
                    let room = if let Some(rooms) = dm_rooms.get(other_user_id) {
                        let mut final_rooms: Vec<Room> = vec![];
                        for room in rooms {
                            let Some(room) = room.upgrade() else {
                                continue;
                            };
                            let members = room.members();

                            if !room.is_joined() || room.matrix_room().active_members_count() > 2 {
                                continue;
                            }

                            // Make sure we have all members loaded, in most cases members should
                            // already be loaded
                            room.members().load().await;

                            if members.n_items() >= 1 {
                                let mut found_others = false;
                                for member in members.iter::<Member>() {
                                    match member {
                                        Ok(member) => {
                                            if member.user_id() != own_user_id
                                                && &member.user_id() != other_user_id
                                            {
                                                // We found other members in this room, let's ignore
                                                // the
                                                // room
                                                found_others = true;
                                                break;
                                            }
                                        }
                                        Err(error) => {
                                            debug!("Error iterating through room members: {error}");
                                            break;
                                        }
                                    }
                                }

                                if found_others {
                                    continue;
                                }
                            }

                            final_rooms.push(room);
                        }

                        final_rooms
                            .into_iter()
                            .max_by(|x, y| x.latest_unread().cmp(&y.latest_unread()))
                    } else {
                        None
                    };

                    let user = DmUser::new(
                        &session,
                        &item.user_id,
                        item.display_name.as_deref(),
                        item.avatar_url.as_deref(),
                        room.as_ref(),
                    );
                    // If it is the "custom user" from the search term, fetch the avatar
                    // and display name
                    if add_custom && user.user_id() == search_term {
                        user.load_profile();
                    }
                    users.push(user);
                }

                match users.is_empty() {
                    true => self.set_state(DmUserListState::NoMatching),
                    false => self.set_state(DmUserListState::Matching),
                }
                self.set_list(users);
            }
            Err(error) => {
                error!("Couldn’t load matching users: {error}");
                self.set_state(DmUserListState::Error);
                self.clear_list();
            }
        }
    }

    async fn load_dm_rooms(&self) {
        let client = self.session().client();
        let handle = spawn_tokio!(async move {
            client
                .account()
                .account_data::<ruma::events::direct::DirectEventContent>()
                .await?
                .map(|c| c.deserialize())
                .transpose()
                .map_err(matrix_sdk::Error::from)
        });

        match handle.await.unwrap() {
            Ok(Some(list)) => {
                let session = self.session();
                let room_list = session.room_list();
                let list = list
                    .into_iter()
                    .map(|(user_id, room_ids)| {
                        let rooms = room_ids
                            .iter()
                            .filter_map(|room_id| Some(room_list.get(room_id)?.downgrade()))
                            .collect();
                        (user_id, rooms)
                    })
                    .collect();
                self.imp().dm_rooms.replace(list);
            }
            Ok(None) => {
                self.imp().dm_rooms.take();
            }
            Err(error) => {
                error!("Can’t read account data: {error}");
                self.imp().dm_rooms.take();
            }
        };
    }
}

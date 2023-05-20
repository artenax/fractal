use gettextrs::gettext;
use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use log::error;
use matrix_sdk::{
    ruma::{
        api::client::{profile::get_profile, user_directory::search_users},
        OwnedUserId, UserId,
    },
    HttpError,
};

use super::Invitee;
use crate::{
    session::{room::Membership, user::UserExt, Room},
    spawn, spawn_tokio,
};

#[derive(Debug, Default, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "ContentInviteeListState")]
pub enum InviteeListState {
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

    use futures::future::AbortHandle;
    use glib::subclass::Signal;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct InviteeList {
        pub list: RefCell<Vec<Invitee>>,
        pub room: OnceCell<Room>,
        pub state: Cell<InviteeListState>,
        pub search_term: RefCell<Option<String>>,
        pub invitee_list: RefCell<HashMap<OwnedUserId, Invitee>>,
        pub abort_handle: RefCell<Option<AbortHandle>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for InviteeList {
        const NAME: &'static str = "InviteeList";
        type Type = super::InviteeList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for InviteeList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Room>("room")
                        .construct_only()
                        .build(),
                    glib::ParamSpecString::builder("search-term")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("has-selected")
                        .read_only()
                        .build(),
                    glib::ParamSpecEnum::builder::<InviteeListState>("state")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("invitee-added")
                        .param_types([Invitee::static_type()])
                        .build(),
                    Signal::builder("invitee-removed")
                        .param_types([Invitee::static_type()])
                        .build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "room" => self.room.set(value.get().unwrap()).unwrap(),
                "search-term" => self.obj().set_search_term(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "room" => obj.room().to_value(),
                "search-term" => obj.search_term().to_value(),
                "has-selected" => obj.has_selected().to_value(),
                "state" => obj.state().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for InviteeList {
        fn item_type(&self) -> glib::Type {
            Invitee::static_type()
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
    /// List of users matching the `search term`.
    pub struct InviteeList(ObjectSubclass<imp::InviteeList>)
        @implements gio::ListModel;
}

impl InviteeList {
    pub fn new(room: &Room) -> Self {
        glib::Object::builder().property("room", room).build()
    }

    /// The room this invitee list refers to.
    pub fn room(&self) -> &Room {
        self.imp().room.get().unwrap()
    }

    /// Set the search term.
    pub fn set_search_term(&self, search_term: Option<String>) {
        let imp = self.imp();

        if search_term.as_ref() == imp.search_term.borrow().as_ref() {
            return;
        }

        if search_term.as_ref().map_or(false, |s| s.is_empty()) {
            imp.search_term.replace(None);
        } else {
            imp.search_term.replace(search_term);
        }

        self.search_users();
        self.notify("search_term");
    }

    /// The search term.
    fn search_term(&self) -> Option<String> {
        self.imp().search_term.borrow().clone()
    }

    /// Set the state of the list.
    fn set_state(&self, state: InviteeListState) {
        let imp = self.imp();

        if state == self.state() {
            return;
        }

        imp.state.set(state);
        self.notify("state");
    }

    /// The state of the list.
    pub fn state(&self) -> InviteeListState {
        self.imp().state.get()
    }

    fn set_list(&self, users: Vec<Invitee>) {
        let added = users.len();

        let prev_users = self.imp().list.replace(users);

        self.items_changed(0, prev_users.len() as u32, added as u32);
    }

    fn clear_list(&self) {
        self.set_list(Vec::new());
    }

    fn finish_search(
        &self,
        search_term: String,
        response: Result<search_users::v3::Response, HttpError>,
    ) {
        let session = self.room().session();
        let member_list = self.room().members();

        if Some(&search_term) != self.search_term().as_ref() {
            return;
        }

        match response {
            Ok(mut response) => {
                // If the search term looks like an UserId and is not already in the response,
                // insert it.
                if let Ok(user_id) = UserId::parse(&search_term) {
                    if !response.results.iter().any(|item| item.user_id == user_id) {
                        let user = search_users::v3::User::new(user_id);
                        response.results.insert(0, user);
                    }
                }

                let users: Vec<Invitee> = response
                    .results
                    .into_iter()
                    .map(|item| {
                        let user = match self.get_invitee(&item.user_id) {
                            Some(user) => {
                                // The avatar or the display name may have changed in the meantime
                                user.set_avatar_url(item.avatar_url);
                                user.set_display_name(item.display_name);

                                user
                            }
                            None => {
                                let user = Invitee::new(
                                    &session,
                                    &item.user_id,
                                    item.display_name.as_deref(),
                                    item.avatar_url.as_deref(),
                                );
                                user.connect_notify_local(
                                    Some("invited"),
                                    clone!(@weak self as obj => move |user, _| {
                                        if user.is_invited() && user.invite_exception().is_none() {
                                            obj.add_invitee(user.clone());
                                        } else {
                                            obj.remove_invitee(&user.user_id())
                                        }
                                    }),
                                );
                                // If it is the "custom user" from the search term, fetch the avatar
                                // and display name
                                let user_id = user.user_id();
                                if user_id == search_term {
                                    let client = session.client();
                                    let handle = spawn_tokio!(async move {
                                        let request = get_profile::v3::Request::new(user_id);
                                        client.send(request, None).await
                                    });
                                    spawn!(clone!(@weak user => async move {
                                        let response = handle.await.unwrap();
                                        let (display_name, avatar_url) = match response {
                                            Ok(response) => {
                                                (response.displayname, response.avatar_url)
                                            },
                                            Err(_) => {
                                                return;
                                            }
                                        };
                                        // If the display name and or the avatar were returned, the Invitee gets updated.
                                        if display_name.is_some() {
                                            user.set_display_name(display_name);
                                        }
                                        if avatar_url.is_some() {
                                            user.set_avatar_url(avatar_url);
                                        }
                                    }));
                                }

                                user
                            }
                        };
                        // 'Disable' users that can't be invited
                        match member_list.get_membership(&item.user_id) {
                            Membership::Join => user.set_invite_exception(Some(gettext("Member"))),
                            Membership::Ban => user.set_invite_exception(Some(gettext("Banned"))),
                            Membership::Invite => {
                                user.set_invite_exception(Some(gettext("Invited")))
                            }
                            _ => {}
                        };
                        user
                    })
                    .collect();
                match users.is_empty() {
                    true => self.set_state(InviteeListState::NoMatching),
                    false => self.set_state(InviteeListState::Matching),
                }
                self.set_list(users);
            }
            Err(error) => {
                error!("Couldn’t load matching users: {error}");
                self.set_state(InviteeListState::Error);
                self.clear_list();
            }
        }
    }

    fn search_users(&self) {
        let client = self.room().session().client();
        let search_term = if let Some(search_term) = self.search_term() {
            search_term
        } else {
            // Do nothing for no search term except when currently loading
            if self.state() == InviteeListState::Loading {
                self.set_state(InviteeListState::Initial);
            }
            return;
        };

        self.set_state(InviteeListState::Loading);
        self.clear_list();

        let search_term_clone = search_term.clone();
        let handle = spawn_tokio!(async move {
            let request = search_users::v3::Request::new(search_term_clone);
            client.send(request, None).await
        });

        let (future, handle) = futures::future::abortable(handle);

        if let Some(abort_handle) = self.imp().abort_handle.replace(Some(handle)) {
            abort_handle.abort();
        }

        spawn!(clone!(@weak self as obj => async move {
            if let Ok(result) = future.await {
                obj.finish_search(search_term, result.unwrap());
            }
        }));
    }

    fn get_invitee(&self, user_id: &UserId) -> Option<Invitee> {
        self.imp().invitee_list.borrow().get(user_id).cloned()
    }

    pub fn add_invitee(&self, user: Invitee) {
        user.set_invited(true);
        self.imp()
            .invitee_list
            .borrow_mut()
            .insert(user.user_id(), user.clone());
        self.emit_by_name::<()>("invitee-added", &[&user]);
        self.notify("has-selected");
    }

    pub fn invitees(&self) -> Vec<Invitee> {
        self.imp()
            .invitee_list
            .borrow()
            .values()
            .map(Clone::clone)
            .collect()
    }

    pub fn remove_invitee(&self, user_id: &UserId) {
        let removed = self.imp().invitee_list.borrow_mut().remove(user_id);
        if let Some(user) = removed {
            user.set_invited(false);
            self.emit_by_name::<()>("invitee-removed", &[&user]);
            self.notify("has-selected");
        }
    }

    /// Whether some users are selected.
    pub fn has_selected(&self) -> bool {
        !self.imp().invitee_list.borrow().is_empty()
    }

    pub fn connect_invitee_added<F: Fn(&Self, &Invitee) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("invitee-added", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            let invitee = values[1].get::<Invitee>().unwrap();
            f(&obj, &invitee);
            None
        })
    }

    pub fn connect_invitee_removed<F: Fn(&Self, &Invitee) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("invitee-removed", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            let invitee = values[1].get::<Invitee>().unwrap();
            f(&obj, &invitee);
            None
        })
    }
}

use gtk::{gio, glib, glib::SignalHandlerId, prelude::*, subclass::prelude::*};
use indexmap::map::IndexMap;

use crate::session::model::Session;

mod imp {
    use std::cell::RefCell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct SessionList {
        /// A map of session ID to session.
        pub list: RefCell<IndexMap<String, Session>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SessionList {
        const NAME: &'static str = "SessionList";
        type Type = super::SessionList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for SessionList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecBoolean::builder("is-empty")
                    .read_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "is-empty" => self.obj().is_empty().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for SessionList {
        fn item_type(&self) -> glib::Type {
            Session::static_type()
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
    /// List of all logged in sessions.
    pub struct SessionList(ObjectSubclass<imp::SessionList>)
        @implements gio::ListModel;
}

impl SessionList {
    /// Create a new empty `SessionList`.
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Whether this list is empty.
    pub fn is_empty(&self) -> bool {
        self.imp().list.borrow().is_empty()
    }

    /// The session with the given ID, if any.
    pub fn get(&self, session_id: &str) -> Option<Session> {
        self.imp().list.borrow().get(session_id).cloned()
    }

    /// The index of the session with the given ID, if any.
    pub fn index(&self, session_id: &str) -> Option<usize> {
        self.imp().list.borrow().get_index_of(session_id)
    }

    /// Add the given session to the list.
    ///
    /// Returns the index of the session.
    pub fn add(&self, session: Session) -> usize {
        let was_empty = self.is_empty();

        let (index, replaced) = self
            .imp()
            .list
            .borrow_mut()
            .insert_full(session.session_id().to_owned(), session);

        let added = if replaced.is_some() { 0 } else { 1 };

        self.items_changed(index as u32, 0, added);

        if was_empty {
            self.notify("is-empty")
        }

        index
    }

    /// Remove the session with the given ID from the list.
    pub fn remove(&self, session_id: &str) {
        let removed = self.imp().list.borrow_mut().shift_remove_full(session_id);

        if let Some((position, ..)) = removed {
            self.items_changed(position as u32, 1, 0);

            if self.is_empty() {
                self.notify("is-empty");
            }
        }
    }

    pub fn connect_is_empty_notify<F: Fn(&Self) + 'static>(&self, f: F) -> SignalHandlerId {
        self.connect_notify_local(Some("is-empty"), move |obj, _| {
            f(obj);
        })
    }
}

impl Default for SessionList {
    fn default() -> Self {
        Self::new()
    }
}

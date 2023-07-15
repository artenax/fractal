use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk_ui::timeline::ReactionGroup as SdkReactionGroup;

use super::EventKey;
use crate::{prelude::*, session::model::User};

mod imp {
    use std::cell::RefCell;

    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct ReactionGroup {
        /// The user of the parent session.
        pub user: OnceCell<User>,

        /// The key of the group.
        pub key: OnceCell<String>,

        /// The reactions in the group.
        pub reactions: RefCell<Option<SdkReactionGroup>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ReactionGroup {
        const NAME: &'static str = "ReactionGroup";
        type Type = super::ReactionGroup;
    }

    impl ObjectImpl for ReactionGroup {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<User>("user")
                        .construct_only()
                        .build(),
                    glib::ParamSpecString::builder("key")
                        .construct_only()
                        .build(),
                    glib::ParamSpecUInt::builder("count").read_only().build(),
                    glib::ParamSpecBoolean::builder("has-user")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "user" => {
                    self.user.set(value.get().unwrap()).unwrap();
                }
                "key" => {
                    self.key.set(value.get().unwrap()).unwrap();
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "user" => obj.user().to_value(),
                "key" => obj.key().to_value(),
                "count" => (obj.count() as u32).to_value(),
                "has-user" => obj.has_user().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// Reactions grouped by a given key.
    pub struct ReactionGroup(ObjectSubclass<imp::ReactionGroup>);
}

impl ReactionGroup {
    pub fn new(key: &str, user: &User) -> Self {
        glib::Object::builder()
            .property("key", key)
            .property("user", user)
            .build()
    }

    /// The user of the parent session.
    pub fn user(&self) -> &User {
        self.imp().user.get().unwrap()
    }

    /// The key of the group.
    pub fn key(&self) -> &str {
        self.imp().key.get().unwrap()
    }

    /// The number of reactions in this group
    pub fn count(&self) -> u64 {
        self.imp()
            .reactions
            .borrow()
            .as_ref()
            .map(|reactions| reactions.len() as u64)
            .unwrap_or_default()
    }

    /// The event ID of the reaction in this group sent by the logged-in user,
    /// if any.
    pub fn user_reaction_event_key(&self) -> Option<EventKey> {
        let user_id = self.user().user_id();
        self.imp()
            .reactions
            .borrow()
            .as_ref()
            .and_then(|reactions| {
                reactions
                    .by_sender(&user_id)
                    .next()
                    .and_then(|timeline_key| match timeline_key {
                        (Some(txn_id), None) => Some(EventKey::TransactionId(txn_id.clone())),
                        (_, Some(event_id)) => Some(EventKey::EventId(event_id.clone())),
                        _ => None,
                    })
            })
    }

    /// Whether this group has a reaction from the logged-in user.
    pub fn has_user(&self) -> bool {
        let user_id = self.user().user_id();
        self.imp()
            .reactions
            .borrow()
            .as_ref()
            .filter(|reactions| reactions.by_sender(&user_id).next().is_some())
            .is_some()
    }

    /// Update this group with the given reactions.
    pub fn update(&self, new_reactions: SdkReactionGroup) {
        let prev_has_user = self.has_user();
        let prev_count = self.count();

        *self.imp().reactions.borrow_mut() = Some(new_reactions);

        if self.count() != prev_count {
            self.notify("count");
        }

        if self.has_user() != prev_has_user {
            self.notify("has-user");
        }
    }
}

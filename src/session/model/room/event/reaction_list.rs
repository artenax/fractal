use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use matrix_sdk::room::timeline::BundledReactions;

use super::ReactionGroup;
use crate::session::model::User;

mod imp {
    use std::cell::RefCell;

    use indexmap::IndexMap;
    use once_cell::sync::OnceCell;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ReactionList {
        /// The user of the parent session.
        pub user: OnceCell<User>,

        /// The list of reactions grouped by key.
        pub reactions: RefCell<IndexMap<String, ReactionGroup>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ReactionList {
        const NAME: &'static str = "ReactionList";
        type Type = super::ReactionList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for ReactionList {}

    impl ListModelImpl for ReactionList {
        fn item_type(&self) -> glib::Type {
            ReactionGroup::static_type()
        }
        fn n_items(&self) -> u32 {
            self.reactions.borrow().len() as u32
        }
        fn item(&self, position: u32) -> Option<glib::Object> {
            let reactions = self.reactions.borrow();

            reactions
                .get_index(position as usize)
                .map(|(_key, reaction_group)| reaction_group.clone().upcast())
        }
    }
}

glib::wrapper! {
    /// List of all `ReactionGroup`s for a `SupportedEvent`. Implements `ListModel`.
    ///
    /// `ReactionGroup`s are sorted in "insertion order".
    pub struct ReactionList(ObjectSubclass<imp::ReactionList>)
        @implements gio::ListModel;
}

impl ReactionList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The user of the parent session.
    pub fn user(&self) -> &User {
        self.imp().user.get().unwrap()
    }

    /// Set the user of the parent session.
    pub fn set_user(&self, user: User) {
        let _ = self.imp().user.set(user);
    }

    /// Update the reaction list with the given reactions.
    pub fn update(&self, new_reactions: BundledReactions) {
        let reactions = &self.imp().reactions;

        let changed = {
            let old_reactions = reactions.borrow();

            old_reactions.len() != new_reactions.len()
                || new_reactions
                    .keys()
                    .zip(old_reactions.keys())
                    .any(|(new_key, old_key)| new_key != old_key)
        };

        if changed {
            let mut reactions = reactions.borrow_mut();
            let user = self.user();
            let prev_len = reactions.len();
            let new_len = new_reactions.len();

            *reactions = new_reactions
                .into_iter()
                .map(|(key, reactions)| {
                    let group = ReactionGroup::new(&key, user);
                    group.update(reactions);
                    (key, group)
                })
                .collect();

            // We can't have the borrow active when items_changed is emitted because that
            // will probably cause reads of the reactions field.
            std::mem::drop(reactions);

            self.items_changed(0, prev_len as u32, new_len as u32);
        } else {
            let reactions = reactions.borrow();
            for (reactions, group) in new_reactions.into_values().zip(reactions.values()) {
                group.update(reactions);
            }
        }
    }

    /// Get a reaction group by its key.
    ///
    /// Returns `None` if no action group was found with this key.
    pub fn reaction_group_by_key(&self, key: &str) -> Option<ReactionGroup> {
        self.imp().reactions.borrow().get(key).cloned()
    }

    /// Remove a reaction group by its key.
    pub fn remove_reaction_group(&self, key: &str) {
        let (pos, ..) = self
            .imp()
            .reactions
            .borrow_mut()
            .shift_remove_full(key)
            .unwrap();
        self.items_changed(pos as u32, 1, 0);
    }

    /// Removes all reactions.
    pub fn clear(&self) {
        let mut reactions = self.imp().reactions.borrow_mut();
        let len = reactions.len();
        reactions.clear();
        std::mem::drop(reactions);
        self.items_changed(0, len as u32, 0);
    }
}

impl Default for ReactionList {
    fn default() -> Self {
        Self::new()
    }
}

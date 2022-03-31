use std::collections::HashMap;

use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use matrix_sdk::ruma::events::AnyMessageLikeEventContent;

use super::{Event, ReactionGroup};

mod imp {
    use std::cell::RefCell;

    use indexmap::IndexMap;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ReactionList {
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
        fn item_type(&self, _list_model: &Self::Type) -> glib::Type {
            ReactionGroup::static_type()
        }
        fn n_items(&self, _list_model: &Self::Type) -> u32 {
            self.reactions.borrow().len() as u32
        }
        fn item(&self, _list_model: &Self::Type, position: u32) -> Option<glib::Object> {
            let reactions = self.reactions.borrow();

            reactions
                .get_index(position as usize)
                .map(|(_key, reaction_group)| reaction_group.clone().upcast::<glib::Object>())
        }
    }
}

glib::wrapper! {
    /// List of all `ReactionGroup`s for an `Event`. Implements `ListModel`.
    ///
    /// `ReactionGroup`s are sorted in "insertion order".
    pub struct ReactionList(ObjectSubclass<imp::ReactionList>)
        @implements gio::ListModel;
}

impl ReactionList {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create ReactionList")
    }

    /// Add reactions with the given reaction `Event`s.
    ///
    /// Ignores `Event`s that are not reactions.
    pub fn add_reactions(&self, new_reactions: Vec<Event>) {
        let mut reactions = self.imp().reactions.borrow_mut();
        let prev_len = reactions.len();

        // Group reactions by key
        let mut grouped_reactions: HashMap<String, Vec<Event>> = HashMap::new();
        for event in new_reactions {
            if let Some(AnyMessageLikeEventContent::Reaction(reaction)) = event.message_content() {
                let relation = reaction.relates_to;
                grouped_reactions
                    .entry(relation.key)
                    .or_default()
                    .push(event);
            }
        }

        // Add groups to the list
        for (key, reactions_list) in grouped_reactions {
            reactions
                .entry(key)
                .or_insert_with_key(|key| {
                    let group = ReactionGroup::new(key);
                    group.connect_notify_local(
                        Some("count"),
                        clone!(@weak self as obj => move |group, _| {
                            if group.count() == 0 {
                                obj.remove_reaction_group(group.key());
                            }
                        }),
                    );
                    group
                })
                .add_reactions(reactions_list);
        }

        let num_reactions_added = reactions.len().saturating_sub(prev_len);

        // We can't have the borrow active when items_changed is emitted because that
        // will probably cause reads of the reactions field.
        std::mem::drop(reactions);

        if num_reactions_added > 0 {
            // IndexMap preserves insertion order, so all the new items will be at the end.
            self.items_changed(prev_len as u32, 0, num_reactions_added as u32);
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

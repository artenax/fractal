use adw::subclass::prelude::*;
use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::session::room::ReactionList;

struct ReactionGridItem<'a> {
    key: &'a str,
    column: i32,
    row: i32,
}

static QUICK_REACTIONS: &[ReactionGridItem] = &[
    ReactionGridItem {
        key: "👍️",
        column: 0,
        row: 0,
    },
    ReactionGridItem {
        key: "👎️",
        column: 1,
        row: 0,
    },
    ReactionGridItem {
        key: "😄",
        column: 2,
        row: 0,
    },
    ReactionGridItem {
        key: "🎉",
        column: 3,
        row: 0,
    },
    ReactionGridItem {
        key: "😕",
        column: 0,
        row: 1,
    },
    ReactionGridItem {
        key: "❤️",
        column: 1,
        row: 1,
    },
    ReactionGridItem {
        key: "🚀",
        column: 2,
        row: 1,
    },
];

mod imp {

    use std::{cell::RefCell, collections::HashMap};

    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-reaction-chooser.ui")]
    pub struct ReactionChooser {
        /// The `ReactionList` associated to this chooser
        pub reactions: RefCell<Option<ReactionList>>,
        pub reactions_handler: RefCell<Option<glib::SignalHandlerId>>,
        pub reaction_bindings: RefCell<HashMap<String, glib::Binding>>,
        #[template_child]
        pub reaction_grid: TemplateChild<gtk::Grid>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ReactionChooser {
        const NAME: &'static str = "ComponentsReactionChooser";
        type Type = super::ReactionChooser;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ReactionChooser {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            let grid = &self.reaction_grid;
            for reaction_item in QUICK_REACTIONS {
                let button = gtk::ToggleButton::builder()
                    .label(reaction_item.key)
                    .action_name("event.toggle-reaction")
                    .action_target(&reaction_item.key.to_variant())
                    .css_classes(vec!["flat".to_string(), "circular".to_string()])
                    .build();
                button.connect_clicked(|button| {
                    button.activate_action("context-menu.close", None).unwrap();
                });
                grid.attach(&button, reaction_item.column, reaction_item.row, 1, 1);
            }
        }
    }

    impl WidgetImpl for ReactionChooser {}

    impl BinImpl for ReactionChooser {}
}

glib::wrapper! {
    /// A widget displaying a `ReactionChooser` for a `ReactionList`.
    pub struct ReactionChooser(ObjectSubclass<imp::ReactionChooser>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl ReactionChooser {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create ReactionChooser")
    }

    pub fn reactions(&self) -> Option<ReactionList> {
        self.imp().reactions.borrow().clone()
    }

    pub fn set_reactions(&self, reactions: Option<ReactionList>) {
        let priv_ = self.imp();
        let prev_reactions = self.reactions();

        if prev_reactions == reactions {
            return;
        }

        if let Some(reactions) = prev_reactions.as_ref() {
            if let Some(signal_handler) = priv_.reactions_handler.take() {
                reactions.disconnect(signal_handler);
            }
            for (_, binding) in priv_.reaction_bindings.borrow_mut().drain() {
                binding.unbind();
            }
        }

        if let Some(reactions) = reactions.as_ref() {
            let signal_handler =
                reactions.connect_items_changed(clone!(@weak self as obj => move |_, _, _, _| {
                    obj.update_reactions();
                }));
            priv_.reactions_handler.replace(Some(signal_handler));
        }
        priv_.reactions.replace(reactions);
        self.update_reactions();
    }

    fn update_reactions(&self) {
        let priv_ = self.imp();
        let mut reaction_bindings = priv_.reaction_bindings.borrow_mut();
        let reactions = self.reactions();

        for reaction_item in QUICK_REACTIONS {
            if let Some(reaction) = reactions
                .as_ref()
                .and_then(|reactions| reactions.reaction_group_by_key(reaction_item.key))
            {
                if reaction_bindings.get(reaction_item.key).is_none() {
                    let button = priv_
                        .reaction_grid
                        .child_at(reaction_item.column, reaction_item.row)
                        .unwrap();
                    let binding = reaction
                        .bind_property("has-user", &button, "active")
                        .flags(glib::BindingFlags::SYNC_CREATE)
                        .build();
                    reaction_bindings.insert(reaction_item.key.to_string(), binding);
                }
            } else if let Some(binding) = reaction_bindings.remove(reaction_item.key) {
                binding.unbind();
            }
        }
    }
}

impl Default for ReactionChooser {
    fn default() -> Self {
        Self::new()
    }
}

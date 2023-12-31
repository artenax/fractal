use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};

use crate::{session::model::ReactionGroup, utils::EMOJI_REGEX};

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_history/message_row/reaction.ui"
    )]
    pub struct MessageReaction {
        /// The reaction group to display.
        pub group: OnceCell<ReactionGroup>,
        #[template_child]
        pub button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub reaction_key: TemplateChild<gtk::Label>,
        #[template_child]
        pub reaction_count: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MessageReaction {
        const NAME: &'static str = "ContentMessageReaction";
        type Type = super::MessageReaction;
        type ParentType = gtk::FlowBoxChild;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MessageReaction {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<ReactionGroup>("group")
                    .construct_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "group" => {
                    self.obj().set_group(value.get().unwrap());
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "group" => self.obj().group().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for MessageReaction {}

    impl FlowBoxChildImpl for MessageReaction {}
}

glib::wrapper! {
    /// A widget displaying the reactions of a message.
    pub struct MessageReaction(ObjectSubclass<imp::MessageReaction>)
        @extends gtk::Widget, gtk::FlowBoxChild, @implements gtk::Accessible;
}

impl MessageReaction {
    pub fn new(reaction_group: ReactionGroup) -> Self {
        glib::Object::builder()
            .property("group", &reaction_group)
            .build()
    }

    /// The reaction group to display.
    pub fn group(&self) -> Option<&ReactionGroup> {
        self.imp().group.get()
    }

    /// Set the reaction group to display.
    fn set_group(&self, group: ReactionGroup) {
        let imp = self.imp();
        let key = group.key();
        imp.reaction_key.set_label(key);

        if EMOJI_REGEX.is_match(key) {
            imp.reaction_key.add_css_class("emoji");
        } else {
            imp.reaction_key.remove_css_class("emoji");
        }

        imp.button.set_action_target_value(Some(&key.to_variant()));
        group
            .bind_property("has-user", &*imp.button, "active")
            .sync_create()
            .build();
        group
            .bind_property("count", &*imp.reaction_count, "label")
            .sync_create()
            .build();

        imp.group.set(group).unwrap();
    }
}

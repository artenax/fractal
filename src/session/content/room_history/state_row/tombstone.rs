use adw::{prelude::*, subclass::prelude::*};
use gtk::{glib, CompositeTemplate};
use matrix_sdk::ruma::events::room::tombstone::RoomTombstoneEventContent;

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-state-tombstone.ui")]
    pub struct StateTombstone {
        #[template_child]
        pub new_room_btn: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StateTombstone {
        const NAME: &'static str = "ContentStateTombstone";
        type Type = super::StateTombstone;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for StateTombstone {}
    impl WidgetImpl for StateTombstone {}
    impl BinImpl for StateTombstone {}
}

glib::wrapper! {
    pub struct StateTombstone(ObjectSubclass<imp::StateTombstone>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl StateTombstone {
    pub fn new(event: &RoomTombstoneEventContent) -> Self {
        let obj: Self = glib::Object::new(&[]).expect("Failed to create StateTombstone");
        obj.set_event(event);
        obj
    }

    fn set_event(&self, event: &RoomTombstoneEventContent) {
        self.imp().new_room_btn.set_detailed_action_name(&format!(
            "session.show-room::{}",
            event.replacement_room.as_str()
        ));
    }
}

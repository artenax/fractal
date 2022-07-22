use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{glib, CompositeTemplate};
use matrix_sdk::ruma::events::room::create::RoomCreateEventContent;

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-state-creation.ui")]
    pub struct StateCreation {
        #[template_child]
        pub previous_room_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub description: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StateCreation {
        const NAME: &'static str = "ContentStateCreation";
        type Type = super::StateCreation;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for StateCreation {}
    impl WidgetImpl for StateCreation {}
    impl BinImpl for StateCreation {}
}

glib::wrapper! {
    pub struct StateCreation(ObjectSubclass<imp::StateCreation>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl StateCreation {
    pub fn new(event: &RoomCreateEventContent) -> Self {
        let obj: Self = glib::Object::new(&[]).expect("Failed to create StateCreation");
        obj.set_event(event);
        obj
    }

    fn set_event(&self, event: &RoomCreateEventContent) {
        let priv_ = self.imp();
        if let Some(predecessor) = &event.predecessor {
            priv_.previous_room_btn.set_detailed_action_name(&format!(
                "session.show-room::{}",
                predecessor.room_id.as_str()
            ));
            priv_.previous_room_btn.show();
            priv_
                .description
                .set_label(&gettext("This is the continuation of an upgraded room."));
        } else {
            priv_.previous_room_btn.hide();
            priv_.previous_room_btn.set_action_name(None);
            priv_
                .description
                .set_label(&gettext("This is the beginning of this room."));
        }
    }
}

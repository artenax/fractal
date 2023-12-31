use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{glib, CompositeTemplate};
use matrix_sdk::ruma::events::room::create::RoomCreateEventContent;
use ruma::events::FullStateEventContent;

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_history/state_row/creation.ui"
    )]
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
    pub fn new(event: &FullStateEventContent<RoomCreateEventContent>) -> Self {
        let obj: Self = glib::Object::new();
        obj.set_event(event);
        obj
    }

    fn set_event(&self, event: &FullStateEventContent<RoomCreateEventContent>) {
        let imp = self.imp();

        let predecessor = match event {
            FullStateEventContent::Original { content, .. } => content.predecessor.as_ref(),
            FullStateEventContent::Redacted(_) => None,
        };

        if let Some(predecessor) = &predecessor {
            imp.previous_room_btn
                .set_detailed_action_name(&format!("session.show-room::{}", predecessor.room_id));
            imp.previous_room_btn.set_visible(true);
            imp.description
                .set_label(&gettext("This is the continuation of an upgraded room."));
        } else {
            imp.previous_room_btn.set_visible(false);
            imp.previous_room_btn.set_action_name(None);
            imp.description
                .set_label(&gettext("This is the beginning of this room."));
        }
    }
}

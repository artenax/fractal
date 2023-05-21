use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{glib, glib::clone, CompositeTemplate};

use crate::{session::model::Room, spawn, toast, utils::BoundObjectWeakRef, Window};

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-state-tombstone.ui")]
    pub struct StateTombstone {
        #[template_child]
        pub new_room_btn: TemplateChild<gtk::Button>,
        /// The [`Room`] this event belongs to.
        pub room: BoundObjectWeakRef<Room>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StateTombstone {
        const NAME: &'static str = "ContentStateTombstone";
        type Type = super::StateTombstone;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for StateTombstone {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Room>("room")
                    .construct_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "room" => obj.set_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "room" => obj.room().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            self.room.disconnect_signals();
        }
    }

    impl WidgetImpl for StateTombstone {}
    impl BinImpl for StateTombstone {}
}

glib::wrapper! {
    pub struct StateTombstone(ObjectSubclass<imp::StateTombstone>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl StateTombstone {
    /// Construct a new `StateTombstone` with the given room.
    pub fn new(room: &Room) -> Self {
        glib::Object::builder().property("room", room).build()
    }

    /// Set the room this event belongs to.
    fn set_room(&self, room: Room) {
        let imp = self.imp();

        let successor_handler = room.connect_notify_local(
            Some("successor"),
            clone!(@weak self as obj => move |room, _| {
                obj.imp().new_room_btn.set_visible(room.successor().is_some());
            }),
        );
        imp.new_room_btn.set_visible(room.successor().is_some());

        let successor_room_handler = room.connect_notify_local(
            Some("successor-room"),
            clone!(@weak self as obj => move |room, _| {
                obj.update_button_label(room);
            }),
        );
        self.update_button_label(&room);

        imp.room
            .set(&room, vec![successor_handler, successor_room_handler]);
    }

    /// The room this event belongs to.
    pub fn room(&self) -> Option<Room> {
        self.imp().room.obj()
    }

    /// Update the button of the label.
    fn update_button_label(&self, room: &Room) {
        let button = &self.imp().new_room_btn;
        if room.successor_room().is_some() {
            button.set_label(&gettext("View"));
        } else {
            button.set_label(&gettext("Join"));
        }
    }

    /// Join or view the successor of this event's room.
    #[template_callback]
    fn join_or_view_successor(&self) {
        let Some(room) = self.room() else {
            return;
        };
        let Some(successor) = room.successor() else {
            return;
        };
        let session = room.session();
        let room_list = session.room_list();

        // Join or view the room with the given identifier.
        if let Some(successor_room) = room_list.joined_room(successor.into()) {
            let Some(window) = self.root().and_downcast::<Window>() else {
                return;
            };

            window.session_view().select_room(Some(successor_room));
        } else {
            let successor = successor.to_owned();

            spawn!(clone!(@weak self as obj, @weak room_list => async move {
                if let Err(error) = room_list.join_by_id_or_alias(successor.into(), vec![]).await {
                    toast!(obj, error);
                }
            }));
        }
    }
}

use adw::{prelude::BinExt, subclass::prelude::BinImpl};
use gettextrs::gettext;
use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::{
    components::{Avatar, SpinnerButton},
    session::content::explore::PublicRoom,
};

mod imp {
    use std::cell::RefCell;

    use glib::{signal::SignalHandlerId, subclass::InitializingObject};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-public-room-row.ui")]
    pub struct PublicRoomRow {
        pub public_room: RefCell<Option<PublicRoom>>,
        #[template_child]
        pub avatar: TemplateChild<Avatar>,
        #[template_child]
        pub display_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub description: TemplateChild<gtk::Label>,
        #[template_child]
        pub alias: TemplateChild<gtk::Label>,
        #[template_child]
        pub members_count: TemplateChild<gtk::Label>,
        #[template_child]
        pub button: TemplateChild<SpinnerButton>,
        pub original_child: RefCell<Option<gtk::Widget>>,
        pub pending_handler: RefCell<Option<SignalHandlerId>>,
        pub room_handler: RefCell<Option<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PublicRoomRow {
        const NAME: &'static str = "ContentPublicRoomRow";
        type Type = super::PublicRoomRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PublicRoomRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<PublicRoom>("public-room")
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "public-room" => self.obj().set_public_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "public-room" => self.obj().public_room().to_value(),
                _ => unimplemented!(),
            }
        }
        fn constructed(&self) {
            self.parent_constructed();
            self.button
                .connect_clicked(clone!(@weak self as imp => move |_| {
                    if let Some(public_room) = &*imp.public_room.borrow() {
                        public_room.join_or_view();
                    };
                }));
        }

        fn dispose(&self) {
            if let Some(ref old_public_room) = self.obj().public_room() {
                if let Some(handler) = self.pending_handler.take() {
                    old_public_room.disconnect(handler);
                }
                if let Some(handler_id) = self.room_handler.take() {
                    old_public_room.disconnect(handler_id);
                }
            }
        }
    }

    impl WidgetImpl for PublicRoomRow {}
    impl BinImpl for PublicRoomRow {}
}

glib::wrapper! {
    pub struct PublicRoomRow(ObjectSubclass<imp::PublicRoomRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl PublicRoomRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The public room displayed by this row.
    pub fn public_room(&self) -> Option<PublicRoom> {
        self.imp().public_room.borrow().clone()
    }

    /// Set the public room displayed by this row.
    pub fn set_public_room(&self, public_room: Option<PublicRoom>) {
        let imp = self.imp();
        let old_public_room = self.public_room();

        if old_public_room == public_room {
            return;
        }

        if let Some(ref old_public_room) = old_public_room {
            if let Some(handler) = imp.room_handler.take() {
                old_public_room.disconnect(handler);
            }
            if let Some(handler) = imp.pending_handler.take() {
                old_public_room.disconnect(handler);
            }
        }

        if let Some(ref public_room) = public_room {
            if let Some(child) = imp.original_child.take() {
                self.set_child(Some(&child));
            }
            if let Some(matrix_public_room) = public_room.matrix_public_room() {
                imp.avatar
                    .set_item(Some(public_room.avatar().clone().upcast()));

                let display_name = matrix_public_room
                    .name
                    .as_deref()
                    .map(AsRef::as_ref)
                    // FIXME: display some other identification for this room
                    .unwrap_or("Room without name");
                imp.display_name.set_text(display_name);

                let has_topic = if let Some(ref topic) = matrix_public_room.topic {
                    imp.description.set_text(topic);
                    true
                } else {
                    false
                };

                imp.description.set_visible(has_topic);

                let has_alias = if let Some(ref alias) = matrix_public_room.canonical_alias {
                    imp.alias.set_text(alias.as_str());
                    true
                } else {
                    false
                };

                imp.alias.set_visible(has_alias);
                imp.members_count
                    .set_text(&matrix_public_room.num_joined_members.to_string());

                let pending_handler = public_room.connect_notify_local(
                    Some("pending"),
                    clone!(@weak self as obj => move |public_room, _| {
                            obj.update_button(public_room);
                    }),
                );

                imp.pending_handler.replace(Some(pending_handler));

                let room_handler = public_room.connect_notify_local(
                    Some("room"),
                    clone!(@weak self as obj => move |public_room, _| {
                        obj.update_button(public_room);
                    }),
                );

                imp.room_handler.replace(Some(room_handler));

                self.update_button(public_room);
            } else if imp.original_child.borrow().is_none() {
                let spinner = gtk::Spinner::builder()
                    .spinning(true)
                    .margin_top(12)
                    .margin_bottom(12)
                    .build();
                imp.original_child.replace(self.child());
                self.set_child(Some(&spinner));
            }
        }
        imp.avatar
            .set_item(public_room.clone().map(|room| room.avatar().clone()));
        imp.public_room.replace(public_room);
        self.notify("public-room");
    }

    fn update_button(&self, public_room: &PublicRoom) {
        let button = &self.imp().button;
        if public_room.room().is_some() {
            button.set_label(&gettext("View"));
        } else {
            button.set_label(&gettext("Join"));
        }

        button.set_loading(public_room.is_pending());
    }
}

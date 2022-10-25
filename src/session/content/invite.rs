use adw::subclass::prelude::*;
use gtk::{glib, glib::clone, prelude::*, CompositeTemplate};

use crate::{
    components::{Avatar, LabelWithWidgets, Pill, SpinnerButton},
    gettext_f,
    session::room::{Room, RoomType},
    spawn,
};

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashSet,
    };

    use glib::{signal::SignalHandlerId, subclass::InitializingObject};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-invite.ui")]
    pub struct Invite {
        pub compact: Cell<bool>,
        pub room: RefCell<Option<Room>>,
        pub accept_requests: RefCell<HashSet<Room>>,
        pub reject_requests: RefCell<HashSet<Room>>,
        pub category_handler: RefCell<Option<SignalHandlerId>>,
        #[template_child]
        pub headerbar: TemplateChild<adw::HeaderBar>,
        #[template_child]
        pub room_topic: TemplateChild<gtk::Label>,
        #[template_child]
        pub inviter: TemplateChild<LabelWithWidgets>,
        #[template_child]
        pub accept_button: TemplateChild<SpinnerButton>,
        #[template_child]
        pub reject_button: TemplateChild<SpinnerButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Invite {
        const NAME: &'static str = "ContentInvite";
        type Type = super::Invite;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Pill::static_type();
            Avatar::static_type();
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Group);

            klass.install_action("invite.reject", None, move |widget, _, _| {
                widget.reject();
            });
            klass.install_action("invite.accept", None, move |widget, _, _| {
                widget.accept();
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Invite {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::new(
                        "compact",
                        "Compact",
                        "Whether a compact view is used",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecObject::new(
                        "room",
                        "Room",
                        "The room currently shown",
                        Room::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "compact" => self.compact.set(value.get().unwrap()),
                "room" => self.obj().set_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "compact" => self.compact.get().to_value(),
                "room" => self.obj().room().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.room_topic
                .connect_notify_local(Some("label"), |room_topic, _| {
                    room_topic.set_visible(!room_topic.label().is_empty());
                });

            self.room_topic
                .set_visible(!self.room_topic.label().is_empty());

            // Translators: Do NOT translate the content between '{' and '}', this is a
            // variable name.
            self.inviter
                .set_label(Some(gettext_f("{user} invited you", &[("user", "widget")])));
        }
    }

    impl WidgetImpl for Invite {}
    impl BinImpl for Invite {}
}

glib::wrapper! {
    pub struct Invite(ObjectSubclass<imp::Invite>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Invite {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    pub fn set_room(&self, room: Option<Room>) {
        let priv_ = self.imp();

        if self.room() == room {
            return;
        }

        match room {
            Some(ref room) if priv_.accept_requests.borrow().contains(room) => {
                self.action_set_enabled("invite.accept", false);
                self.action_set_enabled("invite.reject", false);
                priv_.accept_button.set_loading(true);
            }
            Some(ref room) if priv_.reject_requests.borrow().contains(room) => {
                self.action_set_enabled("invite.accept", false);
                self.action_set_enabled("invite.reject", false);
                priv_.reject_button.set_loading(true);
            }
            _ => self.reset(),
        }

        if let Some(category_handler) = priv_.category_handler.take() {
            if let Some(room) = self.room() {
                room.disconnect(category_handler);
            }
        }

        if let Some(ref room) = room {
            let handler_id = room.connect_notify_local(
                Some("category"),
                clone!(@weak self as obj => move |room, _| {
                        if room.category() != RoomType::Invited {
                                let priv_ = obj.imp();
                                priv_.reject_requests.borrow_mut().remove(room);
                                priv_.accept_requests.borrow_mut().remove(room);
                                obj.reset();
                                if let Some(category_handler) = priv_.category_handler.take() {
                                    room.disconnect(category_handler);
                                }
                        }
                }),
            );
            priv_.category_handler.replace(Some(handler_id));
        }

        priv_.room.replace(room);

        self.notify("room");
    }

    pub fn room(&self) -> Option<Room> {
        self.imp().room.borrow().clone()
    }

    fn reset(&self) {
        let priv_ = self.imp();
        priv_.accept_button.set_loading(false);
        priv_.reject_button.set_loading(false);
        self.action_set_enabled("invite.accept", true);
        self.action_set_enabled("invite.reject", true);
    }

    fn accept(&self) -> Option<()> {
        let priv_ = self.imp();
        let room = self.room()?;

        self.action_set_enabled("invite.accept", false);
        self.action_set_enabled("invite.reject", false);
        priv_.accept_button.set_loading(true);
        priv_.accept_requests.borrow_mut().insert(room.clone());

        spawn!(
            clone!(@weak self as obj, @strong room => move || async move {
                    let result = room.accept_invite().await;
                    if result.is_err() {
                        obj.imp().accept_requests.borrow_mut().remove(&room);
                        obj.reset();
                    }
            })()
        );

        Some(())
    }

    fn reject(&self) -> Option<()> {
        let priv_ = self.imp();
        let room = self.room()?;

        self.action_set_enabled("invite.accept", false);
        self.action_set_enabled("invite.reject", false);
        priv_.reject_button.set_loading(true);
        priv_.reject_requests.borrow_mut().insert(room.clone());

        spawn!(
            clone!(@weak self as obj, @strong room => move || async move {
                    let result = room.reject_invite().await;
                    if result.is_err() {
                        obj.imp().reject_requests.borrow_mut().remove(&room);
                        obj.reset();
                    }
            })()
        );

        Some(())
    }
}

impl Default for Invite {
    fn default() -> Self {
        Self::new()
    }
}

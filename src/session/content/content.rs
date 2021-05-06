use adw::subclass::prelude::*;
use gtk::{
    gdk, glib, glib::clone, glib::signal::Inhibit, prelude::*, subclass::prelude::*,
    CompositeTemplate,
};

use crate::session::{content::ItemRow, room::Room};

mod imp {
    use super::*;
    use glib::subclass::InitializingObject;
    use std::cell::{Cell, RefCell};

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/content.ui")]
    pub struct Content {
        pub compact: Cell<bool>,
        pub room: RefCell<Option<Room>>,
        pub md_enabled: Cell<bool>,
        #[template_child]
        pub headerbar: TemplateChild<adw::HeaderBar>,
        #[template_child]
        pub listview: TemplateChild<gtk::ListView>,
        #[template_child]
        pub scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub message_entry: TemplateChild<sourceview::View>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Content {
        const NAME: &'static str = "Content";
        type Type = super::Content;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            ItemRow::static_type();
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Group);

            klass.install_action("content.send-text-message", None, move |widget, _, _| {
                widget.send_text_message();
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Content {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpec::new_boolean(
                        "compact",
                        "Compact",
                        "Wheter a compact view is used or not",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpec::new_object(
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

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "compact" => {
                    let compact = value.get().unwrap();
                    self.compact.set(compact);
                }
                "room" => {
                    let room = value.get().unwrap();
                    obj.set_room(room);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "compact" => self.compact.get().to_value(),
                "room" => obj.room().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            let adj = self.scrolled_window.vadjustment().unwrap();
            // TODO: make sure that we have enough messages to fill at least to scroll pages, if the room history is long enough

            adj.connect_value_changed(clone!(@weak obj => move |adj| {
                // Load more message when the user gets close to the end of the known room history
                // Use the page size twice to detect if the user gets close the end
                if adj.value() < adj.page_size() * 2.0 {
                    if let Some(room) = obj.room() {
                        room.load_previous_events();
                        }
                }
            }));

            let key_events = gtk::EventControllerKey::new();
            self.message_entry.add_controller(&key_events);

            key_events
                .connect_key_pressed(clone!(@weak obj => @default-return Inhibit(false), move |_, key, _, modifier| {
                if !modifier.contains(gdk::ModifierType::SHIFT_MASK) && (key == gdk::keys::constants::Return || key == gdk::keys::constants::KP_Enter) {
                    obj.activate_action("content.send-text-message", None);
                    Inhibit(true)
                } else {
                    Inhibit(false)
                }
            }));
            self.message_entry
                .buffer()
                .connect_text_notify(clone!(@weak obj => move |buffer| {
                   let (start_iter, end_iter) = buffer.bounds();
                   obj.action_set_enabled("content.send-text-message", start_iter != end_iter);
                }));

            let (start_iter, end_iter) = self.message_entry.buffer().bounds();
            obj.action_set_enabled("content.send-text-message", start_iter != end_iter);

            self.parent_constructed(obj);
        }
    }

    impl WidgetImpl for Content {}
    impl BinImpl for Content {}
}

glib::wrapper! {
    pub struct Content(ObjectSubclass<imp::Content>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Content {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create Content")
    }

    pub fn set_room(&self, room: Option<Room>) {
        let priv_ = imp::Content::from_instance(self);

        if self.room() == room {
            return;
        }

        // TODO: use gtk::MultiSelection to allow selection
        let model = room
            .as_ref()
            .and_then(|room| Some(gtk::NoSelection::new(Some(room.timeline()))));

        priv_.listview.set_model(model.as_ref());
        priv_.room.replace(room);
        self.notify("room");
    }

    pub fn room(&self) -> Option<Room> {
        let priv_ = imp::Content::from_instance(self);
        priv_.room.borrow().clone()
    }

    pub fn send_text_message(&self) {
        let priv_ = imp::Content::from_instance(self);
        let buffer = priv_.message_entry.buffer();
        let (start_iter, end_iter) = buffer.bounds();
        let body = buffer.text(&start_iter, &end_iter, true);

        if let Some(room) = &*priv_.room.borrow() {
            room.send_text_message(body.as_str(), priv_.md_enabled.get());
        }

        buffer.set_text("");
    }
}

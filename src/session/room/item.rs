use std::sync::Arc;

use gtk::{glib, glib::DateTime, prelude::*, subclass::prelude::*};
use matrix_sdk::ruma::{events::AnySyncRoomEvent, EventId, UserId};

use crate::session::room::Event;

/// This enum contains all possible types the room history can hold.
#[derive(Debug, Clone)]
pub enum ItemType {
    Event(Event),
    // TODO: Add item type for grouped events
    DayDivider(DateTime),
    NewMessageDivider,
    LoadingSpinner,
}

#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "BoxedItemType")]
pub struct BoxedItemType(ItemType);

impl From<ItemType> for BoxedItemType {
    fn from(type_: ItemType) -> Self {
        BoxedItemType(type_)
    }
}

mod imp {
    use std::cell::Cell;

    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct Item {
        pub type_: OnceCell<ItemType>,
        pub activatable: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Item {
        const NAME: &'static str = "RoomItem";
        type Type = super::Item;
    }

    impl ObjectImpl for Item {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoxed::new(
                        "type",
                        "Type",
                        "The type of this item",
                        BoxedItemType::static_type(),
                        glib::ParamFlags::WRITABLE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecBoolean::new(
                        "selectable",
                        "Selectable",
                        "Whether this item is selectable.",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "show-header",
                        "Show Header",
                        "Whether this item should show a header. This does do nothing if this event doesn’t have a header. ",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "can-hide-header",
                        "Can hide header",
                        "Whether this item is allowed to hide its header.",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "activatable",
                        "Activatable",
                        "Whether this item is activatable.",
                        false,
                        glib::ParamFlags::READWRITE,
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
                "type" => {
                    let type_ = value.get::<BoxedItemType>().unwrap();
                    self.type_.set(type_.0).unwrap();
                }
                "show-header" => {
                    let show_header = value.get().unwrap();
                    let _ = obj.set_show_header(show_header);
                }
                "activatable" => self.activatable.set(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "selectable" => obj.selectable().to_value(),
                "show-header" => obj.show_header().to_value(),
                "can-hide-header" => obj.can_hide_header().to_value(),
                "activatable" => self.activatable.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            if let Some(event) = obj.event() {
                event
                    .bind_property("can-view-media", obj, "activatable")
                    .flags(glib::BindingFlags::SYNC_CREATE)
                    .build();
            }
        }
    }
}

glib::wrapper! {
    /// A row inside the RoomHistory.
    ///
    /// This can be AnySyncRoomEvent, a day divider or new message divider.
    pub struct Item(ObjectSubclass<imp::Item>);
}

impl Item {
    pub fn for_event(event: Event) -> Self {
        let type_ = BoxedItemType(ItemType::Event(event));
        glib::Object::new(&[("type", &type_)]).expect("Failed to create Item")
    }

    pub fn for_day_divider(day: DateTime) -> Self {
        let type_ = BoxedItemType(ItemType::DayDivider(day));
        glib::Object::new(&[("type", &type_)]).expect("Failed to create Item")
    }

    pub fn for_new_message_divider() -> Self {
        let type_ = BoxedItemType(ItemType::NewMessageDivider);
        glib::Object::new(&[("type", &type_)]).expect("Failed to create Item")
    }

    pub fn for_loading_spinner() -> Self {
        let type_ = BoxedItemType(ItemType::LoadingSpinner);
        glib::Object::new(&[("type", &type_)]).expect("Failed to create Item")
    }

    pub fn selectable(&self) -> bool {
        matches!(self.type_(), ItemType::Event(_event))
    }

    pub fn matrix_event(&self) -> Option<AnySyncRoomEvent> {
        if let ItemType::Event(event) = self.imp().type_.get().unwrap() {
            event.matrix_event()
        } else {
            None
        }
    }

    pub fn event(&self) -> Option<&Event> {
        if let ItemType::Event(event) = self.imp().type_.get().unwrap() {
            Some(event)
        } else {
            None
        }
    }

    pub fn matrix_sender(&self) -> Option<Arc<UserId>> {
        if let ItemType::Event(event) = self.imp().type_.get().unwrap() {
            Some(event.matrix_sender())
        } else {
            None
        }
    }

    pub fn matrix_event_id(&self) -> Option<Box<EventId>> {
        if let ItemType::Event(event) = self.imp().type_.get().unwrap() {
            Some(event.matrix_event_id())
        } else {
            None
        }
    }

    pub fn event_timestamp(&self) -> Option<DateTime> {
        if let ItemType::Event(event) = self.imp().type_.get().unwrap() {
            Some(event.timestamp())
        } else {
            None
        }
    }

    pub fn set_show_header(&self, visible: bool) {
        if self.show_header() == visible {
            return;
        }

        if let ItemType::Event(event) = self.imp().type_.get().unwrap() {
            event.set_show_header(visible);
        }

        self.notify("show-header");
    }

    pub fn show_header(&self) -> bool {
        if let ItemType::Event(event) = self.imp().type_.get().unwrap() {
            event.show_header()
        } else {
            false
        }
    }

    pub fn can_hide_header(&self) -> bool {
        if let ItemType::Event(event) = self.imp().type_.get().unwrap() {
            event.can_hide_header()
        } else {
            false
        }
    }

    pub fn type_(&self) -> &ItemType {
        self.imp().type_.get().unwrap()
    }

    pub fn connect_show_header_notify<F: Fn(&Self, &glib::ParamSpec) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_notify_local(Some("show-header"), f)
    }
}

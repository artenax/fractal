use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{gio, glib, glib::clone, subclass::prelude::*};
use matrix_sdk::ruma::events::AnySyncRoomEvent;

use crate::{
    components::{ContextMenuBin, ContextMenuBinExt, ContextMenuBinImpl, ReactionChooser},
    session::{
        content::room_history::{message_row::MessageRow, DividerRow, StateRow},
        room::{Event, EventActions, Item, ItemType},
    },
};

mod imp {
    use std::cell::RefCell;

    use glib::signal::SignalHandlerId;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ItemRow {
        pub item: RefCell<Option<Item>>,
        pub menu_model: RefCell<Option<gio::MenuModel>>,
        pub event_notify_handler: RefCell<Option<SignalHandlerId>>,
        pub reaction_chooser: RefCell<Option<ReactionChooser>>,
        pub emoji_chooser: RefCell<Option<gtk::EmojiChooser>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ItemRow {
        const NAME: &'static str = "ContentItemRow";
        type Type = super::ItemRow;
        type ParentType = ContextMenuBin;
    }

    impl ObjectImpl for ItemRow {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "item",
                    "item",
                    "The item represented by this row",
                    Item::static_type(),
                    glib::ParamFlags::READWRITE,
                )]
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
                "item" => {
                    let item = value.get::<Option<Item>>().unwrap();
                    obj.set_item(item);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "item" => self.item.borrow().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self, _obj: &Self::Type) {
            if let Some(ItemType::Event(event)) =
                self.item.borrow().as_ref().map(|item| item.type_())
            {
                if let Some(handler) = self.event_notify_handler.borrow_mut().take() {
                    event.disconnect(handler);
                }
            }
        }
    }

    impl WidgetImpl for ItemRow {}
    impl BinImpl for ItemRow {}
    impl ContextMenuBinImpl for ItemRow {}
}

glib::wrapper! {
    pub struct ItemRow(ObjectSubclass<imp::ItemRow>)
        @extends gtk::Widget, adw::Bin, ContextMenuBin, @implements gtk::Accessible;
}

// TODO:
// - [ ] Don't show rows for items that don't have a visible UI
impl ItemRow {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create ItemRow")
    }

    /// Get the row's `Item`.
    pub fn item(&self) -> Option<Item> {
        self.imp().item.borrow().clone()
    }

    /// This method sets this row to a new `Item`.
    ///
    /// It tries to reuse the widget and only update the content whenever
    /// possible, but it will create a new widget and drop the old one if it
    /// has to.
    fn set_item(&self, item: Option<Item>) {
        let priv_ = self.imp();

        if let Some(ItemType::Event(event)) = priv_.item.borrow().as_ref().map(|item| item.type_())
        {
            if let Some(handler) = priv_.event_notify_handler.borrow_mut().take() {
                event.disconnect(handler);
            }
        }

        if let Some(ref item) = item {
            match item.type_() {
                ItemType::Event(event) => {
                    if event.message_content().is_some() {
                        let action_group = self.set_event_actions(Some(event)).unwrap();
                        self.set_factory(clone!(@weak event => move |obj, popover| {
                            popover.set_menu_model(Some(Self::event_message_menu_model()));
                            let reaction_chooser = ReactionChooser::new();
                            reaction_chooser.set_reactions(Some(event.reactions().to_owned()));
                            popover.add_child(&reaction_chooser, "reaction-chooser");

                            // Open emoji chooser
                            let more_reactions = gio::SimpleAction::new("more-reactions", None);
                            more_reactions.connect_activate(clone!(@weak obj, @weak popover => move |_, _| {
                                obj.show_emoji_chooser(&popover);
                            }));
                            action_group.add_action(&more_reactions);
                        }));
                    } else {
                        self.set_factory(|_, popover| {
                            popover.set_menu_model(Some(Self::event_state_menu_model()));
                        });
                    }

                    let event_notify_handler = event.connect_notify_local(
                        Some("event"),
                        clone!(@weak self as obj => move |event, _| {
                            obj.set_event_widget(event);
                        }),
                    );

                    priv_
                        .event_notify_handler
                        .borrow_mut()
                        .replace(event_notify_handler);

                    self.set_event_widget(event);
                }
                ItemType::DayDivider(date) => {
                    self.remove_factory();
                    self.set_event_actions(None);

                    let fmt = if date.year() == glib::DateTime::now_local().unwrap().year() {
                        // Translators: This is a date format in the day divider without the year
                        gettext("%A, %B %e")
                    } else {
                        // Translators: This is a date format in the day divider with the year
                        gettext("%A, %B %e, %Y")
                    };
                    let date = date.format(&fmt).unwrap().to_string();

                    if let Some(Ok(child)) = self.child().map(|w| w.downcast::<DividerRow>()) {
                        child.set_label(&date);
                    } else {
                        let child = DividerRow::new(date);
                        self.set_child(Some(&child));
                    };
                }
                ItemType::NewMessageDivider => {
                    self.remove_factory();
                    self.set_event_actions(None);

                    let label = gettext("New Messages");

                    if let Some(Ok(child)) = self.child().map(|w| w.downcast::<DividerRow>()) {
                        child.set_label(&label);
                    } else {
                        let child = DividerRow::new(label);
                        self.set_child(Some(&child));
                    };
                }
                ItemType::LoadingSpinner => {
                    if !self
                        .child()
                        .map_or(false, |widget| widget.is::<gtk::Spinner>())
                    {
                        let spinner = gtk::Spinner::builder()
                            .spinning(true)
                            .margin_top(12)
                            .margin_bottom(12)
                            .build();
                        self.set_child(Some(&spinner));
                    }
                }
            }
        }
        priv_.item.replace(item);
    }

    fn set_event_widget(&self, event: &Event) {
        match event.matrix_event() {
            Some(AnySyncRoomEvent::State(state)) => {
                let child = if let Some(Ok(child)) = self.child().map(|w| w.downcast::<StateRow>())
                {
                    child
                } else {
                    let child = StateRow::new();
                    self.set_child(Some(&child));
                    child
                };
                child.update(&state);
            }
            _ => {
                let child =
                    if let Some(Ok(child)) = self.child().map(|w| w.downcast::<MessageRow>()) {
                        child
                    } else {
                        let child = MessageRow::new();
                        self.set_child(Some(&child));
                        child
                    };
                child.set_event(event.clone());
            }
        }
    }

    fn show_emoji_chooser(&self, popover: &gtk::PopoverMenu) {
        let emoji_chooser = gtk::EmojiChooser::builder().has_arrow(false).build();
        emoji_chooser.connect_emoji_picked(|emoji_chooser, emoji| {
            emoji_chooser
                .activate_action("event.toggle-reaction", Some(&emoji.to_variant()))
                .unwrap();
        });
        emoji_chooser.set_parent(self);
        emoji_chooser.connect_closed(|emoji_chooser| {
            emoji_chooser.unparent();
        });

        let (_, rectangle) = popover.pointing_to();
        emoji_chooser.set_pointing_to(Some(&rectangle));

        popover.popdown();
        emoji_chooser.popup();
    }
}

impl Default for ItemRow {
    fn default() -> Self {
        Self::new()
    }
}

impl EventActions for ItemRow {}

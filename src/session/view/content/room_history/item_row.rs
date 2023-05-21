use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{gdk, gio, glib, glib::clone};
use matrix_sdk::room::timeline::TimelineItemContent;

use super::{DividerRow, EventActions, MessageRow, RoomHistory, StateRow, TypingRow};
use crate::{
    components::{ContextMenuBin, ContextMenuBinExt, ContextMenuBinImpl, ReactionChooser, Spinner},
    session::model::{Event, TimelineItem, VirtualItem, VirtualItemKind},
    utils::BoundObjectWeakRef,
};

mod imp {
    use std::{cell::RefCell, collections::HashMap, rc::Rc};

    use glib::signal::SignalHandlerId;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ItemRow {
        pub room_history: BoundObjectWeakRef<RoomHistory>,
        pub item: RefCell<Option<TimelineItem>>,
        pub action_group: RefCell<Option<gio::SimpleActionGroup>>,
        pub notify_handlers: RefCell<Vec<SignalHandlerId>>,
        pub binding: RefCell<Option<glib::Binding>>,
        pub reaction_chooser: RefCell<Option<ReactionChooser>>,
        pub emoji_chooser: RefCell<Option<gtk::EmojiChooser>>,
        pub actions_expression_watches: RefCell<HashMap<&'static str, gtk::ExpressionWatch>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ItemRow {
        const NAME: &'static str = "RoomHistoryItemRow";
        type Type = super::ItemRow;
        type ParentType = ContextMenuBin;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("room-history-row");
        }
    }

    impl ObjectImpl for ItemRow {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<TimelineItem>("item").build(),
                    glib::ParamSpecObject::builder::<RoomHistory>("room-history")
                        .construct_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "item" => obj.set_item(value.get().unwrap()),
                "room-history" => obj.set_room_history(value.get().ok().as_ref()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "item" => obj.item().to_value(),
                "room-history" => obj.room_history().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.obj().connect_parent_notify(|obj| {
                obj.update_highlight();
            });
        }

        fn dispose(&self) {
            if let Some(event) = self
                .item
                .borrow()
                .as_ref()
                .and_then(|item| item.downcast_ref::<Event>())
            {
                let handlers = self.notify_handlers.take();

                for handler in handlers {
                    event.disconnect(handler);
                }
            } else if let Some(binding) = self.binding.take() {
                binding.unbind();
            }

            for expr_watch in self.actions_expression_watches.take().values() {
                expr_watch.unwatch();
            }

            self.room_history.disconnect_signals();
        }
    }

    impl WidgetImpl for ItemRow {}
    impl BinImpl for ItemRow {}

    impl ContextMenuBinImpl for ItemRow {
        fn menu_opened(&self) {
            let obj = self.obj();

            let Some(event) = obj.item().and_then(|item| item.downcast::<Event>().ok()) else {
                obj.set_popover(None);
                return;
            };

            let room_history = obj.room_history();
            let popover = room_history.item_context_menu().to_owned();
            room_history.set_sticky(false);

            obj.add_css_class("has-open-popup");

            let cell: Rc<RefCell<Option<glib::signal::SignalHandlerId>>> =
                Rc::new(RefCell::new(None));
            let signal_id = popover.connect_closed(
                clone!(@weak obj, @strong cell, @weak room_history => move |popover| {
                    room_history.enable_sticky_mode();

                    obj.remove_css_class("has-open-popup");

                    if let Some(signal_id) = cell.take() {
                        popover.disconnect(signal_id);
                    }
                }),
            );
            cell.replace(Some(signal_id));

            if let Some(event) = event
                .downcast_ref::<Event>()
                .filter(|event| event.is_message())
            {
                let menu_model = Self::Type::event_message_menu_model();
                let reaction_chooser = room_history.item_reaction_chooser();
                if popover.menu_model().as_ref() != Some(menu_model) {
                    popover.set_menu_model(Some(menu_model));
                    popover.add_child(reaction_chooser, "reaction-chooser");
                }

                reaction_chooser.set_reactions(Some(event.reactions().to_owned()));

                // Open emoji chooser
                let more_reactions = gio::SimpleAction::new("more-reactions", None);
                more_reactions.connect_activate(clone!(@weak obj, @weak popover => move |_, _| {
                    obj.show_emoji_chooser(&popover);
                }));
                obj.action_group().unwrap().add_action(&more_reactions);
            } else {
                let menu_model = Self::Type::event_state_menu_model();
                if popover.menu_model().as_ref() != Some(menu_model) {
                    popover.set_menu_model(Some(menu_model));
                }
            }

            obj.set_popover(Some(popover));
        }
    }
}

glib::wrapper! {
    pub struct ItemRow(ObjectSubclass<imp::ItemRow>)
        @extends gtk::Widget, adw::Bin, ContextMenuBin, @implements gtk::Accessible;
}

impl ItemRow {
    pub fn new(room_history: &RoomHistory) -> Self {
        glib::Object::builder()
            .property("room-history", room_history)
            .property("focusable", true)
            .build()
    }

    /// The ancestor room history of this row.
    pub fn room_history(&self) -> RoomHistory {
        self.imp().room_history.obj().unwrap()
    }

    /// Set the ancestor room history of this row.
    fn set_room_history(&self, room_history: Option<&RoomHistory>) {
        let Some(room_history) = room_history else {
            return;
        };

        let related_event_handler = room_history.connect_notify_local(
            Some("related-event"),
            clone!(@weak self as obj => move |_, _| {
                obj.update_for_related_event();
            }),
        );

        self.imp()
            .room_history
            .set(room_history, vec![related_event_handler]);
    }

    pub fn action_group(&self) -> Option<gio::SimpleActionGroup> {
        self.imp().action_group.borrow().clone()
    }

    fn set_action_group(&self, action_group: Option<gio::SimpleActionGroup>) {
        if self.action_group() == action_group {
            return;
        }

        self.imp().action_group.replace(action_group);
    }

    /// Get the row's [`TimelineItem`].
    pub fn item(&self) -> Option<TimelineItem> {
        self.imp().item.borrow().clone()
    }

    /// This method sets this row to a new [`TimelineItem`].
    ///
    /// It tries to reuse the widget and only update the content whenever
    /// possible, but it will create a new widget and drop the old one if it
    /// has to.
    fn set_item(&self, item: Option<TimelineItem>) {
        let imp = self.imp();

        // Reinitialize the header.
        self.remove_css_class("has-header");

        if let Some(event) = imp
            .item
            .borrow()
            .as_ref()
            .and_then(|item| item.downcast_ref::<Event>())
        {
            let handlers = imp.notify_handlers.take();

            for handler in handlers {
                event.disconnect(handler);
            }
        } else if let Some(binding) = imp.binding.take() {
            binding.unbind()
        }

        if let Some(ref item) = item {
            if let Some(event) = item.downcast_ref::<Event>() {
                let source_notify_handler =
                    event.connect_source_notify(clone!(@weak self as obj => move |event| {
                        obj.set_event_widget(event);
                        obj.set_action_group(obj.set_event_actions(Some(event.upcast_ref())));
                    }));
                let is_highlighted_notify_handler = event.connect_notify_local(
                    Some("is-highlighted"),
                    clone!(@weak self as obj => move |_, _| {
                        obj.update_highlight();
                    }),
                );
                imp.notify_handlers
                    .replace(vec![source_notify_handler, is_highlighted_notify_handler]);

                self.set_event_widget(event);
                self.set_action_group(self.set_event_actions(Some(event.upcast_ref())));
            } else if let Some(item) = item.downcast_ref::<VirtualItem>() {
                self.set_popover(None);
                self.set_action_group(None);
                self.set_event_actions(None);

                match item.kind() {
                    VirtualItemKind::Spinner => {
                        if !self.child().map_or(false, |widget| widget.is::<Spinner>()) {
                            let spinner = Spinner::default();
                            spinner.set_margin_top(12);
                            spinner.set_margin_bottom(12);
                            self.set_child(Some(&spinner));
                        }
                    }
                    VirtualItemKind::Typing => {
                        let child = if let Some(child) =
                            self.child().and_then(|w| w.downcast::<TypingRow>().ok())
                        {
                            child
                        } else {
                            let child = TypingRow::new();
                            self.set_child(Some(&child));
                            child
                        };

                        child.set_list(
                            self.room_history()
                                .room()
                                .as_ref()
                                .map(|room| room.typing_list()),
                        );
                    }
                    VirtualItemKind::TimelineStart => {
                        let label = gettext("This is the start of the visible history");

                        if let Some(Ok(child)) = self.child().map(|w| w.downcast::<DividerRow>()) {
                            child.set_label(&label);
                        } else {
                            let child = DividerRow::with_label(label);
                            self.set_child(Some(&child));
                        };
                    }
                    VirtualItemKind::DayDivider(date) => {
                        let child = if let Some(child) =
                            self.child().and_then(|w| w.downcast::<DividerRow>().ok())
                        {
                            child
                        } else {
                            let child = DividerRow::new();
                            self.set_child(Some(&child));
                            child
                        };

                        let fmt = if date.year() == glib::DateTime::now_local().unwrap().year() {
                            // Translators: This is a date format in the day divider without the
                            // year
                            gettext("%A, %B %e")
                        } else {
                            // Translators: This is a date format in the day divider with the year
                            gettext("%A, %B %e, %Y")
                        };

                        child.set_label(&date.format(&fmt).unwrap())
                    }
                    VirtualItemKind::NewMessages => {
                        let label = gettext("New Messages");

                        if let Some(Ok(child)) = self.child().map(|w| w.downcast::<DividerRow>()) {
                            child.set_label(&label);
                        } else {
                            let child = DividerRow::with_label(label);
                            self.set_child(Some(&child));
                        };
                    }
                }
            }
        }
        imp.item.replace(item);

        self.update_highlight();
    }

    fn set_event_widget(&self, event: &Event) {
        match event.content() {
            TimelineItemContent::MembershipChange(_)
            | TimelineItemContent::ProfileChange(_)
            | TimelineItemContent::OtherState(_) => {
                let child = if let Some(Ok(child)) = self.child().map(|w| w.downcast::<StateRow>())
                {
                    child
                } else {
                    let child = StateRow::new();
                    self.set_child(Some(&child));
                    child
                };
                child.set_event(event);
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

    /// Update the highlight state of this row.
    fn update_highlight(&self) {
        let item_ref = self.imp().item.borrow();
        if let Some(event) = item_ref.as_ref().and_then(|i| i.downcast_ref::<Event>()) {
            if event.is_highlighted() {
                self.add_css_class("highlight");
                return;
            }
        }
        self.remove_css_class("highlight");
    }

    fn show_emoji_chooser(&self, popover: &gtk::PopoverMenu) {
        let emoji_chooser = gtk::EmojiChooser::builder().has_arrow(false).build();
        emoji_chooser.connect_emoji_picked(clone!(@weak self as obj => move |_, emoji| {
            obj
                .activate_action("event.toggle-reaction", Some(&emoji.to_variant()))
                .unwrap();
        }));
        emoji_chooser.set_parent(self);
        emoji_chooser.connect_closed(|emoji_chooser| {
            emoji_chooser.unparent();
        });

        let (_, rectangle) = popover.pointing_to();
        emoji_chooser.set_pointing_to(Some(&rectangle));

        popover.popdown();
        emoji_chooser.popup();
    }

    /// Update this row for the currently related event.
    fn update_for_related_event(&self) {
        let related_event = self.room_history().related_event();
        let event = self.item().and_then(|item| item.downcast::<Event>().ok());

        if event.is_some() && event == related_event {
            self.add_css_class("selected");
        } else {
            self.remove_css_class("selected");
        }
    }
}

impl EventActions for ItemRow {
    fn texture(&self) -> Option<gdk::Texture> {
        self.child()
            .and_then(|w| w.downcast::<MessageRow>().ok())
            .and_then(|r| r.texture())
    }

    fn set_expression_watch(&self, key: &'static str, expr_watch: gtk::ExpressionWatch) {
        self.imp()
            .actions_expression_watches
            .borrow_mut()
            .insert(key, expr_watch);
    }

    fn expression_watch(&self, key: &&str) -> Option<gtk::ExpressionWatch> {
        self.imp()
            .actions_expression_watches
            .borrow()
            .get(key)
            .cloned()
    }

    fn clear_expression_watches(&self) {
        for expr_watch in self.imp().actions_expression_watches.take().values() {
            expr_watch.unwatch();
        }
    }
}

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{gio, glib, glib::clone};
use matrix_sdk_ui::timeline::TimelineItemContent;
use once_cell::sync::Lazy;
use ruma::events::room::{message::MessageType, power_levels::PowerLevelAction};
use tracing::error;

use super::{DividerRow, MessageRow, RoomHistory, StateRow, TypingRow};
use crate::{
    components::{ContextMenuBin, ContextMenuBinExt, ContextMenuBinImpl, ReactionChooser, Spinner},
    prelude::*,
    session::{
        model::{Event, EventKey, TimelineItem, VirtualItem, VirtualItemKind},
        view::EventSourceDialog,
    },
    spawn, spawn_tokio, toast,
    utils::{media::save_to_file, BoundObjectWeakRef},
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
            if let Some(event) = self.item.borrow().and_downcast_ref::<Event>() {
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

            let Some(event) = obj.item().and_downcast::<Event>() else {
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
                let menu_model = event_message_menu_model();
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
                let menu_model = event_state_menu_model();
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

        if let Some(event) = imp.item.borrow().and_downcast_ref::<Event>() {
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
                        obj.set_event_widget(event.clone());
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

                self.set_event_widget(event.clone());
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
                        let child = if let Some(child) = self.child().and_downcast::<TypingRow>() {
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

                        if let Some(child) = self.child().and_downcast::<DividerRow>() {
                            child.set_label(&label);
                        } else {
                            let child = DividerRow::with_label(label);
                            self.set_child(Some(&child));
                        };
                    }
                    VirtualItemKind::DayDivider(date) => {
                        let child = if let Some(child) = self.child().and_downcast::<DividerRow>() {
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

                        if let Some(child) = self.child().and_downcast::<DividerRow>() {
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

    fn set_event_widget(&self, event: Event) {
        match event.content() {
            TimelineItemContent::MembershipChange(_)
            | TimelineItemContent::ProfileChange(_)
            | TimelineItemContent::OtherState(_) => {
                let child = if let Some(child) = self.child().and_downcast::<StateRow>() {
                    child
                } else {
                    let child = StateRow::new();
                    self.set_child(Some(&child));
                    child
                };
                child.set_event(event);
            }
            _ => {
                let child = if let Some(child) = self.child().and_downcast::<MessageRow>() {
                    child
                } else {
                    let child = MessageRow::new();
                    self.set_child(Some(&child));
                    child
                };
                child.set_event(event);
            }
        }
    }

    /// Update the highlight state of this row.
    fn update_highlight(&self) {
        let item_ref = self.imp().item.borrow();
        if let Some(event) = item_ref.and_downcast_ref::<Event>() {
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
        let event = self.item().and_downcast::<Event>();

        if event.is_some() && event == related_event {
            self.add_css_class("selected");
        } else {
            self.remove_css_class("selected");
        }
    }

    /// Set the actions available on `self` for `event`.
    ///
    /// Unsets the actions if `event` is `None`.
    fn set_event_actions(&self, event: Option<&Event>) -> Option<gio::SimpleActionGroup> {
        self.clear_expression_watches();
        let event = match event {
            Some(event) => event,
            None => {
                self.insert_action_group("event", gio::ActionGroup::NONE);
                return None;
            }
        };
        let action_group = gio::SimpleActionGroup::new();

        if event.raw().is_some() {
            action_group.add_action_entries([
                // View Event Source
                gio::ActionEntry::builder("view-source")
                    .activate(clone!(@weak self as widget, @weak event => move |_, _, _| {
                        let window = widget.root().and_downcast().unwrap();
                        let dialog = EventSourceDialog::new(&window, &event);
                        dialog.present();
                    }))
                    .build(),
            ]);
        }

        if event.event_id().is_some() {
            action_group.add_action_entries([
                // Create a permalink
                gio::ActionEntry::builder("permalink")
                    .activate(clone!(@weak self as widget, @weak event => move |_, _, _| {
                        let matrix_room = event.room().matrix_room();
                        let event_id = event.event_id().unwrap();
                        spawn!(clone!(@weak widget => async move {
                            let handle = spawn_tokio!(async move {
                                matrix_room.matrix_to_event_permalink(event_id).await
                            });
                            match handle.await.unwrap() {
                                Ok(permalink) => {
                                        widget.clipboard().set_text(&permalink.to_string());
                                        toast!(widget, gettext("Permalink copied to clipboard"));
                                    },
                                Err(error) => {
                                    error!("Could not get permalink: {error}");
                                    toast!(widget, gettext("Failed to copy the permalink"));
                                }
                            }
                        })
                    );
                }))
                .build()
            ]);

            if let TimelineItemContent::Message(message) = event.content() {
                let own_user_id = event
                    .room()
                    .session()
                    .user()
                    .map(|user| user.user_id())
                    .unwrap();
                let is_from_own_user = event.sender_id() == own_user_id;

                // Remove message
                fn update_remove_action(
                    action_group: &gio::SimpleActionGroup,
                    event: &Event,
                    allowed: bool,
                ) {
                    if allowed {
                        action_group.add_action_entries([gio::ActionEntry::builder("remove")
                            .activate(clone!(@weak event, => move |_, _, _| {
                                if let Some(event_id) = event.event_id() {
                                    event.room().redact(event_id, None);
                                }
                            }))
                            .build()]);
                    } else {
                        action_group.remove_action("remove");
                    }
                }

                if is_from_own_user {
                    update_remove_action(&action_group, event, true);
                } else {
                    let remove_watch = event
                        .room()
                        .own_user_is_allowed_to_expr(PowerLevelAction::Redact)
                        .watch(
                            glib::Object::NONE,
                            clone!(@weak self as widget, @weak action_group, @weak event => move || {
                                let Some(allowed) = widget.expression_watch(&"remove").and_then(|e| e.evaluate_as::<bool>()) else {
                                    return;
                                };

                                update_remove_action(&action_group, &event, allowed);
                            }),
                        );

                    let allowed = remove_watch.evaluate_as::<bool>().unwrap();
                    update_remove_action(&action_group, event, allowed);

                    self.set_expression_watch("remove", remove_watch);
                }

                action_group.add_action_entries([
                    // Send/redact a reaction
                    gio::ActionEntry::builder("toggle-reaction")
                        .parameter_type(Some(&String::static_variant_type()))
                        .activate(clone!(@weak event => move |_, _, variant| {
                            let key: String = variant.unwrap().get().unwrap();
                            let room = event.room();

                            let reaction_group = event.reactions().reaction_group_by_key(&key);

                            if let Some(reaction_key) = reaction_group.and_then(|group| group.user_reaction_event_key()) {
                                // The user already sent that reaction, redact it if it has been sent.
                                if let EventKey::EventId(reaction_id) = reaction_key {
                                    room.redact(reaction_id, None);
                                }
                            } else if let Some(event_id) = event.event_id() {
                                // The user didn't send that reaction, send it.
                                room.send_reaction(key, event_id);
                            }
                        }))
                        .build(),
                    // Reply
                    gio::ActionEntry::builder("reply")
                        .activate(clone!(@weak event, @weak self as widget => move |_, _, _| {
                            if let Some(event_id) = event.event_id() {
                                let _ = widget.activate_action(
                                    "room-history.reply",
                                    Some(&event_id.as_str().to_variant())
                                );
                            }
                        }))
                    .build()
                ]);

                match message.msgtype() {
                    MessageType::Text(text_message) => {
                        // Copy text message.
                        let body = text_message.body.clone();

                        action_group.add_action_entries([gio::ActionEntry::builder("copy-text")
                            .activate(clone!(@weak self as widget => move |_, _, _| {
                                widget.clipboard().set_text(&body);
                                toast!(widget, gettext("Message copied to clipboard"));
                            }))
                            .build()]);

                        // Edit
                        if is_from_own_user {
                            action_group.add_action_entries([gio::ActionEntry::builder("edit")
                                .activate(
                                    clone!(@weak event, @weak self as widget => move |_, _, _| {
                                        if let Some(event_id) = event.event_id() {
                                            let _ = widget.activate_action(
                                                "room-history.edit",
                                                Some(&event_id.as_str().to_variant())
                                            );
                                        }
                                    }),
                                )
                                .build()]);
                        }
                    }
                    MessageType::File(_) => {
                        // Save message's file.
                        action_group.add_action_entries([gio::ActionEntry::builder("file-save")
                            .activate(clone!(@weak self as widget, @weak event => move |_, _, _| {
                                widget.save_event_file(event);
                            }))
                            .build()]);
                    }
                    MessageType::Emote(message) => {
                        // Copy text message.
                        let message = message.clone();

                        action_group.add_action_entries([gio::ActionEntry::builder("copy-text")
                            .activate(clone!(@weak self as widget, @weak event => move |_, _, _| {
                                let display_name = event.sender().display_name();
                                let message = format!("{display_name} {}", message.body);
                                widget.clipboard().set_text(&message);
                                toast!(widget, gettext("Message copied to clipboard"));
                            }))
                            .build()]);

                        // Edit
                        if is_from_own_user {
                            action_group.add_action_entries([gio::ActionEntry::builder("edit")
                                .activate(
                                    clone!(@weak event, @weak self as widget => move |_, _, _| {
                                        if let Some(event_id) = event.event_id() {
                                            let _ = widget.activate_action(
                                                "room-history.edit",
                                                Some(&event_id.as_str().to_variant())
                                            );
                                        }
                                    }),
                                )
                                .build()]);
                        }
                    }
                    MessageType::Notice(message) => {
                        // Copy text message.
                        let body = message.body.clone();

                        action_group.add_action_entries([gio::ActionEntry::builder("copy-text")
                            .activate(clone!(@weak self as widget => move |_, _, _| {
                                widget.clipboard().set_text(&body);
                                toast!(widget, gettext("Message copied to clipboard"));
                            }))
                            .build()]);
                    }
                    MessageType::Image(_) => {
                        action_group.add_action_entries([
                            // Copy the texture to the clipboard.
                            gio::ActionEntry::builder("copy-image")
                                .activate(clone!(@weak self as widget, @weak event => move |_, _, _| {
                                    let texture = widget.child()
                                        .and_downcast::<MessageRow>()
                                        .and_then(|r| r.texture())
                                        .expect("An ItemRow with an image should have a texture");

                                    widget.clipboard().set_texture(&texture);
                                    toast!(widget, gettext("Thumbnail copied to clipboard"));
                                })
                            ).build(),
                            // Save the image to a file.
                            gio::ActionEntry::builder("save-image")
                                .activate(clone!(@weak self as widget, @weak event => move |_, _, _| {
                                    widget.save_event_file(event);
                                })
                            ).build()
                        ]);
                    }
                    MessageType::Video(_) => {
                        // Save the video to a file.
                        action_group.add_action_entries([gio::ActionEntry::builder("save-video")
                            .activate(clone!(@weak self as widget, @weak event => move |_, _, _| {
                                widget.save_event_file(event);
                            }))
                            .build()]);
                    }
                    MessageType::Audio(_) => {
                        // Save the audio to a file.
                        action_group.add_action_entries([gio::ActionEntry::builder("save-audio")
                            .activate(clone!(@weak self as widget, @weak event => move |_, _, _| {
                                widget.save_event_file(event);
                            }))
                            .build()]);
                    }
                    _ => {}
                }
            }
        }

        self.insert_action_group("event", Some(&action_group));

        Some(action_group)
    }

    /// Save the file in `event`.
    ///
    /// See [`Event::get_media_content()`] for compatible events.
    /// Panics on an incompatible event.
    fn save_event_file(&self, event: Event) {
        spawn!(clone!(@weak self as obj => async move {
            let (filename, data) = match event.get_media_content().await {
                Ok(res) => res,
                Err(error) => {
                    error!("Could not get event file: {error}");
                    toast!(obj, error.to_user_facing());

                    return;
                }
            };

            save_to_file(&obj, data, filename).await;
        }));
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

// This is only safe because the trait `EventActions` can
// only be implemented on `gtk::Widgets` that run only on the main thread
struct MenuModelSendSync(gio::MenuModel);
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for MenuModelSendSync {}
unsafe impl Sync for MenuModelSendSync {}

/// The `MenuModel` for common message event actions.
fn event_message_menu_model() -> &'static gio::MenuModel {
    static MODEL: Lazy<MenuModelSendSync> = Lazy::new(|| {
        MenuModelSendSync(
            gtk::Builder::from_resource(
                "/org/gnome/Fractal/ui/session/view/content/room_history/event_actions.ui",
            )
            .object::<gio::MenuModel>("message_menu_model")
            .unwrap(),
        )
    });
    &MODEL.0
}

/// The `MenuModel` for common state event actions.
fn event_state_menu_model() -> &'static gio::MenuModel {
    static MODEL: Lazy<MenuModelSendSync> = Lazy::new(|| {
        MenuModelSendSync(
            gtk::Builder::from_resource(
                "/org/gnome/Fractal/ui/session/view/content/room_history/event_actions.ui",
            )
            .object::<gio::MenuModel>("state_menu_model")
            .unwrap(),
        )
    });
    &MODEL.0
}

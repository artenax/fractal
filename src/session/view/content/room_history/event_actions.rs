use gettextrs::gettext;
use gtk::{gdk, gio, glib, glib::clone, prelude::*};
use log::error;
use matrix_sdk::{room::timeline::TimelineItemContent, ruma::events::room::message::MessageType};
use once_cell::sync::Lazy;
use ruma::events::room::power_levels::PowerLevelAction;

use crate::{
    prelude::*,
    session::{
        model::{Event, EventKey},
        view::EventSourceDialog,
    },
    spawn, spawn_tokio, toast,
    utils::media::save_to_file,
};

// This is only safe because the trait `EventActions` can
// only be implemented on `gtk::Widgets` that run only on the main thread
struct MenuModelSendSync(gio::MenuModel);
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for MenuModelSendSync {}
unsafe impl Sync for MenuModelSendSync {}

pub trait EventActions
where
    Self: IsA<gtk::Widget>,
    Self: glib::clone::Downgrade,
    <Self as glib::clone::Downgrade>::Weak: glib::clone::Upgrade<Strong = Self>,
{
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

    /// The default `MenuModel` for common state event actions.
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

    /// Store a `GtkExpressionWatch` for re-use later.
    fn set_expression_watch(&self, key: &'static str, expr_watch: gtk::ExpressionWatch);

    /// Get a `GtkExpressionWatch` by key.
    fn expression_watch(&self, key: &&str) -> Option<gtk::ExpressionWatch>;

    /// Unwatch and drop all the expression watches on this widget.
    fn clear_expression_watches(&self);

    /// Set the actions available on `self` for `event`.
    ///
    /// Unsets the actions if `event` is `None`.
    ///
    /// Should be paired with the `EventActions` menu models.
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
                        let window = widget.root().unwrap().downcast().unwrap();
                        let dialog = EventSourceDialog::new(&window, &event);
                        dialog.present();
                    }))
                    .build(),
            ]);
        }

        if let Some(event) = event
            .downcast_ref::<Event>()
            .filter(|event| event.event_id().is_some())
        {
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
                let user_id = event
                    .room()
                    .session()
                    .user()
                    .map(|user| user.user_id())
                    .unwrap();
                let user = event.room().members().get_or_create(user_id);

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

                if event.sender() == user {
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
                                    let texture = widget.texture().expect("A widget with an image should have a texture");

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

    /// Get the texture displayed by this widget, if any.
    fn texture(&self) -> Option<gdk::Texture>;
}

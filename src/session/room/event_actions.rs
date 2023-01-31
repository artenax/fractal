use gettextrs::gettext;
use gtk::{gdk, gio, glib, glib::clone, prelude::*};
use log::error;
use matrix_sdk::{room::timeline::TimelineItemContent, ruma::events::room::message::MessageType};
use once_cell::sync::Lazy;

use crate::{
    prelude::*,
    session::{
        event_source_dialog::EventSourceDialog,
        room::{Event, EventKey, RoomAction},
    },
    spawn, spawn_tokio, toast, UserFacingError, Window,
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
                gtk::Builder::from_resource("/org/gnome/Fractal/event-menu.ui")
                    .object::<gio::MenuModel>("message_menu_model")
                    .unwrap(),
            )
        });
        &MODEL.0
    }

    /// The `MenuModel` for common media message event actions.
    fn event_media_menu_model() -> &'static gio::MenuModel {
        static MODEL: Lazy<MenuModelSendSync> = Lazy::new(|| {
            MenuModelSendSync(
                gtk::Builder::from_resource("/org/gnome/Fractal/event-menu.ui")
                    .object::<gio::MenuModel>("media_menu_model")
                    .unwrap(),
            )
        });
        &MODEL.0
    }

    /// The default `MenuModel` for common state event actions.
    fn event_state_menu_model() -> &'static gio::MenuModel {
        static MODEL: Lazy<MenuModelSendSync> = Lazy::new(|| {
            MenuModelSendSync(
                gtk::Builder::from_resource("/org/gnome/Fractal/event-menu.ui")
                    .object::<gio::MenuModel>("state_menu_model")
                    .unwrap(),
            )
        });
        &MODEL.0
    }

    /// Set the actions available on `self` for `event`.
    ///
    /// Unsets the actions if `event` is `None`.
    ///
    /// Should be paired with the `EventActions` menu models.
    fn set_event_actions(&self, event: Option<&Event>) -> Option<gio::SimpleActionGroup> {
        let event = match event {
            Some(event) => event,
            None => {
                self.insert_action_group("event", gio::ActionGroup::NONE);
                return None;
            }
        };
        let action_group = gio::SimpleActionGroup::new();

        // View Event Source
        gtk_macros::action!(
            &action_group,
            "view-source",
            clone!(@weak self as widget, @weak event => move |_, _| {
                let window = widget.root().unwrap().downcast().unwrap();
                let dialog = EventSourceDialog::new(&window, &event);
                dialog.show();
            })
        );

        // Create a permalink
        if event.event_id().is_some() {
            gtk_macros::action!(
                &action_group,
                "permalink",
                clone!(@weak self as widget, @weak event => move |_, _| {
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
                                    error!("Could not get permalink: {}", error);
                                    toast!(widget, gettext("Failed to copy the permalink"));
                                }
                            }
                        })
                    );
                })
            );
        }

        if let Some(event) = event
            .downcast_ref::<Event>()
            .filter(|event| event.event_id().is_some())
        {
            if let TimelineItemContent::Message(message) = event.content() {
                let user_id = event
                    .room()
                    .session()
                    .user()
                    .map(|user| user.user_id())
                    .unwrap();
                let user = event.room().members().member_by_id(user_id);

                // Remove message
                if event.sender() == user
                    || event
                        .room()
                        .power_levels()
                        .min_level_for_room_action(&RoomAction::Redact)
                        <= user.power_level()
                {
                    gtk_macros::action!(
                        &action_group,
                        "remove",
                        clone!(@weak event, => move |_, _| {
                            if let Some(event_id) = event.event_id() {
                                event.room().redact(event_id, None);
                            }
                        })
                    );
                }

                // Send/redact a reaction
                gtk_macros::action!(
                    &action_group,
                    "toggle-reaction",
                    Some(&String::static_variant_type()),
                    clone!(@weak event => move |_, variant| {
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
                    })
                );

                // Reply
                gtk_macros::action!(
                    &action_group,
                    "reply",
                    None,
                    clone!(@weak event, @weak self as widget => move |_, _| {
                        if let Some(event_id) = event.event_id() {
                            let _ = widget.activate_action(
                                "room-history.reply",
                                Some(&event_id.as_str().to_variant())
                            );
                        }
                    })
                );

                match message.msgtype() {
                    // Copy Text-Message
                    MessageType::Text(text_message) => {
                        let body = text_message.body.clone();

                        gtk_macros::action!(
                            &action_group,
                            "copy-text",
                            clone!(@weak self as widget => move |_, _| {
                                widget.clipboard().set_text(&body);
                                toast!(widget, gettext("Message copied to clipboard"));
                            })
                        );
                    }
                    MessageType::File(_) => {
                        // Save message's file
                        gtk_macros::action!(
                            &action_group,
                            "file-save",
                            clone!(@weak self as widget, @weak event => move |_, _| {
                            widget.save_event_file(event);
                            })
                        );
                    }
                    MessageType::Emote(message) => {
                        let message = message.clone();

                        gtk_macros::action!(
                            &action_group,
                            "copy-text",
                            clone!(@weak self as widget, @weak event => move |_, _| {
                                let display_name = event.sender().display_name();
                                let message = display_name + " " + &message.body;
                                widget.clipboard().set_text(&message);
                                toast!(widget, gettext("Message copied to clipboard"));
                            })
                        );
                    }

                    MessageType::Image(_) => {
                        // Copy the texture to the clipboard.
                        gtk_macros::action!(
                            &action_group,
                            "copy-image",
                            clone!(@weak self as widget, @weak event => move |_, _| {
                                let texture = widget.texture().expect("A widget with an image should have a texture");

                                match texture {
                                    EventTexture::Original(texture) => {
                                        widget.clipboard().set_texture(&texture);
                                        toast!(widget, gettext("Image copied to clipboard"));
                                    }
                                    EventTexture::Thumbnail(texture) => {
                                        widget.clipboard().set_texture(&texture);
                                        toast!(widget, gettext("Thumbnail copied to clipboard"));
                                    }
                                }
                            })
                        );

                        // Save the image to a file.
                        gtk_macros::action!(
                            &action_group,
                            "save-image",
                            clone!(@weak self as widget, @weak event => move |_, _| {
                                widget.save_event_file(event);
                            })
                        );
                    }
                    MessageType::Video(_) => {
                        gtk_macros::action!(
                            &action_group,
                            "save-video",
                            clone!(@weak self as widget, @weak event => move |_, _| {
                                widget.save_event_file(event);
                            })
                        );
                    }
                    MessageType::Audio(_) => {
                        gtk_macros::action!(
                            &action_group,
                            "save-audio",
                            clone!(@weak self as widget, @weak event => move |_, _| {
                                widget.save_event_file(event);
                            })
                        );
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
        let window: Window = self.root().unwrap().downcast().unwrap();
        spawn!(
            glib::PRIORITY_LOW,
            clone!(@weak self as obj, @weak window => async move {
                let (_, filename, data) = match event.get_media_content().await {
                    Ok(res) => res,
                    Err(err) => {
                        error!("Could not get file: {}", err);
                        toast!(obj, err.to_user_facing());

                        return;
                    }
                };

                let dialog = gtk::FileChooserNative::new(
                    Some(&gettext("Save File")),
                    Some(&window),
                    gtk::FileChooserAction::Save,
                    Some(&gettext("Save")),
                    Some(&gettext("Cancel")),
                );
                dialog.set_current_name(&filename);

                let response = dialog.run_future().await;

                if response == gtk::ResponseType::Accept {
                    if let Some(file) = dialog.file() {
                        file.replace_contents(
                            &data,
                            None,
                            false,
                            gio::FileCreateFlags::REPLACE_DESTINATION,
                            gio::Cancellable::NONE,
                        )
                        .unwrap();
                    }
                }

                dialog.destroy();
            })
        );
    }

    /// Get the texture displayed by this widget, if any.
    fn texture(&self) -> Option<EventTexture>;
}

/// A texture from an event.
pub enum EventTexture {
    /// The texture is the original image.
    Original(gdk::Texture),

    /// The texture is a thumbnail of the image.
    Thumbnail(gdk::Texture),
}

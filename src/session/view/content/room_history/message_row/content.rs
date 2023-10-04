use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{gdk, glib, glib::clone};
use matrix_sdk_ui::timeline::{TimelineDetails, TimelineItemContent};
use ruma::events::room::message::MessageType;
use tracing::{error, warn};

use super::{
    audio::MessageAudio, file::MessageFile, location::MessageLocation, media::MessageMedia,
    reply::MessageReply, text::MessageText,
};
use crate::{
    session::model::{Event, Member, Room},
    spawn,
    utils::media::filename_for_mime,
};

#[derive(Debug, Default, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(i32)]
#[enum_type(name = "ContentFormat")]
pub enum ContentFormat {
    /// The content should appear at its natural size.
    #[default]
    Natural = 0,

    /// The content should appear in a smaller format without interactions, if
    /// possible.
    ///
    /// This has no effect on text replies.
    ///
    /// The related events of replies are not displayed.
    Compact = 1,

    /// Like `Compact`, but the content should be ellipsized if possible to show
    /// only a single line.
    Ellipsized = 2,
}

mod imp {
    use std::cell::Cell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct MessageContent {
        pub format: Cell<ContentFormat>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MessageContent {
        const NAME: &'static str = "ContentMessageContent";
        type Type = super::MessageContent;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for MessageContent {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecEnum::builder::<ContentFormat>("format")
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "format" => self.obj().set_format(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "format" => self.obj().format().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for MessageContent {}
    impl BinImpl for MessageContent {}
}

glib::wrapper! {
    pub struct MessageContent(ObjectSubclass<imp::MessageContent>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl MessageContent {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The displayed format of the message.
    pub fn format(&self) -> ContentFormat {
        self.imp().format.get()
    }

    /// Set the displayed format of the message.
    pub fn set_format(&self, format: ContentFormat) {
        if self.format() == format {
            return;
        }

        self.imp().format.set(format);
        self.notify("format");
    }

    /// Access the widget with the own content of the event.
    ///
    /// This allows to access the descendant content while discarding the
    /// content of a related message, like a replied-to event.
    pub fn content_widget(&self) -> Option<gtk::Widget> {
        let child = self.child()?;

        if let Some(reply) = child.downcast_ref::<MessageReply>() {
            reply.content().child()
        } else {
            Some(child)
        }
    }

    pub fn update_for_event(&self, event: &Event) {
        let format = self.format();
        if format == ContentFormat::Natural {
            if let Some(related_content) = event.reply_to_event_content() {
                match related_content {
                    TimelineDetails::Unavailable => {
                        spawn!(
                            glib::Priority::HIGH,
                            clone!(@weak event => async move {
                                if let Err(error) = event.fetch_missing_details().await {
                                    error!("Failed to fetch event details: {error}");
                                }
                            })
                        );
                    }
                    TimelineDetails::Error(error) => {
                        error!(
                            "Failed to fetch replied to event '{}': {error}",
                            event.reply_to_id().unwrap()
                        );
                    }
                    TimelineDetails::Ready(related_content) => {
                        let room = event.room();
                        // We should have a strong reference to the list in the RoomHistory so we
                        // can use `get_or_create_members()`.
                        let sender = room
                            .get_or_create_members()
                            .get_or_create(related_content.sender().to_owned());
                        let reply = MessageReply::new();
                        reply.set_related_content_sender(sender.upcast_ref());
                        build_content(
                            reply.related_content(),
                            related_content.content().clone(),
                            ContentFormat::Compact,
                            sender,
                            &room,
                        );
                        build_content(
                            reply.content(),
                            event.content(),
                            ContentFormat::Natural,
                            event.sender(),
                            &room,
                        );
                        self.set_child(Some(&reply));

                        return;
                    }
                    TimelineDetails::Pending => {}
                }
            }
        }

        build_content(self, event.content(), format, event.sender(), &event.room());
    }

    /// Get the texture displayed by this widget, if any.
    pub fn texture(&self) -> Option<gdk::Texture> {
        self.content_widget()?
            .downcast_ref::<MessageMedia>()?
            .texture()
    }
}

/// Build the content widget of `event` as a child of `parent`.
fn build_content(
    parent: &impl IsA<adw::Bin>,
    content: TimelineItemContent,
    format: ContentFormat,
    sender: Member,
    room: &Room,
) {
    let parent = parent.upcast_ref();
    match content {
        TimelineItemContent::Message(message) => {
            match message.msgtype() {
                MessageType::Audio(message) => {
                    let child = if let Some(child) = parent.child().and_downcast::<MessageAudio>() {
                        child
                    } else {
                        let child = MessageAudio::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.audio(message.clone(), &room.session(), format);
                }
                MessageType::Emote(message) => {
                    let child = if let Some(child) = parent.child().and_downcast::<MessageText>() {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.with_emote(
                        message.formatted.clone(),
                        message.body.clone(),
                        sender,
                        room,
                        format,
                    );
                }
                MessageType::File(message) => {
                    let info = message.info.as_ref();
                    let filename = message
                        .filename
                        .clone()
                        .filter(|name| !name.is_empty())
                        .or_else(|| Some(message.body.clone()))
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| {
                            filename_for_mime(info.and_then(|info| info.mimetype.as_deref()), None)
                        });

                    let child = if let Some(child) = parent.child().and_downcast::<MessageFile>() {
                        child
                    } else {
                        let child = MessageFile::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.set_filename(Some(filename));
                    child.set_format(format);
                }
                MessageType::Image(message) => {
                    let child = if let Some(child) = parent.child().and_downcast::<MessageMedia>() {
                        child
                    } else {
                        let child = MessageMedia::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.image(message.clone(), &room.session(), format);
                }
                MessageType::Location(message) => {
                    let child =
                        if let Some(child) = parent.child().and_downcast::<MessageLocation>() {
                            child
                        } else {
                            let child = MessageLocation::new();
                            parent.set_child(Some(&child));
                            child
                        };
                    child.set_geo_uri(&message.geo_uri, format);
                }
                MessageType::Notice(message) => {
                    let child = if let Some(child) = parent.child().and_downcast::<MessageText>() {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.with_markup(
                        message.formatted.clone(),
                        message.body.clone(),
                        room,
                        format,
                    );
                }
                MessageType::ServerNotice(message) => {
                    let child = if let Some(child) = parent.child().and_downcast::<MessageText>() {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.with_text(message.body.clone(), format);
                }
                MessageType::Text(message) => {
                    let child = if let Some(child) = parent.child().and_downcast::<MessageText>() {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.with_markup(
                        message.formatted.clone(),
                        message.body.clone(),
                        room,
                        format,
                    );
                }
                MessageType::Video(message) => {
                    let child = if let Some(child) = parent.child().and_downcast::<MessageMedia>() {
                        child
                    } else {
                        let child = MessageMedia::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.video(message.clone(), &room.session(), format);
                }
                MessageType::VerificationRequest(_) => {
                    // TODO: show more information about the verification
                    let child = if let Some(child) = parent.child().and_downcast::<MessageText>() {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.with_text(gettext("Identity verification was started"), format);
                }
                msgtype => {
                    warn!("Event not supported: {msgtype:?}");
                    let child = if let Some(child) = parent.child().and_downcast::<MessageText>() {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.with_text(gettext("Unsupported event"), format);
                }
            }
        }
        TimelineItemContent::Sticker(sticker) => {
            let child = if let Some(child) = parent.child().and_downcast::<MessageMedia>() {
                child
            } else {
                let child = MessageMedia::new();
                parent.set_child(Some(&child));
                child
            };
            child.sticker(sticker.content().clone(), &room.session(), format);
        }
        TimelineItemContent::UnableToDecrypt(_) => {
            let child = if let Some(child) = parent.child().and_downcast::<MessageText>() {
                child
            } else {
                let child = MessageText::new();
                parent.set_child(Some(&child));
                child
            };
            child.with_text(gettext("Unable to decrypt this message, decryption will be retried once the keys are available."), format);
        }
        TimelineItemContent::RedactedMessage => {
            let child = if let Some(child) = parent.child().and_downcast::<MessageText>() {
                child
            } else {
                let child = MessageText::new();
                parent.set_child(Some(&child));
                child
            };
            child.with_text(gettext("This message was redacted."), format);
        }
        content => {
            warn!("Unsupported event content: {content:?}");
            let child = if let Some(child) = parent.child().and_downcast::<MessageText>() {
                child
            } else {
                let child = MessageText::new();
                parent.set_child(Some(&child));
                child
            };
            child.with_text(gettext("Unsupported event"), format);
        }
    }
}

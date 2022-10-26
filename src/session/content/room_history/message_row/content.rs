use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{glib, glib::clone};
use log::warn;
use matrix_sdk::ruma::events::{
    room::message::{MessageType, Relation},
    AnyMessageLikeEventContent,
};

use super::{
    audio::MessageAudio, file::MessageFile, location::MessageLocation, media::MessageMedia,
    reply::MessageReply, text::MessageText,
};
use crate::{prelude::*, session::room::SupportedEvent, spawn, utils::media::filename_for_mime};

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
                vec![
                    glib::ParamSpecEnum::builder("format", ContentFormat::default())
                        .explicit_notify()
                        .build(),
                ]
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
        glib::Object::new(&[])
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

    pub fn update_for_event(&self, event: &SupportedEvent) {
        let format = self.format();
        if format == ContentFormat::Natural && event.is_reply() {
            spawn!(
                glib::PRIORITY_HIGH,
                clone!(@weak self as obj, @weak event => async move {
                    if let Some(related_event) = event
                        .reply_to_event()
                        .await
                        .ok()
                        .flatten()
                        .and_then(|event| event.downcast::<SupportedEvent>().ok())
                    {
                        let reply = MessageReply::new();
                        reply.set_related_content_sender(related_event.sender().upcast());
                        build_content(reply.related_content(), &related_event, ContentFormat::Compact);
                        build_content(reply.content(), &event, ContentFormat::Natural);
                        obj.set_child(Some(&reply));
                    } else {
                        build_content(&obj, &event, format);
                    }
                })
            );
        } else {
            build_content(self, event, format);
        }
    }
}

impl Default for MessageContent {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the content widget of `event` as a child of `parent`.
fn build_content(parent: &impl IsA<adw::Bin>, event: &SupportedEvent, format: ContentFormat) {
    let parent = parent.upcast_ref();
    match event.content() {
        Some(AnyMessageLikeEventContent::RoomMessage(message)) => {
            let msgtype = if let Some(Relation::Replacement(replacement)) = message.relates_to {
                replacement.new_content.msgtype
            } else {
                message.msgtype
            };
            match msgtype {
                MessageType::Audio(message) => {
                    let child = if let Some(Ok(child)) =
                        parent.child().map(|w| w.downcast::<MessageAudio>())
                    {
                        child
                    } else {
                        let child = MessageAudio::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.audio(message, &event.room().session(), format);
                }
                MessageType::Emote(message) => {
                    let child = if let Some(Ok(child)) =
                        parent.child().map(|w| w.downcast::<MessageText>())
                    {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.emote(
                        message.formatted,
                        message.body,
                        event.sender(),
                        &event.room(),
                        format,
                    );
                }
                MessageType::File(message) => {
                    let info = message.info.as_ref();
                    let filename = message
                        .filename
                        .filter(|name| !name.is_empty())
                        .or(Some(message.body))
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| {
                            filename_for_mime(info.and_then(|info| info.mimetype.as_deref()), None)
                        });

                    let child = if let Some(Ok(child)) =
                        parent.child().map(|w| w.downcast::<MessageFile>())
                    {
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
                    let child = if let Some(Ok(child)) =
                        parent.child().map(|w| w.downcast::<MessageMedia>())
                    {
                        child
                    } else {
                        let child = MessageMedia::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.image(message, &event.room().session(), format);
                }
                MessageType::Location(message) => {
                    let child = if let Some(Ok(child)) =
                        parent.child().map(|w| w.downcast::<MessageLocation>())
                    {
                        child
                    } else {
                        let child = MessageLocation::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.set_geo_uri(&message.geo_uri, format);
                }
                MessageType::Notice(message) => {
                    let child = if let Some(Ok(child)) =
                        parent.child().map(|w| w.downcast::<MessageText>())
                    {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.markup(message.formatted, message.body, &event.room(), format);
                }
                MessageType::ServerNotice(message) => {
                    let child = if let Some(Ok(child)) =
                        parent.child().map(|w| w.downcast::<MessageText>())
                    {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.text(message.body, format);
                }
                MessageType::Text(message) => {
                    let child = if let Some(Ok(child)) =
                        parent.child().map(|w| w.downcast::<MessageText>())
                    {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.markup(message.formatted, message.body, &event.room(), format);
                }
                MessageType::Video(message) => {
                    let child = if let Some(Ok(child)) =
                        parent.child().map(|w| w.downcast::<MessageMedia>())
                    {
                        child
                    } else {
                        let child = MessageMedia::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.video(message, &event.room().session(), format);
                }
                MessageType::VerificationRequest(_) => {
                    // TODO: show more information about the verification
                    let child = if let Some(Ok(child)) =
                        parent.child().map(|w| w.downcast::<MessageText>())
                    {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.text(gettext("Identity verification was started"), format);
                }
                _ => {
                    warn!("Event not supported: {:?}", msgtype);
                    let child = if let Some(Ok(child)) =
                        parent.child().map(|w| w.downcast::<MessageText>())
                    {
                        child
                    } else {
                        let child = MessageText::new();
                        parent.set_child(Some(&child));
                        child
                    };
                    child.text(gettext("Unsupported event"), format);
                }
            }
        }
        Some(AnyMessageLikeEventContent::Sticker(content)) => {
            let child =
                if let Some(Ok(child)) = parent.child().map(|w| w.downcast::<MessageMedia>()) {
                    child
                } else {
                    let child = MessageMedia::new();
                    parent.set_child(Some(&child));
                    child
                };
            child.sticker(content, &event.room().session(), format);
        }
        Some(AnyMessageLikeEventContent::RoomEncrypted(_)) => {
            let child = if let Some(Ok(child)) = parent.child().map(|w| w.downcast::<MessageText>())
            {
                child
            } else {
                let child = MessageText::new();
                parent.set_child(Some(&child));
                child
            };
            child.text(gettext("Unable to decrypt this message, decryption will be retried once the keys are available."), format);
        }
        Some(AnyMessageLikeEventContent::RoomRedaction(_)) => {
            let child = if let Some(Ok(child)) = parent.child().map(|w| w.downcast::<MessageText>())
            {
                child
            } else {
                let child = MessageText::new();
                parent.set_child(Some(&child));
                child
            };
            child.text(gettext("This message was removed."), format);
        }
        _ => {
            warn!("Unsupported event: {:?}", event.content());
            let child = if let Some(Ok(child)) = parent.child().map(|w| w.downcast::<MessageText>())
            {
                child
            } else {
                let child = MessageText::new();
                parent.set_child(Some(&child));
                child
            };
            child.text(gettext("Unsupported event"), format);
        }
    }
}

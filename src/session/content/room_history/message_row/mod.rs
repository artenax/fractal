mod audio;
mod file;
mod media;
mod reaction;
mod reaction_list;
mod reply;
mod text;

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    glib,
    glib::{clone, signal::SignalHandlerId},
    subclass::prelude::*,
    CompositeTemplate,
};
use log::warn;
use matrix_sdk::ruma::events::{
    room::message::{MessageType, Relation},
    AnyMessageLikeEventContent,
};

use self::{
    audio::MessageAudio, file::MessageFile, media::MessageMedia,
    reaction_list::MessageReactionList, reply::MessageReply, text::MessageText,
};
use crate::{
    components::Avatar, prelude::*, session::room::Event, spawn, utils::filename_for_mime,
};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/content-message-row.ui")]
    pub struct MessageRow {
        #[template_child]
        pub avatar: TemplateChild<Avatar>,
        #[template_child]
        pub header: TemplateChild<gtk::Box>,
        #[template_child]
        pub display_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub timestamp: TemplateChild<gtk::Label>,
        #[template_child]
        pub content: TemplateChild<adw::Bin>,
        #[template_child]
        pub reactions: TemplateChild<MessageReactionList>,
        pub source_changed_handler: RefCell<Option<SignalHandlerId>>,
        pub bindings: RefCell<Vec<glib::Binding>>,
        pub event: RefCell<Option<Event>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MessageRow {
        const NAME: &'static str = "ContentMessageRow";
        type Type = super::MessageRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Avatar::static_type();
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MessageRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::new(
                        "show-header",
                        "Show Header",
                        "Whether this item should show a header. This does nothing if this event doesn’t have a header. ",
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
                "show-header" => {
                    let show_header = value.get().unwrap();
                    let _ = obj.set_show_header(show_header);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "show-header" => obj.show_header().to_value(),
                _ => unimplemented!(),
            }
        }
    }
    impl WidgetImpl for MessageRow {}
    impl BinImpl for MessageRow {}
}

glib::wrapper! {
    pub struct MessageRow(ObjectSubclass<imp::MessageRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

// TODO
// - [] Implement widgets to show message events
impl MessageRow {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create MessageRow")
    }

    pub fn show_header(&self) -> bool {
        let priv_ = self.imp();
        priv_.avatar.is_visible() && priv_.header.is_visible()
    }

    pub fn set_show_header(&self, visible: bool) {
        let priv_ = self.imp();
        priv_.avatar.set_visible(visible);
        priv_.header.set_visible(visible);

        if let Some(list_item) = self.parent().and_then(|w| w.parent()) {
            if visible {
                list_item.set_css_classes(&["has-header"]);
            } else {
                list_item.remove_css_class("has-header");
            }
        }

        self.notify("show-header");
    }

    pub fn set_event(&self, event: Event) {
        let priv_ = self.imp();
        // Remove signals and bindings from the previous event
        if let Some(event) = priv_.event.take() {
            if let Some(source_changed_handler) = priv_.source_changed_handler.take() {
                event.disconnect(source_changed_handler);
            }

            while let Some(binding) = priv_.bindings.borrow_mut().pop() {
                binding.unbind();
            }
        }

        priv_.avatar.set_item(Some(event.sender().avatar().clone()));

        let display_name_binding = event
            .sender()
            .bind_property("display-name", &priv_.display_name.get(), "label")
            .flags(glib::BindingFlags::SYNC_CREATE)
            .build();

        let show_header_binding = event
            .bind_property("show-header", self, "show-header")
            .flags(glib::BindingFlags::SYNC_CREATE)
            .build();

        let timestamp_binding = event
            .bind_property("time", &*priv_.timestamp, "label")
            .flags(glib::BindingFlags::SYNC_CREATE)
            .build();

        priv_.bindings.borrow_mut().append(&mut vec![
            display_name_binding,
            show_header_binding,
            timestamp_binding,
        ]);

        priv_
            .source_changed_handler
            .replace(Some(event.connect_notify_local(
                Some("source"),
                clone!(@weak self as obj => move |event, _| {
                    obj.update_content(event);
                }),
            )));
        self.update_content(&event);

        priv_.reactions.set_reaction_list(event.reactions());
        priv_.event.replace(Some(event));
    }

    fn update_content(&self, event: &Event) {
        if event.is_reply() {
            spawn!(
                glib::PRIORITY_HIGH,
                clone!(@weak self as obj, @weak event => async move {
                    let priv_ = obj.imp();

                    if let Ok(Some(related_event)) = event.reply_to_event().await {
                        let reply = MessageReply::new();
                        reply.set_related_content_sender(related_event.sender().upcast());
                        build_content(reply.related_content(), &related_event, true);
                        build_content(reply.content(), &event, false);
                        priv_.content.set_child(Some(&reply));
                    } else {
                        build_content(&*priv_.content, &event, false);
                    }
                })
            );
        } else {
            build_content(&*self.imp().content, event, false);
        }
    }
}

impl Default for MessageRow {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the content widget of `event` as a child of `parent`.
///
/// If `compact` is true, the content should appear in a smaller format without
/// interactions, if possible.
fn build_content(parent: &adw::Bin, event: &Event, compact: bool) {
    // TODO: create widgets for all event types
    // TODO: display reaction events from event.relates_to()
    // TODO: we should reuse the already present child widgets when possible
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
                    child.audio(message, &event.room().session(), compact);
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
                    child.emote(message.formatted, message.body, event.sender());
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
                    child.set_compact(compact);
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
                    child.image(message, &event.room().session(), compact);
                }
                MessageType::Location(_message) => {}
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
                    child.markup(message.formatted, message.body);
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
                    child.text(message.body);
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
                    child.markup(message.formatted, message.body);
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
                    child.video(message, &event.room().session(), compact);
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
                    child.text(gettext("Identity verification was started"));
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
                    child.text(gettext("Unsupported event"));
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
            child.sticker(content, &event.room().session(), compact);
        }
        Some(AnyMessageLikeEventContent::RoomEncrypted(content)) => {
            warn!("Couldn't decrypt event {:?}", content);
            let child = if let Some(Ok(child)) = parent.child().map(|w| w.downcast::<MessageText>())
            {
                child
            } else {
                let child = MessageText::new();
                parent.set_child(Some(&child));
                child
            };
            child.text(gettext("Fractal couldn't decrypt this message."));
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
            child.text(gettext("This message was removed."));
        }
        _ => {
            let child = if let Some(Ok(child)) = parent.child().map(|w| w.downcast::<MessageText>())
            {
                child
            } else {
                let child = MessageText::new();
                parent.set_child(Some(&child));
                child
            };
            child.text(gettext("Unsupported event"));
        }
    }
}

mod audio;
pub mod content;
mod file;
mod location;
mod media;
mod reaction;
mod reaction_list;
mod reply;
mod text;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{
    gdk, glib,
    glib::{clone, signal::SignalHandlerId},
    CompositeTemplate,
};
use matrix_sdk::ruma::events::room::message::MessageType;
use tracing::warn;

pub use self::content::ContentFormat;
use self::{content::MessageContent, media::MessageMedia, reaction_list::MessageReactionList};
use super::ReadReceiptsList;
use crate::{components::Avatar, prelude::*, session::model::Event, Window};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_history/message_row/mod.ui"
    )]
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
        pub content: TemplateChild<MessageContent>,
        #[template_child]
        pub reactions: TemplateChild<MessageReactionList>,
        #[template_child]
        pub read_receipts: TemplateChild<ReadReceiptsList>,
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
            Self::bind_template(klass);

            klass.install_action("message-row.show-media", None, move |obj, _, _| {
                obj.show_media();
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MessageRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::builder("show-header")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<Event>("event")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "show-header" => obj.set_show_header(value.get().unwrap()),
                "event" => obj.set_event(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "show-header" => obj.show_header().to_value(),
                "event" => obj.event().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.content.connect_notify_local(
                Some("format"),
                clone!(@weak self as imp => move |content, _|
                    imp.reactions.set_visible(!matches!(
                        content.format(),
                        ContentFormat::Compact | ContentFormat::Ellipsized
                    ));
                ),
            );
        }
    }

    impl WidgetImpl for MessageRow {}
    impl BinImpl for MessageRow {}
}

glib::wrapper! {
    pub struct MessageRow(ObjectSubclass<imp::MessageRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl MessageRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Whether this item should show its header.
    ///
    /// This is ignored if this event doesn’t have a header.
    pub fn show_header(&self) -> bool {
        let imp = self.imp();
        imp.avatar.is_visible() && imp.header.is_visible()
    }

    /// Set whether this item should show its header.
    pub fn set_show_header(&self, visible: bool) {
        let imp = self.imp();
        imp.avatar.set_visible(visible);
        imp.header.set_visible(visible);

        if let Some(row) = self.parent() {
            if visible {
                row.add_css_class("has-header");
            } else {
                row.remove_css_class("has-header");
            }
        }

        self.notify("show-header");
    }

    pub fn set_content_format(&self, format: ContentFormat) {
        self.imp().content.set_format(format);
    }

    pub fn event(&self) -> Option<Event> {
        self.imp().event.borrow().clone()
    }

    pub fn set_event(&self, event: Event) {
        let imp = self.imp();
        // Remove signals and bindings from the previous event
        if let Some(event) = imp.event.take() {
            if let Some(source_changed_handler) = imp.source_changed_handler.take() {
                event.disconnect(source_changed_handler);
            }

            while let Some(binding) = imp.bindings.borrow_mut().pop() {
                binding.unbind();
            }
        }

        imp.avatar
            .set_data(Some(event.sender().avatar_data().clone()));

        let display_name_binding = event
            .sender()
            .bind_property("display-name", &imp.display_name.get(), "label")
            .sync_create()
            .build();

        let show_header_binding = event
            .bind_property("show-header", self, "show-header")
            .sync_create()
            .build();

        let timestamp_binding = event
            .bind_property("time", &*imp.timestamp, "label")
            .sync_create()
            .build();

        imp.bindings.borrow_mut().append(&mut vec![
            display_name_binding,
            show_header_binding,
            timestamp_binding,
        ]);

        imp.source_changed_handler
            .replace(Some(event.connect_notify_local(
                Some("source"),
                clone!(@weak self as obj => move |event, _| {
                    obj.update_content(event);
                }),
            )));
        self.update_content(&event);

        imp.reactions.set_reaction_list(event.reactions());
        imp.read_receipts
            .set_list(&event.room(), event.read_receipts());
        imp.event.replace(Some(event));
        self.notify("event");
    }

    fn update_content(&self, event: &Event) {
        self.imp().content.update_for_event(event);
    }

    /// Get the texture displayed by this widget, if any.
    pub fn texture(&self) -> Option<gdk::Texture> {
        self.imp().content.texture()
    }

    /// Open the media viewer with the media content of this row.
    fn show_media(&self) {
        let imp = self.imp();
        let Some(window) = self.root().and_downcast::<Window>() else {
            return;
        };
        let borrowed_event = imp.event.borrow();
        let Some(event) = borrowed_event.as_ref() else {
            return;
        };
        let Some(message) = event.message() else {
            return;
        };

        if matches!(message, MessageType::Image(_) | MessageType::Video(_)) {
            let Some(media_widget) = imp.content.content_widget().and_downcast::<MessageMedia>()
            else {
                warn!("Trying to show media of a non-media message");
                return;
            };

            window.session_view().show_media(event, &media_widget);
        }
    }
}

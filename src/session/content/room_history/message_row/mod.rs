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
    glib,
    glib::{clone, signal::SignalHandlerId},
    CompositeTemplate,
};

pub use self::content::ContentFormat;
use self::{content::MessageContent, reaction_list::MessageReactionList};
use crate::{components::Avatar, prelude::*, session::room::SupportedEvent};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-message-row.ui")]
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
        pub source_changed_handler: RefCell<Option<SignalHandlerId>>,
        pub bindings: RefCell<Vec<glib::Binding>>,
        pub event: RefCell<Option<SupportedEvent>>,
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
                "show-header" => obj.set_show_header(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "show-header" => obj.show_header().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.content.connect_notify_local(
                Some("format"),
                clone!(@weak obj => move |content, _|
                    obj.imp().reactions.set_visible(!matches!(
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

    pub fn set_content_format(&self, format: ContentFormat) {
        self.imp().content.set_format(format);
    }

    pub fn set_event(&self, event: SupportedEvent) {
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

    fn update_content(&self, event: &SupportedEvent) {
        self.imp().content.update_for_event(event);
    }
}

impl Default for MessageRow {
    fn default() -> Self {
        Self::new()
    }
}

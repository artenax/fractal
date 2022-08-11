use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{glib, CompositeTemplate};
use matrix_sdk::ruma::events::{room::message::MessageType, AnyMessageLikeEventContent};

use crate::session::content::room_details::history_viewer::HistoryViewerEvent;

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-file-history-viewer-row.ui")]
    pub struct FileRow {
        pub event: RefCell<Option<HistoryViewerEvent>>,
        #[template_child]
        pub title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub size_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileRow {
        const NAME: &'static str = "ContentFileHistoryViewerRow";
        type Type = super::FileRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for FileRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<HistoryViewerEvent>("event")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "event" => self.obj().set_event(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "event" => self.obj().event().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for FileRow {}
    impl BinImpl for FileRow {}
}

glib::wrapper! {
    pub struct FileRow(ObjectSubclass<imp::FileRow>)
        @extends gtk::Widget, adw::Bin;
}

impl FileRow {
    pub fn set_event(&self, event: Option<HistoryViewerEvent>) {
        let imp = self.imp();

        if self.event() == event {
            return;
        }

        if let Some(ref event) = event {
            if let Some(AnyMessageLikeEventContent::RoomMessage(content)) = event.original_content()
            {
                if let MessageType::File(file) = content.msgtype {
                    imp.title_label.set_label(&file.body);

                    if let Some(size) = file.info.and_then(|i| i.size) {
                        let size = glib::format_size(size.into());
                        imp.size_label.set_label(&size);
                    } else {
                        imp.size_label.set_label(&gettext("Unknown size"));
                    }
                }
            }
        }

        imp.event.replace(event);
        self.notify("event");
    }

    pub fn event(&self) -> Option<HistoryViewerEvent> {
        self.imp().event.borrow().clone()
    }
}

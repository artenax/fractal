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
    #[template(resource = "/org/gnome/Fractal/content-audio-history-viewer-row.ui")]
    pub struct AudioRow {
        pub event: RefCell<Option<HistoryViewerEvent>>,
        #[template_child]
        pub title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub duration_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AudioRow {
        const NAME: &'static str = "ContentAudioHistoryViewerRow";
        type Type = super::AudioRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AudioRow {
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

    impl WidgetImpl for AudioRow {}
    impl BinImpl for AudioRow {}
}

glib::wrapper! {
    pub struct AudioRow(ObjectSubclass<imp::AudioRow>)
        @extends gtk::Widget, adw::Bin;
}

impl AudioRow {
    pub fn set_event(&self, event: Option<HistoryViewerEvent>) {
        let imp = self.imp();

        if self.event() == event {
            return;
        }

        if let Some(ref event) = event {
            if let Some(AnyMessageLikeEventContent::RoomMessage(content)) = event.original_content()
            {
                if let MessageType::Audio(audio) = content.msgtype {
                    imp.title_label.set_label(&audio.body);

                    if let Some(duration) = audio.info.and_then(|i| i.duration) {
                        let duration_secs = duration.as_secs();
                        let secs = duration_secs % 60;
                        let mins = (duration_secs % (60 * 60)) / 60;
                        let hours = duration_secs / (60 * 60);

                        let duration = if hours > 0 {
                            format!("{hours:02}:{mins:02}:{secs:02}")
                        } else {
                            format!("{mins:02}:{secs:02}")
                        };

                        imp.duration_label.set_label(&duration);
                    } else {
                        imp.duration_label.set_label(&gettext("Unknown duration"));
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

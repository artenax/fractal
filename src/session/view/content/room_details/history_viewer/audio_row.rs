use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use glib::clone;
use gtk::{gio, glib, CompositeTemplate};
use matrix_sdk::ruma::events::{
    room::message::{AudioMessageEventContent, MessageType},
    AnyMessageLikeEventContent,
};
use tracing::warn;

use super::HistoryViewerEvent;
use crate::{session::model::Session, spawn, spawn_tokio};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_details/history_viewer/audio_row.ui"
    )]
    pub struct AudioRow {
        pub event: RefCell<Option<HistoryViewerEvent>>,
        pub media_file: RefCell<Option<gtk::MediaFile>>,
        #[template_child]
        pub play_button: TemplateChild<gtk::Button>,
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

            klass.install_action("audio-row.toggle-play", None, move |widget, _, _| {
                widget.toggle_play();
            });
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

                    if let Some(duration) = audio.info.as_ref().and_then(|i| i.duration) {
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

                    let session = event.room().unwrap().session();
                    spawn!(clone!(@weak self as obj => async move {
                        obj.download_audio(audio, &session).await;
                    }));
                }
            }
        }

        imp.event.replace(event);
        self.notify("event");
    }

    pub fn event(&self) -> Option<HistoryViewerEvent> {
        self.imp().event.borrow().clone()
    }

    async fn download_audio(&self, audio: AudioMessageEventContent, session: &Session) {
        let client = session.client();
        let handle = spawn_tokio!(async move { client.media().get_file(audio, true).await });

        match handle.await.unwrap() {
            Ok(Some(data)) => {
                // The GStreamer backend doesn't work with input streams so
                // we need to store the file.
                // See: https://gitlab.gnome.org/GNOME/gtk/-/issues/4062
                let (file, _) = gio::File::new_tmp(Option::<String>::None).unwrap();
                file.replace_contents(
                    &data,
                    None,
                    false,
                    gio::FileCreateFlags::REPLACE_DESTINATION,
                    gio::Cancellable::NONE,
                )
                .unwrap();
                self.prepare_audio(file);
            }
            Ok(None) => {
                warn!("Could not retrieve invalid audio file");
            }
            Err(error) => {
                warn!("Could not retrieve audio file: {error}");
            }
        }
    }

    fn prepare_audio(&self, file: gio::File) {
        let media_file = gtk::MediaFile::for_file(&file);

        media_file.connect_error_notify(clone!(@weak self as obj => move |media_file| {
            if let Some(error) = media_file.error() {
                warn!("Error reading audio file: {}", error);
            }
        }));
        media_file.connect_ended_notify(clone!(@weak self as obj => move |media_file| {
            if media_file.is_ended() {
                obj.imp().play_button.set_icon_name("media-playback-start-symbolic");
            }
        }));

        self.imp().media_file.replace(Some(media_file));
    }

    fn toggle_play(&self) {
        let imp = self.imp();

        if let Some(media_file) = self.imp().media_file.borrow().as_ref() {
            if media_file.is_playing() {
                media_file.pause();
                imp.play_button
                    .set_icon_name("media-playback-start-symbolic");
            } else {
                media_file.play();
                imp.play_button
                    .set_icon_name("media-playback-pause-symbolic");
            }
        }
    }
}

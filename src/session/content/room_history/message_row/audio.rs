use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    CompositeTemplate,
};
use log::warn;
use matrix_sdk::{media::MediaEventContent, ruma::events::room::message::AudioMessageEventContent};

use super::{media::MediaState, ContentFormat};
use crate::{
    components::AudioPlayer, session::Session, spawn, spawn_tokio, utils::media::media_type_uid,
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-message-audio.ui")]
    pub struct MessageAudio {
        /// The body of the audio message.
        pub body: RefCell<Option<String>>,
        /// The state of the audio file.
        pub state: Cell<MediaState>,
        /// Whether to display this audio message in a compact format.
        pub compact: Cell<bool>,
        #[template_child]
        pub player: TemplateChild<AudioPlayer>,
        #[template_child]
        pub state_spinner: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub state_error: TemplateChild<gtk::Image>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MessageAudio {
        const NAME: &'static str = "ContentMessageAudio";
        type Type = super::MessageAudio;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MessageAudio {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::new(
                        "body",
                        "Body",
                        "The body of the audio message",
                        None,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecEnum::new(
                        "state",
                        "State",
                        "The state of the audio file",
                        MediaState::static_type(),
                        MediaState::default() as i32,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecBoolean::new(
                        "compact",
                        "Compact",
                        "Whether to display this audio message in a compact format",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "state" => self.obj().set_state(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "body" => obj.body().to_value(),
                "state" => obj.state().to_value(),
                "compact" => obj.compact().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for MessageAudio {}

    impl BinImpl for MessageAudio {}
}

glib::wrapper! {
    /// A widget displaying an audio message in the timeline.
    pub struct MessageAudio(ObjectSubclass<imp::MessageAudio>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl MessageAudio {
    /// Create a new audio message.
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// The body of the audio message.
    pub fn body(&self) -> Option<String> {
        self.imp().body.borrow().to_owned()
    }

    /// Set the body of the audio message.
    fn set_body(&self, body: Option<String>) {
        if self.body() == body {
            return;
        }

        self.imp().body.replace(body);
        self.notify("body");
    }

    /// Whether to display this audio message in a compact format.
    pub fn compact(&self) -> bool {
        self.imp().compact.get()
    }

    /// Set the compact format of this audio message.
    fn set_compact(&self, compact: bool) {
        self.imp().compact.set(compact);

        if compact {
            self.remove_css_class("osd");
            self.remove_css_class("toolbar");
        } else {
            self.add_css_class("osd");
            self.add_css_class("toolbar");
        }

        self.notify("compact");
    }

    /// The state of the audio file.
    pub fn state(&self) -> MediaState {
        self.imp().state.get()
    }

    /// Set the state of the audio file.
    fn set_state(&self, state: MediaState) {
        let priv_ = self.imp();

        if self.state() == state {
            return;
        }

        match state {
            MediaState::Loading | MediaState::Initial => {
                priv_.state_spinner.set_visible(true);
                priv_.state_error.set_visible(false);
            }
            MediaState::Ready => {
                priv_.state_spinner.set_visible(false);
                priv_.state_error.set_visible(false);
            }
            MediaState::Error => {
                priv_.state_spinner.set_visible(false);
                priv_.state_error.set_visible(true);
            }
        }

        priv_.state.set(state);
        self.notify("state");
    }

    /// Convenience method to set the state to `Error` with the given error
    /// message.
    fn set_error(&self, error: String) {
        self.set_state(MediaState::Error);
        self.imp().state_error.set_tooltip_text(Some(&error));
    }

    /// Display the given `audio` message.
    pub fn audio(&self, audio: AudioMessageEventContent, session: &Session, format: ContentFormat) {
        self.set_body(Some(audio.body.clone()));

        let compact = matches!(format, ContentFormat::Compact | ContentFormat::Ellipsized);
        self.set_compact(compact);
        if compact {
            self.set_state(MediaState::Ready);
            return;
        }

        self.set_state(MediaState::Loading);

        let mut path = glib::tmp_dir();
        path.push(media_type_uid(audio.source()));
        let file = gio::File::for_path(path);

        if file.query_exists(gio::Cancellable::NONE) {
            self.display_file(file);
            return;
        }

        let client = session.client();
        let handle = spawn_tokio!(async move { client.media().get_file(audio, true).await });

        spawn!(
            glib::PRIORITY_LOW,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(Some(data)) => {
                        // The GStreamer backend doesn't work with input streams so
                        // we need to store the file.
                        // See: https://gitlab.gnome.org/GNOME/gtk/-/issues/4062
                        file.replace_contents(
                            &data,
                            None,
                            false,
                            gio::FileCreateFlags::REPLACE_DESTINATION,
                            gio::Cancellable::NONE,
                        )
                        .unwrap();
                        obj.display_file(file);
                    }
                    Ok(None) => {
                        warn!("Could not retrieve invalid audio file");
                        obj.set_error(gettext("Could not retrieve audio file"));
                    }
                    Err(error) => {
                        warn!("Could not retrieve audio file: {}", error);
                        obj.set_error(gettext("Could not retrieve audio file"));
                    }
                }
            })
        );
    }

    fn display_file(&self, file: gio::File) {
        let media_file = gtk::MediaFile::for_file(&file);

        media_file.connect_error_notify(clone!(@weak self as obj => move |media_file| {
            if let Some(error) = media_file.error() {
                warn!("Error reading audio file: {}", error);
                obj.set_error(gettext("Error reading audio file"));
            }
        }));

        self.imp().player.set_media_file(Some(media_file));
        self.set_state(MediaState::Ready);
    }
}

impl Default for MessageAudio {
    fn default() -> Self {
        Self::new()
    }
}

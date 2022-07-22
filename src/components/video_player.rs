use adw::subclass::prelude::*;
use gst::ClockTime;
use gst_player::{Player, PlayerGMainContextSignalDispatcher};
use gtk::{gio, glib, prelude::*, CompositeTemplate};

use super::VideoPlayerRenderer;

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-video-player.ui")]
    pub struct VideoPlayer {
        /// Whether this player should be displayed in a compact format.
        pub compact: Cell<bool>,
        pub duration_handler: RefCell<Option<glib::SignalHandlerId>>,
        #[template_child]
        pub video: TemplateChild<gtk::Picture>,
        #[template_child]
        pub timestamp: TemplateChild<gtk::Label>,
        #[template_child]
        pub player: TemplateChild<Player>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VideoPlayer {
        const NAME: &'static str = "ComponentsVideoPlayer";
        type Type = super::VideoPlayer;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            VideoPlayerRenderer::static_type();
            Player::static_type();
            PlayerGMainContextSignalDispatcher::static_type();
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VideoPlayer {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::new(
                        "compact",
                        "Compact",
                        "Whether this player should be displayed in a compact format",
                        false,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecObject::new(
                        "player",
                        "Player",
                        "The GStreamerPlayer for the video",
                        Player::static_type(),
                        glib::ParamFlags::READABLE,
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
                "compact" => obj.set_compact(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "compact" => obj.compact().to_value(),
                "player" => obj.player().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for VideoPlayer {}

    impl BinImpl for VideoPlayer {}
}

glib::wrapper! {
    /// A widget to preview a video media file without controls or sound.
    pub struct VideoPlayer(ObjectSubclass<imp::VideoPlayer>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl VideoPlayer {
    /// Create a new video player.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create VideoPlayer")
    }

    pub fn player(&self) -> &Player {
        &self.imp().player
    }

    pub fn compact(&self) -> bool {
        self.imp().compact.get()
    }

    pub fn set_compact(&self, compact: bool) {
        if self.compact() == compact {
            return;
        }

        self.imp().compact.set(compact);
        self.notify("compact");
    }

    /// Set the file to display.
    pub fn play_media_file(&self, file: &gio::File) {
        self.duration_changed(None);
        let player = self.player();
        player.set_uri(Some(file.uri().as_ref()));
        player.set_audio_track_enabled(false);
        player.play();
    }

    #[template_callback]
    fn duration_changed(&self, duration: Option<ClockTime>) {
        let label = if let Some(duration) = duration {
            let mut time = duration.seconds();

            let sec = time % 60;
            time -= sec;
            let min = (time % (60 * 60)) / 60;
            time -= min * 60;
            let hour = time / (60 * 60);

            if hour > 0 {
                // FIXME: Find how to localize this.
                // hour:minutes:seconds
                format!("{}:{:02}:{:02}", hour, min, sec)
            } else {
                // FIXME: Find how to localize this.
                // minutes:seconds
                format!("{:02}:{:02}", min, sec)
            }
        } else {
            "--:--".to_owned()
        };
        self.imp().timestamp.set_label(&label);
    }
}

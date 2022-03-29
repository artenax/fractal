use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-audio-player.ui")]
    pub struct AudioPlayer {
        /// The media file to play.
        pub media_file: RefCell<Option<gtk::MediaFile>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AudioPlayer {
        const NAME: &'static str = "ComponentsAudioPlayer";
        type Type = super::AudioPlayer;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AudioPlayer {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "media-file",
                    "Media File",
                    "The media file to play",
                    gtk::MediaFile::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                )]
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
                "media-file" => {
                    obj.set_media_file(value.get().unwrap());
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "media-file" => obj.media_file().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for AudioPlayer {}

    impl BinImpl for AudioPlayer {}
}

glib::wrapper! {
    /// A widget displaying a video media file.
    pub struct AudioPlayer(ObjectSubclass<imp::AudioPlayer>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl AudioPlayer {
    /// Create a new audio player.
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create AudioPlayer")
    }

    /// The media file that is playing.
    pub fn media_file(&self) -> Option<gtk::MediaFile> {
        self.imp().media_file.borrow().clone()
    }

    /// Set the media_file to play.
    pub fn set_media_file(&self, media_file: Option<gtk::MediaFile>) {
        if self.media_file() == media_file {
            return;
        }

        self.imp().media_file.replace(media_file);
        self.notify("media-file");
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

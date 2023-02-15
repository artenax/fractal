use adw::subclass::prelude::*;
use gtk::{gio, glib, prelude::*, CompositeTemplate};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-audio-player.ui")]
    pub struct AudioPlayer {
        /// The media file to play.
        pub media_file: RefCell<Option<gtk::MediaFile>>,
        /// Whether to play the media automatically.
        pub autoplay: Cell<bool>,
        pub autoplay_handler: RefCell<Option<glib::SignalHandlerId>>,
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
                vec![
                    glib::ParamSpecObject::builder::<gtk::MediaFile>("media-file")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("autoplay")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "media-file" => {
                    obj.set_media_file(value.get().unwrap());
                }
                "autoplay" => obj.set_autoplay(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "media-file" => obj.media_file().to_value(),
                "autoplay" => obj.autoplay().to_value(),
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
        glib::Object::new()
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

        let imp = self.imp();

        if let Some(media_file) = imp.media_file.take() {
            if let Some(handler_id) = imp.autoplay_handler.take() {
                media_file.disconnect(handler_id);
            }
        }

        if self.autoplay() {
            if let Some(media_file) = &media_file {
                imp.autoplay_handler
                    .replace(Some(media_file.connect_prepared_notify(|media_file| {
                        if media_file.is_prepared() {
                            media_file.play()
                        }
                    })));
            }
        }

        imp.media_file.replace(media_file);
        self.notify("media-file");
    }

    /// Set the file to play.
    ///
    /// This is a convenience method that calls
    /// [`AudioPlayer::set_media_file()`].
    pub fn set_file(&self, file: Option<&gio::File>) {
        self.set_media_file(file.map(gtk::MediaFile::for_file));
    }

    /// Whether to play the media automatically.
    pub fn autoplay(&self) -> bool {
        self.imp().autoplay.get()
    }

    /// Set whether to play the media automatically.
    pub fn set_autoplay(&self, autoplay: bool) {
        if self.autoplay() == autoplay {
            return;
        }

        self.imp().autoplay.set(autoplay);
        self.notify("autoplay");
    }
}

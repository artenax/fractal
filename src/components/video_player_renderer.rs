use adw::subclass::prelude::*;
use gst_gtk::PaintableSink;
use gst_player::{subclass::prelude::*, Player, PlayerVideoRenderer};
use gtk::{gdk, glib, prelude::*};

mod imp {
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct VideoPlayerRenderer {
        /// The sink to use to display the video.
        pub sink: OnceCell<PaintableSink>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VideoPlayerRenderer {
        const NAME: &'static str = "ComponentsVideoPlayerRenderer";
        type Type = super::VideoPlayerRenderer;
        type Interfaces = (PlayerVideoRenderer,);
    }

    impl ObjectImpl for VideoPlayerRenderer {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "paintable",
                    "Paintable",
                    "Paintable to render the video into",
                    gdk::Paintable::static_type(),
                    glib::ParamFlags::READABLE,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "paintable" => obj.paintable().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            obj.imp().sink.set(PaintableSink::new(None)).unwrap();
        }
    }

    impl PlayerVideoRendererImpl for VideoPlayerRenderer {
        fn create_video_sink(&self, _obj: &Self::Type, _player: &Player) -> gst::Element {
            self.sink.get().unwrap().to_owned().upcast()
        }
    }
}

glib::wrapper! {
    /// A widget displaying a video media file.
    pub struct VideoPlayerRenderer(ObjectSubclass<imp::VideoPlayerRenderer>)
        @implements PlayerVideoRenderer;
}

impl VideoPlayerRenderer {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create VideoPlayerRenderer")
    }

    pub fn paintable(&self) -> gdk::Paintable {
        self.imp().sink.get().unwrap().property("paintable")
    }
}

impl Default for VideoPlayerRenderer {
    fn default() -> Self {
        Self::new()
    }
}

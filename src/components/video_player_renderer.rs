use adw::subclass::prelude::*;
use gst_gtk::PaintableSink;
use gst_play::{subclass::prelude::*, Play, PlayVideoRenderer};
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
        type Interfaces = (PlayVideoRenderer,);
    }

    impl ObjectImpl for VideoPlayerRenderer {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<gdk::Paintable>("paintable")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "paintable" => self.obj().paintable().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.sink.set(PaintableSink::new(None)).unwrap();
        }
    }

    impl PlayVideoRendererImpl for VideoPlayerRenderer {
        fn create_video_sink(&self, _player: &Play) -> gst::Element {
            self.sink.get().unwrap().to_owned().upcast()
        }
    }
}

glib::wrapper! {
    /// A widget displaying a video media file.
    pub struct VideoPlayerRenderer(ObjectSubclass<imp::VideoPlayerRenderer>)
        @implements PlayVideoRenderer;
}

impl VideoPlayerRenderer {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// The GdkPaintable to render the video into.
    pub fn paintable(&self) -> gdk::Paintable {
        self.imp().sink.get().unwrap().property("paintable")
    }
}

impl Default for VideoPlayerRenderer {
    fn default() -> Self {
        Self::new()
    }
}

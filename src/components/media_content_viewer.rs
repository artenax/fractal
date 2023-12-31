use adw::{prelude::*, subclass::prelude::*};
use geo_uri::GeoUri;
use gettextrs::gettext;
use gtk::{gdk, gio, glib, glib::clone, CompositeTemplate};
use tracing::warn;

use super::{AudioPlayer, ImagePaintable, LocationViewer, Spinner};
use crate::spawn;

#[derive(Debug, Default, Clone, Copy)]
pub enum ContentType {
    Image,
    Audio,
    Video,
    #[default]
    Unknown,
}

impl ContentType {
    pub fn icon_name(&self) -> &'static str {
        match self {
            ContentType::Image => "image-x-generic-symbolic",
            ContentType::Audio => "audio-x-generic-symbolic",
            ContentType::Video => "video-x-generic-symbolic",
            ContentType::Unknown => "text-x-generic-symbolic",
        }
    }
}

impl From<&str> for ContentType {
    fn from(string: &str) -> Self {
        match string {
            "image" => Self::Image,
            "audio" => Self::Audio,
            "video" => Self::Video,
            _ => Self::Unknown,
        }
    }
}

mod imp {
    use std::cell::Cell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/components/media_content_viewer.ui")]
    pub struct MediaContentViewer {
        /// Whether to play the media content automatically.
        pub autoplay: Cell<bool>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub viewer: TemplateChild<adw::Bin>,
        #[template_child]
        pub fallback: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub spinner: TemplateChild<Spinner>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MediaContentViewer {
        const NAME: &'static str = "ComponentsMediaContentViewer";
        type Type = super::MediaContentViewer;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_css_name("media-content-viewer");
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MediaContentViewer {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecBoolean::builder("autoplay")
                    .construct_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "autoplay" => self.obj().set_autoplay(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "autoplay" => self.obj().autoplay().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for MediaContentViewer {}
    impl BinImpl for MediaContentViewer {}
}

glib::wrapper! {
    /// Widget to view any media file.
    pub struct MediaContentViewer(ObjectSubclass<imp::MediaContentViewer>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl MediaContentViewer {
    pub fn new(autoplay: bool) -> Self {
        glib::Object::builder()
            .property("autoplay", autoplay)
            .build()
    }

    pub fn stop_playback(&self) {
        if let Some(stream) = self
            .imp()
            .viewer
            .child()
            .and_downcast::<gtk::Video>()
            .and_then(|v| v.media_stream())
        {
            if stream.is_playing() {
                stream.pause();
                stream.seek(0);
            }
        }
    }

    /// Whether to play the media content automatically.
    pub fn autoplay(&self) -> bool {
        self.imp().autoplay.get()
    }

    /// Set whether to play the media content automatically.
    fn set_autoplay(&self, autoplay: bool) {
        if self.autoplay() == autoplay {
            return;
        }

        self.imp().autoplay.set(autoplay);
        self.notify("autoplay");
    }

    /// Show the loading screen.
    pub fn show_loading(&self) {
        self.imp().stack.set_visible_child_name("loading");
    }

    /// Show the viewer.
    fn show_viewer(&self) {
        self.imp().stack.set_visible_child_name("viewer");
    }

    /// Show the fallback message for the given content type.
    pub fn show_fallback(&self, content_type: ContentType) {
        let imp = self.imp();
        let fallback = &imp.fallback;

        let title = match content_type {
            ContentType::Image => gettext("Image not Viewable"),
            ContentType::Audio => gettext("Audio Clip not Playable"),
            ContentType::Video => gettext("Video not Playable"),
            ContentType::Unknown => gettext("File not Viewable"),
        };
        fallback.set_title(&title);
        fallback.set_icon_name(Some(content_type.icon_name()));

        imp.stack.set_visible_child_name("fallback");
    }

    /// View the given image as bytes.
    ///
    /// If you have an image file, you can also use
    /// [`MediaContentViewer::view_file()`].
    pub fn view_image(&self, image: &impl IsA<gdk::Paintable>) {
        self.show_loading();

        let imp = self.imp();

        let picture = if let Some(picture) = imp.viewer.child().and_downcast::<gtk::Picture>() {
            picture
        } else {
            let picture = gtk::Picture::new();
            imp.viewer.set_child(Some(&picture));
            picture
        };

        picture.set_paintable(Some(image));
        self.show_viewer();
    }

    /// View the given file.
    pub fn view_file(&self, file: gio::File) {
        self.show_loading();

        spawn!(clone!(@weak self as obj => async move {
            obj.view_file_inner(file).await;
        }));
    }

    async fn view_file_inner(&self, file: gio::File) {
        let imp = self.imp();

        let file_info = file
            .query_info_future(
                gio::FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE,
                gio::FileQueryInfoFlags::NONE,
                glib::Priority::DEFAULT,
            )
            .await
            .ok();

        let content_type: ContentType = file_info
            .as_ref()
            .and_then(|info| info.content_type())
            .and_then(|content_type| gio::content_type_get_mime_type(&content_type))
            .and_then(|mime| mime.split('/').next().map(Into::into))
            .unwrap_or_default();

        match content_type {
            ContentType::Image => match ImagePaintable::from_file(&file) {
                Ok(texture) => {
                    self.view_image(&texture);
                    return;
                }
                Err(error) => {
                    warn!("Could not load GdkTexture from file: {error}");
                }
            },
            ContentType::Audio => {
                let audio = if let Some(audio) = imp.viewer.child().and_downcast::<AudioPlayer>() {
                    audio
                } else {
                    let audio = AudioPlayer::new();
                    audio.add_css_class("toolbar");
                    audio.add_css_class("osd");
                    audio.set_autoplay(self.autoplay());
                    imp.viewer.set_child(Some(&audio));
                    audio
                };

                audio.set_file(Some(&file));
                self.show_viewer();
                return;
            }
            ContentType::Video => {
                let video = if let Some(video) = imp.viewer.child().and_downcast::<gtk::Video>() {
                    video
                } else {
                    let video = gtk::Video::new();
                    video.set_autoplay(self.autoplay());
                    imp.viewer.set_child(Some(&video));
                    video
                };

                video.set_file(Some(&file));
                self.show_viewer();
                return;
            }
            _ => {}
        }

        self.show_fallback(content_type);
    }

    /// View the given location as a geo URI.
    pub fn view_location(&self, geo_uri: &GeoUri) {
        self.show_loading();

        let imp = self.imp();

        let location = if let Some(location) = imp.viewer.child().and_downcast::<LocationViewer>() {
            location
        } else {
            let location = LocationViewer::new();
            imp.viewer.set_child(Some(&location));
            location
        };

        location.set_location(geo_uri);
        self.show_viewer();
    }

    /// Get the texture displayed by this widget, if any.
    pub fn texture(&self) -> Option<gdk::Texture> {
        self.imp()
            .viewer
            .child()
            .and_downcast::<gtk::Picture>()
            .and_then(|p| p.paintable())
            .and_downcast::<ImagePaintable>()
            .and_then(|p| p.current_frame())
    }
}

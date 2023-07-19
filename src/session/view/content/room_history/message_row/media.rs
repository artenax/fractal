use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gdk, gio,
    glib::{self, clone},
    CompositeTemplate,
};
use log::warn;
use matrix_sdk::{
    media::{MediaEventContent, MediaThumbnailSize},
    ruma::{
        api::client::media::get_content_thumbnail::v3::Method,
        events::{
            room::message::{ImageMessageEventContent, VideoMessageEventContent},
            sticker::StickerEventContent,
        },
    },
};

use super::ContentFormat;
use crate::{
    components::{ImagePaintable, Spinner, VideoPlayer},
    session::model::Session,
    spawn, spawn_tokio,
    utils::uint_to_i32,
};

const MAX_THUMBNAIL_WIDTH: i32 = 600;
const MAX_THUMBNAIL_HEIGHT: i32 = 400;
const FALLBACK_WIDTH: i32 = 480;
const FALLBACK_HEIGHT: i32 = 360;
const MAX_COMPACT_THUMBNAIL_WIDTH: i32 = 75;
const MAX_COMPACT_THUMBNAIL_HEIGHT: i32 = 50;

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "MediaType")]
pub enum MediaType {
    Image = 0,
    Sticker = 1,
    Video = 2,
}

#[derive(Debug, Default, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "MediaState")]
pub enum MediaState {
    #[default]
    Initial = 0,
    Loading = 1,
    Ready = 2,
    Error = 3,
}

mod imp {
    use std::cell::Cell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_history/message_row/media.ui"
    )]
    pub struct MessageMedia {
        /// The intended display width of the media.
        pub width: Cell<i32>,
        /// The intended display height of the media.
        pub height: Cell<i32>,
        /// The state of the media.
        pub state: Cell<MediaState>,
        /// Whether to display this media in a compact format.
        pub compact: Cell<bool>,
        #[template_child]
        pub media: TemplateChild<gtk::Overlay>,
        #[template_child]
        pub overlay_error: TemplateChild<gtk::Image>,
        #[template_child]
        pub overlay_spinner: TemplateChild<Spinner>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MessageMedia {
        const NAME: &'static str = "ContentMessageMedia";
        type Type = super::MessageMedia;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MessageMedia {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecInt::builder("width")
                        .minimum(-1)
                        .default_value(-1)
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecInt::builder("height")
                        .minimum(-1)
                        .default_value(-1)
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecEnum::builder::<MediaState>("state")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("compact")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "width" => {
                    obj.set_width(value.get().unwrap());
                }
                "height" => {
                    obj.set_height(value.get().unwrap());
                }
                "state" => {
                    obj.set_state(value.get().unwrap());
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "width" => obj.width().to_value(),
                "height" => obj.height().to_value(),
                "state" => obj.state().to_value(),
                "compact" => obj.compact().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            self.media.unparent();
        }
    }

    impl WidgetImpl for MessageMedia {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let original_width = self.width.get();
            let original_height = self.height.get();

            let compact = self.obj().compact();
            let (max_width, max_height) = if compact {
                (MAX_COMPACT_THUMBNAIL_WIDTH, MAX_COMPACT_THUMBNAIL_HEIGHT)
            } else {
                (MAX_THUMBNAIL_WIDTH, MAX_THUMBNAIL_HEIGHT)
            };

            let (original, max, fallback, original_other, max_other) =
                if orientation == gtk::Orientation::Vertical {
                    (
                        original_height,
                        max_height,
                        FALLBACK_HEIGHT,
                        original_width,
                        max_width,
                    )
                } else {
                    (
                        original_width,
                        max_width,
                        FALLBACK_WIDTH,
                        original_height,
                        max_height,
                    )
                };

            // Limit other side to max size.
            let other = for_size.min(max_other);

            let nat = if original > 0 {
                // We don't want the paintable to be upscaled.
                let other = other.min(original_other);
                other * original / original_other
            } else if let Some(child) = self.media.child() {
                // Get the natural size of the data.
                child.measure(orientation, other).1
            } else {
                fallback
            };

            // Limit this side to max size.
            let size = nat.min(max);
            (0, size, -1, -1)
        }

        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.media.child() {
                // We need to allocate just enough width to the child so it doesn't expand.
                let original_width = self.width.get();
                let original_height = self.height.get();
                let width = if original_height > 0 && original_width > 0 {
                    height * original_width / original_height
                } else {
                    // Get the natural width of the media data.
                    child.measure(gtk::Orientation::Horizontal, height).1
                };

                self.media.allocate(width, height, baseline, None);
            } else {
                self.media.allocate(width, height, baseline, None)
            }
        }
    }
}

glib::wrapper! {
    /// A widget displaying a media message in the timeline.
    pub struct MessageMedia(ObjectSubclass<imp::MessageMedia>)
        @extends gtk::Widget, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl MessageMedia {
    /// Create a new media message.
    pub fn new() -> Self {
        glib::Object::new()
    }

    #[template_callback]
    fn handle_release(&self) {
        self.activate_action("message-row.show-media", None)
            .unwrap();
    }

    /// The intended display width of the media.
    pub fn width(&self) -> i32 {
        self.imp().width.get()
    }

    /// Set the intended display width of the media.
    pub fn set_width(&self, width: i32) {
        if self.width() == width {
            return;
        }

        self.imp().width.set(width);
        self.notify("width");
    }

    /// The intended display height of the media.
    pub fn height(&self) -> i32 {
        self.imp().height.get()
    }

    /// Set the intended display height of the media.
    pub fn set_height(&self, height: i32) {
        if self.height() == height {
            return;
        }

        self.imp().height.set(height);
        self.notify("height");
    }

    /// The state of the media.
    pub fn state(&self) -> MediaState {
        self.imp().state.get()
    }

    /// Set the state of the media.
    pub fn set_state(&self, state: MediaState) {
        let imp = self.imp();

        if self.state() == state {
            return;
        }

        match state {
            MediaState::Loading | MediaState::Initial => {
                imp.overlay_spinner.set_visible(true);
                imp.overlay_error.set_visible(false);
            }
            MediaState::Ready => {
                imp.overlay_spinner.set_visible(false);
                imp.overlay_error.set_visible(false);
            }
            MediaState::Error => {
                imp.overlay_spinner.set_visible(false);
                imp.overlay_error.set_visible(true);
            }
        }

        imp.state.set(state);
        self.notify("state");
    }

    /// Whether to display this media in a compact format.
    pub fn compact(&self) -> bool {
        self.imp().compact.get()
    }

    /// Set whether to display this media in a compact format.
    fn set_compact(&self, compact: bool) {
        self.imp().compact.set(compact);
        self.notify("compact");
    }

    /// Display the given `image`, in a `compact` format or not.
    pub fn image(&self, image: ImageMessageEventContent, session: &Session, format: ContentFormat) {
        let info = image.info.as_deref();
        let width = uint_to_i32(info.and_then(|info| info.width));
        let height = uint_to_i32(info.and_then(|info| info.height));
        let compact = matches!(format, ContentFormat::Compact | ContentFormat::Ellipsized);

        self.set_width(width);
        self.set_height(height);
        self.set_compact(compact);
        self.build(image, None, MediaType::Image, session);
    }

    /// Display the given `sticker`, in a `compact` format or not.
    pub fn sticker(&self, sticker: StickerEventContent, session: &Session, format: ContentFormat) {
        let info = &sticker.info;
        let width = uint_to_i32(info.width);
        let height = uint_to_i32(info.height);
        let body = Some(sticker.body.clone());
        let compact = matches!(format, ContentFormat::Compact | ContentFormat::Ellipsized);

        self.set_width(width);
        self.set_height(height);
        self.set_compact(compact);
        self.build(sticker, body, MediaType::Sticker, session);
    }

    /// Display the given `video`, in a `compact` format or not.
    pub fn video(&self, video: VideoMessageEventContent, session: &Session, format: ContentFormat) {
        let info = &video.info.as_deref();
        let width = uint_to_i32(info.and_then(|info| info.width));
        let height = uint_to_i32(info.and_then(|info| info.height));
        let body = Some(video.body.clone());
        let compact = matches!(format, ContentFormat::Compact | ContentFormat::Ellipsized);

        self.set_width(width);
        self.set_height(height);
        self.set_compact(compact);
        self.build(video, body, MediaType::Video, session);
    }

    fn build<C>(&self, content: C, body: Option<String>, media_type: MediaType, session: &Session)
    where
        C: MediaEventContent + Send + Sync + Clone + 'static,
    {
        self.set_state(MediaState::Loading);
        let scale_factor = self.scale_factor();

        let media = session.client().media();
        let handle = spawn_tokio!(async move {
            let thumbnail =
                if media_type != MediaType::Video && content.thumbnail_source().is_some() {
                    media
                        .get_thumbnail(
                            content.clone(),
                            MediaThumbnailSize {
                                method: Method::Scale,
                                width: ((MAX_THUMBNAIL_WIDTH * scale_factor) as u32).into(),
                                height: ((MAX_THUMBNAIL_HEIGHT * scale_factor) as u32).into(),
                            },
                            true,
                        )
                        .await
                        .ok()
                        .flatten()
                } else {
                    None
                };

            if let Some(data) = thumbnail {
                Ok(Some(data))
            } else {
                media.get_file(content, true).await
            }
        });

        spawn!(
            glib::PRIORITY_LOW,
            clone!(@weak self as obj => async move {
                let imp = obj.imp();

                match handle.await.unwrap() {
                    Ok(Some(data)) => {
                        match media_type {
                            MediaType::Image | MediaType::Sticker => {
                                match ImagePaintable::from_bytes(&glib::Bytes::from(&data), None)
                                    {
                                        Ok(texture) => {
                                            let child = if let Some(child) =
                                                imp.media.child().and_downcast::<gtk::Picture>()
                                            {
                                                child
                                            } else {
                                                let child = gtk::Picture::new();
                                                imp.media.set_child(Some(&child));
                                                child
                                            };
                                            child.set_paintable(Some(&texture));

                                            child.set_tooltip_text(body.as_deref());
                                            if media_type == MediaType::Sticker {
                                                if imp.media.has_css_class("content-thumbnail") {
                                                    imp.media.remove_css_class("content-thumbnail");
                                                }
                                            } else if !imp.media.has_css_class("content-thumbnail") {
                                                imp.media.add_css_class("content-thumbnail");
                                            }
                                        }
                                        Err(error) => {
                                            warn!("Image file not supported: {error}");
                                            imp.overlay_error.set_tooltip_text(Some(&gettext("Image file not supported")));
                                            obj.set_state(MediaState::Error);
                                        }
                                    }
                            }
                            MediaType::Video => {
                                // The GStreamer backend of GtkVideo doesn't work with input streams so
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

                                let child = if let Some(child) =
                                    imp.media.child().and_downcast::<VideoPlayer>()
                                {
                                    child
                                } else {
                                    let child = VideoPlayer::new();
                                    imp.media.set_child(Some(&child));
                                    child
                                };
                                child.set_compact(obj.compact());
                                child.play_media_file(file)
                            }
                        };

                        obj.set_state(MediaState::Ready);
                    }
                    Ok(None) => {
                        warn!("Could not retrieve invalid media file");
                        imp.overlay_error.set_tooltip_text(Some(&gettext("Could not retrieve media")));
                        obj.set_state(MediaState::Error);
                    }
                    Err(error) => {
                        warn!("Could not retrieve media file: {error}");
                        imp.overlay_error.set_tooltip_text(Some(&gettext("Could not retrieve media")));
                        obj.set_state(MediaState::Error);
                    }
                }
            })
        );
    }

    /// Get the texture displayed by this widget, if any.
    pub fn texture(&self) -> Option<gdk::Texture> {
        self.imp()
            .media
            .child()
            .and_downcast::<gtk::Picture>()
            .and_then(|p| p.paintable())
            .and_downcast::<ImagePaintable>()
            .and_then(|p| p.current_frame())
    }
}

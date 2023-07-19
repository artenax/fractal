use gtk::{gdk, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};
use log::warn;
use matrix_sdk::{
    media::{MediaEventContent, MediaThumbnailSize},
    ruma::{
        api::client::media::get_content_thumbnail::v3::Method,
        events::{
            room::message::{ImageMessageEventContent, MessageType, VideoMessageEventContent},
            AnyMessageLikeEventContent,
        },
        uint,
    },
};

use super::{HistoryViewerEvent, MediaHistoryViewer};
use crate::{session::model::Session, spawn, spawn_tokio};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_details/history_viewer/media_item.ui"
    )]
    pub struct MediaItem {
        pub event: RefCell<Option<HistoryViewerEvent>>,
        pub overlay_icon: RefCell<Option<gtk::Image>>,
        #[template_child]
        pub overlay: TemplateChild<gtk::Overlay>,
        #[template_child]
        pub picture: TemplateChild<gtk::Picture>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MediaItem {
        const NAME: &'static str = "ContentMediaHistoryViewerItem";
        type Type = super::MediaItem;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);

            klass.set_css_name("mediahistoryvieweritem");
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MediaItem {
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

        fn dispose(&self) {
            self.overlay.unparent();
        }
    }

    impl WidgetImpl for MediaItem {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            // Keep the widget squared
            let (min, ..) = self.overlay.measure(orientation, for_size);
            (min, for_size.max(min), -1, -1)
        }

        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.overlay.allocate(width, height, baseline, None);
        }
    }
}

glib::wrapper! {
    pub struct MediaItem(ObjectSubclass<imp::MediaItem>)
        @extends gtk::Widget;
}

#[gtk::template_callbacks]
impl MediaItem {
    pub fn set_event(&self, event: Option<HistoryViewerEvent>) {
        if self.event() == event {
            return;
        }

        if let Some(ref event) = event {
            match event.original_content() {
                Some(AnyMessageLikeEventContent::RoomMessage(message)) => match message.msgtype {
                    MessageType::Image(content) => {
                        self.show_image(content, &event.room().unwrap().session());
                    }
                    MessageType::Video(content) => {
                        self.show_video(content, &event.room().unwrap().session());
                    }
                    _ => {
                        panic!("Unexpected message type");
                    }
                },
                _ => {
                    panic!("Unexpected message type");
                }
            }
        }

        self.imp().event.replace(event);
        self.notify("event");
    }

    pub fn event(&self) -> Option<HistoryViewerEvent> {
        self.imp().event.borrow().clone()
    }

    fn show_image(&self, image: ImageMessageEventContent, session: &Session) {
        let imp = self.imp();

        if let Some(icon) = imp.overlay_icon.take() {
            imp.overlay.remove_overlay(&icon);
        }

        self.load_thumbnail(image, session);
    }

    fn show_video(&self, video: VideoMessageEventContent, session: &Session) {
        let imp = self.imp();

        if imp.overlay_icon.borrow().is_none() {
            let icon = gtk::Image::builder()
                .icon_name("media-playback-start-symbolic")
                .css_classes(vec!["osd".to_string()])
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .build();

            imp.overlay.add_overlay(&icon);
            imp.overlay_icon.replace(Some(icon));
        }

        self.load_thumbnail(video, session);
    }

    fn load_thumbnail<C>(&self, content: C, session: &Session)
    where
        C: MediaEventContent + Send + Sync + Clone + 'static,
    {
        let media = session.client().media();
        let handle = spawn_tokio!(async move {
            let thumbnail = if content.thumbnail_source().is_some() {
                media
                    .get_thumbnail(
                        content.clone(),
                        MediaThumbnailSize {
                            method: Method::Scale,
                            width: uint!(300),
                            height: uint!(300),
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
                        match gdk::Texture::from_bytes(&glib::Bytes::from(&data)) {
                            Ok(texture) => {
                                imp.picture.set_paintable(Some(&texture));
                            }
                            Err(error) => {
                                warn!("Image file not supported: {}", error);
                            }
                        }
                    }
                    Ok(None) => {
                        warn!("Could not retrieve invalid media file");
                    }
                    Err(error) => {
                        warn!("Could not retrieve media file: {}", error);
                    }
                }
            })
        );
    }

    #[template_callback]
    fn handle_release(&self) {
        let media_history_viewer = self
            .ancestor(MediaHistoryViewer::static_type())
            .and_downcast::<MediaHistoryViewer>()
            .unwrap();
        media_history_viewer.show_media(self);
    }
}

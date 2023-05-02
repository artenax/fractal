use gtk::{gdk, glib, glib::clone, prelude::*, subclass::prelude::*};
use log::error;
use matrix_sdk::{
    media::{MediaFormat, MediaRequest, MediaThumbnailSize},
    ruma::{
        api::client::media::get_content_thumbnail::v3::Method, events::room::MediaSource, MxcUri,
        OwnedMxcUri,
    },
};

use crate::{
    components::ImagePaintable,
    session::Session,
    spawn, spawn_tokio,
    utils::notifications::{paintable_as_notification_icon, string_as_notification_icon},
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::object::WeakRef;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct AvatarData {
        pub image: RefCell<Option<gdk::Paintable>>,
        pub needed_size: Cell<u32>,
        pub url: RefCell<Option<OwnedMxcUri>>,
        pub display_name: RefCell<Option<String>>,
        pub session: WeakRef<Session>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AvatarData {
        const NAME: &'static str = "AvatarData";
        type Type = super::AvatarData;
    }

    impl ObjectImpl for AvatarData {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<gdk::Paintable>("image")
                        .read_only()
                        .build(),
                    glib::ParamSpecUInt::builder("needed-size")
                        .minimum(0)
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecString::builder("url")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecString::builder("display-name")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<Session>("session")
                        .construct_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "needed-size" => obj.set_needed_size(value.get().unwrap()),
                "url" => obj.set_url(value.get::<&str>().ok().map(Into::into)),
                "session" => self.session.set(value.get().ok().as_ref()),
                "display-name" => obj.set_display_name(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "image" => obj.image().to_value(),
                "needed-size" => obj.needed_size().to_value(),
                "url" => obj.url().map_or_else(
                    || {
                        let none: Option<&str> = None;
                        none.to_value()
                    },
                    |url| url.as_str().to_value(),
                ),
                "display-name" => obj.display_name().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// Data about a User’s or Room’s avatar.
    pub struct AvatarData(ObjectSubclass<imp::AvatarData>);
}

impl AvatarData {
    pub fn new(session: &Session, url: Option<&MxcUri>) -> Self {
        glib::Object::builder()
            .property("session", session)
            .property("url", &url.map(|url| url.to_string()))
            .build()
    }

    /// The current session.
    fn session(&self) -> Session {
        self.imp().session.upgrade().unwrap()
    }

    /// The user-defined image, if any.
    pub fn image(&self) -> Option<gdk::Paintable> {
        self.imp().image.borrow().clone()
    }

    /// Set the user-defined image.
    fn set_image_data(&self, data: Option<Vec<u8>>) {
        let image = data
            .and_then(|data| ImagePaintable::from_bytes(&glib::Bytes::from(&data), None).ok())
            .map(|texture| texture.upcast());
        self.imp().image.replace(image);
        self.notify("image");
    }

    fn load(&self) {
        // Don't do anything here if we don't need the avatar
        if self.needed_size() == 0 {
            return;
        }

        let Some(url) = self.url() else {
            return;
        };

        let client = self.session().client();
        let needed_size = self.needed_size();
        let request = MediaRequest {
            source: MediaSource::Plain(url),
            format: MediaFormat::Thumbnail(MediaThumbnailSize {
                width: needed_size.into(),
                height: needed_size.into(),
                method: Method::Scale,
            }),
        };
        let handle =
            spawn_tokio!(async move { client.media().get_media_content(&request, true).await });

        spawn!(
            glib::PRIORITY_LOW,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(data) => obj.set_image_data(Some(data)),
                    Err(error) => error!("Couldn’t fetch avatar: {error}"),
                };
            })
        );
    }

    /// Set the display name used for this avatar.
    pub fn set_display_name(&self, display_name: Option<String>) {
        if self.display_name() == display_name {
            return;
        }

        self.imp().display_name.replace(display_name);

        self.notify("display-name");
    }

    /// The display name used for this avatar.
    pub fn display_name(&self) -> Option<String> {
        self.imp().display_name.borrow().clone()
    }

    /// Set the needed size of the user-defined image.
    ///
    /// Only the biggest size will be stored.
    pub fn set_needed_size(&self, size: u32) {
        let imp = self.imp();

        if imp.needed_size.get() < size {
            imp.needed_size.set(size);

            self.load();
        }

        self.notify("needed-size");
    }

    /// Get the biggest needed size of the user-defined image.
    ///
    /// If this is `0`, no image will be loaded.
    pub fn needed_size(&self) -> u32 {
        self.imp().needed_size.get()
    }

    /// Set the url of the `AvatarData`.
    pub fn set_url(&self, url: Option<OwnedMxcUri>) {
        let imp = self.imp();

        if imp.url.borrow().as_ref() == url.as_ref() {
            return;
        }

        let has_url = url.is_some();
        imp.url.replace(url);

        if has_url {
            self.load();
        } else {
            self.set_image_data(None);
        }

        self.notify("url");
    }

    /// The url of the `AvatarData`.
    pub fn url(&self) -> Option<OwnedMxcUri> {
        self.imp().url.borrow().to_owned()
    }

    /// Get this avatar as a notification icon.
    ///
    /// Returns `None` if an error occurred while generating the icon.
    pub fn as_notification_icon(&self, helper_widget: &gtk::Widget) -> Option<gdk::Texture> {
        let icon = if let Some(paintable) = self.image() {
            paintable_as_notification_icon(paintable.upcast_ref(), helper_widget)
        } else {
            string_as_notification_icon(&self.display_name().unwrap_or_default(), helper_widget)
        };

        match icon {
            Ok(icon) => Some(icon),
            Err(error) => {
                log::warn!("Failed to generate icon for notification: {error}");
                None
            }
        }
    }
}

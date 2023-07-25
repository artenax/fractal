use gtk::{gdk, glib, glib::clone, prelude::*, subclass::prelude::*};
use matrix_sdk::{
    media::{MediaFormat, MediaRequest, MediaThumbnailSize},
    ruma::{
        api::client::media::get_content_thumbnail::v3::Method, events::room::MediaSource, MxcUri,
        OwnedMxcUri,
    },
};
use tracing::error;

use crate::{components::ImagePaintable, session::model::Session, spawn, spawn_tokio};

/// The source of an avatar's URI.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "AvatarUriSource")]
pub enum AvatarUriSource {
    /// The URI comes from a Matrix user.
    #[default]
    User = 0,
    /// The URI comes from a Matrix room.
    Room = 1,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::object::WeakRef;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct AvatarImage {
        pub paintable: RefCell<Option<gdk::Paintable>>,
        pub needed_size: Cell<u32>,
        pub uri: RefCell<Option<OwnedMxcUri>>,
        /// The source of the avatar's URI.
        pub uri_source: Cell<AvatarUriSource>,
        pub session: WeakRef<Session>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AvatarImage {
        const NAME: &'static str = "AvatarImage";
        type Type = super::AvatarImage;
    }

    impl ObjectImpl for AvatarImage {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<gdk::Paintable>("paintable")
                        .read_only()
                        .build(),
                    glib::ParamSpecUInt::builder("needed-size")
                        .minimum(0)
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecString::builder("uri")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecEnum::builder::<AvatarUriSource>("uri-source")
                        .construct_only()
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
                "uri" => obj.set_uri(value.get::<&str>().ok().map(Into::into)),
                "uri-source" => obj.set_uri_source(value.get().unwrap()),
                "session" => self.session.set(value.get().ok().as_ref()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "paintable" => obj.paintable().to_value(),
                "needed-size" => obj.needed_size().to_value(),
                "uri" => obj.uri().map_or_else(
                    || {
                        let none: Option<&str> = None;
                        none.to_value()
                    },
                    |url| url.as_str().to_value(),
                ),
                "uri-source" => obj.uri_source().to_value(),
                "session" => obj.session().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// The image data for an avatar.
    pub struct AvatarImage(ObjectSubclass<imp::AvatarImage>);
}

impl AvatarImage {
    /// Construct a new `AvatarImage` with the given session and Matrix URI.
    pub fn new(session: &Session, uri: Option<&MxcUri>, uri_source: AvatarUriSource) -> Self {
        glib::Object::builder()
            .property("session", session)
            .property("uri", uri.map(|uri| uri.to_string()))
            .property("uri-source", uri_source)
            .build()
    }

    /// The current session.
    fn session(&self) -> Session {
        self.imp().session.upgrade().unwrap()
    }

    /// The image content as a paintable, if any.
    pub fn paintable(&self) -> Option<gdk::Paintable> {
        self.imp().paintable.borrow().clone()
    }

    /// Set the content of the image.
    fn set_image_data(&self, data: Option<Vec<u8>>) {
        let paintable = data
            .and_then(|data| ImagePaintable::from_bytes(&glib::Bytes::from(&data), None).ok())
            .map(|texture| texture.upcast());
        self.imp().paintable.replace(paintable);
        self.notify("paintable");
    }

    fn load(&self) {
        // Don't do anything here if we don't need the avatar.
        if self.needed_size() == 0 {
            return;
        }

        let Some(uri) = self.uri() else {
            return;
        };

        let client = self.session().client();
        let needed_size = self.needed_size();
        let request = MediaRequest {
            source: MediaSource::Plain(uri),
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
                    Err(error) => error!("Could not fetch avatar: {error}"),
                };
            })
        );
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

    /// Set the Matrix URI of the `AvatarImage`.
    pub fn set_uri(&self, uri: Option<OwnedMxcUri>) {
        let imp = self.imp();

        if imp.uri.borrow().as_ref() == uri.as_ref() {
            return;
        }

        let has_uri = uri.is_some();
        imp.uri.replace(uri);

        if has_uri {
            self.load();
        } else {
            self.set_image_data(None);
        }

        self.notify("uri");
    }

    /// The Matrix URI of the `AvatarImage`.
    pub fn uri(&self) -> Option<OwnedMxcUri> {
        self.imp().uri.borrow().to_owned()
    }

    /// The source of the avatar's URI.
    pub fn uri_source(&self) -> AvatarUriSource {
        self.imp().uri_source.get()
    }

    /// Set the source of the avatar's URI.
    fn set_uri_source(&self, uri_source: AvatarUriSource) {
        self.imp().uri_source.set(uri_source);
    }
}

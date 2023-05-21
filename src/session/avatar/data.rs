use gtk::{gdk, glib, prelude::*, subclass::prelude::*};

use super::AvatarImage;
use crate::{
    application::Application,
    utils::notifications::{paintable_as_notification_icon, string_as_notification_icon},
};

mod imp {
    use std::cell::RefCell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct AvatarData {
        /// The data of the user-defined image.
        pub image: RefCell<Option<AvatarImage>>,
        /// The display name used as a fallback for this avatar.
        pub display_name: RefCell<Option<String>>,
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
                    glib::ParamSpecObject::builder::<AvatarImage>("image")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecString::builder("display-name")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "image" => obj.set_image(value.get().unwrap()),
                "display-name" => obj.set_display_name(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "image" => obj.image().to_value(),
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
    pub fn new(image: AvatarImage) -> Self {
        glib::Object::builder().property("image", image).build()
    }

    /// The data of the user-defined image.
    pub fn image(&self) -> AvatarImage {
        self.imp().image.borrow().clone().unwrap()
    }

    /// Set the data of the user-defined image.
    pub fn set_image(&self, image: AvatarImage) {
        let imp = self.imp();

        if imp.image.borrow().as_ref() == Some(&image) {
            return;
        }

        imp.image.replace(Some(image));
        self.notify("image");
    }

    /// Set the display name used as a fallback for this avatar.
    pub fn set_display_name(&self, display_name: Option<String>) {
        let imp = self.imp();

        if imp.display_name.borrow().as_ref() == display_name.as_ref() {
            return;
        }

        imp.display_name.replace(display_name);
        self.notify("display-name");
    }

    /// The display name used as a fallback for this avatar.
    pub fn display_name(&self) -> Option<String> {
        self.imp().display_name.borrow().clone()
    }

    /// Get this avatar as a notification icon.
    ///
    /// Returns `None` if an error occurred while generating the icon.
    pub fn as_notification_icon(&self) -> Option<gdk::Texture> {
        let window = Application::default().main_window().upcast();

        let icon = if let Some(paintable) = self.image().paintable() {
            paintable_as_notification_icon(paintable.upcast_ref(), &window)
        } else {
            string_as_notification_icon(&self.display_name().unwrap_or_default(), &window)
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

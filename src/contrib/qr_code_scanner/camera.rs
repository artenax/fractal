// SPDX-License-Identifier: GPL-3.0-or-later
use std::time::Duration;

use ashpd::desktop::camera;
use gtk::{glib, subclass::prelude::*};
use once_cell::sync::Lazy;
use tokio::time::timeout;

use super::camera_paintable::CameraPaintable;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Camera {
        pub paintable: glib::WeakRef<CameraPaintable>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Camera {
        const NAME: &'static str = "Camera";
        type Type = super::Camera;
    }

    impl ObjectImpl for Camera {}
}

glib::wrapper! {
    pub struct Camera(ObjectSubclass<imp::Camera>);
}

impl Camera {
    /// Create a new `Camera`. You should consider using `Camera::default()` to
    /// get a shared Object
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    pub async fn has_camera(&self) -> Result<bool, ashpd::Error> {
        let camera = camera::Camera::new().await?;

        if camera.is_present().await? {
            // Apparently is-camera-present doesn't report the correct value: https://github.com/flatpak/xdg-desktop-portal/issues/486#issuecomment-897636589
            // We need to use the proper timeout based on the executer
            if glib::MainContext::default().is_owner() {
                Ok(
                    crate::utils::timeout_future(Duration::from_secs(1), camera::request())
                        .await
                        .is_ok(),
                )
            } else {
                Ok(timeout(Duration::from_secs(1), camera::request())
                    .await
                    .is_ok())
            }
        } else {
            Ok(false)
        }
    }

    /// Get the a `gdk::Paintable` displaying the content of a camera
    /// This will panic if not called from the `MainContext` gtk is running on
    pub async fn paintable(&self) -> Option<CameraPaintable> {
        // We need to make sure that the Paintable is taken only from the MainContext
        assert!(glib::MainContext::default().is_owner());

        crate::utils::timeout_future(Duration::from_secs(1), self.paintable_internal())
            .await
            .ok()?
    }

    async fn paintable_internal(&self) -> Option<CameraPaintable> {
        if let Some(paintable) = self.imp().paintable.upgrade() {
            Some(paintable)
        } else if let Ok(Some((stream_fd, streams))) = camera::request().await {
            let paintable = CameraPaintable::new(stream_fd, streams).await;
            self.imp().paintable.set(Some(&paintable));
            Some(paintable)
        } else {
            None
        }
    }
}

impl Default for Camera {
    fn default() -> Self {
        static CAMERA: Lazy<Camera> = Lazy::new(Camera::new);

        CAMERA.to_owned()
    }
}

unsafe impl Send for Camera {}
unsafe impl Sync for Camera {}

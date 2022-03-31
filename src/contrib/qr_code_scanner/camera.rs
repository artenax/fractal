// SPDX-License-Identifier: GPL-3.0-or-later
use std::{os::unix::prelude::RawFd, time::Duration};

use ashpd::{desktop::camera, zbus};
use gtk::{glib, subclass::prelude::*};
use once_cell::sync::Lazy;
use tokio::time::timeout;

use super::camera_paintable::CameraPaintable;

mod imp {
    use std::sync::Arc;

    use tokio::sync::OnceCell;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Camera {
        pub connection: Arc<OnceCell<zbus::Connection>>,
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
        glib::Object::new(&[]).expect("Failed to create a Camera")
    }

    async fn connection(&self) -> Result<zbus::Connection, ashpd::Error> {
        let connection = self.imp().connection.clone();
        Ok(connection
            .get_or_try_init(zbus::Connection::session)
            .await?
            .clone())
    }

    async fn file_descriptor(&self) -> Result<(RawFd, Option<u32>), ashpd::Error> {
        let proxy = camera::CameraProxy::new(&self.connection().await?).await?;
        proxy.access_camera().await?;
        let stream_fd = proxy.open_pipe_wire_remote().await?;
        let node_id = camera::pipewire_node_id(stream_fd).await.ok().flatten();

        Ok((stream_fd, node_id))
    }

    pub async fn has_camera(&self) -> Result<bool, ashpd::Error> {
        let proxy = camera::CameraProxy::new(&self.connection().await?).await?;

        if proxy.is_camera_present().await? {
            // Apparently is-camera-present doesn't report the correct value: https://github.com/flatpak/xdg-desktop-portal/issues/486#issuecomment-897636589
            // We need to use the proper timeout based on the executer
            if glib::MainContext::default().is_owner() {
                Ok(
                    crate::utils::timeout_future(Duration::from_secs(1), self.file_descriptor())
                        .await
                        .is_ok(),
                )
            } else {
                Ok(timeout(Duration::from_secs(1), self.file_descriptor())
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
        } else if let Ok((stream_fd, node_id)) = self.file_descriptor().await {
            let paintable = CameraPaintable::new(stream_fd, node_id).await;
            self.imp().paintable.set(Some(&paintable));
            Some(paintable)
        } else {
            None
        }
    }
}

impl Default for Camera {
    fn default() -> Self {
        static CAMERA: Lazy<Camera> =
            Lazy::new(|| glib::Object::new(&[]).expect("Failed to create a Camera"));

        CAMERA.to_owned()
    }
}

unsafe impl Send for Camera {}
unsafe impl Sync for Camera {}

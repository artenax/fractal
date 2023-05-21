use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk::{
    encryption::identities::Device as CryptoDevice,
    ruma::{
        api::client::device::{delete_device, Device as MatrixDevice},
        assign, DeviceId,
    },
};

use crate::{
    components::{AuthDialog, AuthError},
    session::model::Session,
};

mod imp {
    use glib::object::WeakRef;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct Device {
        pub device: OnceCell<MatrixDevice>,
        pub crypto_device: OnceCell<CryptoDevice>,
        pub session: WeakRef<Session>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Device {
        const NAME: &'static str = "Device";
        type Type = super::Device;
    }

    impl ObjectImpl for Device {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Session>("session")
                        .construct_only()
                        .build(),
                    glib::ParamSpecString::builder("device-id")
                        .read_only()
                        .build(),
                    glib::ParamSpecString::builder("display-name")
                        .read_only()
                        .build(),
                    glib::ParamSpecString::builder("last-seen-ip")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoxed::builder::<glib::DateTime>("last-seen-ts")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("verified")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "session" => self.session.set(value.get().ok().as_ref()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.session().to_value(),
                "display-name" => obj.display_name().to_value(),
                "device-id" => obj.device_id().as_str().to_value(),
                "last-seen-ip" => obj.last_seen_ip().to_value(),
                "last-seen-ts" => obj.last_seen_ts().to_value(),
                "verified" => obj.is_verified().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// `glib::Object` representation of a Device/Session of a User.
    pub struct Device(ObjectSubclass<imp::Device>);
}

impl Device {
    pub fn new(
        session: &Session,
        device: MatrixDevice,
        crypto_device: Option<CryptoDevice>,
    ) -> Self {
        let obj: Self = glib::Object::builder().property("session", session).build();

        obj.set_matrix_device(device, crypto_device);

        obj
    }

    /// The current session.
    pub fn session(&self) -> Session {
        self.imp().session.upgrade().unwrap()
    }

    /// Set the Matrix device of this `Device`.
    fn set_matrix_device(&self, device: MatrixDevice, crypto_device: Option<CryptoDevice>) {
        let imp = self.imp();
        imp.device.set(device).unwrap();
        if let Some(crypto_device) = crypto_device {
            imp.crypto_device.set(crypto_device).unwrap();
        }
    }

    /// The ID of this device.
    pub fn device_id(&self) -> &DeviceId {
        &self.imp().device.get().unwrap().device_id
    }

    /// The display name of the device.
    pub fn display_name(&self) -> &str {
        if let Some(ref display_name) = self.imp().device.get().unwrap().display_name {
            display_name
        } else {
            self.device_id().as_str()
        }
    }

    /// The last IP address the device used.
    pub fn last_seen_ip(&self) -> Option<&str> {
        // TODO: Would be nice to also show the location
        // See: https://gitlab.gnome.org/GNOME/fractal/-/issues/700
        self.imp().device.get().unwrap().last_seen_ip.as_deref()
    }

    /// The last time the device was used.
    pub fn last_seen_ts(&self) -> Option<glib::DateTime> {
        self.imp()
            .device
            .get()
            .unwrap()
            .last_seen_ts
            .map(|last_seen_ts| {
                glib::DateTime::from_unix_utc(last_seen_ts.as_secs().into())
                    .and_then(|t| t.to_local())
                    .unwrap()
            })
    }

    /// Deletes the `Device`.
    pub async fn delete(
        &self,
        transient_for: Option<&impl IsA<gtk::Window>>,
    ) -> Result<(), AuthError> {
        let session = self.session();
        let device_id = self.device_id().to_owned();

        let dialog = AuthDialog::new(transient_for, &session);

        dialog
            .authenticate(move |client, auth| {
                let device_id = device_id.clone();
                async move {
                    let request = assign!(delete_device::v3::Request::new(device_id), { auth });
                    client.send(request, None).await.map_err(Into::into)
                }
            })
            .await?;
        Ok(())
    }

    /// Whether this device is verified.
    pub fn is_verified(&self) -> bool {
        self.imp()
            .crypto_device
            .get()
            .map_or(false, |device| device.is_verified())
    }
}

use gettextrs::gettext;
use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use log::error;
use matrix_sdk::{
    encryption::identities::UserDevices as CryptoDevices,
    ruma::api::client::device::Device as MatrixDevice, Error,
};

use super::{Device, DeviceItem};
use crate::{session::Session, spawn, spawn_tokio};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::object::WeakRef;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct DeviceList {
        pub list: RefCell<Vec<DeviceItem>>,
        pub session: WeakRef<Session>,
        pub current_device: RefCell<Option<DeviceItem>>,
        pub loading: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DeviceList {
        const NAME: &'static str = "DeviceList";
        type Type = super::DeviceList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for DeviceList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Session>("session")
                        .construct_only()
                        .build(),
                    glib::ParamSpecObject::builder::<DeviceItem>("current-device")
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
                "current-device" => obj.current_device().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.obj().load_devices();
        }
    }

    impl ListModelImpl for DeviceList {
        fn item_type(&self) -> glib::Type {
            DeviceItem::static_type()
        }
        fn n_items(&self) -> u32 {
            self.list.borrow().len() as u32
        }
        fn item(&self, position: u32) -> Option<glib::Object> {
            self.list
                .borrow()
                .get(position as usize)
                .map(glib::object::Cast::upcast_ref::<glib::Object>)
                .cloned()
        }
    }
}

glib::wrapper! {
    /// List of active devices for the logged in user.
    pub struct DeviceList(ObjectSubclass<imp::DeviceList>)
        @implements gio::ListModel;
}

impl DeviceList {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// The current session.
    pub fn session(&self) -> Session {
        self.imp().session.upgrade().unwrap()
    }

    fn set_loading(&self, loading: bool) {
        let imp = self.imp();

        if loading == imp.loading.get() {
            return;
        }
        if loading {
            self.update_list(vec![DeviceItem::for_loading_spinner()]);
        }
        imp.loading.set(loading);
        self.notify("current-device");
    }

    fn loading(&self) -> bool {
        self.imp().loading.get()
    }

    /// The device of this session.
    pub fn current_device(&self) -> DeviceItem {
        self.imp()
            .current_device
            .borrow()
            .clone()
            .unwrap_or_else(|| {
                if self.loading() {
                    DeviceItem::for_loading_spinner()
                } else {
                    DeviceItem::for_error(gettext("Failed to load connected device."))
                }
            })
    }

    /// Set the device of this session.
    fn set_current_device(&self, device: Option<DeviceItem>) {
        self.imp().current_device.replace(device);

        self.notify("current-device");
    }

    fn update_list(&self, devices: Vec<DeviceItem>) {
        let added = devices.len();

        let prev_devices = self.imp().list.replace(devices);

        self.items_changed(0, prev_devices.len() as u32, added as u32);
    }

    fn finish_loading(
        &self,
        response: Result<(Option<MatrixDevice>, Vec<MatrixDevice>, CryptoDevices), Error>,
    ) {
        let session = self.session();

        match response {
            Ok((current_device, devices, crypto_devices)) => {
                let devices = devices
                    .into_iter()
                    .map(|device| {
                        let crypto_device = crypto_devices.get(&device.device_id);
                        DeviceItem::for_device(Device::new(&session, device, crypto_device))
                    })
                    .collect();

                self.update_list(devices);

                self.set_current_device(current_device.map(|device| {
                    let crypto_device = crypto_devices.get(&device.device_id);
                    DeviceItem::for_device(Device::new(&session, device, crypto_device))
                }));
            }
            Err(error) => {
                error!("Couldn’t load device list: {}", error);
                self.update_list(vec![DeviceItem::for_error(gettext(
                    "Failed to load the list of connected devices.",
                ))]);
            }
        }
        self.set_loading(false);
    }

    pub fn load_devices(&self) {
        let client = self.session().client();

        self.set_loading(true);

        let handle = spawn_tokio!(async move {
            let user_id = client.user_id().unwrap();
            let crypto_devices = client.encryption().get_user_devices(user_id).await?;

            match client.devices().await {
                Ok(mut response) => {
                    response
                        .devices
                        .sort_unstable_by(|a, b| b.last_seen_ts.cmp(&a.last_seen_ts));

                    let current_device = if let Some(current_device_id) = client.device_id() {
                        if let Some(index) = response
                            .devices
                            .iter()
                            .position(|device| *device.device_id == current_device_id.as_ref())
                        {
                            Some(response.devices.remove(index))
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    Ok((current_device, response.devices, crypto_devices))
                }
                Err(error) => Err(Error::Http(error)),
            }
        });

        spawn!(clone!(@weak self as obj => async move {
            obj.finish_loading(handle.await.unwrap());
        }));
    }
}

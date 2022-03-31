use gtk::{glib, prelude::*, subclass::prelude::*};

use super::Device;

/// This enum contains all possible types the device list can hold.
#[derive(Debug, Clone)]
pub enum ItemType {
    Device(Device),
    Error(String),
    LoadingSpinner,
}

#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "BoxedDeviceItemType")]
pub struct BoxedItemType(ItemType);

impl From<ItemType> for BoxedItemType {
    fn from(type_: ItemType) -> Self {
        BoxedItemType(type_)
    }
}

mod imp {
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct Item {
        pub type_: OnceCell<ItemType>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Item {
        const NAME: &'static str = "DeviceItem";
        type Type = super::Item;
    }

    impl ObjectImpl for Item {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecBoxed::new(
                    "type",
                    "Type",
                    "The type of this item",
                    BoxedItemType::static_type(),
                    glib::ParamFlags::WRITABLE | glib::ParamFlags::CONSTRUCT_ONLY,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "type" => {
                    let type_ = value.get::<BoxedItemType>().unwrap();
                    self.type_.set(type_.0).unwrap();
                }

                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct Item(ObjectSubclass<imp::Item>);
}

impl Item {
    pub fn for_device(device: Device) -> Self {
        let type_ = BoxedItemType(ItemType::Device(device));
        glib::Object::new(&[("type", &type_)]).expect("Failed to create Item")
    }

    pub fn for_error(error: String) -> Self {
        let type_ = BoxedItemType(ItemType::Error(error));
        glib::Object::new(&[("type", &type_)]).expect("Failed to create Item")
    }

    pub fn for_loading_spinner() -> Self {
        let type_ = BoxedItemType(ItemType::LoadingSpinner);
        glib::Object::new(&[("type", &type_)]).expect("Failed to create Item")
    }

    pub fn type_(&self) -> &ItemType {
        self.imp().type_.get().unwrap()
    }
}

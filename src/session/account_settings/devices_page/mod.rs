use adw::subclass::prelude::*;
use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};

mod device;
use self::device::Device;
mod device_row;
use self::device_row::DeviceRow;
mod device_item;
use self::device_item::Item as DeviceItem;
mod device_list;
use self::device_list::DeviceList;
use crate::{
    components::LoadingListBoxRow,
    session::{user::UserExt, User},
};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/account-settings-devices-page.ui")]
    pub struct DevicesPage {
        pub user: RefCell<Option<User>>,
        #[template_child]
        pub other_sessions_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub other_sessions: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub current_session: TemplateChild<gtk::ListBox>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DevicesPage {
        const NAME: &'static str = "DevicesPage";
        type Type = super::DevicesPage;
        type ParentType = adw::PreferencesPage;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DevicesPage {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "user",
                    "User",
                    "The user of this account",
                    User::static_type(),
                    glib::ParamFlags::READWRITE,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "user" => obj.set_user(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "user" => obj.user().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for DevicesPage {}
    impl PreferencesPageImpl for DevicesPage {}
}

glib::wrapper! {
    /// Preference Window to display and update room details.
    pub struct DevicesPage(ObjectSubclass<imp::DevicesPage>)
        @extends gtk::Widget, gtk::Window, adw::Window, adw::PreferencesWindow, @implements gtk::Accessible;
}

impl DevicesPage {
    pub fn new(parent_window: &Option<gtk::Window>, user: &User) -> Self {
        glib::Object::new(&[("transient-for", parent_window), ("user", user)])
            .expect("Failed to create DevicesPage")
    }

    pub fn user(&self) -> Option<User> {
        self.imp().user.borrow().clone()
    }

    fn set_user(&self, user: Option<User>) {
        let priv_ = self.imp();

        if self.user() == user {
            return;
        }

        if let Some(ref user) = user {
            let device_list = DeviceList::new(&user.session());
            priv_.other_sessions.bind_model(
                Some(&device_list),
                clone!(@weak device_list => @default-panic, move |item| {
                    match item.downcast_ref::<DeviceItem>().unwrap().type_() {
                        device_item::ItemType::Device(device) => DeviceRow::new(device, false).upcast::<gtk::Widget>(),
                        device_item::ItemType::Error(error) => {
                            let row = LoadingListBoxRow::new();
                            row.set_error(Some(error));
                            row.connect_retry(clone!(@weak device_list => move|_| {
                                device_list.load_devices()
                            }));
                            row.upcast::<gtk::Widget>()
                        }
                        device_item::ItemType::LoadingSpinner => {
                            LoadingListBoxRow::new().upcast::<gtk::Widget>()
                        }
                    }
                }),
            );

            device_list.connect_items_changed(
                clone!(@weak self as obj => move |device_list, _, _, _| {
                    obj.set_other_sessions_visibility(device_list.n_items() > 0)
                }),
            );

            self.set_other_sessions_visibility(device_list.n_items() > 0);

            device_list.connect_notify_local(
                Some("current-device"),
                clone!(@weak self as obj => move |device_list, _| {
                    obj.set_current_device(device_list);
                }),
            );

            self.set_current_device(&device_list);
        } else {
            priv_.other_sessions.unbind_model();

            if let Some(child) = priv_.current_session.first_child() {
                priv_.current_session.remove(&child);
            }
        }

        priv_.user.replace(user);
        self.notify("user");
    }

    fn set_other_sessions_visibility(&self, visible: bool) {
        self.imp().other_sessions_group.set_visible(visible);
    }

    fn set_current_device(&self, device_list: &DeviceList) {
        let priv_ = self.imp();
        if let Some(child) = priv_.current_session.first_child() {
            priv_.current_session.remove(&child);
        }
        let row = match device_list.current_device().type_() {
            device_item::ItemType::Device(device) => {
                DeviceRow::new(device, true).upcast::<gtk::Widget>()
            }
            device_item::ItemType::Error(error) => {
                let row = LoadingListBoxRow::new();
                row.set_error(Some(error));
                row.connect_retry(clone!(@weak device_list => move|_| {
                    device_list.load_devices()
                }));
                row.upcast::<gtk::Widget>()
            }
            device_item::ItemType::LoadingSpinner => {
                LoadingListBoxRow::new().upcast::<gtk::Widget>()
            }
        };
        priv_.current_session.append(&row);
    }
}

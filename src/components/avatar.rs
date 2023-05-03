use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};

use crate::session::{AvatarData, AvatarImage};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-avatar.ui")]
    pub struct Avatar {
        /// A `Room` or `User`
        pub data: RefCell<Option<AvatarData>>,
        #[template_child]
        pub avatar: TemplateChild<adw::Avatar>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Avatar {
        const NAME: &'static str = "ComponentsAvatar";
        type Type = super::Avatar;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            AvatarImage::static_type();
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Avatar {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<AvatarData>("data")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecInt::builder("size")
                        .minimum(-1)
                        .default_value(-1)
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "data" => obj.set_data(value.get().unwrap()),
                "size" => obj.set_size(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "data" => obj.data().to_value(),
                "size" => obj.size().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.obj().connect_map(|avatar| {
                avatar.request_custom_avatar();
            });
        }
    }

    impl WidgetImpl for Avatar {}

    impl BinImpl for Avatar {}
}

glib::wrapper! {
    /// A widget displaying an `Avatar` for a `Room` or `User`.
    pub struct Avatar(ObjectSubclass<imp::Avatar>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Avatar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Set the size of the Avatar.
    pub fn set_size(&self, size: i32) {
        if self.size() == size {
            return;
        }

        self.imp().avatar.set_size(size);

        if self.is_mapped() {
            self.request_custom_avatar();
        }

        self.notify("size");
    }

    /// Set the [`AvatarData`] displayed by this widget.
    pub fn set_data(&self, data: Option<AvatarData>) {
        let imp = self.imp();

        if *imp.data.borrow() == data {
            return;
        }

        imp.data.replace(data);

        if self.is_mapped() {
            self.request_custom_avatar();
        }

        self.notify("data");
    }

    /// The size of the Avatar.
    pub fn size(&self) -> i32 {
        self.imp().avatar.size()
    }

    /// The [`AvatarData`] displayed by this widget.
    pub fn data(&self) -> Option<AvatarData> {
        self.imp().data.borrow().clone()
    }

    fn request_custom_avatar(&self) {
        if let Some(data) = &*self.imp().data.borrow() {
            let size = self.size() * self.scale_factor();
            data.image().set_needed_size(size as u32);
        }
    }
}

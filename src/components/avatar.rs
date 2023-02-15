use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};

use crate::session::Avatar as AvatarItem;

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-avatar.ui")]
    pub struct Avatar {
        /// A `Room` or `User`
        pub item: RefCell<Option<AvatarItem>>,
        #[template_child]
        pub avatar: TemplateChild<adw::Avatar>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Avatar {
        const NAME: &'static str = "ComponentsAvatar";
        type Type = super::Avatar;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
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
                    glib::ParamSpecObject::builder::<AvatarItem>("item")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecInt::builder("size")
                        .minimum(-1)
                        .default_value(-1)
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "item" => obj.set_item(value.get().unwrap()),
                "size" => obj.set_size(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "item" => obj.item().to_value(),
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
        self.imp().avatar.set_size(size);
    }

    /// Set the Avatar item displayed by this widget.
    pub fn set_item(&self, item: Option<AvatarItem>) {
        let imp = self.imp();

        if *imp.item.borrow() == item {
            return;
        }

        imp.item.replace(item);

        if self.is_mapped() {
            self.request_custom_avatar();
        }

        self.notify("item");
    }

    /// The size of the Avatar.
    pub fn size(&self) -> i32 {
        self.imp().avatar.size()
    }

    /// The Avatar item displayed by this widget.
    pub fn item(&self) -> Option<AvatarItem> {
        self.imp().item.borrow().clone()
    }

    fn request_custom_avatar(&self) {
        if let Some(item) = &*self.imp().item.borrow() {
            // FIXME: update on size changes
            item.set_needed_size(self.size() as u32);
        }
    }
}

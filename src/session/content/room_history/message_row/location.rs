use adw::{prelude::*, subclass::prelude::*};
use gtk::{glib, CompositeTemplate};

use crate::components::LocationViewer;

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-message-location.ui")]
    pub struct MessageLocation {
        #[template_child]
        pub location: TemplateChild<LocationViewer>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MessageLocation {
        const NAME: &'static str = "ContentMessageLocation";
        type Type = super::MessageLocation;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MessageLocation {
        fn dispose(&self, _obj: &Self::Type) {
            self.location.unparent();
        }
    }

    impl WidgetImpl for MessageLocation {
        fn measure(
            &self,
            _widget: &Self::Type,
            _orientation: gtk::Orientation,
            _for_size: i32,
        ) -> (i32, i32, i32, i32) {
            (300, 300, -1, -1)
        }

        fn size_allocate(&self, _widget: &Self::Type, width: i32, height: i32, baseline: i32) {
            self.location
                .size_allocate(&gtk::Allocation::new(0, 0, width, height), baseline)
        }
    }
}

glib::wrapper! {
    /// A widget displaying a location message in the timeline.
    pub struct MessageLocation(ObjectSubclass<imp::MessageLocation>)
        @extends gtk::Widget, @implements gtk::Accessible;
}

impl MessageLocation {
    /// Create a new location message.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create MessageLocation")
    }

    pub fn set_geo_uri(&self, uri: &str) {
        self.imp().location.set_geo_uri(uri);
    }
}

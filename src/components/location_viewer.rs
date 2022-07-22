use adw::{prelude::*, subclass::prelude::*};
use gtk::{glib, CompositeTemplate};
use shumate::prelude::*;

use crate::i18n::gettext_f;

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-location-viewer.ui")]
    pub struct LocationViewer {
        #[template_child]
        pub map: TemplateChild<shumate::SimpleMap>,
        #[template_child]
        pub marker_img: TemplateChild<gtk::Image>,
        pub marker: shumate::Marker,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LocationViewer {
        const NAME: &'static str = "ComponentsLocationViewer";
        type Type = super::LocationViewer;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_css_name("location-viewer");
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LocationViewer {
        fn constructed(&self, obj: &Self::Type) {
            self.marker.set_child(Some(&*self.marker_img));

            let registry = shumate::MapSourceRegistry::with_defaults();
            let source = registry.by_id(&shumate::MAP_SOURCE_OSM_MAPNIK).unwrap();
            self.map.set_map_source(Some(&source));

            let viewport = self.map.viewport().unwrap();
            viewport.set_zoom_level(12.0);
            let marker_layer = shumate::MarkerLayer::new(&viewport);
            marker_layer.add_marker(&self.marker);
            self.map.add_overlay_layer(&marker_layer);

            // Hide the scale
            self.map.scale().unwrap().hide();
            self.parent_constructed(obj);
        }
    }

    impl WidgetImpl for LocationViewer {}
    impl BinImpl for LocationViewer {}
}

glib::wrapper! {
    /// A widget displaying a location message in the timeline.
    pub struct LocationViewer(ObjectSubclass<imp::LocationViewer>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl LocationViewer {
    /// Create a new location message.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create LocationViewer")
    }

    pub fn set_geo_uri(&self, uri: &str) {
        let imp = self.imp();

        let mut uri = uri.trim_start_matches("geo:").split(',');
        let latitude = uri
            .next()
            .and_then(|lat_s| lat_s.parse::<f64>().ok())
            .unwrap_or_default();
        let longitude = uri
            .next()
            .and_then(|lon_s| lon_s.parse::<f64>().ok())
            .unwrap_or_default();

        imp.map
            .viewport()
            .unwrap()
            .set_location(latitude, longitude);
        imp.marker.set_location(latitude, longitude);

        self.update_property(&[gtk::accessible::Property::Description(&gettext_f(
            "Location at latitude {latitude} and longitude {longitude}",
            &[
                ("latitude", &latitude.to_string()),
                ("longitude", &longitude.to_string()),
            ],
        ))]);
    }
}

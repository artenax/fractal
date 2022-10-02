use adw::{prelude::*, subclass::prelude::*};
use geo_uri::GeoUri;
use gtk::{glib, CompositeTemplate};
use shumate::prelude::*;

use crate::i18n::gettext_f;

mod imp {
    use std::cell::Cell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-location-viewer.ui")]
    pub struct LocationViewer {
        #[template_child]
        pub map: TemplateChild<shumate::SimpleMap>,
        #[template_child]
        pub marker_img: TemplateChild<gtk::Image>,
        pub marker: shumate::Marker,
        pub compact: Cell<bool>,
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
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecBoolean::new(
                    "compact",
                    "Compact",
                    "Whether to display this location in a compact format",
                    false,
                    glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
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
                "compact" => obj.set_compact(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "compact" => obj.compact().to_value(),
                _ => unimplemented!(),
            }
        }

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

    /// Whether to display this location in a compact format.
    pub fn compact(&self) -> bool {
        self.imp().compact.get()
    }

    /// Set the compact format of this location.
    pub fn set_compact(&self, compact: bool) {
        if self.compact() == compact {
            return;
        }

        let map = &self.imp().map;
        map.set_show_zoom_buttons(!compact);
        if let Some(license) = map.license() {
            license.set_visible(!compact);
        }

        self.imp().compact.set(compact);
        self.notify("compact");
    }

    // Move the map viewport to the provided coordinates and draw a marker.
    pub fn set_location(&self, geo_uri: &GeoUri) {
        let imp = self.imp();
        let latitude = geo_uri.latitude();
        let longitude = geo_uri.longitude();

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

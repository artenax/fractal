use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;

use crate::session::room::{MemberRole, PowerLevel, POWER_LEVEL_MAX, POWER_LEVEL_MIN};

mod imp {
    use std::cell::Cell;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Badge {
        pub power_level: Cell<PowerLevel>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Badge {
        const NAME: &'static str = "Badge";
        type Type = super::Badge;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for Badge {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecInt64::builder("power-level")
                    .minimum(POWER_LEVEL_MIN)
                    .maximum(POWER_LEVEL_MAX)
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "power-level" => self.obj().set_power_level(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "power-level" => self.obj().power_level().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.add_css_class("badge");
            let label = gtk::Label::new(Some("default"));
            obj.set_child(Some(&label));
        }
    }

    impl WidgetImpl for Badge {}
    impl BinImpl for Badge {}
}

glib::wrapper! {
    /// Inline widget displaying a badge with a power level.
    ///
    /// The badge displays admin for a power level of 100 and mod for levels
    /// over or equal to 50.
    pub struct Badge(ObjectSubclass<imp::Badge>)
        @extends gtk::Widget, adw::Bin;
}

impl Badge {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// The power level this badge displays.
    pub fn power_level(&self) -> PowerLevel {
        self.imp().power_level.get()
    }

    /// Set the power level this badge displays.
    pub fn set_power_level(&self, power_level: PowerLevel) {
        self.update_badge(power_level);
        self.imp().power_level.set(power_level);
        self.notify("power-level");
    }

    fn update_badge(&self, power_level: PowerLevel) {
        let label: gtk::Label = self.child().unwrap().downcast().unwrap();
        let role = MemberRole::from(power_level);

        match role {
            MemberRole::ADMIN => {
                label.set_text(&format!("{} {}", role, power_level));
                self.add_css_class("admin");
                self.remove_css_class("mod");
                self.show();
            }
            MemberRole::MOD => {
                label.set_text(&format!("{} {}", role, power_level));
                self.add_css_class("mod");
                self.remove_css_class("admin");
                self.show();
            }
            MemberRole::PEASANT if power_level != 0 => {
                label.set_text(&power_level.to_string());
                self.remove_css_class("admin");
                self.remove_css_class("mod");
                self.show()
            }
            _ => self.hide(),
        }
    }
}

impl Default for Badge {
    fn default() -> Self {
        Self::new()
    }
}

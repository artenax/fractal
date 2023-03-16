use adw::subclass::prelude::BinImpl;
use gtk::{self, gio, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::gettext;

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/greeter.ui")]
    pub struct Greeter {
        #[template_child]
        pub back_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub login_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub development_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub offline_banner: TemplateChild<adw::Banner>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Greeter {
        const NAME: &'static str = "Greeter";
        type Type = super::Greeter;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Greeter {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let monitor = gio::NetworkMonitor::default();
            monitor.connect_network_changed(clone!(@weak obj => move |_, _| {
                obj.update_network_state();
            }));

            obj.update_network_state();
        }
    }

    impl WidgetImpl for Greeter {}

    impl BinImpl for Greeter {}
}

glib::wrapper! {
    pub struct Greeter(ObjectSubclass<imp::Greeter>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Greeter {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn default_widget(&self) -> gtk::Widget {
        self.imp().login_button.get().upcast()
    }

    fn update_network_state(&self) {
        let imp = self.imp();
        let monitor = gio::NetworkMonitor::default();

        if !monitor.is_network_available() {
            imp.development_banner.set_revealed(false);
            imp.offline_banner
                .set_title(&gettext("No network connection"));
            imp.offline_banner.set_revealed(true);
        } else if monitor.connectivity() < gio::NetworkConnectivity::Full {
            imp.development_banner.set_revealed(false);
            imp.offline_banner
                .set_title(&gettext("No Internet connection"));
            imp.offline_banner.set_revealed(true);
        } else {
            imp.development_banner.set_revealed(true);
            imp.offline_banner.set_revealed(false);
        }
    }
}

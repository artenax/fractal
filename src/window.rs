use crate::config::{APP_ID, PROFILE};
use crate::Application;
use crate::Login;
use crate::Session;
use adw::subclass::prelude::AdwApplicationWindowImpl;
use glib::signal::Inhibit;
use gtk::subclass::prelude::*;
use gtk::{self, prelude::*};
use gtk::{gio, glib, glib::clone, CompositeTemplate};
use log::warn;

mod imp {
    use super::*;
    use glib::subclass::InitializingObject;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/window.ui")]
    pub struct Window {
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub login: TemplateChild<Login>,
        pub settings: gio::Settings,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "Window";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn new() -> Self {
            Self {
                main_stack: TemplateChild::default(),
                login: TemplateChild::default(),
                settings: gio::Settings::new(APP_ID),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            let builder = gtk::Builder::from_resource("/org/gnome/FractalNext/shortcuts.ui");
            let shortcuts = builder.object("shortcuts").unwrap();
            obj.set_help_overlay(Some(&shortcuts));

            // Devel Profile
            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }

            // load latest window state
            obj.load_window_size();

            // TODO: tell user that a stored session couldn't be restored
            let result = self.login.restore_sessions();
            if let Err(error) = result {
                warn!("Failed to restore a session: {:?}", error);
            }
            self.login.connect_new_session(
                clone!(@weak obj => move |_login, session| obj.add_session(session)),
            );
        }
    }

    impl WindowImpl for Window {
        // save window state on delete event
        fn close_request(&self, obj: &Self::Type) -> Inhibit {
            if let Err(err) = obj.save_window_size() {
                warn!("Failed to save window state, {}", &err);
            }
            Inhibit(false)
        }
    }

    impl WidgetImpl for Window {}
    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow, @implements gio::ActionMap, gio::ActionGroup;
}

impl Window {
    pub fn new(app: &Application) -> Self {
        glib::Object::new(&[("application", &Some(app)), ("icon-name", &Some(APP_ID))])
            .expect("Failed to create Window")
    }

    pub fn add_session(&self, session: &Session) {
        let priv_ = &imp::Window::from_instance(self);
        priv_.main_stack.add_child(session);
        priv_.main_stack.set_visible_child(session);
    }

    pub fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let settings = &imp::Window::from_instance(self).settings;

        let size = self.default_size();

        settings.set_int("window-width", size.0)?;
        settings.set_int("window-height", size.1)?;

        settings.set_boolean("is-maximized", self.is_maximized())?;

        Ok(())
    }

    fn load_window_size(&self) {
        let settings = &imp::Window::from_instance(self).settings;

        let width = settings.int("window-width");
        let height = settings.int("window-height");
        let is_maximized = settings.boolean("is-maximized");

        self.set_default_size(width, height);
        self.set_property("maximized", &is_maximized).unwrap();
    }
}
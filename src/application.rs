use gettextrs::gettext;
use gio::{ApplicationFlags, Settings};
use glib::{clone, WeakRef};
use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use gtk_macros::action;
use log::{debug, info};

use crate::{config, Window};

mod imp {
    use adw::subclass::prelude::AdwApplicationImpl;
    use once_cell::unsync::OnceCell;

    use super::*;

    #[derive(Debug)]
    pub struct Application {
        pub window: OnceCell<WeakRef<Window>>,
        pub settings: Settings,
    }

    impl Default for Application {
        fn default() -> Self {
            Self {
                window: Default::default(),
                settings: Settings::new(config::APP_ID),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "Application";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn activate(&self, app: &Self::Type) {
            debug!("GtkApplication<Application>::activate");

            if let Some(window) = self.window.get() {
                let window = window.upgrade().unwrap();
                window.show();
                window.present();
                return;
            }

            let window = Window::new(app);
            self.window
                .set(window.downgrade())
                .expect("Window already set.");

            app.setup_gactions();
            app.setup_accels();

            app.get_main_window().present();
        }

        fn startup(&self, app: &Self::Type) {
            debug!("GtkApplication<Application>::startup");
            self.parent_startup(app);
        }
    }

    impl GtkApplicationImpl for Application {}
    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends gio::Application, gtk::Application, adw::Application, @implements gio::ActionMap, gio::ActionGroup;
}

impl Application {
    pub fn new() -> Self {
        glib::Object::new(&[
            ("application-id", &Some(config::APP_ID)),
            ("flags", &ApplicationFlags::default()),
            ("resource-base-path", &Some("/org/gnome/Fractal/")),
        ])
        .expect("Application initialization failed")
    }

    fn get_main_window(&self) -> Window {
        self.imp().window.get().unwrap().upgrade().unwrap()
    }

    pub fn settings(&self) -> Settings {
        self.imp().settings.clone()
    }

    fn setup_gactions(&self) {
        // Quit
        action!(
            self,
            "quit",
            clone!(@weak self as app => move |_, _| {
                // This is needed to trigger the delete event
                // and saving the window state
                app.get_main_window().close();
                app.quit();
            })
        );

        // About
        action!(
            self,
            "about",
            clone!(@weak self as app => move |_, _| {
                app.show_about_dialog();
            })
        );

        action!(
            self,
            "new-session",
            clone!(@weak self as app => move |_, _| {
                app.get_main_window().switch_to_greeter_page(true);
            })
        );

        action!(
            self,
            "show-greeter",
            clone!(@weak self as app => move |_, _| {
                app.get_main_window().switch_to_greeter_page(false);
            })
        );

        action!(
            self,
            "show-login",
            clone!(@weak self as app => move |_, _| {
                app.get_main_window().switch_to_login_page();
            })
        );

        let show_sessions_action = gio::SimpleAction::new("show-sessions", None);
        show_sessions_action.connect_activate(clone!(@weak self as app => move |_, _| {
            app.get_main_window().switch_to_sessions_page();
        }));
        self.add_action(&show_sessions_action);
        let win = self.get_main_window();
        win.connect_notify_local(
            Some("has-sessions"),
            clone!(@weak show_sessions_action => move |win, _| {
                show_sessions_action.set_enabled(win.has_sessions());
            }),
        );
        show_sessions_action.set_enabled(win.has_sessions());
    }

    /// Sets up keyboard shortcuts for application and window actions.
    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("win.show-help-overlay", &["<Control>question"]);
    }

    fn show_about_dialog(&self) {
        let dialog = gtk::AboutDialog::builder()
            .logo_icon_name(config::APP_ID)
            .license_type(gtk::License::Gpl30)
            .website("https://gitlab.gnome.org/GNOME/fractal/")
            .website_label(gettext("Learn more about Fractal").as_str())
            .version(config::VERSION)
            .transient_for(&self.get_main_window())
            .modal(true)
            .comments(gettext("A Matrix client for GNOME").as_str())
            .copyright(gettext("© 2017-2021 The Fractal Team").as_str())
            .authors(vec![
                "Alejandro Domínguez".to_string(),
                "Alexandre Franke".to_string(),
                "Bilal Elmoussaoui".to_string(),
                "Christopher Davis".to_string(),
                "Daniel García Moreno".to_string(),
                "Eisha Chen-yen-su".to_string(),
                "Jordan Petridis".to_string(),
                "Julian Sparber".to_string(),
                "Kévin Commaille".to_string(),
                "Saurav Sachidanand".to_string(),
            ])
            .artists(vec!["Tobias Bernard".to_string()])
            .translator_credits(gettext("translator-credits").as_str())
            .build();

        // This can't be added via the builder
        dialog.add_credit_section(gettext("Name by").as_str(), &["Regina Bíró"]);

        dialog.show();
    }

    pub fn run(&self) {
        info!("Fractal ({})", config::APP_ID);
        info!("Version: {} ({})", config::VERSION, config::PROFILE);
        info!("Datadir: {}", config::PKGDATADIR);

        ApplicationExtManual::run(self);
    }
}

impl Default for Application {
    fn default() -> Self {
        gio::Application::default()
            .unwrap()
            .downcast::<Application>()
            .unwrap()
    }
}

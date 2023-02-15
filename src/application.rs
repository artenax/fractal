use std::borrow::Cow;

use gettextrs::gettext;
use gio::{ApplicationFlags, Settings};
use glib::{clone, WeakRef};
use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use gtk_macros::action;
use log::{debug, info};
use ruma::{OwnedRoomId, RoomId};

use crate::{config, Window};

mod imp {
    use adw::subclass::prelude::AdwApplicationImpl;

    use super::*;

    #[derive(Debug)]
    pub struct Application {
        pub window: WeakRef<Window>,
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
        fn activate(&self) {
            debug!("GtkApplication<Application>::activate");
            let app = self.obj();

            if let Some(window) = self.window.upgrade() {
                window.show();
                window.present();
                return;
            }

            let window = Window::new(&app);
            self.window.set(Some(&window));

            app.setup_gactions();
            app.setup_accels();

            let monitor = gio::NetworkMonitor::default();
            monitor.connect_network_changed(clone!(@weak app => move |monitor, _| {
                app.lookup_action("show-login")
                    .unwrap()
                    .downcast::<gio::SimpleAction>()
                    .unwrap()
                    .set_enabled(monitor.is_network_available());
            }));

            app.lookup_action("show-login")
                .unwrap()
                .downcast::<gio::SimpleAction>()
                .unwrap()
                .set_enabled(monitor.is_network_available());

            app.get_main_window().present();
        }

        fn startup(&self) {
            debug!("GtkApplication<Application>::startup");
            self.parent_startup();
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
        glib::Object::builder()
            .property("application-id", &Some(config::APP_ID))
            .property("flags", &ApplicationFlags::default())
            .property("resource-base-path", &Some("/org/gnome/Fractal/"))
            .build()
    }

    fn get_main_window(&self) -> Window {
        self.imp().window.upgrade().unwrap()
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
                app.get_main_window().switch_to_greeter_page();
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

        action!(
            self,
            "show-room",
            Some(&AppShowRoomPayload::static_variant_type()),
            clone!(@weak self as app => move |_, v| {
                if let Some(payload) = v.and_then(|v| v.get::<AppShowRoomPayload>()) {
                    app.get_main_window().show_room(&payload.session_id, &payload.room_id);
                }
            })
        );
    }

    /// Sets up keyboard shortcuts for application and window actions.
    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("win.show-help-overlay", &["<Control>question"]);
    }

    fn show_about_dialog(&self) {
        let dialog = adw::AboutWindow::builder()
            .application_name("Fractal")
            .application_icon(config::APP_ID)
            .developer_name(gettext("The Fractal Team"))
            .license_type(gtk::License::Gpl30)
            .website("https://gitlab.gnome.org/GNOME/fractal/")
            .issue_url("https://gitlab.gnome.org/GNOME/fractal/-/issues")
            .support_url("https://matrix.to/#/#fractal:gnome.org")
            .version(config::VERSION)
            .transient_for(&self.get_main_window())
            .modal(true)
            .copyright(gettext("© 2017-2023 The Fractal Team"))
            .developers(vec![
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
            .designers(vec!["Tobias Bernard".to_string()])
            .translator_credits(gettext("translator-credits"))
            .build();

        // This can't be added via the builder
        dialog.add_credit_section(Some(&gettext("Name by")), &["Regina Bíró"]);

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

#[derive(Debug, Clone)]
pub struct AppShowRoomPayload {
    pub session_id: String,
    pub room_id: OwnedRoomId,
}

impl glib::StaticVariantType for AppShowRoomPayload {
    fn static_variant_type() -> Cow<'static, glib::VariantTy> {
        <(String, String)>::static_variant_type()
    }
}

impl glib::ToVariant for AppShowRoomPayload {
    fn to_variant(&self) -> glib::Variant {
        (&self.session_id, self.room_id.as_str()).to_variant()
    }
}

impl glib::FromVariant for AppShowRoomPayload {
    fn from_variant(variant: &glib::Variant) -> Option<Self> {
        let (session_id, room_id) = variant.get::<(String, String)>()?;
        let room_id = RoomId::parse(room_id).ok()?;
        Some(Self {
            session_id,
            room_id,
        })
    }
}

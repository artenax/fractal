use std::{borrow::Cow, fmt};

use gettextrs::gettext;
use gio::{ApplicationFlags, Settings};
use glib::{clone, WeakRef};
use gtk::{gio, glib, prelude::*, subclass::prelude::*};
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
                    .and_downcast::<gio::SimpleAction>()
                    .unwrap()
                    .set_enabled(monitor.is_network_available());
            }));

            app.lookup_action("show-login")
                .and_downcast::<gio::SimpleAction>()
                .unwrap()
                .set_enabled(monitor.is_network_available());

            app.main_window().present();
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
            .property("application-id", Some(config::APP_ID))
            .property("flags", ApplicationFlags::default())
            .property("resource-base-path", Some("/org/gnome/Fractal/"))
            .build()
    }

    pub fn main_window(&self) -> Window {
        self.imp().window.upgrade().unwrap()
    }

    pub fn settings(&self) -> Settings {
        self.imp().settings.clone()
    }

    fn setup_gactions(&self) {
        self.add_action_entries([
            // Quit
            gio::ActionEntry::builder("quit")
                .activate(|app: &Application, _, _| {
                    // This is needed to trigger the delete event
                    // and saving the window state
                    app.main_window().close();
                    app.quit();
                })
                .build(),
            // About
            gio::ActionEntry::builder("about")
                .activate(|app: &Application, _, _| {
                    app.show_about_dialog();
                })
                .build(),
            gio::ActionEntry::builder("new-session")
                .activate(|app: &Application, _, _| {
                    app.main_window().switch_to_greeter_page();
                })
                .build(),
            gio::ActionEntry::builder("show-login")
                .activate(|app: &Application, _, _| {
                    app.main_window().switch_to_login_page();
                })
                .build(),
            gio::ActionEntry::builder("show-room")
                .parameter_type(Some(&AppShowRoomPayload::static_variant_type()))
                .activate(|app: &Application, _, v| {
                    if let Some(payload) = v.and_then(|v| v.get::<AppShowRoomPayload>()) {
                        app.main_window()
                            .show_room(&payload.session_id, &payload.room_id);
                    }
                })
                .build(),
        ]);

        let show_session_action = gio::SimpleAction::new("show-session", None);
        show_session_action.connect_activate(clone!(@weak self as app => move |_, _| {
            app.main_window().switch_to_session_page();
        }));
        self.add_action(&show_session_action);

        let win = self.main_window();
        let session_list = win.session_list();
        session_list.connect_is_empty_notify(
            clone!(@weak show_session_action => move |session_list| {
                show_session_action.set_enabled(!session_list.is_empty());
            }),
        );
        show_session_action.set_enabled(!session_list.is_empty());
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
            .transient_for(&self.main_window())
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

        dialog.present();
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
            .and_downcast::<Application>()
            .unwrap()
    }
}

/// The profile that was built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AppProfile {
    /// A stable release.
    Stable,
    /// A beta release.
    Beta,
    /// A nightly release.
    Devel,
    /// A development release.
    Hack,
}

impl AppProfile {
    /// The string representation of this `AppProfile`.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Devel => "devel",
            Self::Hack => "hack",
        }
    }

    /// Whether this `AppProfile` should use the `.devel` CSS class on windows.
    pub fn should_use_devel_class(&self) -> bool {
        matches!(self, Self::Devel | Self::Hack)
    }
}

impl fmt::Display for AppProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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

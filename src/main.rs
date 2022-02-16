#![doc(
    html_logo_url = "https://gitlab.gnome.org/GNOME/fractal/-/raw/fractal-next/data/icons/org.gnome.FractalNext.svg?inline=false"
)]

mod application;
#[rustfmt::skip]
mod config;
mod prelude;

mod components;
mod contrib;
mod greeter;
mod login;
mod login_advanced_dialog;
mod secret;
mod session;
mod user_facing_error;
mod utils;
mod window;

use config::{GETTEXT_PACKAGE, LOCALEDIR, RESOURCES_FILE};
use gettextrs::*;
use gtk::{gdk::Display, gio, IconTheme};
use once_cell::sync::Lazy;

use self::{
    application::Application, greeter::Greeter, login::Login, session::Session,
    user_facing_error::UserFacingError, window::Window,
};

/// The default tokio runtime to be used for async tasks
pub static RUNTIME: Lazy<tokio::runtime::Runtime> =
    Lazy::new(|| tokio::runtime::Runtime::new().unwrap());

fn main() {
    // Initialize logger, debug is carried out via debug!, info!, and warn!.
    tracing_subscriber::fmt::init();

    // Prepare i18n
    setlocale(LocaleCategory::LcAll, "");
    bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR).expect("Invalid argument passed to bindtextdomain");
    textdomain(GETTEXT_PACKAGE).expect("Invalid string passed to textdomain");

    gtk::glib::set_application_name("Fractal");

    gtk::init().expect("Unable to start GTK4");
    gst::init().expect("Failed to initialize gst");
    gst_gtk::plugin_register_static().expect("Failed to initialize gstreamer gtk plugins");

    let res = gio::Resource::load(RESOURCES_FILE).expect("Could not load gresource file");
    gio::resources_register(&res);

    IconTheme::for_display(&Display::default().unwrap())
        .add_resource_path("/org/gnome/FractalNext/icons");

    let app = Application::new();
    app.run();
}

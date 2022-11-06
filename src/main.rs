#![doc(
    html_logo_url = "https://gitlab.gnome.org/GNOME/fractal/-/raw/main/data/icons/org.gnome.Fractal.svg?inline=false",
    html_favicon_url = "https://gitlab.gnome.org/GNOME/fractal/-/raw/main/data/icons/org.gnome.Fractal-symbolic.svg?inline=false"
)]
#![allow(clippy::new_without_default)]

mod application;
#[rustfmt::skip]
mod config;
mod prelude;

mod account_switcher;
mod components;
mod contrib;
mod error_page;
mod greeter;
mod i18n;
mod login;
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
    application::Application, error_page::ErrorPage, greeter::Greeter, i18n::*, login::Login,
    session::Session, user_facing_error::UserFacingError, window::Window,
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
        .add_resource_path("/org/gnome/Fractal/icons");

    let app = Application::new();
    app.run();
}

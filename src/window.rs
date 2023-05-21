use std::cell::Cell;

use adw::subclass::prelude::AdwApplicationWindowImpl;
use gettextrs::gettext;
use glib::signal::Inhibit;
use gtk::{self, gdk, gio, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};
use log::{error, info, warn};
use ruma::RoomId;

use crate::{
    account_switcher::AccountSwitcher,
    components::Spinner,
    config::{APP_ID, PROFILE},
    secret::{self, SecretError, StoredSession},
    session::{AccountSettings, SessionState, SessionView},
    session_list::SessionList,
    spawn, spawn_tokio, toast,
    user_facing_error::UserFacingError,
    Application, ErrorPage, Greeter, Login, Session,
};

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/org/gnome/Fractal/window.ui")]
    pub struct Window {
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub loading: TemplateChild<gtk::WindowHandle>,
        #[template_child]
        pub greeter: TemplateChild<Greeter>,
        #[template_child]
        pub login: TemplateChild<Login>,
        #[template_child]
        pub error_page: TemplateChild<ErrorPage>,
        #[template_child]
        pub session: TemplateChild<SessionView>,
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub offline_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub spinner: TemplateChild<Spinner>,
        /// The list of logged-in sessions.
        pub session_list: SessionList,
        /// The selection of the logged-in sessions.
        ///
        /// The one that is selected being the one that is visible.
        pub session_selection: gtk::SingleSelection,
        pub account_switcher: AccountSwitcher,
        pub waiting_sessions: Cell<usize>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "Window";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.add_binding_action(
                gdk::Key::v,
                gdk::ModifierType::CONTROL_MASK,
                "win.paste",
                None,
            );
            klass.add_binding_action(
                gdk::Key::Insert,
                gdk::ModifierType::SHIFT_MASK,
                "win.paste",
                None,
            );
            klass.install_action("win.paste", None, move |obj, _, _| {
                obj.imp().session.handle_paste_action();
            });

            klass.install_action(
                "win.open-account-settings",
                Some("s"),
                move |obj, _, variant| {
                    if let Some(session_id) = variant.and_then(|v| v.get::<String>()) {
                        obj.open_account_settings(&session_id);
                    }
                },
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<SessionList>("session-list")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<gtk::SingleSelection>("session-selection")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session-list" => self.obj().session_list().to_value(),
                "session-selection" => self.obj().session_selection().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let builder = gtk::Builder::from_resource("/org/gnome/Fractal/shortcuts.ui");
            let shortcuts = builder.object("shortcuts").unwrap();
            obj.set_help_overlay(Some(&shortcuts));

            // Development Profile
            if PROFILE.should_use_devel_class() {
                obj.add_css_class("devel");
            }

            obj.load_window_size();

            // Ask for the toggle fullscreen state
            let fullscreen = gio::SimpleAction::new("toggle-fullscreen", None);
            fullscreen.connect_activate(clone!(@weak obj as window => move |_, _| {
                if window.is_fullscreened() {
                    window.unfullscreen();
                } else {
                    window.fullscreen();
                }
            }));
            obj.add_action(&fullscreen);

            self.main_stack.connect_visible_child_notify(
                clone!(@weak obj => move |_| obj.set_default_by_child()),
            );

            obj.set_default_by_child();

            self.session_selection.set_model(Some(&self.session_list));
            self.session_selection.set_autoselect(true);

            spawn!(clone!(@weak obj => async move {
                obj.restore_sessions().await;
            }));

            self.account_switcher
                .set_session_selection(Some(self.session_selection.clone()));

            let monitor = gio::NetworkMonitor::default();
            monitor.connect_network_changed(clone!(@weak obj => move |_, _| {
                obj.update_network_state();
            }));

            obj.update_network_state();
        }
    }

    impl WindowImpl for Window {
        // save window state on delete event
        fn close_request(&self) -> Inhibit {
            if let Err(err) = self.obj().save_window_size() {
                warn!("Failed to save window state, {}", &err);
            }
            if let Err(err) = self.obj().save_current_visible_session() {
                warn!("Failed to save current session: {err}");
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
        @extends gtk::Widget, gtk::Window, gtk::Root, gtk::ApplicationWindow, adw::ApplicationWindow, @implements gtk::Accessible, gio::ActionMap, gio::ActionGroup;
}

impl Window {
    pub fn new(app: &Application) -> Self {
        glib::Object::builder()
            .property("application", Some(app))
            .property("icon-name", Some(APP_ID))
            .build()
    }

    /// The list of logged-in sessions with a selection.
    ///
    /// The one that is selected being the one that is visible.
    pub fn session_list(&self) -> &SessionList {
        &self.imp().session_list
    }

    /// The selection of the logged-in sessions.
    ///
    /// The one that is selected being the one that is visible.
    pub fn session_selection(&self) -> &gtk::SingleSelection {
        &self.imp().session_selection
    }

    pub fn add_session(&self, session: &Session) {
        let imp = &self.imp();

        let index = imp.session_list.add(session.clone());
        let settings = Application::default().settings();
        let mut is_opened = false;
        if session.session_id() == settings.string("current-session") {
            imp.session_selection.set_selected(index as u32);
            is_opened = true;

            if session.state() == SessionState::Ready {
                imp.session.show_content();
            } else {
                session.connect_ready(clone!(@weak self as obj => move |_| {
                    obj.imp().session.show_content();
                }));
                self.switch_to_loading_page();
            }
        } else if imp.waiting_sessions.get() > 0 {
            imp.waiting_sessions.set(imp.waiting_sessions.get() - 1);
        }

        if imp.waiting_sessions.get() == 0 && !is_opened {
            imp.session_selection.set_selected(index as u32);

            if session.state() == SessionState::Ready {
                imp.session.show_content();
            } else {
                session.connect_ready(clone!(@weak self as obj => move |_| {
                    obj.imp().session.show_content();
                }));
                self.switch_to_loading_page();
            }
        }
        // We need to grab the focus so that keyboard shortcuts work
        imp.session.grab_focus();

        session.connect_logged_out(clone!(@weak self as obj => move |session| {
            obj.remove_session(session)
        }));
    }

    fn remove_session(&self, session: &Session) {
        let imp = self.imp();

        imp.session_list.remove(session.session_id());

        if imp.session_list.is_empty() {
            self.switch_to_greeter_page();
        }
    }

    pub async fn restore_sessions(&self) {
        let imp = self.imp();
        let handle = spawn_tokio!(secret::restore_sessions());
        match handle.await.unwrap() {
            Ok(sessions) => {
                if sessions.is_empty() {
                    self.switch_to_greeter_page();
                } else {
                    imp.waiting_sessions.set(sessions.len());
                    for stored_session in sessions {
                        info!(
                            "Restoring previous session for user: {}",
                            stored_session.user_id
                        );
                        if let Some(path) = stored_session.path.to_str() {
                            info!("Database path: {path}");
                        }

                        spawn!(
                            glib::PRIORITY_DEFAULT_IDLE,
                            clone!(@weak self as obj => async move {
                                obj.restore_stored_session(stored_session).await;
                            })
                        );
                    }
                }
            }
            Err(error) => match error {
                SecretError::OldVersion { item, session } => {
                    if session.version == 0 {
                        warn!("Found old session with sled store, removing…");
                        session.delete(Some(item), true).await
                    } else if session.version < 3 {
                        session.migrate_to_v3(item).await
                    }

                    // Restart.
                    spawn!(clone!(@weak self as obj => async move {
                        obj.restore_sessions().await;
                    }));
                }
                _ => {
                    error!("Failed to restore previous sessions: {error}");

                    let (message, item) = error.into_parts();
                    self.switch_to_error_page(
                        &format!(
                            "{}\n\n{}",
                            gettext("Failed to restore previous sessions"),
                            message,
                        ),
                        item,
                    );
                }
            },
        }
    }

    /// Restore a stored session.
    async fn restore_stored_session(&self, session_info: StoredSession) {
        match Session::restore(session_info).await {
            Ok(session) => {
                session.prepare().await;
                self.add_session(&session);
            }
            Err(error) => {
                warn!("Failed to restore previous login: {error}");
                toast!(self, error.to_user_facing());
            }
        }
    }

    /// The ID of the currently visible session, if any.
    pub fn current_session_id(&self) -> Option<String> {
        Some(
            self.imp()
                .session_selection
                .selected_item()
                .and_downcast::<Session>()?
                .session_id()
                .to_owned(),
        )
    }

    /// Set the current session by its ID.
    ///
    /// Returns `true` if the session was set as the current session.
    pub fn set_current_session_by_id(&self, session_id: &str) -> bool {
        let imp = self.imp();

        if let Some(index) = imp.session_list.index(session_id) {
            imp.session_selection.set_selected(index as u32);
        } else {
            return false;
        }

        self.switch_to_session_page();
        true
    }

    pub fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let settings = Application::default().settings();

        let size = self.default_size();

        settings.set_int("window-width", size.0)?;
        settings.set_int("window-height", size.1)?;

        settings.set_boolean("is-maximized", self.is_maximized())?;

        Ok(())
    }

    fn load_window_size(&self) {
        let settings = Application::default().settings();

        let width = settings.int("window-width");
        let height = settings.int("window-height");
        let is_maximized = settings.boolean("is-maximized");

        self.set_default_size(width, height);
        self.set_property("maximized", is_maximized);
    }

    /// Change the default widget of the window based on the visible child.
    ///
    /// These are the default widgets:
    /// - `Greeter` screen => `Login` button.
    fn set_default_by_child(&self) {
        let imp = self.imp();

        if imp.main_stack.visible_child() == Some(imp.greeter.get().upcast()) {
            self.set_default_widget(Some(&imp.greeter.default_widget()));
        } else {
            self.set_default_widget(gtk::Widget::NONE);
        }
    }

    pub fn switch_to_loading_page(&self) {
        let imp = self.imp();
        imp.main_stack.set_visible_child(&*imp.loading);
    }

    pub fn switch_to_session_page(&self) {
        let imp = self.imp();
        imp.main_stack.set_visible_child(&imp.session.get());
    }

    pub fn switch_to_login_page(&self) {
        let imp = self.imp();
        imp.main_stack.set_visible_child(&*imp.login);
        imp.login.focus_default();
    }

    pub fn switch_to_greeter_page(&self) {
        let imp = self.imp();
        imp.main_stack.set_visible_child(&*imp.greeter);
    }

    pub fn switch_to_error_page(&self, message: &str, item: Option<oo7::Item>) {
        let imp = self.imp();
        imp.error_page.display_secret_error(message, item);
        imp.main_stack.set_visible_child(&*imp.error_page);
    }

    /// This appends a new toast to the list
    pub fn add_toast(&self, toast: adw::Toast) {
        self.imp().toast_overlay.add_toast(toast);
    }

    pub fn account_switcher(&self) -> &AccountSwitcher {
        &self.imp().account_switcher
    }

    /// The `SessionView` of this window.
    pub fn session_view(&self) -> &SessionView {
        &self.imp().session
    }

    fn update_network_state(&self) {
        let imp = self.imp();
        let monitor = gio::NetworkMonitor::default();

        if !monitor.is_network_available() {
            imp.offline_banner
                .set_title(&gettext("No network connection"));
            imp.offline_banner.set_revealed(true);
        } else if monitor.connectivity() < gio::NetworkConnectivity::Full {
            imp.offline_banner
                .set_title(&gettext("No Internet connection"));
            imp.offline_banner.set_revealed(true);
        } else {
            imp.offline_banner.set_revealed(false);
        }
    }

    /// Show the given room for the given session.
    pub fn show_room(&self, session_id: &str, room_id: &RoomId) {
        if self.set_current_session_by_id(session_id) {
            self.imp().session.select_room_by_id(room_id);

            self.present();
        }
    }

    pub fn save_current_visible_session(&self) -> Result<(), glib::BoolError> {
        let settings = Application::default().settings();

        settings.set_string(
            "current-session",
            self.current_session_id().unwrap_or_default().as_str(),
        )?;

        Ok(())
    }

    /// Open the account settings for the session with the given ID.
    pub fn open_account_settings(&self, session_id: &str) {
        let Some(session) = self.session_list().get(session_id) else {
            error!("Tried to open account settings of unknown session with ID '{session_id}'");
            return;
        };

        let window = AccountSettings::new(Some(self), &session);
        window.present();
    }
}

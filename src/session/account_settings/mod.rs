use adw::{prelude::*, subclass::prelude::*};
use gtk::{
    glib,
    glib::{clone, FromVariant},
    CompositeTemplate,
};

mod devices_page;
mod notifications_page;
mod security_page;
mod user_page;

use self::{
    devices_page::DevicesPage, notifications_page::NotificationsPage, security_page::SecurityPage,
    user_page::UserPage,
};
use super::Session;

mod imp {
    use std::cell::RefCell;

    use glib::{subclass::InitializingObject, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/account-settings.ui")]
    pub struct AccountSettings {
        pub session: WeakRef<Session>,
        pub session_handler: RefCell<Option<glib::SignalHandlerId>>,
        #[template_child]
        pub user_page: TemplateChild<UserPage>,
        #[template_child]
        pub security_page: TemplateChild<SecurityPage>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AccountSettings {
        const NAME: &'static str = "AccountSettings";
        type Type = super::AccountSettings;
        type ParentType = adw::PreferencesWindow;

        fn class_init(klass: &mut Self::Class) {
            DevicesPage::static_type();
            UserPage::static_type();
            NotificationsPage::static_type();
            SecurityPage::static_type();
            Self::bind_template(klass);

            klass.install_action("account-settings.close", None, |obj, _, _| {
                obj.close();
            });

            klass.install_action("account-settings.logout", None, |obj, _, _| {
                obj.imp().user_page.show_log_out_page();
            });

            klass.install_action("account-settings.export_keys", None, |obj, _, _| {
                obj.imp().security_page.show_export_keys_page();
            });

            klass.install_action("win.add-toast", Some("s"), |obj, _, message| {
                if let Some(message) = message.and_then(String::from_variant) {
                    let toast = adw::Toast::new(&message);
                    obj.add_toast(toast);
                }
            });

            klass.install_action("win.close-subpage", None, |obj, _, _| {
                obj.close_subpage();
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AccountSettings {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Session>("session")
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "session" => self.obj().set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session" => self.obj().session().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            if let Some(session) = self.session.upgrade() {
                if let Some(handler) = self.session_handler.take() {
                    session.disconnect(handler);
                }
            }
        }
    }

    impl WidgetImpl for AccountSettings {}
    impl WindowImpl for AccountSettings {}
    impl AdwWindowImpl for AccountSettings {}
    impl PreferencesWindowImpl for AccountSettings {}
}

glib::wrapper! {
    /// Preference Window to display and update room details.
    pub struct AccountSettings(ObjectSubclass<imp::AccountSettings>)
        @extends gtk::Widget, gtk::Window, adw::Window, adw::PreferencesWindow, @implements gtk::Accessible;
}

impl AccountSettings {
    pub fn new(parent_window: Option<&impl IsA<gtk::Window>>, session: &Session) -> Self {
        glib::Object::builder()
            .property("transient-for", &parent_window)
            .property("session", session)
            .build()
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Set the current session.
    pub fn set_session(&self, session: Option<Session>) {
        let prev_session = self.session();
        if prev_session == session {
            return;
        }

        let imp = self.imp();
        if let Some(session) = prev_session {
            if let Some(handler) = imp.session_handler.take() {
                session.disconnect(handler);
            }
        }

        if let Some(session) = &session {
            imp.session_handler.replace(Some(session.connect_logged_out(
                clone!(@weak self as obj => move |_| {
                    obj.close();
                }),
            )));
        }

        self.imp().session.set(session.as_ref());
        self.notify("session");
    }
}

use adw::{prelude::*, subclass::prelude::*};
use gtk::{glib, glib::FromVariant, subclass::prelude::*, CompositeTemplate};

mod devices_page;
mod user_page;
use devices_page::DevicesPage;
use user_page::UserPage;

use super::Session;

mod imp {
    use std::cell::RefCell;

    use glib::{subclass::InitializingObject, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/account-settings.ui")]
    pub struct AccountSettings {
        pub session: RefCell<Option<WeakRef<Session>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AccountSettings {
        const NAME: &'static str = "AccountSettings";
        type Type = super::AccountSettings;
        type ParentType = adw::PreferencesWindow;

        fn class_init(klass: &mut Self::Class) {
            DevicesPage::static_type();
            UserPage::static_type();
            Self::bind_template(klass);

            klass.install_action("account-settings.close", None, |obj, _, _| {
                obj.close();
            });

            klass.install_action("win.add-toast", Some("s"), |obj, _, message| {
                if let Some(message) = message.and_then(String::from_variant) {
                    let toast = adw::Toast::new(&message);
                    obj.add_toast(&toast);
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
                vec![glib::ParamSpecObject::new(
                    "session",
                    "Session",
                    "The session",
                    Session::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "session" => obj.set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session" => obj.session().to_value(),
                _ => unimplemented!(),
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
        glib::Object::new(&[("transient-for", &parent_window), ("session", session)])
            .expect("Failed to create AccountSettings")
    }

    pub fn session(&self) -> Option<Session> {
        self.imp()
            .session
            .borrow()
            .clone()
            .and_then(|session| session.upgrade())
    }

    pub fn set_session(&self, session: Option<Session>) {
        if self.session() == session {
            return;
        }

        self.imp()
            .session
            .replace(session.map(|session| session.downgrade()));
        self.notify("session");
    }
}

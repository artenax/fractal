use adw::{prelude::*, subclass::prelude::*};
use gtk::{glib, CompositeTemplate};

use crate::{components::ButtonRow, session::Session};

mod import_export_keys_subpage;
use import_export_keys_subpage::{ImportExportKeysSubpage, KeysSubpageMode};

mod imp {
    use glib::{subclass::InitializingObject, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/account-settings-security-page.ui")]
    pub struct SecurityPage {
        pub session: WeakRef<Session>,
        #[template_child]
        pub import_export_keys_subpage: TemplateChild<ImportExportKeysSubpage>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SecurityPage {
        const NAME: &'static str = "SecurityPage";
        type Type = super::SecurityPage;
        type ParentType = adw::PreferencesPage;

        fn class_init(klass: &mut Self::Class) {
            ButtonRow::static_type();
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SecurityPage {
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

    impl WidgetImpl for SecurityPage {}
    impl PreferencesPageImpl for SecurityPage {}
}

glib::wrapper! {
    /// Security settings page.
    pub struct SecurityPage(ObjectSubclass<imp::SecurityPage>)
        @extends gtk::Widget, adw::PreferencesPage, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl SecurityPage {
    pub fn new(parent_window: &Option<gtk::Window>, session: &Session) -> Self {
        glib::Object::new(&[("transient-for", parent_window), ("session", session)])
            .expect("Failed to create SecurityPage")
    }

    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    pub fn set_session(&self, session: Option<Session>) {
        if self.session() == session {
            return;
        }

        self.imp().session.set(session.as_ref());
        self.notify("session");
    }

    #[template_callback]
    fn handle_export_keys(&self) {
        let subpage = &*self.imp().import_export_keys_subpage;
        subpage.set_mode(KeysSubpageMode::Export);
        self.root()
            .as_ref()
            .and_then(|root| root.downcast_ref::<adw::PreferencesWindow>())
            .unwrap()
            .present_subpage(subpage);
    }

    #[template_callback]
    fn handle_import_keys(&self) {
        let subpage = &*self.imp().import_export_keys_subpage;
        subpage.set_mode(KeysSubpageMode::Import);
        self.root()
            .as_ref()
            .and_then(|root| root.downcast_ref::<adw::PreferencesWindow>())
            .unwrap()
            .present_subpage(subpage);
    }
}

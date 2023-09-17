use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{glib, glib::clone, CompositeTemplate};

use crate::{components::ButtonRow, session::model::Session, spawn, spawn_tokio};

mod import_export_keys_subpage;
use import_export_keys_subpage::{ImportExportKeysSubpage, KeysSubpageMode};

mod imp {
    use glib::{subclass::InitializingObject, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/account_settings/security_page/mod.ui"
    )]
    pub struct SecurityPage {
        pub session: WeakRef<Session>,
        #[template_child]
        pub import_export_keys_subpage: TemplateChild<ImportExportKeysSubpage>,
        #[template_child]
        pub master_key_status: TemplateChild<gtk::Label>,
        #[template_child]
        pub self_signing_key_status: TemplateChild<gtk::Label>,
        #[template_child]
        pub user_signing_key_status: TemplateChild<gtk::Label>,
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
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Set the current session.
    pub fn set_session(&self, session: Option<Session>) {
        if self.session() == session {
            return;
        }

        self.imp().session.set(session.as_ref());
        self.notify("session");

        spawn!(clone!(@weak self as obj => async move {
            obj.load_cross_signing_status().await;
        }));
    }

    #[template_callback]
    pub fn show_export_keys_page(&self) {
        let subpage = &*self.imp().import_export_keys_subpage;
        subpage.set_mode(KeysSubpageMode::Export);
        self.root()
            .and_downcast_ref::<adw::PreferencesWindow>()
            .unwrap()
            .push_subpage(subpage);
    }

    #[template_callback]
    fn handle_import_keys(&self) {
        let subpage = &*self.imp().import_export_keys_subpage;
        subpage.set_mode(KeysSubpageMode::Import);
        self.root()
            .and_downcast_ref::<adw::PreferencesWindow>()
            .unwrap()
            .push_subpage(subpage);
    }

    async fn load_cross_signing_status(&self) {
        let Some(session) = self.session() else {
            return;
        };
        let client = session.client();

        let cross_signing_status =
            spawn_tokio!(async move { client.encryption().cross_signing_status().await })
                .await
                .unwrap();

        let imp = self.imp();
        update_cross_signing_key_status(
            &imp.master_key_status,
            cross_signing_status
                .as_ref()
                .map(|s| s.has_master)
                .unwrap_or_default(),
        );
        update_cross_signing_key_status(
            &imp.self_signing_key_status,
            cross_signing_status
                .as_ref()
                .map(|s| s.has_self_signing)
                .unwrap_or_default(),
        );
        update_cross_signing_key_status(
            &imp.user_signing_key_status,
            cross_signing_status
                .as_ref()
                .map(|s| s.has_user_signing)
                .unwrap_or_default(),
        );
    }
}

fn update_cross_signing_key_status(label: &gtk::Label, available: bool) {
    if available {
        label.add_css_class("success");
        label.remove_css_class("error");
        label.set_text(&gettext("Available"));
    } else {
        label.add_css_class("error");
        label.remove_css_class("success");
        label.set_text(&gettext("Not available"));
    }
}

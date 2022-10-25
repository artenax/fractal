use adw::subclass::prelude::BinImpl;
use gtk::{self, glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use super::avatar_with_selection::AvatarWithSelection;
use crate::session::Session;

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/user-entry-row.ui")]
    pub struct UserEntryRow {
        #[template_child]
        pub account_avatar: TemplateChild<AvatarWithSelection>,
        #[template_child]
        pub display_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub user_id: TemplateChild<gtk::Label>,
        pub session: glib::WeakRef<Session>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for UserEntryRow {
        const NAME: &'static str = "UserEntryRow";
        type Type = super::UserEntryRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for UserEntryRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::new(
                        "session",
                        "Session",
                        "The session this entry represents",
                        Session::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecBoolean::new(
                        "selected",
                        "Selected",
                        "Whether this session is selected",
                        false,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.set_session(value.get().unwrap()),
                "selected" => obj.set_selected(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.session().to_value(),
                "selected" => obj.is_selected().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for UserEntryRow {}
    impl BinImpl for UserEntryRow {}
    impl ListBoxRowImpl for UserEntryRow {}
}

glib::wrapper! {
    pub struct UserEntryRow(ObjectSubclass<imp::UserEntryRow>)
        @extends gtk::Widget, gtk::ListBoxRow, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl UserEntryRow {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    pub fn set_selected(&self, selected: bool) {
        let priv_ = self.imp();

        if priv_.account_avatar.is_selected() == selected {
            return;
        }

        priv_.account_avatar.set_selected(selected);

        if selected {
            priv_.display_name.add_css_class("bold");
        } else {
            priv_.display_name.remove_css_class("bold");
        }

        self.notify("selected");
    }

    pub fn is_selected(&self) -> bool {
        self.imp().account_avatar.is_selected()
    }

    #[template_callback]
    pub fn show_account_settings(&self) {
        if let Some(session) = self.session() {
            self.activate_action("account-switcher.close", None)
                .unwrap();
            session
                .activate_action("session.open-account-settings", None)
                .unwrap();
        }
    }

    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    pub fn set_session(&self, session: Option<&Session>) {
        self.imp().session.set(session);
    }
}

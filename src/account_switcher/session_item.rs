use gtk::{self, glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use super::avatar_with_selection::AvatarWithSelection;
use crate::session::model::Session;

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/account_switcher/session_item.ui")]
    pub struct SessionItemRow {
        #[template_child]
        pub avatar: TemplateChild<AvatarWithSelection>,
        #[template_child]
        pub display_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub user_id: TemplateChild<gtk::Label>,
        /// The session this item represents.
        pub session: glib::WeakRef<Session>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SessionItemRow {
        const NAME: &'static str = "SessionItemRow";
        type Type = super::SessionItemRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SessionItemRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Session>("session")
                        .construct_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("selected")
                        .explicit_notify()
                        .build(),
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

    impl WidgetImpl for SessionItemRow {}
    impl ListBoxRowImpl for SessionItemRow {}
}

glib::wrapper! {
    /// A `GtkListBoxRow` representing a logged-in session.
    pub struct SessionItemRow(ObjectSubclass<imp::SessionItemRow>)
        @extends gtk::Widget, gtk::ListBoxRow, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl SessionItemRow {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// Set whether this session is selected.
    pub fn set_selected(&self, selected: bool) {
        let imp = self.imp();

        if imp.avatar.is_selected() == selected {
            return;
        }

        imp.avatar.set_selected(selected);

        if selected {
            imp.display_name.add_css_class("bold");
        } else {
            imp.display_name.remove_css_class("bold");
        }

        self.notify("selected");
    }

    /// Whether this session is selected.
    pub fn is_selected(&self) -> bool {
        self.imp().avatar.is_selected()
    }

    #[template_callback]
    pub fn show_account_settings(&self) {
        let Some(session) = self.session() else {
            return;
        };

        self.activate_action("account-switcher.close", None)
            .unwrap();
        self.activate_action(
            "win.open-account-settings",
            Some(&session.session_id().to_variant()),
        )
        .unwrap();
    }

    /// The session this item represents.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Set the session this item represents.
    pub fn set_session(&self, session: Option<&Session>) {
        self.imp().session.set(session);
    }
}

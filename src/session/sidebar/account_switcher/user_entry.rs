use adw::subclass::prelude::BinImpl;
use gtk::{self, glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use super::avatar_with_selection::AvatarWithSelection;
use crate::session::Session;

mod imp {
    use std::cell::RefCell;

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
        pub session_page: RefCell<Option<gtk::StackPage>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for UserEntryRow {
        const NAME: &'static str = "UserEntryRow";
        type Type = super::UserEntryRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            AvatarWithSelection::static_type();
            Self::bind_template(klass);

            klass.install_action(
                "user-entry-row.open-account-settings",
                None,
                move |item, _, _| {
                    item.activate_action("account-switcher.close", None)
                        .unwrap();
                    item.show_account_settings();
                },
            );
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
                        "session-page",
                        "Session StackPage",
                        "The stack page of the session that this entry represents",
                        gtk::StackPage::static_type(),
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "hint",
                        "Selection hint",
                        "The hint of the session that owns the account switcher which this entry belongs to",
                        false,
                        glib::ParamFlags::WRITABLE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                ]
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
                "session-page" => {
                    let session_page = value.get().unwrap();
                    self.session_page.replace(Some(session_page));
                }
                "hint" => obj.set_hint(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session-page" => self.session_page.borrow().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for UserEntryRow {}
    impl BinImpl for UserEntryRow {}
}

glib::wrapper! {
    pub struct UserEntryRow(ObjectSubclass<imp::UserEntryRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl UserEntryRow {
    pub fn new(session_page: &gtk::StackPage) -> Self {
        glib::Object::new(&[("session-page", session_page)]).expect("Failed to create UserEntryRow")
    }

    pub fn set_hint(&self, hinted: bool) {
        let priv_ = self.imp();

        priv_.account_avatar.set_selected(hinted);
        priv_
            .display_name
            .set_css_classes(if hinted { &["bold"] } else { &[] });
    }

    pub fn show_account_settings(&self) {
        let session = self
            .imp()
            .session_page
            .borrow()
            .as_ref()
            .map(|widget| widget.child())
            .unwrap()
            .downcast::<Session>()
            .unwrap();

        session
            .activate_action("session.open-account-settings", None)
            .unwrap();
    }
}

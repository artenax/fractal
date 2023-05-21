use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use super::DmUser;

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/session/view/create_dm_dialog/dm_user_row.ui")]
    pub struct DmUserRow {
        pub user: RefCell<Option<DmUser>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DmUserRow {
        const NAME: &'static str = "CreateDmDialogUserRow";
        type Type = super::DmUserRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DmUserRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<DmUser>("user")
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "user" => self.obj().set_user(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "user" => self.obj().user().to_value(),
                _ => unimplemented!(),
            }
        }
    }
    impl WidgetImpl for DmUserRow {}
    impl ListBoxRowImpl for DmUserRow {}
}

glib::wrapper! {
    pub struct DmUserRow(ObjectSubclass<imp::DmUserRow>)
        @extends gtk::Widget, gtk::ListBoxRow, @implements gtk::Accessible;
}

impl DmUserRow {
    pub fn new(user: &DmUser) -> Self {
        glib::Object::builder().property("user", user).build()
    }

    /// The user displayed by this row.
    pub fn user(&self) -> Option<DmUser> {
        self.imp().user.borrow().clone()
    }

    /// Set the user displayed by this row.
    pub fn set_user(&self, user: Option<DmUser>) {
        let imp = self.imp();
        let prev_user = self.user();

        if prev_user == user {
            return;
        }

        imp.user.replace(user);
        self.notify("user");
    }
}

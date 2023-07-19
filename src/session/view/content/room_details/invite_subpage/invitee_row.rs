use adw::subclass::prelude::BinImpl;
use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use super::Invitee;

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;
    use crate::utils::template_callbacks::TemplateCallbacks;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_details/invite_subpage/invitee_row.ui"
    )]
    pub struct InviteeRow {
        pub user: RefCell<Option<Invitee>>,
        pub binding: RefCell<Option<glib::Binding>>,
        #[template_child]
        pub check_button: TemplateChild<gtk::CheckButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for InviteeRow {
        const NAME: &'static str = "ContentInviteInviteeRow";
        type Type = super::InviteeRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            TemplateCallbacks::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for InviteeRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Invitee>("user")
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "user" => {
                    self.obj().set_user(value.get().unwrap());
                }
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
    impl WidgetImpl for InviteeRow {}
    impl BinImpl for InviteeRow {}
}

glib::wrapper! {
    pub struct InviteeRow(ObjectSubclass<imp::InviteeRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl InviteeRow {
    pub fn new(user: &Invitee) -> Self {
        glib::Object::builder().property("user", user).build()
    }

    /// The user displayed by this row.
    pub fn user(&self) -> Option<Invitee> {
        self.imp().user.borrow().clone()
    }

    /// Set the user displayed by this row.
    pub fn set_user(&self, user: Option<Invitee>) {
        let imp = self.imp();

        if self.user() == user {
            return;
        }

        if let Some(binding) = imp.binding.take() {
            binding.unbind();
        }

        if let Some(ref user) = user {
            // We can't use `gtk::Expression` because we need a bidirectional binding
            let binding = user
                .bind_property("invited", &*imp.check_button, "active")
                .sync_create()
                .bidirectional()
                .build();

            imp.binding.replace(Some(binding));
        }

        imp.user.replace(user);
        self.notify("user");
    }
}

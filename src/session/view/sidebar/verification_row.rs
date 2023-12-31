use adw::subclass::prelude::BinImpl;
use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::session::model::IdentityVerification;

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/session/view/sidebar/verification_row.ui")]
    pub struct VerificationRow {
        pub verification: RefCell<Option<IdentityVerification>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VerificationRow {
        const NAME: &'static str = "SidebarVerificationRow";
        type Type = super::VerificationRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VerificationRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<IdentityVerification>(
                    "identity-verification",
                )
                .explicit_notify()
                .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "identity-verification" => {
                    self.obj().set_identity_verification(value.get().unwrap())
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "identity-verification" => self.obj().identity_verification().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for VerificationRow {}
    impl BinImpl for VerificationRow {}
}

glib::wrapper! {
    pub struct VerificationRow(ObjectSubclass<imp::VerificationRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl VerificationRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The identity verification represented by this row.
    pub fn identity_verification(&self) -> Option<IdentityVerification> {
        self.imp().verification.borrow().clone()
    }

    /// Set the identity verification represented by this row.
    pub fn set_identity_verification(&self, verification: Option<IdentityVerification>) {
        if self.identity_verification() == verification {
            return;
        }

        self.imp().verification.replace(verification);
        self.notify("identity-verification");
    }
}

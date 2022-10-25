use adw::subclass::prelude::BinImpl;
use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::session::verification::IdentityVerification;

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/sidebar-verification-row.ui")]
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
                vec![glib::ParamSpecObject::new(
                    "identity-verification",
                    "Identity Verification",
                    "The identity verification of this row",
                    IdentityVerification::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                )]
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
        glib::Object::new(&[])
    }

    pub fn identity_verification(&self) -> Option<IdentityVerification> {
        self.imp().verification.borrow().clone()
    }

    pub fn set_identity_verification(&self, verification: Option<IdentityVerification>) {
        if self.identity_verification() == verification {
            return;
        }

        self.imp().verification.replace(verification);
        self.notify("identity-verification");
    }
}

impl Default for VerificationRow {
    fn default() -> Self {
        Self::new()
    }
}

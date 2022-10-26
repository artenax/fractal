use gtk::{
    gio, glib,
    glib::{prelude::*, subclass::prelude::*},
};

use crate::session::room::Membership;

mod imp {
    use std::cell::Cell;

    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct MembershipSubpageItem {
        pub state: Cell<Membership>,
        pub model: OnceCell<gio::ListModel>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MembershipSubpageItem {
        const NAME: &'static str = "ContentMemberPageMembershipSubpageItem";
        type Type = super::MembershipSubpageItem;
    }

    impl ObjectImpl for MembershipSubpageItem {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecEnum::builder("state", Membership::default())
                        .construct_only()
                        .build(),
                    glib::ParamSpecObject::builder::<gio::ListModel>("model")
                        .construct_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "state" => obj.set_state(value.get().unwrap()),
                "model" => obj.set_model(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "state" => obj.state().to_value(),
                "model" => obj.model().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct MembershipSubpageItem(ObjectSubclass<imp::MembershipSubpageItem>);
}

impl MembershipSubpageItem {
    pub fn new(state: Membership, model: &impl IsA<gio::ListModel>) -> Self {
        glib::Object::builder()
            .property("state", &state)
            .property("model", model)
            .build()
    }

    /// The membership state this list contains.
    pub fn state(&self) -> Membership {
        self.imp().state.get()
    }

    /// Set the membership state this list contains.
    fn set_state(&self, state: Membership) {
        self.imp().state.set(state);
    }

    /// The model used for this subpage.
    pub fn model(&self) -> &gio::ListModel {
        self.imp().model.get().unwrap()
    }

    /// Set the model used for this subpage.
    fn set_model(&self, model: gio::ListModel) {
        self.imp().model.set(model).unwrap();
    }
}

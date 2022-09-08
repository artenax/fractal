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
                    glib::ParamSpecEnum::new(
                        "state",
                        "State",
                        "The membership state this list contains",
                        Membership::static_type(),
                        Membership::default() as i32,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecObject::new(
                        "model",
                        "Model",
                        "The model used for this subview",
                        gio::ListModel::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
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
                "state" => obj.set_state(value.get().unwrap()),
                "model" => obj.set_model(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
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
        glib::Object::new(&[("state", &state), ("model", model)])
            .expect("Failed to create MembershipSubpageItem")
    }

    pub fn state(&self) -> Membership {
        self.imp().state.get()
    }

    fn set_state(&self, state: Membership) {
        self.imp().state.set(state);
    }

    pub fn model(&self) -> &gio::ListModel {
        self.imp().model.get().unwrap()
    }

    fn set_model(&self, model: gio::ListModel) {
        self.imp().model.set(model).unwrap();
    }
}

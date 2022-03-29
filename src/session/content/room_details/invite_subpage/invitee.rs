use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk::ruma::{MxcUri, UserId};

use crate::session::{user::UserExt, Session, User};

mod imp {
    use std::cell::{Cell, RefCell};

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Invitee {
        pub invited: Cell<bool>,
        pub anchor: RefCell<Option<gtk::TextChildAnchor>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Invitee {
        const NAME: &'static str = "Invitee";
        type Type = super::Invitee;
        type ParentType = User;
    }

    impl ObjectImpl for Invitee {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::new(
                        "invited",
                        "Invited",
                        "Whether this Invitee is actually invited",
                        false,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecObject::new(
                        "anchor",
                        "Anchor",
                        "The anchor location in the text buffer",
                        gtk::TextChildAnchor::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
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
                "invited" => obj.set_invited(value.get().unwrap()),
                "anchor" => obj.set_anchor(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "invited" => obj.is_invited().to_value(),
                "anchor" => obj.anchor().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// A User in the context of a given room.
    pub struct Invitee(ObjectSubclass<imp::Invitee>) @extends User;
}

impl Invitee {
    pub fn new(
        session: &Session,
        user_id: &UserId,
        display_name: Option<&str>,
        avatar_url: Option<&MxcUri>,
    ) -> Self {
        let obj: Self = glib::Object::new(&[
            ("session", session),
            ("user-id", &user_id.as_str()),
            ("display-name", &display_name),
        ])
        .expect("Failed to create Invitee");
        // FIXME: we should make the avatar_url settable as property
        obj.set_avatar_url(avatar_url.map(std::borrow::ToOwned::to_owned));
        obj
    }

    pub fn is_invited(&self) -> bool {
        self.imp().invited.get()
    }

    pub fn set_invited(&self, invited: bool) {
        if self.is_invited() == invited {
            return;
        }

        self.imp().invited.set(invited);
        self.notify("invited");
    }

    pub fn anchor(&self) -> Option<gtk::TextChildAnchor> {
        self.imp().anchor.borrow().clone()
    }

    pub fn take_anchor(&self) -> Option<gtk::TextChildAnchor> {
        let anchor = self.imp().anchor.take();
        self.notify("anchor");
        anchor
    }

    pub fn set_anchor(&self, anchor: Option<gtk::TextChildAnchor>) {
        if self.anchor() == anchor {
            return;
        }

        self.imp().anchor.replace(anchor);
        self.notify("anchor");
    }
}

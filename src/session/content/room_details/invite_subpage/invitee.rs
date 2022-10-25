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
        pub invite_exception: RefCell<Option<String>>,
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
                    glib::ParamSpecString::new(
                        "invite-exception",
                        "Invite Exception",
                        "The reason the user can't be invited",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "invited" => obj.set_invited(value.get().unwrap()),
                "anchor" => obj.set_anchor(value.get().unwrap()),
                "invite-exception" => obj.set_invite_exception(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "invited" => obj.is_invited().to_value(),
                "anchor" => obj.anchor().to_value(),
                "invite-exception" => obj.invite_exception().to_value(),
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
        let obj: Self = glib::Object::builder()
            .property("session", session)
            .property("user-id", &user_id.as_str())
            .property("display-name", &display_name)
            .build();
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

    pub fn invite_exception(&self) -> Option<String> {
        self.imp().invite_exception.borrow().clone()
    }

    pub fn set_invite_exception(&self, exception: Option<String>) {
        if exception == self.invite_exception() {
            return;
        }

        self.imp().invite_exception.replace(exception);
        self.notify("invite-exception");
    }
}

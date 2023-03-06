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
                    glib::ParamSpecBoolean::builder("invited")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<gtk::TextChildAnchor>("anchor")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecString::builder("invite-exception")
                        .explicit_notify()
                        .build(),
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
            .property("user-id", user_id.as_str())
            .property("display-name", display_name)
            .build();
        // FIXME: we should make the avatar_url settable as property
        obj.set_avatar_url(avatar_url.map(std::borrow::ToOwned::to_owned));
        obj
    }

    /// Whether this user is invited.
    pub fn is_invited(&self) -> bool {
        self.imp().invited.get()
    }

    /// Set whether this user is invited.
    pub fn set_invited(&self, invited: bool) {
        if self.is_invited() == invited {
            return;
        }

        self.imp().invited.set(invited);
        self.notify("invited");
    }

    /// The anchor for this user in the text buffer.
    pub fn anchor(&self) -> Option<gtk::TextChildAnchor> {
        self.imp().anchor.borrow().clone()
    }

    /// Take the anchor for this user in the text buffer.
    ///
    /// The anchor will be `None` after calling this method.
    pub fn take_anchor(&self) -> Option<gtk::TextChildAnchor> {
        let anchor = self.imp().anchor.take();
        self.notify("anchor");
        anchor
    }

    /// Set the anchor for this user in the text buffer.
    pub fn set_anchor(&self, anchor: Option<gtk::TextChildAnchor>) {
        if self.anchor() == anchor {
            return;
        }

        self.imp().anchor.replace(anchor);
        self.notify("anchor");
    }

    /// The reason the user can't be invited.
    pub fn invite_exception(&self) -> Option<String> {
        self.imp().invite_exception.borrow().clone()
    }

    /// Set the reason the user can't be invited.
    pub fn set_invite_exception(&self, exception: Option<String>) {
        if exception == self.invite_exception() {
            return;
        }

        self.imp().invite_exception.replace(exception);
        self.notify("invite-exception");
    }
}

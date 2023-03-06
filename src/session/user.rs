use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*};
use log::error;
use matrix_sdk::{
    encryption::identities::UserIdentity,
    ruma::{OwnedMxcUri, OwnedUserId, UserId},
};

use crate::{
    components::Pill,
    session::{
        verification::{IdentityVerification, VerificationState},
        Avatar, Session,
    },
    spawn, spawn_tokio,
};

#[glib::flags(name = "UserActions")]
pub enum UserActions {
    VERIFY = 0b00000001,
}

impl Default for UserActions {
    fn default() -> Self {
        Self::empty()
    }
}

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::object::WeakRef;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct User {
        pub user_id: OnceCell<OwnedUserId>,
        pub display_name: RefCell<Option<String>>,
        pub session: WeakRef<Session>,
        pub avatar: OnceCell<Avatar>,
        pub is_verified: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for User {
        const NAME: &'static str = "User";
        type Type = super::User;
    }

    impl ObjectImpl for User {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::builder("user-id")
                        .construct_only()
                        .build(),
                    glib::ParamSpecString::builder("display-name")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<Avatar>("avatar")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<Session>("session")
                        .construct_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("verified")
                        .read_only()
                        .build(),
                    glib::ParamSpecFlags::builder::<UserActions>("allowed-actions")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "user-id" => {
                    self.user_id
                        .set(UserId::parse(value.get::<&str>().unwrap()).unwrap())
                        .unwrap();
                }
                "display-name" => {
                    self.obj().set_display_name(value.get().unwrap());
                }
                "session" => self.session.set(value.get().ok().as_ref()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "display-name" => obj.display_name().to_value(),
                "user-id" => obj.user_id().as_str().to_value(),
                "session" => obj.session().to_value(),
                "avatar" => obj.avatar().to_value(),
                "verified" => obj.is_verified().to_value(),
                "allowed-actions" => obj.allowed_actions().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let avatar = Avatar::new(&obj.session(), None);
            self.avatar.set(avatar).unwrap();

            obj.bind_property("display-name", obj.avatar(), "display-name")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build();

            obj.init_is_verified();
        }
    }
}

glib::wrapper! {
    /// `glib::Object` representation of a Matrix user.
    pub struct User(ObjectSubclass<imp::User>);
}

impl User {
    pub fn new(session: &Session, user_id: &UserId) -> Self {
        glib::Object::builder()
            .property("session", session)
            .property("user-id", user_id.as_str())
            .build()
    }

    pub async fn crypto_identity(&self) -> Option<UserIdentity> {
        let encryption = self.session().client().encryption();
        let user_id = self.user_id();
        let handle = spawn_tokio!(async move { encryption.get_user_identity(&user_id).await });

        match handle.await.unwrap() {
            Ok(identity) => identity,
            Err(error) => {
                error!("Failed to find crypto identity: {}", error);
                None
            }
        }
    }

    pub async fn verify_identity(&self) -> IdentityVerification {
        let request = IdentityVerification::create(&self.session(), Some(self)).await;
        self.session().verification_list().add(request.clone());
        // FIXME: actually listen to room events to get updates for verification state
        request.connect_notify_local(
            Some("state"),
            clone!(@weak self as obj => move |request,_| {
                if request.state() == VerificationState::Completed {
                    obj.init_is_verified();
                }
            }),
        );
        request
    }

    /// Whether this user has been verified.
    pub fn is_verified(&self) -> bool {
        self.imp().is_verified.get()
    }

    fn init_is_verified(&self) {
        spawn!(clone!(@weak self as obj => async move {
            let is_verified = obj.crypto_identity().await.map_or(false, |i| i.is_verified());

            if is_verified == obj.is_verified() {
                return;
            }

            obj.imp().is_verified.set(is_verified);
            obj.notify("verified");
            obj.notify("allowed-actions");
        }));
    }
}

pub trait UserExt: IsA<User> {
    /// The current session.
    fn session(&self) -> Session {
        self.upcast_ref().imp().session.upgrade().unwrap()
    }

    /// The ID of this user.
    fn user_id(&self) -> OwnedUserId {
        self.upcast_ref().imp().user_id.get().unwrap().clone()
    }

    /// The display name of this user.
    fn display_name(&self) -> String {
        let imp = self.upcast_ref().imp();

        if let Some(display_name) = imp.display_name.borrow().to_owned() {
            display_name
        } else {
            imp.user_id.get().unwrap().localpart().to_owned()
        }
    }

    /// Set the display name of this user.
    fn set_display_name(&self, display_name: Option<String>) {
        if Some(self.display_name()) == display_name {
            return;
        }
        self.upcast_ref().imp().display_name.replace(display_name);
        self.notify("display-name");
    }

    /// The avatar of this user.
    fn avatar(&self) -> &Avatar {
        self.upcast_ref().imp().avatar.get().unwrap()
    }

    /// Set the avatar URL of this user.
    fn set_avatar_url(&self, url: Option<OwnedMxcUri>) {
        self.avatar().set_url(url);
    }

    /// The actions the currently logged-in user is allowed to perform on this
    /// user.
    fn allowed_actions(&self) -> UserActions {
        let user = self.upcast_ref();

        let is_other = self.session().user().map_or(false, |session_user| {
            session_user.user_id() != self.user_id()
        });

        if !user.is_verified() && is_other {
            UserActions::VERIFY
        } else {
            UserActions::empty()
        }
    }

    /// Get a `Pill` representing this `User`.
    fn to_pill(&self) -> Pill {
        let user = self.upcast_ref();
        Pill::for_user(user)
    }

    /// Get the HTML mention representation for this `User`.
    fn html_mention(&self) -> String {
        let uri = self.user_id().matrix_to_uri();
        format!("<a href=\"{uri}\">{}</a>", self.display_name())
    }
}

impl<T: IsA<User>> UserExt for T {}

unsafe impl<T: ObjectImpl + 'static> IsSubclassable<T> for User {
    fn class_init(class: &mut glib::Class<Self>) {
        <glib::Object as IsSubclassable<T>>::class_init(class.upcast_ref_mut());
    }

    fn instance_init(instance: &mut glib::subclass::InitializingObject<T>) {
        <glib::Object as IsSubclassable<T>>::instance_init(instance);
    }
}

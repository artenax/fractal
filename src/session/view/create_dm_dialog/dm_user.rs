use gtk::{glib, prelude::*, subclass::prelude::*};
use log::{debug, error};
use matrix_sdk::ruma::{
    api::client::room::create_room,
    assign,
    events::{room::encryption::RoomEncryptionEventContent, InitialStateEvent},
    MxcUri, UserId,
};

use crate::{
    prelude::*,
    session::model::{Room, Session, User},
    spawn_tokio,
};

mod imp {
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct DmUser {
        pub dm_room: glib::WeakRef<Room>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DmUser {
        const NAME: &'static str = "CreateDmDialogUser";
        type Type = super::DmUser;
        type ParentType = User;
    }

    impl ObjectImpl for DmUser {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Room>("dm-room")
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "dm-room" => obj.set_dm_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "dm-room" => obj.dm_room().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// A User in the context of creating a direct chat.
    pub struct DmUser(ObjectSubclass<imp::DmUser>) @extends User;
}

impl DmUser {
    pub fn new(
        session: &Session,
        user_id: &UserId,
        display_name: Option<&str>,
        avatar_url: Option<&MxcUri>,
        dm_room: Option<&Room>,
    ) -> Self {
        let obj: Self = glib::Object::builder()
            .property("session", session)
            .property("user-id", user_id.as_str())
            .property("display-name", display_name)
            .property("dm-room", dm_room)
            .build();
        // FIXME: we should make the avatar_url settable as property
        obj.set_avatar_url(avatar_url.map(std::borrow::ToOwned::to_owned));
        obj
    }

    /// Get the DM chat with this user, if any.
    pub fn dm_room(&self) -> Option<Room> {
        self.imp().dm_room.upgrade()
    }

    /// Set the DM chat with this user.
    pub fn set_dm_room(&self, dm_room: Option<&Room>) {
        if self.dm_room().as_ref() == dm_room {
            return;
        }

        self.imp().dm_room.set(dm_room);
        self.notify("dm-room");
    }

    /// Creates a new DM chat with this user
    ////
    /// If A DM chat exists already no new room is created and the existing one
    /// is returned.
    pub async fn start_chat(&self) -> Result<Room, ()> {
        let session = self.session();
        let client = session.client();
        let other_user = self.user_id();

        if let Some(room) = self.dm_room() {
            debug!(
                "A Direct Chat with the user {other_user} exists already, not creating a new one"
            );

            // We can be sure that this room has only ourself and maybe the other user as
            // member.
            if room.matrix_room().active_members_count() < 2 {
                debug!("{other_user} left the chat, re-invite them");

                if room.invite(&[self.clone().upcast()]).await.is_err() {
                    return Err(());
                }
            }

            return Ok(room);
        }

        let handle = spawn_tokio!(async move { create_dm(client, other_user).await });

        match handle.await.unwrap() {
            Ok(matrix_room) => {
                let room = session
                    .room_list()
                    .get_wait(matrix_room.room_id())
                    .await
                    .expect("The newly created room was not found");
                self.set_dm_room(Some(&room));
                Ok(room)
            }
            Err(error) => {
                error!("Couldn’t create a new Direct Chat: {error}");
                Err(())
            }
        }
    }
}

async fn create_dm(
    client: matrix_sdk::Client,
    other_user: ruma::OwnedUserId,
) -> Result<matrix_sdk::Room, matrix_sdk::Error> {
    let request = assign!(create_room::v3::Request::new(),
    {
        is_direct: true,
        invite: vec![other_user],
        preset: Some(create_room::v3::RoomPreset::TrustedPrivateChat),
        initial_state: vec![
           InitialStateEvent::new(RoomEncryptionEventContent::with_recommended_defaults()).to_raw_any(),
        ],
    });

    client.create_room(request).await
}

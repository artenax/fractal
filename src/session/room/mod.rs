mod event;
mod highlight_flags;
mod member;
mod member_list;
mod member_role;
mod power_levels;
mod room_type;
mod timeline;
mod typing_list;

use std::{cell::RefCell, io::Cursor};

use gettextrs::gettext;
use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*};
use log::{debug, error, warn};
use matrix_sdk::{
    attachment::{generate_image_thumbnail, AttachmentConfig, AttachmentInfo, Thumbnail},
    deserialized_responses::SyncTimelineEvent,
    room::Room as MatrixRoom,
    ruma::{
        api::client::sync::sync_events::v3::InvitedRoom,
        events::{
            reaction::ReactionEventContent,
            receipt::{ReceiptEventContent, ReceiptType},
            relation::Annotation,
            room::member::MembershipState,
            tag::{TagInfo, TagName},
            AnyRoomAccountDataEvent, AnyStrippedStateEvent, AnySyncStateEvent,
            AnySyncTimelineEvent, StateEventType, SyncStateEvent,
        },
        OwnedEventId, OwnedRoomId, OwnedUserId, RoomId,
    },
    sync::{JoinedRoom, LeftRoom},
    DisplayName, Result as MatrixResult, RoomMemberships,
};
use ruma::events::{
    receipt::ReceiptThread, room::power_levels::PowerLevelAction, typing::TypingEventContent,
    AnyMessageLikeEventContent, SyncEphemeralRoomEvent,
};

pub use self::{
    event::*,
    highlight_flags::HighlightFlags,
    member::{Member, Membership},
    member_list::MemberList,
    member_role::MemberRole,
    power_levels::{PowerLevel, PowerLevels, POWER_LEVEL_MAX, POWER_LEVEL_MIN},
    room_type::RoomType,
    timeline::*,
    typing_list::TypingList,
};
use super::verification::IdentityVerification;
use crate::{
    components::Pill,
    gettext_f,
    prelude::*,
    session::{
        sidebar::{SidebarItem, SidebarItemImpl},
        AvatarData, AvatarImage, AvatarUriSource, Session, User,
    },
    spawn, spawn_tokio,
};

mod imp {
    use std::cell::Cell;

    use glib::{object::WeakRef, subclass::Signal};
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Default)]
    pub struct Room {
        pub room_id: OnceCell<OwnedRoomId>,
        pub matrix_room: RefCell<Option<MatrixRoom>>,
        pub session: WeakRef<Session>,
        pub name: RefCell<Option<String>>,
        pub avatar_data: OnceCell<AvatarData>,
        pub category: Cell<RoomType>,
        pub timeline: OnceCell<Timeline>,
        pub members: OnceCell<MemberList>,
        /// The user who sent the invite to this room. This is only set when
        /// this room is an invitiation.
        pub inviter: RefCell<Option<Member>>,
        pub members_loaded: Cell<bool>,
        pub power_levels: RefCell<PowerLevels>,
        /// The timestamp of the latest possibly unread event in this room.
        pub latest_unread: Cell<u64>,
        /// The event of the user's read receipt for this room.
        pub read_receipt: RefCell<Option<AnySyncTimelineEvent>>,
        /// The latest read event in the room's timeline.
        pub latest_read: RefCell<Option<Event>>,
        /// The highlight state of the room,
        pub highlight: Cell<HighlightFlags>,
        /// The ID of the room that was upgraded and that this one replaces.
        pub predecessor: OnceCell<OwnedRoomId>,
        /// The ID of the successor of this Room, if this room was upgraded.
        pub successor: OnceCell<OwnedRoomId>,
        /// The successor of this Room, if this room was upgraded.
        pub successor_room: WeakRef<super::Room>,
        /// The most recent verification request event.
        pub verification: RefCell<Option<IdentityVerification>>,
        /// Whether this room is encrypted
        pub is_encrypted: Cell<bool>,
        /// The list of members currently typing in this room.
        pub typing_list: TypingList,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Room {
        const NAME: &'static str = "Room";
        type Type = super::Room;
        type ParentType = SidebarItem;
    }

    impl ObjectImpl for Room {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::builder("room-id")
                        .construct_only()
                        .build(),
                    glib::ParamSpecObject::builder::<Session>("session")
                        .construct_only()
                        .build(),
                    glib::ParamSpecString::builder("name").read_only().build(),
                    glib::ParamSpecString::builder("display-name")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<Member>("inviter")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<AvatarData>("avatar-data")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<Timeline>("timeline")
                        .read_only()
                        .build(),
                    glib::ParamSpecFlags::builder::<HighlightFlags>("highlight")
                        .read_only()
                        .build(),
                    glib::ParamSpecUInt64::builder("notification-count")
                        .read_only()
                        .build(),
                    glib::ParamSpecEnum::builder::<RoomType>("category")
                        .read_only()
                        .build(),
                    glib::ParamSpecString::builder("topic").read_only().build(),
                    glib::ParamSpecUInt64::builder("latest-unread")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<Event>("latest-read")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<MemberList>("members")
                        .read_only()
                        .build(),
                    glib::ParamSpecString::builder("predecessor")
                        .read_only()
                        .build(),
                    glib::ParamSpecString::builder("successor")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<super::Room>("successor-room")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<IdentityVerification>("verification")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("encrypted")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<TypingList>("typing-list")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "session" => self.session.set(value.get().ok().as_ref()),
                "room-id" => self
                    .room_id
                    .set(RoomId::parse(value.get::<&str>().unwrap()).unwrap())
                    .unwrap(),
                "verification" => obj.set_verification(value.get().unwrap()),
                "encrypted" => obj.set_is_encrypted(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "room-id" => obj.room_id().as_str().to_value(),
                "session" => obj.session().to_value(),
                "inviter" => obj.inviter().to_value(),
                "name" => obj.name().to_value(),
                "display-name" => obj.display_name().to_value(),
                "avatar-data" => obj.avatar_data().to_value(),
                "timeline" => self.timeline.get().unwrap().to_value(),
                "category" => obj.category().to_value(),
                "highlight" => obj.highlight().to_value(),
                "topic" => obj.topic().to_value(),
                "members" => obj.members().to_value(),
                "notification-count" => obj.notification_count().to_value(),
                "latest-unread" => obj.latest_unread().to_value(),
                "latest-read" => obj.latest_read().to_value(),
                "predecessor" => obj.predecessor().map(|id| id.as_str()).to_value(),
                "successor" => obj.successor().map(|id| id.as_str()).to_value(),
                "successor-room" => obj.successor_room().to_value(),
                "verification" => obj.verification().to_value(),
                "encrypted" => obj.is_encrypted().to_value(),
                "typing-list" => obj.typing_list().to_value(),
                _ => unimplemented!(),
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> =
                Lazy::new(|| vec![Signal::builder("room-forgotten").build()]);
            SIGNALS.as_ref()
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.set_matrix_room(obj.session().client().get_room(obj.room_id()).unwrap());
            self.timeline.set(Timeline::new(&obj)).unwrap();
            self.members.set(MemberList::new(&obj)).unwrap();

            // Initialize the avatar first since loading is async.
            self.avatar_data
                .set(AvatarData::new(AvatarImage::new(
                    &obj.session(),
                    obj.matrix_room().avatar_url().as_deref(),
                    AvatarUriSource::Room,
                )))
                .unwrap();
            spawn!(clone!(@weak obj => async move {
                obj.load_avatar().await;
            }));

            obj.load_power_levels();

            spawn!(clone!(@strong obj => async move {
                obj.setup_is_encrypted().await;
            }));

            obj.bind_property("display-name", obj.avatar_data(), "display-name")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build();

            if !matches!(obj.category(), RoomType::Left | RoomType::Outdated) {
                // Load the room history when idle
                spawn!(
                    glib::source::PRIORITY_LOW,
                    clone!(@weak obj => async move {
                        obj.timeline().load().await;
                    })
                );
            }
        }
    }

    impl SidebarItemImpl for Room {}
}

glib::wrapper! {
    /// GObject representation of a Matrix room.
    ///
    /// Handles populating the Timeline.
    pub struct Room(ObjectSubclass<imp::Room>) @extends SidebarItem;
}

impl Room {
    pub fn new(session: &Session, room_id: &RoomId) -> Self {
        glib::Object::builder()
            .property("session", session)
            .property("room-id", &room_id.to_string())
            .build()
    }

    /// The current session.
    pub fn session(&self) -> Session {
        self.imp().session.upgrade().unwrap()
    }

    /// The ID of this room.
    pub fn room_id(&self) -> &RoomId {
        self.imp().room_id.get().unwrap()
    }

    /// Whether this room is direct or not.
    pub async fn is_direct(&self) -> bool {
        let matrix_room = self.matrix_room();

        spawn_tokio!(async move { matrix_room.is_direct().await.unwrap_or_default() })
            .await
            .unwrap()
    }

    pub fn matrix_room(&self) -> MatrixRoom {
        self.imp().matrix_room.borrow().as_ref().unwrap().clone()
    }

    /// Set the new sdk room struct represented by this `Room`
    fn set_matrix_room(&self, matrix_room: MatrixRoom) {
        let imp = self.imp();

        // Check if the previous type was different
        if let Some(ref old_matrix_room) = *imp.matrix_room.borrow() {
            let changed = match old_matrix_room {
                MatrixRoom::Joined(_) => !matches!(matrix_room, MatrixRoom::Joined(_)),
                MatrixRoom::Left(_) => !matches!(matrix_room, MatrixRoom::Left(_)),
                MatrixRoom::Invited(_) => !matches!(matrix_room, MatrixRoom::Invited(_)),
            };
            if changed {
                debug!("The matrix room struct for `Room` changed");
            } else {
                return;
            }
        }

        imp.matrix_room.replace(Some(matrix_room));

        self.load_display_name();
        self.load_predecessor();
        self.load_successor();
        self.load_category();
        self.setup_receipts();
        self.setup_typing();
    }

    /// Forget a room that is left.
    pub async fn forget(&self) -> MatrixResult<()> {
        if self.category() != RoomType::Left {
            warn!("Cannot forget a room that is not left");
            return Ok(());
        }

        let matrix_room = self.matrix_room();

        let handle = spawn_tokio!(async move {
            match matrix_room {
                MatrixRoom::Left(room) => room.forget().await,
                _ => unimplemented!(),
            }
        });

        match handle.await.unwrap() {
            Ok(_) => {
                self.emit_by_name::<()>("room-forgotten", &[]);
                Ok(())
            }
            Err(error) => {
                error!("Couldn’t forget the room: {error}");

                // Load the previous category
                self.load_category();

                Err(error)
            }
        }
    }

    pub fn is_joined(&self) -> bool {
        matches!(
            self.category(),
            RoomType::Favorite
                | RoomType::Normal
                | RoomType::LowPriority
                | RoomType::Outdated
                | RoomType::Space
                | RoomType::Direct
        )
    }

    pub fn category(&self) -> RoomType {
        self.imp().category.get()
    }

    fn set_category_internal(&self, category: RoomType) {
        if self.category() == category {
            return;
        }

        self.imp().category.set(category);
        self.notify("category");
    }

    /// Set the category of this room.
    ///
    /// This makes the necessary to propagate the category to the homeserver.
    ///
    /// Note: Rooms can't be moved to the invite category and they can't be
    /// moved once they are upgraded.
    pub async fn set_category(&self, category: RoomType) -> MatrixResult<()> {
        let matrix_room = self.matrix_room();
        let previous_category = self.category();

        if previous_category == category {
            return Ok(());
        }

        if previous_category == RoomType::Outdated {
            warn!("Can't set the category of an upgraded room");
            return Ok(());
        }

        match category {
            RoomType::Invited => {
                warn!("Rooms can’t be moved to the invite Category");
                return Ok(());
            }
            RoomType::Outdated => {
                // Outdated rooms don't need to propagate anything to the server
                self.set_category_internal(category);
                return Ok(());
            }
            _ => {}
        }

        self.set_category_internal(category);

        let handle = spawn_tokio!(async move {
            match matrix_room {
                MatrixRoom::Invited(room) => match category {
                    RoomType::Invited => {}
                    RoomType::Favorite => {
                        if let Some(tags) = room.tags().await? {
                            if !tags.contains_key(&TagName::Favorite) {
                                room.set_tag(TagName::Favorite, TagInfo::new()).await?;
                            }
                            if tags.contains_key(&TagName::LowPriority) {
                                room.remove_tag(TagName::LowPriority).await?;
                            }
                        }
                        room.accept_invitation().await?;
                    }
                    RoomType::Normal => {
                        if let Some(tags) = room.tags().await? {
                            if tags.contains_key(&TagName::Favorite) {
                                room.remove_tag(TagName::Favorite).await?;
                            }
                            if tags.contains_key(&TagName::LowPriority) {
                                room.remove_tag(TagName::LowPriority).await?;
                            }
                        }

                        if room.is_direct().await.unwrap_or_default() {
                            room.set_is_direct(false).await?;
                        }

                        room.accept_invitation().await?;
                    }
                    RoomType::LowPriority => {
                        if let Some(tags) = room.tags().await? {
                            if tags.contains_key(&TagName::Favorite) {
                                room.remove_tag(TagName::Favorite).await?;
                            }
                            if !tags.contains_key(&TagName::LowPriority) {
                                room.set_tag(TagName::LowPriority, TagInfo::new()).await?;
                            }
                        }
                        room.accept_invitation().await?;
                    }
                    RoomType::Left => {
                        room.reject_invitation().await?;
                    }
                    RoomType::Outdated => unimplemented!(),
                    RoomType::Space => unimplemented!(),
                    RoomType::Direct => {
                        if !room.is_direct().await.unwrap_or_default() {
                            room.set_is_direct(true).await?;
                        }

                        if let Some(tags) = room.tags().await? {
                            if tags.contains_key(&TagName::Favorite) {
                                room.remove_tag(TagName::Favorite).await?;
                            }
                            if tags.contains_key(&TagName::LowPriority) {
                                room.remove_tag(TagName::LowPriority).await?;
                            }
                        }

                        room.accept_invitation().await?;
                    }
                },
                MatrixRoom::Joined(room) => match category {
                    RoomType::Invited => {}
                    RoomType::Favorite => {
                        room.set_tag(TagName::Favorite, TagInfo::new()).await?;
                        if previous_category == RoomType::LowPriority {
                            room.remove_tag(TagName::LowPriority).await?;
                        }
                    }
                    RoomType::Normal => {
                        if room.is_direct().await.unwrap_or_default() {
                            room.set_is_direct(false).await?;
                        }
                        match previous_category {
                            RoomType::Favorite => {
                                room.remove_tag(TagName::Favorite).await?;
                            }
                            RoomType::LowPriority => {
                                room.remove_tag(TagName::LowPriority).await?;
                            }
                            _ => {}
                        }
                    }
                    RoomType::LowPriority => {
                        room.set_tag(TagName::LowPriority, TagInfo::new()).await?;
                        if previous_category == RoomType::Favorite {
                            room.remove_tag(TagName::Favorite).await?;
                        }
                    }
                    RoomType::Left => {
                        room.leave().await?;
                    }
                    RoomType::Outdated => unimplemented!(),
                    RoomType::Space => unimplemented!(),
                    RoomType::Direct => {
                        if !room.is_direct().await.unwrap_or_default() {
                            room.set_is_direct(true).await?;
                        }

                        if let Some(tags) = room.tags().await? {
                            if tags.contains_key(&TagName::LowPriority) {
                                room.remove_tag(TagName::LowPriority).await?;
                            }
                            if tags.contains_key(&TagName::Favorite) {
                                room.remove_tag(TagName::Favorite).await?;
                            }
                        }
                    }
                },
                MatrixRoom::Left(room) => match category {
                    RoomType::Invited => {}
                    RoomType::Favorite => {
                        if let Some(tags) = room.tags().await? {
                            if !tags.contains_key(&TagName::Favorite) {
                                room.set_tag(TagName::Favorite, TagInfo::new()).await?;
                            }
                            if tags.contains_key(&TagName::LowPriority) {
                                room.remove_tag(TagName::LowPriority).await?;
                            }
                        }
                        room.join().await?;
                    }
                    RoomType::Normal => {
                        if let Some(tags) = room.tags().await? {
                            if tags.contains_key(&TagName::Favorite) {
                                room.remove_tag(TagName::Favorite).await?;
                            }
                            if tags.contains_key(&TagName::LowPriority) {
                                room.remove_tag(TagName::LowPriority).await?;
                            }
                        }
                        room.join().await?;
                    }
                    RoomType::LowPriority => {
                        if let Some(tags) = room.tags().await? {
                            if tags.contains_key(&TagName::Favorite) {
                                room.remove_tag(TagName::Favorite).await?;
                            }
                            if !tags.contains_key(&TagName::LowPriority) {
                                room.set_tag(TagName::LowPriority, TagInfo::new()).await?;
                            }
                        }
                        room.join().await?;
                    }
                    RoomType::Left => {}
                    RoomType::Outdated => unimplemented!(),
                    RoomType::Space => unimplemented!(),
                    RoomType::Direct => {
                        if !room.is_direct().await.unwrap_or_default() {
                            room.set_is_direct(true).await?;
                        }

                        if let Some(tags) = room.tags().await? {
                            if tags.contains_key(&TagName::LowPriority) {
                                room.remove_tag(TagName::LowPriority).await?;
                            }
                            if tags.contains_key(&TagName::Favorite) {
                                room.remove_tag(TagName::Favorite).await?;
                            }
                        }

                        room.join().await?;
                    }
                },
            }

            Result::<_, matrix_sdk::Error>::Ok(())
        });

        match handle.await.unwrap() {
            Ok(_) => Ok(()),
            Err(error) => {
                error!("Couldn’t set the room category: {error}");

                // Load the previous category
                self.load_category();

                Err(error)
            }
        }
    }

    pub fn load_category(&self) {
        // Don't load the category if this room was upgraded
        if self.category() == RoomType::Outdated {
            return;
        }

        let matrix_room = self.matrix_room();

        match matrix_room {
            MatrixRoom::Joined(_) => {
                if matrix_room.is_space() {
                    self.set_category_internal(RoomType::Space);
                } else {
                    let matrix_room_clone = matrix_room.clone();
                    let is_direct = spawn_tokio!(async move {
                        matrix_room_clone.is_direct().await.unwrap_or_default()
                    });
                    let tags = spawn_tokio!(async move { matrix_room.tags().await });

                    spawn!(
                        glib::PRIORITY_DEFAULT_IDLE,
                        clone!(@weak self as obj => async move {
                            let mut category = if is_direct.await.unwrap() {
                                RoomType::Direct
                            } else {
                                RoomType::Normal
                            };

                            if let Ok(Some(tags)) = tags.await.unwrap() {
                                if tags.get(&TagName::Favorite).is_some() {
                                    category = RoomType::Favorite;
                                } else if tags.get(&TagName::LowPriority).is_some() {
                                    category = RoomType::LowPriority;
                                }
                            }

                            obj.set_category_internal(category);
                        })
                    );
                }
            }
            MatrixRoom::Invited(_) => self.set_category_internal(RoomType::Invited),
            MatrixRoom::Left(_) => self.set_category_internal(RoomType::Left),
        };
    }

    pub fn typing_list(&self) -> &TypingList {
        &self.imp().typing_list
    }

    fn setup_typing(&self) {
        let MatrixRoom::Joined(matrix_room) = self.matrix_room() else {
            return;
        };

        let room_weak = glib::SendWeakRef::from(self.downgrade());
        matrix_room.add_event_handler(move |event: SyncEphemeralRoomEvent<TypingEventContent>| {
            let room_weak = room_weak.clone();
            async move {
                let ctx = glib::MainContext::default();
                ctx.spawn(async move {
                    spawn!(async move {
                        if let Some(obj) = room_weak.upgrade() {
                            obj.handle_typing_event(event.content).await
                        }
                    });
                });
            }
        });
    }

    fn setup_receipts(&self) {
        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                let user_id = obj.session().user().unwrap().user_id();
                let matrix_room = obj.matrix_room();

                let handle = spawn_tokio!(async move { matrix_room.user_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, &user_id).await });

                match handle.await.unwrap() {
                    Ok(Some((event_id, _))) => {
                        obj.update_read_receipt(event_id).await;
                    },
                    Err(error) => {
                        error!(
                            "Couldn’t get the user’s read receipt for room {}: {error}",
                            obj.room_id(),
                        );
                    }
                    _ => {}
                }

                // Listen to changes in the read receipts.
                let room_weak = glib::SendWeakRef::from(obj.downgrade());
                obj.matrix_room().add_event_handler(
                    move |event: SyncEphemeralRoomEvent<ReceiptEventContent>| {
                        let room_weak = room_weak.clone();
                        async move {
                            let ctx = glib::MainContext::default();
                            ctx.spawn(async move {
                                spawn!(async move {
                                    if let Some(obj) = room_weak.upgrade() {
                                        obj.handle_receipt_event(event.content).await
                                    }
                                });
                            });
                        }
                    },
                );
            })
        );
    }

    async fn handle_receipt_event(&self, content: ReceiptEventContent) {
        let user_id = self.session().user().unwrap().user_id();

        for (event_id, receipts) in content.iter() {
            if let Some(users) = receipts.get(&ReceiptType::Read) {
                for user in users.keys() {
                    if user == &user_id {
                        self.update_read_receipt(event_id.clone()).await;
                        return;
                    }
                }
            }
        }
    }

    /// Update the user's read receipt event for this room with the given event
    /// ID.
    async fn update_read_receipt(&self, event_id: OwnedEventId) {
        if Some(event_id.as_ref()) == self.read_receipt().as_ref().map(|event| event.event_id()) {
            return;
        }

        match self.timeline().fetch_event_by_id(event_id).await {
            Ok(read_receipt) => {
                self.set_read_receipt(Some(read_receipt));
            }
            Err(error) => {
                error!(
                    "Couldn’t get the event of the user’s read receipt for room {}: {error}",
                    self.room_id(),
                );
            }
        }
    }

    /// The user's read receipt event for this room.
    pub fn read_receipt(&self) -> Option<AnySyncTimelineEvent> {
        self.imp().read_receipt.borrow().clone()
    }

    /// Set the user's read receipt event for this room.
    fn set_read_receipt(&self, read_receipt: Option<AnySyncTimelineEvent>) {
        if read_receipt.as_ref().map(|event| event.event_id())
            == self
                .imp()
                .read_receipt
                .borrow()
                .as_ref()
                .map(|event| event.event_id())
        {
            return;
        }

        self.imp().read_receipt.replace(read_receipt);
        self.update_latest_read()
    }

    fn update_latest_read(&self) {
        let read_receipt = self.read_receipt();
        let user_id = self.session().user().unwrap().user_id();
        let timeline = self.timeline();

        let latest_read = read_receipt.and_then(|read_receipt| {
            (0..timeline.n_items()).rev().find_map(|i| {
                timeline
                    .item(i)
                    .as_ref()
                    .and_then(|obj| obj.downcast_ref::<Event>())
                    .and_then(|event| {
                        // The user sent the event so it's the latest read event.
                        // Necessary because we don't get read receipts for the user's own events.
                        if event.sender().user_id() == user_id {
                            return Some(event.to_owned());
                        }

                        // This is the event corresponding to the read receipt.
                        if event.event_id().as_deref() == Some(read_receipt.event_id()) {
                            return Some(event.to_owned());
                        }

                        // The event is older than the read receipt so it has been read.
                        if event.counts_as_unread()
                            && event.origin_server_ts() <= read_receipt.origin_server_ts()
                        {
                            return Some(event.to_owned());
                        }

                        None
                    })
            })
        });

        self.set_latest_read(latest_read);
    }

    /// The latest read event in the room's timeline.
    pub fn latest_read(&self) -> Option<Event> {
        self.imp().latest_read.borrow().clone()
    }

    /// Set the latest read event.
    fn set_latest_read(&self, latest_read: Option<Event>) {
        if latest_read == self.latest_read() {
            return;
        }

        self.imp().latest_read.replace(latest_read);
        self.notify("latest-read");
        self.update_highlight();
    }

    async fn handle_typing_event(&self, content: TypingEventContent) {
        let own_user_id = self.session().user().unwrap().user_id();

        let members = content
            .user_ids
            .into_iter()
            .filter_map(|user_id| {
                (user_id != own_user_id).then(|| self.members().member_by_id(user_id))
            })
            .collect();

        self.imp().typing_list.update(members);
    }

    /// The timeline of this room.
    pub fn timeline(&self) -> &Timeline {
        self.imp().timeline.get().unwrap()
    }

    /// The members of this room.
    pub fn members(&self) -> &MemberList {
        self.imp().members.get().unwrap()
    }

    fn notify_notification_count(&self) {
        self.notify("notification-count");
    }

    fn update_highlight(&self) {
        let mut highlight = HighlightFlags::empty();

        let counts = self
            .imp()
            .matrix_room
            .borrow()
            .as_ref()
            .unwrap()
            .unread_notification_counts();

        if counts.highlight_count > 0 {
            highlight = HighlightFlags::all();
        } else if counts.notification_count > 0 || self.has_unread_messages() {
            highlight = HighlightFlags::BOLD;
        }

        self.set_highlight(highlight);
    }

    /// How this room is highlighted.
    pub fn highlight(&self) -> HighlightFlags {
        self.imp().highlight.get()
    }

    /// Set how this room is highlighted.
    fn set_highlight(&self, highlight: HighlightFlags) {
        if self.highlight() == highlight {
            return;
        }

        self.imp().highlight.set(highlight);
        self.notify("highlight");
    }

    /// Whether this room has unread messages.
    fn has_unread_messages(&self) -> bool {
        self.latest_read()
            .filter(|latest_read| {
                let timeline = self.timeline();

                for i in (0..timeline.n_items()).rev() {
                    if let Some(event) = timeline
                        .item(i)
                        .as_ref()
                        .and_then(|obj| obj.downcast_ref::<Event>())
                    {
                        // This is the event corresponding to the read receipt so there's no unread
                        // messages.
                        if event == latest_read {
                            return true;
                        }

                        // The user hasn't read the latest message.
                        if event.counts_as_unread() {
                            return false;
                        }
                    }
                }

                false
            })
            .is_none()
    }

    /// The name of this room.
    ///
    /// This can be empty, the display name should be used instead in the
    /// interface.
    pub fn name(&self) -> Option<String> {
        self.matrix_room().name()
    }

    /// The display name of this room.
    pub fn display_name(&self) -> String {
        let display_name = self.imp().name.borrow().clone();
        display_name.unwrap_or_else(|| gettext("Unknown"))
    }

    /// Set the display name of this room.
    fn set_display_name(&self, display_name: Option<String>) {
        if Some(self.display_name()) == display_name {
            return;
        }

        self.imp().name.replace(display_name);
        self.notify("display-name");
    }

    fn load_display_name(&self) {
        let matrix_room = self.matrix_room();
        let handle = spawn_tokio!(async move { matrix_room.display_name().await });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                // FIXME: We should retry to if the request failed
                match handle.await.unwrap() {
                        Ok(display_name) => { let name = match display_name {
                            DisplayName::Named(s) | DisplayName::Calculated(s) | DisplayName::Aliased(s) => {
                                s
                            }
                            // Translators: This is the name of a room that is empty but had another user before.
                            // Do NOT translate the content between '{' and '}', this is a variable name.
                            DisplayName::EmptyWas(s) => gettext_f("Empty Room (was {user})", &[("user", &s)]),
                            // Translators: This is the name of a room without other users.
                            DisplayName::Empty => gettext("Empty Room"),
                        };
                            obj.set_display_name(Some(name))
                    }
                        Err(error) => error!("Couldn’t fetch display name: {error}"),
                };
            })
        );
    }

    /// The number of unread notifications of this room.
    pub fn notification_count(&self) -> u64 {
        let matrix_room = self.imp().matrix_room.borrow();
        matrix_room
            .as_ref()
            .unwrap()
            .unread_notification_counts()
            .notification_count
    }

    /// The Avatar of this room.
    pub fn avatar_data(&self) -> &AvatarData {
        self.imp().avatar_data.get().unwrap()
    }

    /// The topic of this room.
    pub fn topic(&self) -> Option<String> {
        self.matrix_room()
            .topic()
            .filter(|topic| !topic.is_empty() && topic.find(|c: char| !c.is_whitespace()).is_some())
    }

    pub fn power_levels(&self) -> PowerLevels {
        self.imp().power_levels.borrow().clone()
    }

    /// The user who sent the invite to this room.
    ///
    /// This is only set when this room represents an invite.
    pub fn inviter(&self) -> Option<Member> {
        self.imp().inviter.borrow().clone()
    }

    /// Handle stripped state events.
    ///
    /// Events passed to this function aren't added to the timeline.
    pub fn handle_invite_events(&self, events: Vec<AnyStrippedStateEvent>) {
        let invite_event = events
            .iter()
            .find(|event| {
                if let AnyStrippedStateEvent::RoomMember(event) = event {
                    event.content.membership == MembershipState::Invite
                        && event.state_key == self.session().user().unwrap().user_id().as_str()
                } else {
                    false
                }
            })
            .unwrap();

        let inviter_id = invite_event.sender();

        let inviter_event = events.iter().find(|event| {
            if let AnyStrippedStateEvent::RoomMember(event) = event {
                event.state_key == inviter_id.as_str()
            } else {
                false
            }
        });

        let inviter = Member::new(self, inviter_id);
        if let Some(AnyStrippedStateEvent::RoomMember(event)) = inviter_event {
            inviter.update_from_member_event(event);
        }

        self.imp().inviter.replace(Some(inviter));
        self.notify("inviter");
    }

    /// Update the room state based on the new sync response
    /// FIXME: We should use the sdk's event handler to get updates
    pub fn update_for_events(&self, batch: Vec<SyncTimelineEvent>) {
        // FIXME: notify only when the count has changed
        self.notify_notification_count();

        let events: Vec<_> = batch
            .iter()
            .flat_map(|e| e.event.deserialize().ok())
            .collect();

        for event in events.iter() {
            if let AnySyncTimelineEvent::State(state_event) = event {
                match state_event {
                    AnySyncStateEvent::RoomMember(SyncStateEvent::Original(event)) => {
                        self.members().update_member_for_member_event(event);
                        // If we show the other user's avatar, a member joining or leaving changes
                        // the avatar.
                        spawn!(clone!(@weak self as obj => async move {
                            obj.load_avatar().await;
                        }));
                    }
                    AnySyncStateEvent::RoomAvatar(SyncStateEvent::Original(_)) => {
                        spawn!(clone!(@weak self as obj => async move {
                            obj.load_avatar().await;
                        }));
                    }
                    AnySyncStateEvent::RoomName(_) => {
                        self.notify("name");
                        // FIXME: this doesn't take into account changes in the calculated name
                        self.load_display_name()
                    }
                    AnySyncStateEvent::RoomTopic(_) => {
                        self.notify("topic");
                    }
                    AnySyncStateEvent::RoomPowerLevels(SyncStateEvent::Original(event)) => {
                        self.power_levels().update_from_event(event.clone());
                    }
                    AnySyncStateEvent::RoomTombstone(_) => {
                        self.load_successor();
                    }
                    _ => {}
                }
            }
        }
        self.session()
            .verification_list()
            .handle_response_room(self, events.iter());
    }

    /// The timestamp of the room's latest possibly unread event.
    ///
    /// If it is not known, it will return `0`.
    pub fn latest_unread(&self) -> u64 {
        self.imp().latest_unread.get()
    }

    /// Set the timestamp of the room's latest possibly unread event.
    fn set_latest_unread(&self, latest_unread: u64) {
        if latest_unread == self.latest_unread() {
            return;
        }

        self.imp().latest_unread.set(latest_unread);
        self.notify("latest-unread");
        self.update_highlight();
        // Necessary because we don't get read receipts for the user's own events.
        self.update_latest_read();
    }

    /// Load the room members in the list.
    pub async fn load_members(&self) {
        let imp = self.imp();
        if imp.members_loaded.get() {
            return;
        }

        imp.members_loaded.set(true);

        let matrix_room = self.matrix_room();
        let handle = spawn_tokio!(async move {
            let mut memberships = RoomMemberships::all();
            memberships.remove(RoomMemberships::LEAVE);

            matrix_room.members(memberships).await
        });

        // FIXME: We should retry to load the room members if the request failed
        match handle.await.unwrap() {
            Ok(members) => {
                // Add all members needed to display room events.
                self.members().update_from_room_members(&members);
            }
            Err(error) => {
                self.imp().members_loaded.set(false);
                error!("Couldn’t load room members: {error}")
            }
        };
    }

    fn load_power_levels(&self) {
        let matrix_room = self.matrix_room();
        let handle = spawn_tokio!(async move {
            let state_event = match matrix_room
                .get_state_event(StateEventType::RoomPowerLevels, "")
                .await
            {
                Ok(state_event) => state_event,
                Err(error) => {
                    error!("Initial load of room power levels failed: {error}");
                    return None;
                }
            };

            state_event
                .and_then(|e| e.deserialize().ok())
                .and_then(|e| {
                    if let AnySyncStateEvent::RoomPowerLevels(SyncStateEvent::Original(e)) = e {
                        Some(e)
                    } else {
                        None
                    }
                })
        });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                if let Some(event) = handle.await.unwrap() {
                    obj.power_levels().update_from_event(event);
                }
            })
        );
    }

    /// Send a message with the given `content` in this room.
    pub fn send_room_message_event(&self, content: impl Into<AnyMessageLikeEventContent>) {
        let timeline = self.timeline().matrix_timeline();
        let content = content.into();

        let handle = spawn_tokio!(async move { timeline.send(content, None).await });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                handle.await.unwrap();
            })
        );
    }

    /// Send a `key` reaction for the `relates_to` event ID in this room.
    pub fn send_reaction(&self, key: String, relates_to: OwnedEventId) {
        self.send_room_message_event(ReactionEventContent::new(Annotation::new(relates_to, key)));
    }

    /// Redact `redacted_event_id` in this room because of `reason`.
    pub fn redact(&self, redacted_event_id: OwnedEventId, reason: Option<String>) {
        let MatrixRoom::Joined(matrix_room) = self.matrix_room() else {
            return;
        };

        let handle = spawn_tokio!(async move {
            matrix_room
                .redact(&redacted_event_id, reason.as_deref(), None)
                .await
        });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                // FIXME: We should retry the request if it fails
                match handle.await.unwrap() {
                    Ok(_) => {},
                    Err(error) => error!("Couldn’t redact event: {error}"),
                };
            })
        );
    }

    pub fn send_typing_notification(&self, is_typing: bool) {
        let MatrixRoom::Joined(matrix_room) = self.matrix_room() else {
            return;
        };

        let handle = spawn_tokio!(async move { matrix_room.typing_notice(is_typing).await });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(_) => {},
                    Err(error) => error!("Couldn’t send typing notification: {error}"),
                };
            })
        );
    }

    /// Creates an expression that is true when our own user is allowed to do
    /// the given action in this `Room`.
    pub fn own_user_is_allowed_to_expr(
        &self,
        room_action: PowerLevelAction,
    ) -> gtk::ClosureExpression {
        let session = self.session();
        let user_id = session.user().unwrap().user_id();
        self.power_levels()
            .member_is_allowed_to_expr(user_id, room_action)
    }

    pub async fn accept_invite(&self) -> MatrixResult<()> {
        let matrix_room = self.matrix_room();

        let MatrixRoom::Invited(matrix_room) = matrix_room else {
            error!("Can’t accept invite, because this room isn’t an invited room");
            return Ok(());
        };

        let handle = spawn_tokio!(async move { matrix_room.accept_invitation().await });
        match handle.await.unwrap() {
            Ok(_) => Ok(()),
            Err(error) => {
                error!("Accepting invitation failed: {error}");
                Err(error)
            }
        }
    }

    pub async fn reject_invite(&self) -> MatrixResult<()> {
        let matrix_room = self.matrix_room();

        let MatrixRoom::Invited(matrix_room) = matrix_room else {
            error!("Can’t reject invite, because this room isn’t an invited room");
            return Ok(());
        };

        let handle = spawn_tokio!(async move { matrix_room.reject_invitation().await });
        match handle.await.unwrap() {
            Ok(_) => Ok(()),
            Err(error) => {
                error!("Rejecting invitation failed: {error}");

                Err(error)
            }
        }
    }

    pub fn handle_left_response(&self, response_room: LeftRoom) {
        self.set_matrix_room(self.session().client().get_room(self.room_id()).unwrap());
        self.update_for_events(response_room.timeline.events);
    }

    pub fn handle_joined_response(&self, response_room: JoinedRoom) {
        self.set_matrix_room(self.session().client().get_room(self.room_id()).unwrap());

        if response_room
            .account_data
            .iter()
            .any(|e| matches!(e.deserialize(), Ok(AnyRoomAccountDataEvent::Tag(_))))
        {
            self.load_category();
        }

        self.update_for_events(response_room.timeline.events);
    }

    pub fn handle_invited_response(&self, response_room: InvitedRoom) {
        self.set_matrix_room(self.session().client().get_room(self.room_id()).unwrap());

        self.handle_invite_events(
            response_room
                .invite_state
                .events
                .into_iter()
                .filter_map(|event| {
                    if let Ok(event) = event.deserialize() {
                        Some(event)
                    } else {
                        error!("Couldn’t deserialize event: {event:?}");
                        None
                    }
                })
                .collect(),
        )
    }

    /// Connect to the signal sent when a room was forgotten.
    pub fn connect_room_forgotten<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("room-forgotten", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            f(&obj);
            None
        })
    }

    /// The ID of the predecessor of this room, if this room is an upgrade to a
    /// previous room.
    pub fn predecessor(&self) -> Option<&RoomId> {
        self.imp().predecessor.get().map(std::ops::Deref::deref)
    }

    /// Load the predecessor of this room.
    fn load_predecessor(&self) -> Option<()> {
        let imp = self.imp();

        if imp.predecessor.get().is_some() {
            return None;
        }

        let event = self.matrix_room().create_content()?;
        let room_id = event.predecessor?.room_id;

        imp.predecessor.set(room_id).unwrap();
        self.notify("predecessor");
        Some(())
    }

    /// The ID of the successor of this Room, if this room was upgraded.
    pub fn successor(&self) -> Option<&RoomId> {
        self.imp().successor.get().map(std::ops::Deref::deref)
    }

    /// The successor of this Room, if this room was upgraded.
    pub fn successor_room(&self) -> Option<Room> {
        self.imp().successor_room.upgrade()
    }

    /// Set the successor of this Room, if this room was upgraded.
    fn set_successor_room(&self, successor_room: &Room) {
        self.imp().successor_room.set(Some(successor_room));
        self.notify("successor-room")
    }

    /// Load the successor of this room.
    pub fn load_successor(&self) {
        let imp = self.imp();

        if imp.successor.get().is_some() {
            return;
        }

        let Some(room_tombstone) = self.matrix_room().tombstone() else {
            return;
        };

        imp.successor.set(room_tombstone.replacement_room).unwrap();
        self.notify("successor");

        if !self.update_outdated() {
            self.session()
                .room_list()
                .add_tombstoned_room(self.room_id().to_owned());
        }
    }

    /// Update whether this `Room` is outdated.
    ///
    /// A room is outdated when it was tombstoned and we joined its successor.
    ///
    /// Returns `true` if the `Room` was set as outdated, `false` otherwise.
    pub fn update_outdated(&self) -> bool {
        if self.category() == RoomType::Outdated {
            return true;
        }

        let Some(successor) = self.imp().successor.get() else {
            return false;
        };

        if let Some(successor_room) = self.session().room_list().get(successor) {
            self.set_successor_room(&successor_room);
            self.set_category_internal(RoomType::Outdated);
            true
        } else {
            false
        }
    }

    pub fn send_attachment(
        &self,
        bytes: Vec<u8>,
        mime: mime::Mime,
        body: &str,
        info: AttachmentInfo,
    ) {
        let MatrixRoom::Joined(matrix_room) = self.matrix_room() else {
            return;
        };

        let body = body.to_string();
        spawn_tokio!(async move {
            // Needed to hold the thumbnail data until it is sent.
            let data_slot;

            // The method will filter compatible mime types so we don't need to
            // since we ignore errors.
            let thumbnail = match generate_image_thumbnail(&mime, Cursor::new(&bytes), None) {
                Ok((data, info)) => {
                    data_slot = data;
                    Some(Thumbnail {
                        data: data_slot,
                        content_type: mime::IMAGE_JPEG,
                        info: Some(info),
                    })
                }
                _ => None,
            };

            let config = if let Some(thumbnail) = thumbnail {
                AttachmentConfig::with_thumbnail(thumbnail)
            } else {
                AttachmentConfig::new()
            }
            .info(info);

            matrix_room
                // TODO This should be added to pending messages instead of
                // sending it directly.
                .send_attachment(&body, &mime, bytes, config)
                .await
                .unwrap();
        });
    }

    /// Invite the given users to this room.
    ///
    /// Returns `Ok(())` if all the invites are sent successfully, otherwise
    /// returns the list of users who could not be invited.
    pub async fn invite<'a>(&self, users: &'a [User]) -> Result<(), Vec<&'a User>> {
        let MatrixRoom::Joined(matrix_room) = self.matrix_room() else {
            error!("Can’t invite users, because this room isn’t a joined room");
            return Ok(());
        };
        let user_ids: Vec<OwnedUserId> = users.iter().map(|user| user.user_id()).collect();

        let handle = spawn_tokio!(async move {
            let invitations = user_ids
                .iter()
                .map(|user_id| matrix_room.invite_user_by_id(user_id));
            futures::future::join_all(invitations).await
        });

        let mut failed_invites = Vec::new();
        for (index, result) in handle.await.unwrap().iter().enumerate() {
            match result {
                Ok(_) => {}
                Err(error) => {
                    error!(
                        "Failed to invite user with id {}: {error}",
                        users[index].user_id(),
                    );
                    failed_invites.push(&users[index]);
                }
            }
        }

        if failed_invites.is_empty() {
            Ok(())
        } else {
            Err(failed_invites)
        }
    }

    /// Set the most recent active verification for a user in this room.
    pub fn set_verification(&self, verification: IdentityVerification) {
        self.imp().verification.replace(Some(verification));
        self.notify("verification");
    }

    /// The most recent active verification for a user in this room.
    pub fn verification(&self) -> Option<IdentityVerification> {
        self.imp().verification.borrow().clone()
    }

    /// Update the latest possibly unread event of the room with the given
    /// events.
    ///
    /// The events must be in reverse chronological order.
    pub fn update_latest_unread<'a>(&self, events: impl IntoIterator<Item = &'a Event>) {
        let mut latest_unread = self.latest_unread();

        for event in events {
            if event.counts_as_unread() {
                latest_unread = latest_unread.max(event.origin_server_ts_u64());
                break;
            }
        }

        self.set_latest_unread(latest_unread);
    }

    /// Whether this room is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.imp().is_encrypted.get()
    }

    /// Set whether this room is encrypted.
    pub fn set_is_encrypted(&self, is_encrypted: bool) {
        let was_encrypted = self.is_encrypted();
        if was_encrypted == is_encrypted {
            return;
        }

        if was_encrypted && !is_encrypted {
            error!("Encryption for a room can't be disabled");
            return;
        }

        // if self.matrix_room().is_encrypted() != is_encrypted {
        // TODO: enable encryption if it isn't enabled yet
        // }

        spawn!(clone!(@strong self as obj => async move {
            obj.setup_is_encrypted().await;
        }));
    }

    async fn setup_is_encrypted(&self) {
        let matrix_room = self.matrix_room();
        let handle = spawn_tokio!(async move { matrix_room.is_encrypted().await });

        if handle
            .await
            .unwrap()
            .ok()
            .filter(|encrypted| *encrypted)
            .is_none()
        {
            return;
        }

        self.imp().is_encrypted.set(true);
        self.notify("encrypted");
    }

    /// Get a `Pill` representing this `Room`.
    pub fn to_pill(&self) -> Pill {
        Pill::for_room(self)
    }

    /// Get a human-readable ID for this `Room`.
    ///
    /// This is to identify the room easily in logs.
    pub fn human_readable_id(&self) -> String {
        format!("{} ({})", self.display_name(), self.room_id())
    }

    /// Load the avatar for the room.
    async fn load_avatar(&self) {
        let matrix_room = self.matrix_room();
        let avatar_url = matrix_room.avatar_url();
        let avatar_data = self.avatar_data();

        if avatar_url.is_none() && matrix_room.active_members_count() == 2 {
            // Fallback to other user's avatar if this is a 1-to-1 room.

            // First, make sure the members are loaded.
            self.load_members().await;

            let own_user_id = self.session().user().unwrap().user_id();
            let members = self.members();

            if members.n_items() >= 1 {
                // Try to get the member from the list.
                for member in members.iter::<Member>() {
                    match member {
                        Ok(member) => {
                            if member.user_id() != own_user_id
                                && matches!(
                                    member.membership(),
                                    Membership::Join | Membership::Invite
                                )
                            {
                                avatar_data.set_image(member.avatar_data().image());
                                return;
                            }
                        }
                        Err(error) => {
                            debug!("Error iterating through room members: {error}");
                            break;
                        }
                    }
                }
            }
        }

        let avatar_image = avatar_data.image();
        if avatar_image.uri_source() == AvatarUriSource::Room {
            // We can just change the image URI.
            avatar_image.set_uri(avatar_url);
        } else {
            // We need to create an AvatarImage since this one belongs to a user.
            let avatar_image = AvatarImage::new(
                &self.session(),
                avatar_url.as_deref(),
                AvatarUriSource::Room,
            );
            avatar_data.set_image(avatar_image);
        }
    }
}

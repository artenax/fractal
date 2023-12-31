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
use matrix_sdk::{
    attachment::{generate_image_thumbnail, AttachmentConfig, AttachmentInfo, Thumbnail},
    deserialized_responses::{MemberEvent, SyncOrStrippedState, SyncTimelineEvent},
    room::Room as MatrixRoom,
    sync::{JoinedRoom, LeftRoom},
    DisplayName, Result as MatrixResult, RoomMemberships, RoomState,
};
use ruma::{
    events::{
        reaction::ReactionEventContent,
        receipt::{ReceiptEventContent, ReceiptType},
        relation::Annotation,
        room::power_levels::{PowerLevelAction, RoomPowerLevelsEventContent},
        tag::{TagInfo, TagName},
        typing::TypingEventContent,
        AnyMessageLikeEventContent, AnyRoomAccountDataEvent, AnySyncStateEvent,
        AnySyncTimelineEvent, SyncEphemeralRoomEvent, SyncStateEvent,
    },
    OwnedEventId, OwnedRoomId, OwnedUserId, RoomId,
};
use tracing::{debug, error, warn};

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
use super::{
    AvatarData, AvatarImage, AvatarUriSource, IdentityVerification, Session, SidebarItem,
    SidebarItemImpl, User,
};
use crate::{components::Pill, gettext_f, prelude::*, spawn, spawn_tokio};

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
        pub members: WeakRef<MemberList>,
        /// The user who sent the invite to this room. This is only set when
        /// this room is an invitation.
        pub inviter: RefCell<Option<Member>>,
        pub power_levels: RefCell<PowerLevels>,
        /// The timestamp of the room's latest activity.
        ///
        /// This is the timestamp of the latest event that counts as possibly
        /// unread.
        ///
        /// If it is not known, it will return `0`.
        pub latest_activity: Cell<u64>,
        /// Whether all messages of this room are read.
        pub is_read: Cell<bool>,
        /// The highlight state of the room,
        pub highlight: Cell<HighlightFlags>,
        /// The ID of the room that was upgraded and that this one replaces.
        pub predecessor_id: OnceCell<OwnedRoomId>,
        /// The ID of the successor of this Room, if this room was upgraded.
        pub successor_id: OnceCell<OwnedRoomId>,
        /// The successor of this Room, if this room was upgraded.
        pub successor: WeakRef<super::Room>,
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
                    glib::ParamSpecUInt64::builder("latest-activity")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("is-read")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<MemberList>("members")
                        .read_only()
                        .build(),
                    glib::ParamSpecString::builder("predecessor-id")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("is-tombstoned")
                        .read_only()
                        .build(),
                    glib::ParamSpecString::builder("successor-id")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<super::Room>("successor")
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
                "latest-activity" => obj.latest_activity().to_value(),
                "is-read" => obj.is_read().to_value(),
                "predecessor-id" => obj.predecessor_id().map(|id| id.as_str()).to_value(),
                "is-tombstoned" => obj.is_tombstoned().to_value(),
                "successor-id" => obj.successor_id().map(|id| id.as_str()).to_value(),
                "successor" => obj.successor().to_value(),
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

            self.timeline
                .get()
                .unwrap()
                .sdk_items()
                .connect_items_changed(clone!(@weak obj => move |_, _, _, _| {
                    obj.update_is_read();
                }));

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
                .sync_create()
                .build();

            if !matches!(obj.category(), RoomType::Left | RoomType::Outdated) {
                // Load the room history when idle
                spawn!(
                    glib::source::Priority::LOW,
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

        imp.matrix_room.replace(Some(matrix_room));

        self.load_display_name();
        self.load_predecessor();
        self.load_tombstone();
        self.load_category();
        self.setup_receipts();
        self.setup_typing();

        spawn!(clone!(@weak self as obj => async move {
            obj.load_inviter().await;
        }));
    }

    /// The state of the room.
    pub fn state(&self) -> RoomState {
        self.matrix_room().state()
    }

    /// Forget a room that is left.
    pub async fn forget(&self) -> MatrixResult<()> {
        if self.category() != RoomType::Left {
            warn!("Cannot forget a room that is not left");
            return Ok(());
        }

        let matrix_room = self.matrix_room();

        let handle = spawn_tokio!(async move { matrix_room.forget().await });

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
        let old_category = self.category();

        if old_category == RoomType::Outdated || old_category == category {
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
            match matrix_room.state() {
                RoomState::Invited => match category {
                    RoomType::Invited => {}
                    RoomType::Favorite => {
                        if let Some(tags) = matrix_room.tags().await? {
                            if !tags.contains_key(&TagName::Favorite) {
                                matrix_room
                                    .set_tag(TagName::Favorite, TagInfo::new())
                                    .await?;
                            }
                            if tags.contains_key(&TagName::LowPriority) {
                                matrix_room.remove_tag(TagName::LowPriority).await?;
                            }
                        }
                        matrix_room.join().await?;
                    }
                    RoomType::Normal => {
                        if let Some(tags) = matrix_room.tags().await? {
                            if tags.contains_key(&TagName::Favorite) {
                                matrix_room.remove_tag(TagName::Favorite).await?;
                            }
                            if tags.contains_key(&TagName::LowPriority) {
                                matrix_room.remove_tag(TagName::LowPriority).await?;
                            }
                        }

                        if matrix_room.is_direct().await.unwrap_or_default() {
                            matrix_room.set_is_direct(false).await?;
                        }

                        matrix_room.join().await?;
                    }
                    RoomType::LowPriority => {
                        if let Some(tags) = matrix_room.tags().await? {
                            if tags.contains_key(&TagName::Favorite) {
                                matrix_room.remove_tag(TagName::Favorite).await?;
                            }
                            if !tags.contains_key(&TagName::LowPriority) {
                                matrix_room
                                    .set_tag(TagName::LowPriority, TagInfo::new())
                                    .await?;
                            }
                        }
                        matrix_room.join().await?;
                    }
                    RoomType::Left => {
                        matrix_room.leave().await?;
                    }
                    RoomType::Outdated => unimplemented!(),
                    RoomType::Space => unimplemented!(),
                    RoomType::Direct => {
                        if !matrix_room.is_direct().await.unwrap_or_default() {
                            matrix_room.set_is_direct(true).await?;
                        }

                        if let Some(tags) = matrix_room.tags().await? {
                            if tags.contains_key(&TagName::Favorite) {
                                matrix_room.remove_tag(TagName::Favorite).await?;
                            }
                            if tags.contains_key(&TagName::LowPriority) {
                                matrix_room.remove_tag(TagName::LowPriority).await?;
                            }
                        }

                        matrix_room.join().await?;
                    }
                },
                RoomState::Joined => match category {
                    RoomType::Invited => {}
                    RoomType::Favorite => {
                        matrix_room
                            .set_tag(TagName::Favorite, TagInfo::new())
                            .await?;
                        if previous_category == RoomType::LowPriority {
                            matrix_room.remove_tag(TagName::LowPriority).await?;
                        }
                    }
                    RoomType::Normal => {
                        if matrix_room.is_direct().await.unwrap_or_default() {
                            matrix_room.set_is_direct(false).await?;
                        }
                        match previous_category {
                            RoomType::Favorite => {
                                matrix_room.remove_tag(TagName::Favorite).await?;
                            }
                            RoomType::LowPriority => {
                                matrix_room.remove_tag(TagName::LowPriority).await?;
                            }
                            _ => {}
                        }
                    }
                    RoomType::LowPriority => {
                        matrix_room
                            .set_tag(TagName::LowPriority, TagInfo::new())
                            .await?;
                        if previous_category == RoomType::Favorite {
                            matrix_room.remove_tag(TagName::Favorite).await?;
                        }
                    }
                    RoomType::Left => {
                        matrix_room.leave().await?;
                    }
                    RoomType::Outdated => unimplemented!(),
                    RoomType::Space => unimplemented!(),
                    RoomType::Direct => {
                        if !matrix_room.is_direct().await.unwrap_or_default() {
                            matrix_room.set_is_direct(true).await?;
                        }

                        if let Some(tags) = matrix_room.tags().await? {
                            if tags.contains_key(&TagName::LowPriority) {
                                matrix_room.remove_tag(TagName::LowPriority).await?;
                            }
                            if tags.contains_key(&TagName::Favorite) {
                                matrix_room.remove_tag(TagName::Favorite).await?;
                            }
                        }
                    }
                },
                RoomState::Left => match category {
                    RoomType::Invited => {}
                    RoomType::Favorite => {
                        if let Some(tags) = matrix_room.tags().await? {
                            if !tags.contains_key(&TagName::Favorite) {
                                matrix_room
                                    .set_tag(TagName::Favorite, TagInfo::new())
                                    .await?;
                            }
                            if tags.contains_key(&TagName::LowPriority) {
                                matrix_room.remove_tag(TagName::LowPriority).await?;
                            }
                        }
                        matrix_room.join().await?;
                    }
                    RoomType::Normal => {
                        if let Some(tags) = matrix_room.tags().await? {
                            if tags.contains_key(&TagName::Favorite) {
                                matrix_room.remove_tag(TagName::Favorite).await?;
                            }
                            if tags.contains_key(&TagName::LowPriority) {
                                matrix_room.remove_tag(TagName::LowPriority).await?;
                            }
                        }
                        matrix_room.join().await?;
                    }
                    RoomType::LowPriority => {
                        if let Some(tags) = matrix_room.tags().await? {
                            if tags.contains_key(&TagName::Favorite) {
                                matrix_room.remove_tag(TagName::Favorite).await?;
                            }
                            if !tags.contains_key(&TagName::LowPriority) {
                                matrix_room
                                    .set_tag(TagName::LowPriority, TagInfo::new())
                                    .await?;
                            }
                        }
                        matrix_room.join().await?;
                    }
                    RoomType::Left => {}
                    RoomType::Outdated => unimplemented!(),
                    RoomType::Space => unimplemented!(),
                    RoomType::Direct => {
                        if !matrix_room.is_direct().await.unwrap_or_default() {
                            matrix_room.set_is_direct(true).await?;
                        }

                        if let Some(tags) = matrix_room.tags().await? {
                            if tags.contains_key(&TagName::LowPriority) {
                                matrix_room.remove_tag(TagName::LowPriority).await?;
                            }
                            if tags.contains_key(&TagName::Favorite) {
                                matrix_room.remove_tag(TagName::Favorite).await?;
                            }
                        }

                        matrix_room.join().await?;
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

        match matrix_room.state() {
            RoomState::Joined => {
                if matrix_room.is_space() {
                    self.set_category_internal(RoomType::Space);
                } else {
                    let matrix_room_clone = matrix_room.clone();
                    let is_direct = spawn_tokio!(async move {
                        matrix_room_clone.is_direct().await.unwrap_or_default()
                    });
                    let tags = spawn_tokio!(async move { matrix_room.tags().await });

                    spawn!(
                        glib::Priority::DEFAULT_IDLE,
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
            RoomState::Invited => self.set_category_internal(RoomType::Invited),
            RoomState::Left => self.set_category_internal(RoomType::Left),
        };
    }

    pub fn typing_list(&self) -> &TypingList {
        &self.imp().typing_list
    }

    fn setup_typing(&self) {
        let matrix_room = self.matrix_room();
        if matrix_room.state() != RoomState::Joined {
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
        // Listen to changes in the read receipts.
        let room_weak = glib::SendWeakRef::from(self.downgrade());
        self.matrix_room().add_event_handler(
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
    }

    async fn handle_receipt_event(&self, content: ReceiptEventContent) {
        let own_user_id = self.session().user().unwrap().user_id();

        for (_event_id, receipts) in content.iter() {
            if let Some(users) = receipts.get(&ReceiptType::Read) {
                if users.contains_key(&own_user_id) {
                    self.update_is_read();
                }
            }
        }
    }

    async fn handle_typing_event(&self, content: TypingEventContent) {
        let typing_list = &self.imp().typing_list;

        let Some(members) = self.members() else {
            // If we don't have a members list, the room is not shown so we don't need to
            // update the typing list.
            typing_list.update(vec![]);
            return;
        };

        let own_user_id = self.session().user().unwrap().user_id();

        let members = content
            .user_ids
            .into_iter()
            .filter_map(|user_id| (user_id != own_user_id).then(|| members.get_or_create(user_id)))
            .collect();

        typing_list.update(members);
    }

    /// The timeline of this room.
    pub fn timeline(&self) -> &Timeline {
        self.imp().timeline.get().unwrap()
    }

    /// The members of this room.
    ///
    /// This creates the [`MemberList`] if no strong reference to it exists.
    pub fn get_or_create_members(&self) -> MemberList {
        let members = &self.imp().members;
        if let Some(list) = members.upgrade() {
            list
        } else {
            let list = MemberList::new(self);
            members.set(Some(&list));
            list
        }
    }

    /// The members of this room, if a strong reference to the list exists.
    pub fn members(&self) -> Option<MemberList> {
        self.imp().members.upgrade()
    }

    fn notify_notification_count(&self) {
        self.notify("notification-count");
    }

    fn update_highlight(&self) {
        let mut highlight = HighlightFlags::empty();

        if matches!(self.category(), RoomType::Left) {
            // Consider that all left rooms are read.
            self.set_highlight(highlight);
            return;
        }

        let counts = self
            .imp()
            .matrix_room
            .borrow()
            .as_ref()
            .unwrap()
            .unread_notification_counts();

        if counts.highlight_count > 0 {
            highlight = HighlightFlags::all();
        } else if counts.notification_count > 0 || !self.is_read() {
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

    fn update_is_read(&self) {
        spawn!(clone!(@weak self as obj => async move {
            if let Some(has_unread) = obj.timeline().has_unread_messages().await {
                obj.set_is_read(!has_unread);
            }

            obj.update_highlight();
        }));
    }

    /// Whether all messages of this room are read.
    pub fn is_read(&self) -> bool {
        self.imp().is_read.get()
    }

    /// Set whether all messages of this room are read.
    pub fn set_is_read(&self, is_read: bool) {
        if is_read == self.is_read() {
            return;
        }

        self.imp().is_read.set(is_read);
        self.notify("is-read");
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
            glib::Priority::DEFAULT_IDLE,
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

    /// Load the member that invited us to this room, when applicable.
    async fn load_inviter(&self) {
        let matrix_room = self.matrix_room();

        if matrix_room.state() != RoomState::Invited {
            return;
        }

        let Some(own_user_id) = self.session().user().map(|user| user.user_id()) else {
            return;
        };

        let matrix_room_clone = matrix_room.clone();
        let handle =
            spawn_tokio!(async move { matrix_room_clone.get_member_no_sync(&own_user_id).await });

        let own_member = match handle.await.unwrap() {
            Ok(Some(member)) => member,
            Ok(None) => return,
            Err(error) => {
                error!("Failed to get room member: {error}");
                return;
            }
        };

        let inviter_id = match &**own_member.event() {
            MemberEvent::Sync(_) => return,
            MemberEvent::Stripped(event) => event.sender.clone(),
        };

        let inviter_id_clone = inviter_id.clone();
        let handle =
            spawn_tokio!(async move { matrix_room.get_member_no_sync(&inviter_id_clone).await });

        let inviter_member = match handle.await.unwrap() {
            Ok(Some(member)) => member,
            Ok(None) => return,
            Err(error) => {
                error!("Failed to get room member: {error}");
                return;
            }
        };

        let inviter = Member::new(self, &inviter_id);
        inviter.update_from_room_member(&inviter_member);

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
                        if let Some(members) = self.members() {
                            members.update_member_for_member_event(event);
                        }

                        // If we show the other user's avatar or name, a member event might change
                        // one of them.
                        self.load_display_name();
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
                        self.load_display_name();
                    }
                    AnySyncStateEvent::RoomTopic(_) => {
                        self.notify("topic");
                    }
                    AnySyncStateEvent::RoomPowerLevels(SyncStateEvent::Original(event)) => {
                        self.power_levels().update_from_event(event.clone());
                    }
                    AnySyncStateEvent::RoomTombstone(_) => {
                        self.load_tombstone();
                    }
                    _ => {}
                }
            }
        }
        self.session()
            .verification_list()
            .handle_response_room(self.clone(), events);
    }

    /// The timestamp of the room's latest activity.
    ///
    /// This is the timestamp of the latest event that counts as possibly
    /// unread.
    ///
    /// If it is not known, it will return `0`.
    pub fn latest_activity(&self) -> u64 {
        self.imp().latest_activity.get()
    }

    /// Set the timestamp of the room's latest possibly unread event.
    fn set_latest_activity(&self, latest_activity: u64) {
        if latest_activity == self.latest_activity() {
            return;
        }

        self.imp().latest_activity.set(latest_activity);
        self.notify("latest-activity");
    }

    fn load_power_levels(&self) {
        let matrix_room = self.matrix_room();
        let handle = spawn_tokio!(async move {
            let state_event = match matrix_room
                .get_state_event_static::<RoomPowerLevelsEventContent>()
                .await
            {
                Ok(state_event) => state_event,
                Err(error) => {
                    error!("Initial load of room power levels failed: {error}");
                    return None;
                }
            };

            state_event
                .and_then(|r| r.deserialize().ok())
                .and_then(|ev| match ev {
                    SyncOrStrippedState::Sync(SyncStateEvent::Original(e)) => Some(e),
                    _ => None,
                })
        });

        spawn!(
            glib::Priority::DEFAULT_IDLE,
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

        let handle = spawn_tokio!(async move { timeline.send(content).await });

        spawn!(
            glib::Priority::DEFAULT_IDLE,
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
        let matrix_room = self.matrix_room();
        if matrix_room.state() != RoomState::Joined {
            return;
        };

        let handle = spawn_tokio!(async move {
            matrix_room
                .redact(&redacted_event_id, reason.as_deref(), None)
                .await
        });

        spawn!(
            glib::Priority::DEFAULT_IDLE,
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
        let matrix_room = self.matrix_room();
        if matrix_room.state() != RoomState::Joined {
            return;
        };

        let handle = spawn_tokio!(async move { matrix_room.typing_notice(is_typing).await });

        spawn!(
            glib::Priority::DEFAULT_IDLE,
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

        if matrix_room.state() != RoomState::Invited {
            error!("Can’t accept invite, because this room isn’t an invited room");
            return Ok(());
        }

        let handle = spawn_tokio!(async move { matrix_room.join().await });
        match handle.await.unwrap() {
            Ok(_) => Ok(()),
            Err(error) => {
                error!("Accepting invitation failed: {error}");
                Err(error)
            }
        }
    }

    pub async fn decline_invite(&self) -> MatrixResult<()> {
        let matrix_room = self.matrix_room();

        if matrix_room.state() != RoomState::Invited {
            error!("Cannot decline invite, because this room is not an invited room");
            return Ok(());
        }

        let handle = spawn_tokio!(async move { matrix_room.leave().await });
        match handle.await.unwrap() {
            Ok(_) => Ok(()),
            Err(error) => {
                error!("Declining invitation failed: {error}");

                Err(error)
            }
        }
    }

    /// Reload the room from the SDK when its state might have changed.
    pub fn update_room(&self) {
        let state = self.matrix_room().state();
        let category = self.category();

        // Check if the previous state was different.
        if category.is_state(state) {
            // Nothing needs to be reloaded.
            return;
        }

        debug!(room_id = %self.room_id(), ?state, "The state of `Room` changed");

        if state == RoomState::Joined {
            if let Some(members) = self.members() {
                // If we where invited or left before, the list was likely not completed or
                // might have changed.
                members.reload();
            }
        }

        self.load_category();
        spawn!(clone!(@weak self as obj => async move {
            obj.load_inviter().await;
        }));
    }

    pub fn handle_left_response(&self, response_room: LeftRoom) {
        self.update_for_events(response_room.timeline.events);
    }

    pub fn handle_joined_response(&self, response_room: JoinedRoom) {
        if response_room
            .account_data
            .iter()
            .any(|e| matches!(e.deserialize(), Ok(AnyRoomAccountDataEvent::Tag(_))))
        {
            self.load_category();
        }

        self.update_for_events(response_room.timeline.events);
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
    pub fn predecessor_id(&self) -> Option<&RoomId> {
        self.imp().predecessor_id.get().map(std::ops::Deref::deref)
    }

    /// Load the predecessor of this room.
    fn load_predecessor(&self) {
        if self.predecessor_id().is_some() {
            return;
        }

        let Some(event) = self.matrix_room().create_content() else {
            return;
        };
        let Some(predecessor) = event.predecessor else {
            return;
        };

        self.imp().predecessor_id.set(predecessor.room_id).unwrap();
        self.notify("predecessor-id");
    }

    /// Whether this room was tombstoned.
    pub fn is_tombstoned(&self) -> bool {
        self.matrix_room().is_tombstoned()
    }

    /// The ID of the successor of this Room, if this room was upgraded.
    pub fn successor_id(&self) -> Option<&RoomId> {
        self.imp().successor_id.get().map(std::ops::Deref::deref)
    }

    /// The successor of this Room, if this room was upgraded and the successor
    /// was joined.
    pub fn successor(&self) -> Option<Room> {
        self.imp().successor.upgrade()
    }

    /// Set the successor of this Room.
    fn set_successor(&self, successor: &Room) {
        self.imp().successor.set(Some(successor));
        self.notify("successor")
    }

    /// Load the tombstone for this room.
    pub fn load_tombstone(&self) {
        let imp = self.imp();

        if !self.is_tombstoned() || self.successor_id().is_some() {
            return;
        }

        if let Some(room_tombstone) = self.matrix_room().tombstone() {
            imp.successor_id
                .set(room_tombstone.replacement_room)
                .unwrap();
            self.notify("successor-id");
        };

        if !self.update_outdated() {
            self.session()
                .room_list()
                .add_tombstoned_room(self.room_id().to_owned());
        }

        self.notify("is-tombstoned");
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

        let session = self.session();
        let room_list = session.room_list();

        if let Some(successor_id) = self.successor_id() {
            if let Some(successor) = room_list.get(successor_id) {
                // The Matrix spec says that we should use the "predecessor" field of the
                // m.room.create event of the successor, not the "successor" field of the
                // m.room.tombstone event, so check it just to be sure.
                if let Some(predecessor_id) = successor.predecessor_id() {
                    if predecessor_id == self.room_id() {
                        self.set_successor(&successor);
                        self.set_category_internal(RoomType::Outdated);
                        return true;
                    }
                }
            }
        }

        // The tombstone event can be redacted and we lose the successor, so search in
        // the room predecessors of other rooms.
        for room in room_list.iter::<Room>() {
            let Ok(room) = room else {
                break;
            };

            if let Some(predecessor_id) = room.predecessor_id() {
                if predecessor_id == self.room_id() {
                    self.set_successor(&room);
                    self.set_category_internal(RoomType::Outdated);
                    return true;
                }
            }
        }

        false
    }

    pub fn send_attachment(
        &self,
        bytes: Vec<u8>,
        mime: mime::Mime,
        body: &str,
        info: AttachmentInfo,
    ) {
        let matrix_room = self.matrix_room();
        if matrix_room.state() != RoomState::Joined {
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
        let matrix_room = self.matrix_room();
        if matrix_room.state() != RoomState::Joined {
            error!("Can’t invite users, because this room isn’t a joined room");
            return Ok(());
        }
        let user_ids: Vec<OwnedUserId> = users.iter().map(|user| user.user_id()).collect();

        let handle = spawn_tokio!(async move {
            let invitations = user_ids
                .iter()
                .map(|user_id| matrix_room.invite_user_by_id(user_id));
            futures_util::future::join_all(invitations).await
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

    /// Update the latest activity of the room with the given events.
    ///
    /// The events must be in reverse chronological order.
    pub fn update_latest_activity<'a>(&self, events: impl IntoIterator<Item = &'a Event>) {
        let mut latest_activity = self.latest_activity();

        for event in events {
            if event.counts_as_unread() {
                latest_activity = latest_activity.max(event.origin_server_ts_u64());
                break;
            }
        }

        self.set_latest_activity(latest_activity);
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
        let mut avatar_url = matrix_room.avatar_url();

        let members_count = if matrix_room.state() == RoomState::Invited {
            // We don't have the members count for invited rooms, use the SDK's
            // members instead.
            let matrix_room_clone = matrix_room.clone();
            spawn_tokio!(async move {
                matrix_room_clone
                    .members_no_sync(RoomMemberships::ACTIVE)
                    .await
            })
            .await
            .unwrap()
            .map(|m| m.len() as u64)
            .unwrap_or_default()
        } else {
            matrix_room.active_members_count()
        };

        // Check if this is a 1-to-1 room to see if we can use a fallback.
        // We don't have the active member count for invited rooms so process them too.
        if avatar_url.is_none() && members_count > 0 && members_count <= 2 {
            let handle =
                spawn_tokio!(async move { matrix_room.members(RoomMemberships::ACTIVE).await });
            let members = match handle.await.unwrap() {
                Ok(m) => m,
                Err(e) => {
                    error!("Failed to load room members: {e}");
                    vec![]
                }
            };

            let own_user_id = self.session().user().unwrap().user_id();
            let mut has_own_member = false;
            let mut other_member = None;

            // Get the other member from the list.
            for member in members {
                if member.user_id() == own_user_id {
                    has_own_member = true;
                } else {
                    other_member = Some(member);
                }

                if has_own_member && other_member.is_some() {
                    break;
                }
            }

            // Fallback to other user's avatar if this is a 1-to-1 room.
            if members_count == 1 || (members_count == 2 && has_own_member) {
                if let Some(other_member) = other_member {
                    avatar_url = other_member.avatar_url().map(ToOwned::to_owned)
                }
            }
        }

        self.avatar_data().image().set_uri(avatar_url);
    }
}

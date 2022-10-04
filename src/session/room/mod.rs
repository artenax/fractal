mod event;
mod event_actions;
mod highlight_flags;
mod member;
mod member_list;
mod member_role;
mod power_levels;
mod reaction_group;
mod reaction_list;
mod room_type;
mod timeline;

use std::{cell::RefCell, path::PathBuf};

use gettextrs::{gettext, ngettext};
use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*};
use log::{debug, error, info, warn};
use matrix_sdk::{
    attachment::AttachmentConfig,
    deserialized_responses::{JoinedRoom, LeftRoom, SyncTimelineEvent},
    room::Room as MatrixRoom,
    ruma::{
        api::client::sync::sync_events::v3::InvitedRoom,
        events::{
            reaction::{ReactionEventContent, Relation as ReactionRelation},
            receipt::{ReceiptEventContent, ReceiptType},
            room::{
                member::MembershipState,
                name::RoomNameEventContent,
                redaction::{OriginalSyncRoomRedactionEvent, RoomRedactionEventContent},
                topic::RoomTopicEventContent,
            },
            room_key::ToDeviceRoomKeyEventContent,
            tag::{TagInfo, TagName},
            AnyRoomAccountDataEvent, AnyStrippedStateEvent, AnySyncStateEvent,
            AnySyncTimelineEvent, MessageLikeUnsigned, OriginalSyncMessageLikeEvent,
            StateEventType, SyncStateEvent, ToDeviceEvent,
        },
        serde::Raw,
        EventId, MilliSecondsSinceUnixEpoch, OwnedEventId, OwnedRoomId, OwnedUserId, RoomId,
    },
    DisplayName, Result as MatrixResult,
};
use ruma::events::{MessageLikeEventContent, SyncEphemeralRoomEvent};

pub use self::{
    event::*,
    event_actions::EventActions,
    highlight_flags::HighlightFlags,
    member::{Member, Membership},
    member_list::MemberList,
    member_role::MemberRole,
    power_levels::{PowerLevel, PowerLevels, RoomAction, POWER_LEVEL_MAX, POWER_LEVEL_MIN},
    reaction_group::ReactionGroup,
    reaction_list::ReactionList,
    room_type::RoomType,
    timeline::*,
};
use super::verification::IdentityVerification;
use crate::{
    components::Pill,
    gettext_f,
    prelude::*,
    session::{
        avatar::update_room_avatar_from_file,
        sidebar::{SidebarItem, SidebarItemImpl},
        Avatar, Session, User,
    },
    spawn, spawn_tokio, toast,
    utils::pending_event_ids,
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
        pub avatar: OnceCell<Avatar>,
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
        pub read_receipt: RefCell<Option<Event>>,
        /// The latest read event in the room's timeline.
        pub latest_read: RefCell<Option<SupportedEvent>>,
        /// The highlight state of the room,
        pub highlight: Cell<HighlightFlags>,
        pub predecessor: OnceCell<OwnedRoomId>,
        pub successor: OnceCell<OwnedRoomId>,
        /// The most recent verification request event.
        pub verification: RefCell<Option<IdentityVerification>>,
        /// Whether this room is encrypted
        pub is_encrypted: Cell<bool>,
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
                    glib::ParamSpecString::new(
                        "room-id",
                        "Room id",
                        "The room id of this Room",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecObject::new(
                        "session",
                        "Session",
                        "The session",
                        Session::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecString::new(
                        "display-name",
                        "Display Name",
                        "The display name of this room",
                        None,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecObject::new(
                        "inviter",
                        "Inviter",
                        "The user who sent the invite to this room, this is only set when this room represents an invite",
                        Member::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "avatar",
                        "Avatar",
                        "The Avatar of this room",
                        Avatar::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "timeline",
                        "Timeline",
                        "The timeline of this room",
                        Timeline::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecFlags::new(
                        "highlight",
                        "Highlight",
                        "How this room is highlighted",
                        HighlightFlags::static_type(),
                        HighlightFlags::default().bits(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecUInt64::new(
                        "notification-count",
                        "Notification count",
                        "The notification count of this room",
                        std::u64::MIN,
                        std::u64::MAX,
                        0,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecEnum::new(
                        "category",
                        "Category",
                        "The category of this room",
                        RoomType::static_type(),
                        RoomType::default() as i32,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecString::new(
                        "topic",
                        "Topic",
                        "The topic of this room",
                        None,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecUInt64::new(
                        "latest-unread",
                        "Latest Unread",
                        "Timestamp of the latest possibly unread event",
                        u64::MIN,
                        u64::MAX,
                        u64::default(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "latest-read",
                        "Latest Read",
                        "The latest read event in the room’s timeline",
                        Event::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "members",
                        "Members",
                        "Model of the room’s members",
                        MemberList::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecString::new(
                        "predecessor",
                        "Predecessor",
                        "The room id of predecessor of this Room",
                        None,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecString::new(
                        "successor",
                        "Successor",
                        "The room id of successor of this Room",
                        None,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "verification",
                        "Verification",
                        "The most recent active verification for a user in this room",
                        IdentityVerification::static_type(),
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "encrypted",
                        "Encrypted",
                        "Whether this room is encrypted",
                        false,
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
                "session" => self.session.set(value.get().ok().as_ref()),
                "display-name" => {
                    let room_name = value.get().unwrap();
                    obj.store_room_name(room_name)
                }
                "category" => {
                    let category = value.get().unwrap();
                    obj.set_category(category);
                }
                "room-id" => self
                    .room_id
                    .set(RoomId::parse(value.get::<&str>().unwrap()).unwrap())
                    .unwrap(),
                "topic" => {
                    let topic = value.get().unwrap();
                    obj.store_topic(topic);
                }
                "verification" => obj.set_verification(value.get().unwrap()),
                "encrypted" => obj.set_is_encrypted(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room-id" => obj.room_id().as_str().to_value(),
                "session" => obj.session().to_value(),
                "inviter" => obj.inviter().to_value(),
                "display-name" => obj.display_name().to_value(),
                "avatar" => obj.avatar().to_value(),
                "timeline" => self.timeline.get().unwrap().to_value(),
                "category" => obj.category().to_value(),
                "highlight" => obj.highlight().to_value(),
                "topic" => obj.topic().to_value(),
                "members" => obj.members().to_value(),
                "notification-count" => obj.notification_count().to_value(),
                "latest-unread" => obj.latest_unread().to_value(),
                "latest-read" => obj.latest_read().to_value(),
                "predecessor" => obj.predecessor().map_or_else(
                    || {
                        let none: Option<&str> = None;
                        none.to_value()
                    },
                    |id| id.as_ref().to_value(),
                ),
                "successor" => obj.successor().map_or_else(
                    || {
                        let none: Option<&str> = None;
                        none.to_value()
                    },
                    |id| id.as_ref().to_value(),
                ),
                "verification" => obj.verification().to_value(),
                "encrypted" => obj.is_encrypted().to_value(),
                _ => unimplemented!(),
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("order-changed", &[], <()>::static_type().into()).build(),
                    Signal::builder("room-forgotten", &[], <()>::static_type().into()).build(),
                    Signal::builder("new-encryption-keys", &[], <()>::static_type().into()).build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            obj.set_matrix_room(obj.session().client().get_room(obj.room_id()).unwrap());
            self.timeline.set(Timeline::new(obj)).unwrap();
            self.members.set(MemberList::new(obj)).unwrap();
            self.avatar
                .set(Avatar::new(
                    &obj.session(),
                    obj.matrix_room().avatar_url().as_deref(),
                ))
                .unwrap();

            obj.load_power_levels();
            obj.setup_is_encrypted();

            obj.bind_property("display-name", obj.avatar(), "display-name")
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
        glib::Object::new(&[("session", session), ("room-id", &room_id.to_string())])
            .expect("Failed to create Room")
    }

    pub fn session(&self) -> Session {
        self.imp().session.upgrade().unwrap()
    }

    pub fn room_id(&self) -> &RoomId {
        self.imp().room_id.get().unwrap()
    }

    /// Whether this room is a DM
    pub fn is_direct(&self) -> bool {
        self.imp()
            .matrix_room
            .borrow()
            .as_ref()
            .unwrap()
            .is_direct()
    }

    pub fn matrix_room(&self) -> MatrixRoom {
        self.imp().matrix_room.borrow().as_ref().unwrap().clone()
    }

    /// Set the new sdk room struct represented by this `Room`
    fn set_matrix_room(&self, matrix_room: MatrixRoom) {
        let priv_ = self.imp();

        // Check if the previous type was different
        if let Some(ref old_matrix_room) = *priv_.matrix_room.borrow() {
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

        priv_.matrix_room.replace(Some(matrix_room));

        self.load_display_name();
        self.load_predecessor();
        self.load_successor();
        self.load_category();
        self.setup_receipts();
    }

    /// Forget a room that is left.
    pub fn forget(&self) {
        if self.category() != RoomType::Left {
            warn!("Cannot forget a room that is not left");
            return;
        }

        let matrix_room = self.matrix_room();

        let handle = spawn_tokio!(async move {
            match matrix_room {
                MatrixRoom::Left(room) => room.forget().await,
                _ => unimplemented!(),
            }
        });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(_) => {
                        obj.emit_by_name::<()>("room-forgotten", &[]);
                    }
                    Err(error) => {
                        error!("Couldn’t forget the room: {}", error);

                        toast!(
                            obj.session(),
                            // Translators: Do NOT translate the content between '{' and '}', this is a variable name.
                            gettext("Failed to forget {room}."),
                            @room = &obj,
                        );

                        // Load the previous category
                        obj.load_category();
                    },
                };
            })
        );
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
        self.emit_by_name::<()>("order-changed", &[]);
    }

    /// Set the category of this room.
    ///
    /// This makes the necessary to propagate the category to the homeserver.
    ///
    /// Note: Rooms can't be moved to the invite category and they can't be
    /// moved once they are upgraded.
    pub fn set_category(&self, category: RoomType) {
        let matrix_room = self.matrix_room();
        let previous_category = self.category();

        if previous_category == category {
            return;
        }

        if previous_category == RoomType::Outdated {
            warn!("Can't set the category of an upgraded room");
            return;
        }

        match category {
            RoomType::Invited => {
                warn!("Rooms can’t be moved to the invite Category");
                return;
            }
            RoomType::Outdated => {
                // Outdated rooms don't need to propagate anything to the server
                self.set_category_internal(category);
                return;
            }
            _ => {}
        }

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

                        if room.is_direct() {
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
                        if !room.is_direct() {
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
                        if room.is_direct() {
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
                        if !room.is_direct() {
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
                        if !room.is_direct() {
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

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                        Ok(_) => {},
                        Err(error) => {
                            error!("Couldn’t set the room category: {}", error);

                            toast!(
                                obj.session(),
                                gettext(
                                    // Translators: Do NOT translate the content between '{' and '}', this is a variable name.
                                    "Failed to move {room} from {previous_category} to {new_category}.",
                                ),
                                @room = obj,
                                previous_category = previous_category.to_string(),
                                new_category = category.to_string(),
                            );

                            // Load the previous category
                            obj.load_category();
                        },
                };
            })
        );

        self.set_category_internal(category);
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
                    let is_direct = matrix_room.is_direct();
                    let handle = spawn_tokio!(async move { matrix_room.tags().await });

                    spawn!(
                        glib::PRIORITY_DEFAULT_IDLE,
                        clone!(@weak self as obj => async move {
                            let mut category = if is_direct {
                                        RoomType::Direct
                                    } else {
                                        RoomType::Normal
                                    };

                            if let Ok(Some(tags)) = handle.await.unwrap() {
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

    fn setup_receipts(&self) {
        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                let user_id = obj.session().user().unwrap().user_id();
                let matrix_room = obj.matrix_room();

                let handle = spawn_tokio!(async move { matrix_room.user_read_receipt(&user_id).await });

                match handle.await.unwrap() {
                    Ok(Some((event_id, _))) => {
                        obj.update_read_receipt(&event_id).await;
                    },
                    Err(error) => {
                        error!(
                            "Couldn’t get the user’s read receipt for room {}: {}",
                            obj.room_id(),
                            error
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
                        self.update_read_receipt(event_id.as_ref()).await;
                        return;
                    }
                }
            }
        }
    }

    /// Update the user's read receipt event for this room with the given event
    /// ID.
    async fn update_read_receipt(&self, event_id: &EventId) {
        if Some(event_id)
            == self
                .read_receipt()
                .and_then(|event| event.event_id())
                .as_deref()
        {
            return;
        }

        match self.timeline().fetch_event_by_id(event_id).await {
            Ok(read_receipt) => {
                self.set_read_receipt(Some(read_receipt));
            }
            Err(error) => {
                error!(
                    "Couldn’t get the event of the user’s read receipt for room {}: {}",
                    self.room_id(),
                    error
                );
            }
        }
    }

    /// The user's read receipt event for this room.
    pub fn read_receipt(&self) -> Option<Event> {
        self.imp().read_receipt.borrow().clone()
    }

    /// Set the user's read receipt event for this room.
    fn set_read_receipt(&self, read_receipt: Option<Event>) {
        if read_receipt == self.read_receipt() {
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
                    .and_then(|obj| obj.downcast_ref::<SupportedEvent>())
                    .and_then(|event| {
                        // The user sent the event so it's the latest read event.
                        // Necessary because we don't get read receipts for the user's own events.
                        if event.sender().user_id() == user_id {
                            return Some(event.to_owned());
                        }

                        // This is the event corresponding to the read receipt.
                        if event == &read_receipt {
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
    pub fn latest_read(&self) -> Option<SupportedEvent> {
        self.imp().latest_read.borrow().clone()
    }

    /// Set the latest read event.
    fn set_latest_read(&self, latest_read: Option<SupportedEvent>) {
        if latest_read == self.latest_read() {
            return;
        }

        self.imp().latest_read.replace(latest_read);
        self.notify("latest-read");
        self.update_highlight();
    }

    pub fn timeline(&self) -> &Timeline {
        self.imp().timeline.get().unwrap()
    }

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
        }

        if counts.notification_count > 0 || self.has_unread_messages() {
            highlight = HighlightFlags::BOLD;
        }

        self.set_highlight(highlight);
    }

    pub fn highlight(&self) -> HighlightFlags {
        self.imp().highlight.get()
    }

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
                        .and_then(|obj| obj.downcast_ref::<SupportedEvent>())
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

    pub fn display_name(&self) -> String {
        let display_name = self.imp().name.borrow().clone();
        display_name.unwrap_or_else(|| gettext("Unknown"))
    }

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
                        Err(error) => error!("Couldn’t fetch display name: {}", error),
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

    /// Updates the Matrix room with the given name.
    pub fn store_room_name(&self, room_name: String) {
        if self.display_name() == room_name {
            return;
        }

        let joined_room = match self.matrix_room() {
            MatrixRoom::Joined(joined_room) => joined_room,
            _ => {
                error!("Room name can’t be changed when not a member.");
                return;
            }
        };

        let name_content = RoomNameEventContent::new(Some(room_name));

        let handle = spawn_tokio!(async move { joined_room.send_state_event(name_content).await });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(_room_name) => info!("Successfully updated room name"),
                    Err(error) => error!("Couldn’t update room name: {}", error),
                };
            })
        );
    }

    pub fn avatar(&self) -> &Avatar {
        self.imp().avatar.get().unwrap()
    }

    pub fn topic(&self) -> Option<String> {
        self.matrix_room()
            .topic()
            .filter(|topic| !topic.is_empty() && topic.find(|c: char| !c.is_whitespace()).is_some())
    }

    /// Updates the Matrix room with the given topic.
    pub fn store_topic(&self, topic: String) {
        if self.topic().as_ref() == Some(&topic) {
            return;
        }

        let joined_room = match self.matrix_room() {
            MatrixRoom::Joined(joined_room) => joined_room,
            _ => {
                error!("Room topic can’t be changed when not a member.");
                return;
            }
        };

        let handle = spawn_tokio!(async move {
            joined_room
                .send_state_event(RoomTopicEventContent::new(topic))
                .await
        });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(_topic) => info!("Successfully updated room topic"),
                    Err(error) => error!("Couldn’t update topic: {}", error),
                };
            })
        );
    }

    pub fn power_levels(&self) -> PowerLevels {
        self.imp().power_levels.borrow().clone()
    }

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
                        self.members().update_member_for_member_event(event)
                    }
                    AnySyncStateEvent::RoomAvatar(SyncStateEvent::Original(event)) => {
                        self.avatar().set_url(event.content.url.to_owned());
                    }
                    AnySyncStateEvent::RoomName(_) => {
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
        self.update_latest_unread(events.iter());

        self.emit_by_name::<()>("order-changed", &[]);
    }

    /// The timestamp of the room's latest possibly unread event.
    ///
    /// If it is not known, it will return 0.
    pub fn latest_unread(&self) -> u64 {
        self.imp().latest_unread.get()
    }

    /// Set the timestamp of the room's latest possibly unread event.
    pub fn set_latest_unread(&self, latest_unread: u64) {
        if latest_unread == self.latest_unread() {
            return;
        }

        self.imp().latest_unread.set(latest_unread);
        self.notify("latest-unread");
        self.update_highlight();
        // Necessary because we don't get read receipts for the user's own events.
        self.update_latest_read();
    }

    pub fn load_members(&self) {
        let priv_ = self.imp();
        if priv_.members_loaded.get() {
            return;
        }

        priv_.members_loaded.set(true);
        let matrix_room = self.matrix_room();
        let handle = spawn_tokio!(async move { matrix_room.members().await });
        spawn!(
            glib::PRIORITY_LOW,
            clone!(@weak self as obj => async move {
                // FIXME: We should retry to load the room members if the request failed
                let priv_ = obj.imp();
                match handle.await.unwrap() {
                    Ok(members) => {
                        // Add all members needed to display room events.
                        let members: Vec<_> = members.into_iter().filter(|member| {
                            &MembershipState::Leave != member.membership()
                        }).collect();
                        obj.members().update_from_room_members(&members);
                    },
                    Err(error) => {
                        priv_.members_loaded.set(false);
                        error!("Couldn’t load room members: {}", error)
                    },
                };
            })
        );
    }

    fn load_power_levels(&self) {
        let matrix_room = self.matrix_room();
        let handle = spawn_tokio!(async move {
            let state_event = match matrix_room
                .get_state_event(StateEventType::RoomPowerLevels, "")
                .await
            {
                Ok(state_event) => state_event,
                Err(e) => {
                    error!("Initial load of room power levels failed: {}", e);
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
    pub fn send_room_message_event(&self, content: impl MessageLikeEventContent + Send + 'static) {
        if let MatrixRoom::Joined(matrix_room) = self.matrix_room() {
            let (txn_id, event_id) = pending_event_ids();
            let matrix_event = OriginalSyncMessageLikeEvent {
                content,
                event_id,
                sender: self.session().user().unwrap().user_id(),
                origin_server_ts: MilliSecondsSinceUnixEpoch::now(),
                unsigned: MessageLikeUnsigned::default(),
            };

            let raw_event: Raw<AnySyncTimelineEvent> = Raw::new(&matrix_event).unwrap().cast();
            let event = SupportedEvent::try_from_event(raw_event.into(), self).unwrap();
            self.imp()
                .timeline
                .get()
                .unwrap()
                .append_pending(&txn_id, event);

            let content = matrix_event.content;

            let handle =
                spawn_tokio!(async move { matrix_room.send(content, Some(&txn_id)).await });

            spawn!(
                glib::PRIORITY_DEFAULT_IDLE,
                clone!(@weak self as obj => async move {
                    // FIXME: We should retry the request if it fails
                    match handle.await.unwrap() {
                            Ok(_) => {},
                            Err(error) => error!("Couldn’t send room message event: {}", error),
                    };
                })
            );
        }
    }

    /// Send a `key` reaction for the `relates_to` event ID in this room.
    pub fn send_reaction(&self, key: String, relates_to: OwnedEventId) {
        self.send_room_message_event(ReactionEventContent::new(ReactionRelation::new(
            relates_to, key,
        )));
    }

    /// Redact `redacted_event_id` in this room because of `reason`.
    pub fn redact(&self, redacted_event_id: OwnedEventId, reason: Option<String>) {
        let (txn_id, event_id) = pending_event_ids();
        let content = if let Some(reason) = reason.as_ref() {
            RoomRedactionEventContent::with_reason(reason.clone())
        } else {
            RoomRedactionEventContent::new()
        };
        let event = OriginalSyncRoomRedactionEvent {
            content,
            redacts: redacted_event_id.clone(),
            event_id,
            sender: self.session().user().unwrap().user_id(),
            origin_server_ts: MilliSecondsSinceUnixEpoch::now(),
            unsigned: MessageLikeUnsigned::default(),
        };

        if let MatrixRoom::Joined(matrix_room) = self.matrix_room() {
            let raw_event: Raw<AnySyncTimelineEvent> = Raw::new(&event).unwrap().cast();
            let event = SupportedEvent::try_from_event(raw_event.into(), self).unwrap();
            self.imp()
                .timeline
                .get()
                .unwrap()
                .append_pending(&txn_id, event);

            let handle = spawn_tokio!(async move {
                matrix_room
                    .redact(&redacted_event_id, reason.as_deref(), Some(txn_id))
                    .await
            });

            spawn!(
                glib::PRIORITY_DEFAULT_IDLE,
                clone!(@weak self as obj => async move {
                    // FIXME: We should retry the request if it fails
                    match handle.await.unwrap() {
                            Ok(_) => {},
                            Err(error) => error!("Couldn’t redadct event: {}", error),
                    };
                })
            );
        }
    }

    /// Creates an expression that is true when the user is allowed the given
    /// action.
    pub fn new_allowed_expr(&self, room_action: RoomAction) -> gtk::ClosureExpression {
        let session = self.session();
        let user_id = session.user().unwrap().user_id();
        let member = self.members().member_by_id(user_id);
        self.power_levels().new_allowed_expr(&member, room_action)
    }

    /// Uploads the given file to the server and makes it the room avatar.
    ///
    /// Removes the avatar if no filename is given.
    pub fn store_avatar(&self, filename: Option<PathBuf>) {
        let matrix_room = self.matrix_room();
        let client = self.session().client();

        let handle = spawn_tokio!(async move {
            update_room_avatar_from_file(&client, &matrix_room, filename.as_ref()).await
        });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as this => async move {
                match handle.await.unwrap() {
                    Ok(_avatar_uri) => info!("Successfully updated room avatar"),
                    Err(error) => error!("Couldn’t update room avatar: {}", error),
                };
            })
        );
    }

    pub async fn accept_invite(&self) -> MatrixResult<()> {
        let matrix_room = self.matrix_room();

        if let MatrixRoom::Invited(matrix_room) = matrix_room {
            let handle = spawn_tokio!(async move { matrix_room.accept_invitation().await });
            match handle.await.unwrap() {
                Ok(_) => Ok(()),
                Err(error) => {
                    error!("Accepting invitation failed: {}", error);

                    toast!(
                        self.session(),
                        gettext(
                            // Translators: Do NOT translate the content between '{' and '}', this
                            // is a variable name.
                            "Failed to accept invitation for {room}. Try again later.",
                        ),
                        @room = self,
                    );

                    Err(error)
                }
            }
        } else {
            error!("Can’t accept invite, because this room isn’t an invited room");
            Ok(())
        }
    }

    pub async fn reject_invite(&self) -> MatrixResult<()> {
        let matrix_room = self.matrix_room();

        if let MatrixRoom::Invited(matrix_room) = matrix_room {
            let handle = spawn_tokio!(async move { matrix_room.reject_invitation().await });
            match handle.await.unwrap() {
                Ok(_) => Ok(()),
                Err(error) => {
                    error!("Rejecting invitation failed: {}", error);

                    toast!(
                        self.session(),
                        gettext(
                            // Translators: Do NOT translate the content between '{' and '}', this
                            // is a variable name.
                            "Failed to reject invitation for {room}. Try again later.",
                        ),
                        @room = self,
                    );

                    Err(error)
                }
            }
        } else {
            error!("Can’t reject invite, because this room isn’t an invited room");
            Ok(())
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
            .events
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
                        error!("Couldn’t deserialize event: {:?}", event);
                        None
                    }
                })
                .collect(),
        )
    }

    pub fn connect_order_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("order-changed", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            f(&obj);
            None
        })
    }

    /// Connect to the signal sent when a room was forgotten.
    pub fn connect_room_forgotten<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("room-forgotten", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            f(&obj);
            None
        })
    }

    pub fn predecessor(&self) -> Option<&RoomId> {
        self.imp().predecessor.get().map(std::ops::Deref::deref)
    }

    fn load_predecessor(&self) -> Option<()> {
        let priv_ = self.imp();

        if priv_.predecessor.get().is_some() {
            return None;
        }

        let event = self.matrix_room().create_content()?;
        let room_id = event.predecessor?.room_id;

        priv_.predecessor.set(room_id).unwrap();
        self.notify("predecessor");
        Some(())
    }

    pub fn successor(&self) -> Option<&RoomId> {
        self.imp().successor.get().map(std::ops::Deref::deref)
    }

    pub fn load_successor(&self) -> Option<()> {
        let priv_ = self.imp();

        if priv_.successor.get().is_some() {
            return None;
        }

        let room_id = self.matrix_room().tombstone()?.replacement_room;

        priv_.successor.set(room_id).unwrap();
        self.set_category_internal(RoomType::Outdated);
        self.notify("successor");

        Some(())
    }

    pub fn send_attachment(&self, bytes: Vec<u8>, mime: mime::Mime, body: &str) {
        let matrix_room = self.matrix_room();

        if let MatrixRoom::Joined(matrix_room) = matrix_room {
            let body = body.to_string();
            spawn_tokio!(async move {
                let config = AttachmentConfig::default();
                matrix_room
                    // TODO This should be added to pending messages instead of
                    // sending it directly.
                    .send_attachment(&body, &mime, &bytes, config)
                    .await
                    .unwrap();
            });
        }
    }

    pub async fn invite(&self, users: &[User]) {
        let matrix_room = self.matrix_room();
        let user_ids: Vec<OwnedUserId> = users.iter().map(|user| user.user_id()).collect();

        if let MatrixRoom::Joined(matrix_room) = matrix_room {
            let handle = spawn_tokio!(async move {
                let invitiations = user_ids
                    .iter()
                    .map(|user_id| matrix_room.invite_user_by_id(user_id));
                futures::future::join_all(invitiations).await
            });

            let mut failed_invites: Vec<User> = Vec::new();
            for (index, result) in handle.await.unwrap().iter().enumerate() {
                match result {
                    Ok(_) => {}
                    Err(error) => {
                        error!(
                            "Failed to invite user with id {}: {}",
                            users[index].user_id(),
                            error
                        );
                        failed_invites.push(users[index].clone());
                    }
                }
            }

            if !failed_invites.is_empty() {
                let no_failed = failed_invites.len();
                let first_failed = failed_invites.first().unwrap();

                // TODO: should we show all the failed users?
                if no_failed == 1 {
                    toast!(
                        self.session(),
                        gettext(
                            // Translators: Do NOT translate the content between '{' and '}', this
                            // is a variable name.
                            "Failed to invite {user} to {room}. Try again later.",
                        ),
                        @user = first_failed,
                        @room = self,
                    );
                } else {
                    let n = (no_failed - 1) as u32;
                    toast!(
                        self.session(),
                        ngettext(
                            // Translators: Do NOT translate the content between '{' and '}', this
                            // is a variable name.
                            "Failed to invite {user} and 1 other user to {room}. Try again later.",
                            "Failed to invite {user} and {n} other users to {room}. Try again later.",
                            n,
                        ),
                        @user = first_failed,
                        @room = self,
                        n = n.to_string(),
                    );
                };
            }
        } else {
            error!("Can’t invite users, because this room isn’t a joined room");
        }
    }

    pub fn set_verification(&self, verification: IdentityVerification) {
        self.imp().verification.replace(Some(verification));
        self.notify("verification");
    }

    pub fn verification(&self) -> Option<IdentityVerification> {
        self.imp().verification.borrow().clone()
    }

    /// Update the latest possibly unread event of the room with the given
    /// events.
    ///
    /// The events must be in reverse chronological order.
    pub fn update_latest_unread<'a>(&self, events: impl Iterator<Item = &'a AnySyncTimelineEvent>) {
        let mut latest_unread = self.latest_unread();

        for event in events {
            if event::count_as_unread(event) {
                latest_unread = latest_unread.max(event.origin_server_ts().get().into());
                break;
            }
        }

        self.set_latest_unread(latest_unread);
    }

    pub fn is_encrypted(&self) -> bool {
        self.imp().is_encrypted.get()
    }

    pub fn set_is_encrypted(&self, is_encrypted: bool) {
        let was_encrypted = self.is_encrypted();
        if was_encrypted == is_encrypted {
            return;
        }

        if was_encrypted && !is_encrypted {
            error!("Encryption for a room can't be disabled");
            return;
        }

        if self.matrix_room().is_encrypted() != is_encrypted {
            // TODO: enable encryption if it isn't enabled yet
        }

        self.setup_is_encrypted();
    }

    fn setup_is_encrypted(&self) {
        if !self.matrix_room().is_encrypted() {
            return;
        }
        self.setup_new_encryption_keys_handler();
        self.imp().is_encrypted.set(true);
        self.notify("encrypted");
    }

    fn setup_new_encryption_keys_handler(&self) {
        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                let obj_weak = glib::SendWeakRef::from(obj.downgrade());
                obj.matrix_room().add_event_handler(
                    move |_: ToDeviceEvent<ToDeviceRoomKeyEventContent>| {
                        let obj_weak = obj_weak.clone();
                        async move {
                            let ctx = glib::MainContext::default();
                            ctx.spawn(async move {
                                if let Some(room) = obj_weak.upgrade() {
                                    room.emit_by_name::<()>("new-encryption-keys", &[]);
                                }
                            });
                        }
                    },
                );
            })
        );
    }

    pub fn connect_new_encryption_keys<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("new-encryption-keys", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();

            f(&obj);

            None
        })
    }

    /// Get a `Pill` representing this `Room`.
    pub fn to_pill(&self) -> Pill {
        Pill::for_room(self)
    }
}

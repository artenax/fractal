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

use std::{cell::RefCell, convert::TryInto, ops::Deref, path::PathBuf, sync::Arc};

use gettextrs::gettext;
use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*};
use log::{debug, error, info, warn};
use matrix_sdk::{
    attachment::AttachmentConfig,
    deserialized_responses::{JoinedRoom, LeftRoom, SyncRoomEvent},
    room::Room as MatrixRoom,
    ruma::{
        api::client::sync::sync_events::v3::InvitedRoom,
        events::{
            reaction::{ReactionEventContent, Relation},
            receipt::ReceiptEventContent,
            room::{
                member::MembershipState,
                name::RoomNameEventContent,
                redaction::{RoomRedactionEventContent, SyncRoomRedactionEvent},
                topic::RoomTopicEventContent,
            },
            tag::{TagInfo, TagName},
            AnyRoomAccountDataEvent, AnyStateEventContent, AnyStrippedStateEvent, AnySyncRoomEvent,
            AnySyncStateEvent, EventContent, MessageLikeEventType, MessageLikeUnsigned,
            StateEventType, SyncMessageLikeEvent,
        },
        receipt::ReceiptType,
        serde::Raw,
        EventId, MilliSecondsSinceUnixEpoch, RoomId, UserId,
    },
};
use ruma::events::SyncEphemeralRoomEvent;

pub use self::{
    event::Event,
    event_actions::EventActions,
    highlight_flags::HighlightFlags,
    member::{Member, Membership},
    member_role::MemberRole,
    power_levels::{PowerLevel, PowerLevels, RoomAction, POWER_LEVEL_MAX, POWER_LEVEL_MIN},
    reaction_group::ReactionGroup,
    reaction_list::ReactionList,
    room_type::RoomType,
    timeline::{
        Timeline, TimelineDayDivider, TimelineItem, TimelineItemExt, TimelineNewMessagesDivider,
        TimelineSpinner, TimelineState,
    },
};
use super::verification::IdentityVerification;
use crate::{
    components::{Pill, Toast},
    gettext_f, ngettext_f,
    prelude::*,
    session::{
        avatar::update_room_avatar_from_file,
        room::member_list::MemberList,
        sidebar::{SidebarItem, SidebarItemImpl},
        Avatar, Session, User,
    },
    spawn, spawn_tokio,
    utils::pending_event_ids,
};

mod imp {
    use std::cell::Cell;

    use glib::{object::WeakRef, subclass::Signal};
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Default)]
    pub struct Room {
        pub room_id: OnceCell<Box<RoomId>>,
        pub matrix_room: RefCell<Option<MatrixRoom>>,
        pub session: OnceCell<WeakRef<Session>>,
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
        /// The timestamp of the latest message in the room.
        pub latest_change: Cell<u64>,
        /// The event of the user's read receipt for this room.
        pub read_receipt: RefCell<Option<Event>>,
        /// The latest read event in the room's timeline.
        pub latest_read: RefCell<Option<Event>>,
        /// The highlight state of the room,
        pub highlight: Cell<HighlightFlags>,
        pub predecessor: OnceCell<Box<RoomId>>,
        pub successor: OnceCell<Box<RoomId>>,
        /// The most recent verification request event.
        pub verification: RefCell<Option<IdentityVerification>>,
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
                        "latest-change",
                        "Latest Change",
                        "Timestamp of the latest message",
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
                "session" => self
                    .session
                    .set(value.get::<Session>().unwrap().downgrade())
                    .unwrap(),
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
                "latest-change" => obj.latest_change().to_value(),
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
                _ => unimplemented!(),
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("order-changed", &[], <()>::static_type().into()).build(),
                    Signal::builder("room-forgotten", &[], <()>::static_type().into()).build(),
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
        self.imp().session.get().unwrap().upgrade().unwrap()
    }

    pub fn room_id(&self) -> &RoomId {
        self.imp().room_id.get().unwrap()
    }

    fn matrix_room(&self) -> MatrixRoom {
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

                            let room_pill = Pill::for_room(&obj);
                            let error = Toast::builder()
                                // Translators: Do NOT translate the content between '{' and '}', this is a variable name.
                                .title(&gettext_f("Failed to forget {room}.", &[("room", "<widget>")]))
                                .widgets(&[&room_pill])
                                .build();

                            if let Some(window) = obj.session().parent_window() {
                                window.add_toast(&error);
                            }

                            // Load the previous category
                            obj.load_category();
                    },
                };
            })
        );
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
                    RoomType::Invited => Ok(()),
                    RoomType::Favorite => {
                        room.accept_invitation().await
                        // TODO: set favorite tag
                    }
                    RoomType::Normal => {
                        room.accept_invitation().await
                        // TODO: remove tags
                    }
                    RoomType::LowPriority => {
                        room.accept_invitation().await
                        // TODO: set low priority tag
                    }
                    RoomType::Left => room.reject_invitation().await,
                    RoomType::Outdated => unimplemented!(),
                    RoomType::Space => unimplemented!(),
                },
                MatrixRoom::Joined(room) => match category {
                    RoomType::Invited => Ok(()),
                    RoomType::Favorite => {
                        room.set_tag(TagName::Favorite, TagInfo::new()).await?;
                        if previous_category == RoomType::LowPriority {
                            room.remove_tag(TagName::LowPriority).await?;
                        }
                        Ok(())
                    }
                    RoomType::Normal => {
                        match previous_category {
                            RoomType::Favorite => {
                                room.remove_tag(TagName::Favorite).await?;
                            }
                            RoomType::LowPriority => {
                                room.remove_tag(TagName::LowPriority).await?;
                            }
                            _ => {}
                        }
                        Ok(())
                    }
                    RoomType::LowPriority => {
                        room.set_tag(TagName::LowPriority, TagInfo::new()).await?;
                        if previous_category == RoomType::Favorite {
                            room.remove_tag(TagName::Favorite).await?;
                        }
                        Ok(())
                    }
                    RoomType::Left => room.leave().await,
                    RoomType::Outdated => unimplemented!(),
                    RoomType::Space => unimplemented!(),
                },
                MatrixRoom::Left(room) => match category {
                    RoomType::Invited => Ok(()),
                    RoomType::Favorite => {
                        room.join().await
                        // TODO: set favorite tag
                    }
                    RoomType::Normal => {
                        room.join().await
                        // TODO: remove tags
                    }
                    RoomType::LowPriority => {
                        room.join().await
                        // TODO: set low priority tag
                    }
                    RoomType::Left => Ok(()),
                    RoomType::Outdated => unimplemented!(),
                    RoomType::Space => unimplemented!(),
                },
            }
        });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                        Ok(_) => {},
                        Err(error) => {
                                error!("Couldn’t set the room category: {}", error);

                                let room_pill = Pill::for_room(&obj);
                                let error = Toast::builder()
                                    .title(&gettext_f(
                                        // Translators: Do NOT translate the content between '{' and '}', this is a variable name.
                                        "Failed to move {room} from {previous_category} to {new_category}.",
                                        &[("room", "<widget>"),("previous_category", &previous_category.to_string()), ("new_category", &category.to_string())],
                                    ))
                                    .widgets(&[&room_pill])
                                    .build();

                                if let Some(window) = obj.session().parent_window() {
                                    window.add_toast(&error);
                                }

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
                    let handle = spawn_tokio!(async move { matrix_room.tags().await });

                    spawn!(
                        glib::PRIORITY_DEFAULT_IDLE,
                        clone!(@weak self as obj => async move {
                            let mut category = RoomType::Normal;

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
                obj.session().client().register_event_handler(
                    move |event: SyncEphemeralRoomEvent<ReceiptEventContent>, matrix_room: MatrixRoom| {
                        let room_weak = room_weak.clone();
                        async move {
                            let ctx = glib::MainContext::default();
                            ctx.spawn(async move {
                                spawn!(async move {
                                    if let Some(obj) = room_weak.upgrade() {
                                        if matrix_room.room_id() == obj.room_id() {
                                            obj.handle_receipt_event(event.content).await
                                        }
                                    }
                                });
                            });
                        }
                    },
                )
                .await;
            })
        );
    }

    async fn handle_receipt_event(&self, content: ReceiptEventContent) {
        let user_id = self.session().user().unwrap().user_id();

        for (event_id, receipts) in content.iter() {
            if let Some(users) = receipts.get(&ReceiptType::Read) {
                for user in users.keys() {
                    if user == user_id.as_ref() {
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
                .map(|event| event.matrix_event_id())
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
                    .and_then(|obj| obj.downcast_ref::<Event>())
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
                        if event
                            .matrix_event()
                            .filter(|event| can_be_latest_change(event, &user_id))
                            .is_some()
                            && event.matrix_origin_server_ts()
                                <= read_receipt.matrix_origin_server_ts()
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
                        .and_then(|obj| obj.downcast_ref::<Event>())
                    {
                        // This is the event corresponding to the read receipt so there's no unread
                        // messages.
                        if event == latest_read {
                            return true;
                        }

                        let user_id = self.session().user().unwrap().user_id();
                        // The user hasn't read the latest message.
                        if event
                            .matrix_event()
                            .filter(|event| can_be_latest_change(event, &user_id))
                            .is_some()
                        {
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
                        Ok(display_name) => obj.set_display_name(Some(display_name)),
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
        let room_name = match room_name.try_into() {
            Ok(room_name) => room_name,
            Err(e) => {
                error!("Invalid room name: {}", e);
                return;
            }
        };
        let name_content = RoomNameEventContent::new(Some(room_name));

        let handle = spawn_tokio!(async move {
            let content = AnyStateEventContent::RoomName(name_content);
            joined_room.send_state_event(content, "").await
        });

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
            let content = AnyStateEventContent::RoomTopic(RoomTopicEventContent::new(topic));
            joined_room.send_state_event(content, "").await
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
                event.sender == inviter_id
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
    pub fn update_for_events(&self, batch: Vec<SyncRoomEvent>) {
        // FIXME: notify only when the count has changed
        self.notify_notification_count();

        let events: Vec<_> = batch
            .iter()
            .flat_map(|e| e.event.deserialize().ok())
            .collect();

        for event in events.iter() {
            match event {
                AnySyncRoomEvent::State(AnySyncStateEvent::RoomMember(event)) => {
                    self.members().update_member_for_member_event(event)
                }
                AnySyncRoomEvent::State(AnySyncStateEvent::RoomAvatar(event)) => {
                    self.avatar().set_url(event.content.url.to_owned());
                }
                AnySyncRoomEvent::State(AnySyncStateEvent::RoomName(_)) => {
                    // FIXME: this doesn't take into account changes in the calculated name
                    self.load_display_name()
                }
                AnySyncRoomEvent::State(AnySyncStateEvent::RoomTopic(_)) => {
                    self.notify("topic");
                }
                AnySyncRoomEvent::State(AnySyncStateEvent::RoomPowerLevels(event)) => {
                    self.power_levels().update_from_event(event.clone());
                }
                AnySyncRoomEvent::State(AnySyncStateEvent::RoomTombstone(_)) => {
                    self.load_successor();
                }
                _ => {}
            }
        }
        self.session()
            .verification_list()
            .handle_response_room(self, events.iter());

        self.emit_by_name::<()>("order-changed", &[]);
    }

    /// The timestamp of the room's latest message.
    ///
    /// If it is not known, it will return 0.
    pub fn latest_change(&self) -> u64 {
        self.imp().latest_change.get()
    }

    /// Set the timestamp of the room's latest message.
    pub fn set_latest_change(&self, latest_change: u64) {
        if latest_change == self.latest_change() {
            return;
        }

        self.imp().latest_change.set(latest_change);
        self.notify("latest-change");
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
        let handle = spawn_tokio!(async move { matrix_room.active_members().await });
        spawn!(
            glib::PRIORITY_LOW,
            clone!(@weak self as obj => async move {
                // FIXME: We should retry to load the room members if the request failed
                let priv_ = obj.imp();
                match handle.await.unwrap() {
                    Ok(members) => {
                        // Add all members needed to display room events.
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
                    if let AnySyncStateEvent::RoomPowerLevels(e) = e {
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
    pub fn send_room_message_event(
        &self,
        content: impl EventContent<EventType = MessageLikeEventType> + Send + 'static,
    ) {
        if let MatrixRoom::Joined(matrix_room) = self.matrix_room() {
            let (txn_id, event_id) = pending_event_ids();
            let matrix_event = SyncMessageLikeEvent {
                content,
                event_id,
                sender: self.session().user().unwrap().user_id().as_ref().to_owned(),
                origin_server_ts: MilliSecondsSinceUnixEpoch::now(),
                unsigned: MessageLikeUnsigned::default(),
            };

            let raw_event: Raw<AnySyncRoomEvent> = Raw::new(&matrix_event).unwrap().cast();
            let event = Event::new(raw_event.into(), self);
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
    pub fn send_reaction(&self, key: String, relates_to: Box<EventId>) {
        self.send_room_message_event(ReactionEventContent::new(Relation::new(relates_to, key)));
    }

    /// Redact `redacted_event_id` in this room because of `reason`.
    pub fn redact(&self, redacted_event_id: Box<EventId>, reason: Option<String>) {
        let (txn_id, event_id) = pending_event_ids();
        let content = if let Some(reason) = reason.as_ref() {
            RoomRedactionEventContent::with_reason(reason.clone())
        } else {
            RoomRedactionEventContent::new()
        };
        let event = SyncRoomRedactionEvent {
            content,
            redacts: redacted_event_id.clone(),
            event_id,
            sender: self.session().user().unwrap().user_id().as_ref().to_owned(),
            origin_server_ts: MilliSecondsSinceUnixEpoch::now(),
            unsigned: MessageLikeUnsigned::default(),
        };

        if let MatrixRoom::Joined(matrix_room) = self.matrix_room() {
            let raw_event: Raw<AnySyncRoomEvent> = Raw::new(&event).unwrap().cast();
            let event = Event::new(raw_event.into(), self);
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

    pub async fn accept_invite(&self) -> Result<(), Toast> {
        let matrix_room = self.matrix_room();

        if let MatrixRoom::Invited(matrix_room) = matrix_room {
            let handle = spawn_tokio!(async move { matrix_room.accept_invitation().await });
            match handle.await.unwrap() {
                Ok(result) => Ok(result),
                Err(error) => {
                    error!("Accepting invitation failed: {}", error);

                    let room_pill = Pill::for_room(self);
                    let error = Toast::builder()
                        .title(&gettext_f(
                            // Translators: Do NOT translate the content between '{' and '}', this
                            // is a variable name.
                            "Failed to accept invitation for {room}. Try again later.",
                            &[("room", "<widget>")],
                        ))
                        .widgets(&[&room_pill])
                        .build();

                    if let Some(window) = self.session().parent_window() {
                        window.add_toast(&error);
                    }

                    Err(error)
                }
            }
        } else {
            error!("Can’t accept invite, because this room isn’t an invited room");
            Ok(())
        }
    }

    pub async fn reject_invite(&self) -> Result<(), Toast> {
        let matrix_room = self.matrix_room();

        if let MatrixRoom::Invited(matrix_room) = matrix_room {
            let handle = spawn_tokio!(async move { matrix_room.reject_invitation().await });
            match handle.await.unwrap() {
                Ok(result) => Ok(result),
                Err(error) => {
                    error!("Rejecting invitation failed: {}", error);

                    let room_pill = Pill::for_room(self);
                    let error = Toast::builder()
                        .title(&gettext_f(
                            // Translators: Do NOT translate the content between '{' and '}', this
                            // is a variable name.
                            "Failed to reject invitation for {room}. Try again later.",
                            &[("room", "<widget>")],
                        ))
                        .widgets(&[&room_pill])
                        .build();

                    if let Some(window) = self.session().parent_window() {
                        window.add_toast(&error);
                    }

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

    pub fn send_attachment(&self, bytes: &glib::Bytes, mime: mime::Mime, body: &str) {
        let matrix_room = self.matrix_room();

        if let MatrixRoom::Joined(matrix_room) = matrix_room {
            let body = body.to_string();
            spawn_tokio!(glib::clone!(@strong bytes => async move {
                let config = AttachmentConfig::default();
                let mut cursor = std::io::Cursor::new(bytes.deref());
                matrix_room
                    // TODO This should be added to pending messages instead of
                    // sending it directly.
                    .send_attachment(&body, &mime, &mut cursor, config)
                    .await
                    .unwrap();
            }));
        }
    }

    pub async fn invite(&self, users: &[User]) {
        let matrix_room = self.matrix_room();
        let user_ids: Vec<Arc<UserId>> = users.iter().map(|user| user.user_id()).collect();

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
                let error_message =
                    if no_failed == 1 {
                        gettext_f(
                            // Translators: Do NOT translate the content between '{' and '}', this
                            // is a variable name.
                            "Failed to invite {user} to {room}. Try again later.",
                            &[("user", "<widget>"), ("room", "<widget>")],
                        )
                    } else {
                        let n = (no_failed - 1) as u32;
                        ngettext_f(
                        // Translators: Do NOT translate the content between '{' and '}', this
                        // is a variable name.
                        "Failed to invite {user} and 1 other user to {room}. Try again later.",
                        "Failed to invite {user} and {n} other users to {room}. Try again later.",
                        n,
                        &[("user", "<widget>"), ("room", "<widget>"), ("n", &n.to_string())],
                    )
                    };
                let user_pill = Pill::for_user(first_failed);
                let room_pill = Pill::for_room(self);
                let error = Toast::builder()
                    .title(&error_message)
                    .widgets(&[&user_pill, &room_pill])
                    .build();

                if let Some(window) = self.session().parent_window() {
                    window.add_toast(&error);
                }
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

    /// Update the latest change of the room with the given events.
    ///
    /// The events must be in reverse chronological order.
    pub fn update_latest_change<'a>(&self, events: impl Iterator<Item = &'a AnySyncRoomEvent>) {
        let user_id = self.session().user().unwrap().user_id();
        let mut latest_change = self.latest_change();

        for event in events {
            if can_be_latest_change(event, &user_id) {
                latest_change = latest_change.max(event.origin_server_ts().get().into());
                break;
            }
        }

        self.set_latest_change(latest_change);
    }
}

trait GlibDateTime {
    /// Creates a glib::DateTime from the given unix time.
    fn from_unix_millis_utc(
        unix_time: &MilliSecondsSinceUnixEpoch,
    ) -> Result<glib::DateTime, glib::BoolError> {
        let millis: f64 = unix_time.get().into();
        let unix_epoch = glib::DateTime::from_unix_utc(0)?;
        unix_epoch.add_seconds(millis / 1000.0)
    }
}
impl GlibDateTime for glib::DateTime {}

/// Whether the given event can be used as the `latest_change` of a room.
///
/// `user_id` must be the `UserId` of the current account's user.
///
/// This means that the event is a message, or it is the state event of the
/// user joining the room, which should be the oldest possible change.
fn can_be_latest_change(event: &AnySyncRoomEvent, user_id: &UserId) -> bool {
    matches!(event, AnySyncRoomEvent::MessageLike(_))
        || matches!(event, AnySyncRoomEvent::State(AnySyncStateEvent::RoomMember(event))
            if event.state_key == user_id.as_str()
            && event.content.membership == MembershipState::Join
            && event.unsigned.prev_content.as_ref()
                    .filter(|content| content.membership == MembershipState::Join).is_none()
        )
}

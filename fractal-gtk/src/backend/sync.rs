use crate::globals;
use crate::model::{
    member::Member,
    room::{Room, RoomMembership, RoomTag},
};
use matrix_sdk::api::r0::filter::Filter as EventFilter;
use matrix_sdk::api::r0::filter::FilterDefinition;
use matrix_sdk::api::r0::filter::LazyLoadOptions;
use matrix_sdk::api::r0::filter::RoomEventFilter;
use matrix_sdk::api::r0::filter::RoomFilter;
use matrix_sdk::api::r0::sync::sync_events::Filter;
use matrix_sdk::api::r0::sync::sync_events::JoinedRoom;
use matrix_sdk::api::r0::sync::sync_events::Response;
use matrix_sdk::api::r0::sync::sync_events::UnreadNotificationsCount;
use matrix_sdk::assign;
use matrix_sdk::events::room::member::MemberEventContent;
use matrix_sdk::events::AnyEphemeralRoomEventContent;
use matrix_sdk::events::AnySyncMessageEvent;
use matrix_sdk::events::AnySyncRoomEvent;
use matrix_sdk::events::AnySyncStateEvent;
use matrix_sdk::events::StateEvent;
use matrix_sdk::SyncSettings;

use log::{error, warn};
use matrix_sdk::identifiers::{EventId, RoomId, UserId};
use matrix_sdk::Client as MatrixClient;
use matrix_sdk::Error as MatrixError;
use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use super::{get_ruma_client_error, remove_matrix_access_token_if_present, HandleError};
use crate::app::App;
use crate::APPOP;

pub enum RoomElement {
    Name(RoomId, String),
    Topic(RoomId, String),
    NewAvatar(RoomId),
    MemberEvent(StateEvent<MemberEventContent>),
    RemoveMessage(RoomId, EventId),
}

#[derive(Debug)]
pub struct SyncError(MatrixError, u32);

impl HandleError for SyncError {
    fn handle_error(&self) {
        let err_str = format!("{:?}", self.0);
        error!(
            "SYNC Error: {}",
            remove_matrix_access_token_if_present(&err_str).unwrap_or(err_str)
        );
        let new_number_tries = self.1 + 1;
        APPOP!(sync_error, (new_number_tries));
    }
}

pub struct SyncRet {
    // Only new rooms if it's an initial sync
    pub rooms: Vec<Room>,
    pub next_batch: String,
    // None if it's an initial sync
    pub updates: Option<SyncUpdates>,
}

pub struct SyncUpdates {
    pub room_notifications: HashMap<RoomId, UnreadNotificationsCount>,
    // TODO: Typing events should become RoomElements
    pub typing_events_as_rooms: Vec<Room>,
    pub new_events: Vec<RoomElement>,
}

pub async fn sync(
    session_client: MatrixClient,
    user_id: UserId,
    since: Option<String>,
    number_tries: u32,
) -> Result<SyncRet, SyncError> {
    let initial = since.is_none();
    let timeline_not_types = [String::from("m.call.*")];
    let timeline_types = [String::from("m.room.message"), String::from("m.sticker")];
    let state_types = [String::from("m.room.*")];
    let sync_settings = if !initial {
        SyncSettings::new().timeout(Duration::from_secs(30))
    } else {
        // Don't filter event fields, it breaks deserialization.
        // Clearly the Matrix API is very static-typing-unfriendly right now.
        let filter = assign!(FilterDefinition::empty(), {
            presence: assign!(EventFilter::empty(), {
                types: Some(&[]),
            }),
            room: assign!(RoomFilter::empty(), {
                timeline: assign!(RoomEventFilter::empty(), {
                    not_types: &timeline_not_types,
                    limit: Some(globals::PAGE_LIMIT.into()),
                    types: Some(&timeline_types),
                }),
                ephemeral: assign!(RoomEventFilter::empty(), {
                    types: Some(&[]),
                }),
                state: assign!(RoomEventFilter::empty(), {
                    types: Some(&state_types),
                    lazy_load_options: LazyLoadOptions::Enabled {
                        include_redundant_members: false,
                    },
                }),
            }),
        });

        SyncSettings::new().filter(Filter::FilterDefinition(filter))
    };

    let sync_settings = match since.clone() {
        Some(sync_token) => sync_settings.token(sync_token),
        None => sync_settings,
    };

    match session_client.sync_once(sync_settings).await {
        Ok(response) => Ok(transform_sync_response(response, since.is_some(), user_id)),
        Err(err) => {
            // we wait if there's an error to avoid 100% CPU
            // we wait even longer, if it's a 429 (Too Many Requests) error
            let waiting_time = Duration::from_secs(match get_ruma_client_error(&err) {
                Some(ruma_err) if ruma_err.status_code.as_u16() == 429 => {
                    10 * 2_u64.pow(number_tries)
                }
                _ => 10,
            });
            error!(
                "Sync Error, waiting {} seconds to respond for the next sync",
                waiting_time.as_secs()
            );
            tokio::time::delay_for(waiting_time).await;

            Err(SyncError(err, number_tries))
        }
    }
}

fn transform_sync_response(response: Response, has_since: bool, user_id: UserId) -> SyncRet {
    SyncRet {
        rooms: Room::from_sync_response(&response, user_id.clone()),
        next_batch: response.next_batch,
        updates: if !has_since {
            None
        } else {
            Some(get_sync_updates(&response.rooms.join, &user_id))
        },
    }
}

fn get_sync_updates(join: &BTreeMap<RoomId, JoinedRoom>, user_id: &UserId) -> SyncUpdates {
    SyncUpdates {
        room_notifications: join
            .iter()
            .map(|(k, room)| (k.clone(), room.unread_notifications.clone()))
            .collect(),
        typing_events_as_rooms: join
            .iter()
            .map(|(k, room)| {
                let typing: Vec<Member> = room.ephemeral.events
                    .iter()
                    .map(|ev| ev.deserialize())
                    .inspect(|result_ev| if let Err(err) = result_ev {
                        warn!("Bad event: {}", err);
                    })
                    .filter_map(Result::ok)
                    .filter_map(|event| match event.content() {
                        AnyEphemeralRoomEventContent::Typing(content) => {
                            Some(content.user_ids)
                        }
                        _ => None,
                    })
                    .flatten()
                    // ignoring the user typing notifications
                    .filter(|user| user != user_id)
                    .map(|uid| Member {
                        uid,
                        alias: None,
                        avatar: None,
                    })
                    .collect();

                Room {
                    typing_users: typing,
                    ..Room::new(k.clone(), RoomMembership::Joined(RoomTag::None))
                }
            })
            .collect(),
        new_events: join
            .iter()
            .flat_map(|(room_id, room)| {
                let room_id = room_id.clone();
                room.timeline
                    .events
                    .iter()
                    .map(move |ev| Ok((room_id.clone(), ev.deserialize()?)))
            })
            .inspect(|result_ev: &Result<_, serde_json::Error>| {
                if let Err(err) = result_ev {
                    warn!("Bad event: {}", err);
                }
            })
            .filter_map(Result::ok)
            .filter_map(|(room_id, event)| {
                match event {
                    AnySyncRoomEvent::State(AnySyncStateEvent::RoomName(ev)) => {
                        let name = ev.content.name().map(Into::into).unwrap_or_default();
                        Some(RoomElement::Name(room_id, name))
                    }
                    AnySyncRoomEvent::State(AnySyncStateEvent::RoomTopic(ev)) => {
                        Some(RoomElement::Topic(room_id, ev.content.topic))
                    }
                    AnySyncRoomEvent::State(AnySyncStateEvent::RoomAvatar(_)) => {
                        Some(RoomElement::NewAvatar(room_id))
                    }
                    AnySyncRoomEvent::State(AnySyncStateEvent::RoomMember(ev)) => {
                        Some(RoomElement::MemberEvent(ev.into_full_event(room_id)))
                    }
                    AnySyncRoomEvent::Message(AnySyncMessageEvent::RoomRedaction(ev)) => {
                        Some(RoomElement::RemoveMessage(room_id, ev.redacts))
                    }
                    AnySyncRoomEvent::Message(AnySyncMessageEvent::RoomMessage(_)) => None,
                    AnySyncRoomEvent::Message(AnySyncMessageEvent::Sticker(_)) => {
                        // This event is managed in the room list
                        None
                    }
                    ev => {
                        error!("EVENT NOT MANAGED: {:?}", ev);
                        None
                    }
                }
            })
            .collect(),
    }
}

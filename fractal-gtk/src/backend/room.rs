use log::error;
use serde_json::json;

use fractal_api::reqwest::Error as ReqwestError;
use fractal_api::url::{ParseError as UrlError, Url};
use fractal_api::{
    identifiers::{Error as IdError, EventId, RoomId, RoomIdOrAliasId, UserId},
    Client as MatrixClient, Error as MatrixError, FromHttpResponseError as RumaResponseError,
    ServerError,
};
use std::fs;
use std::io::Error as IoError;
use std::path::{Path, PathBuf};

use std::convert::TryFrom;
use std::time::Duration;

use crate::globals;

use crate::actions::AppState;
use crate::app::RUNTIME;
use crate::backend::{MediaError, HTTP_CLIENT};
use crate::util::cache_dir_path;

use crate::model::{
    member::Member,
    message::Message,
    room::{Room, RoomMembership, RoomTag},
};
use fractal_api::api::r0::config::get_global_account_data::Request as GetGlobalAccountDataRequest;
use fractal_api::api::r0::config::set_global_account_data::Request as SetGlobalAccountDataRequest;
use fractal_api::api::r0::filter::RoomEventFilter;
use fractal_api::api::r0::message::get_message_events::Request as GetMessagesEventsRequest;
use fractal_api::api::r0::room::create_room::Request as CreateRoomRequest;
use fractal_api::api::r0::room::create_room::RoomPreset;
use fractal_api::api::r0::room::Visibility;
use fractal_api::assign;
use fractal_api::events::room::history_visibility::HistoryVisibility;
use fractal_api::events::room::history_visibility::HistoryVisibilityEventContent;
use fractal_api::events::AnyBasicEventContent;
use fractal_api::events::AnyInitialStateEvent;
use fractal_api::events::EventType;
use fractal_api::events::InitialStateEvent;
use fractal_api::r0::config::set_room_account_data::request as set_room_account_data;
use fractal_api::r0::config::set_room_account_data::Parameters as SetRoomAccountDataParameters;
use fractal_api::r0::media::create_content::request as create_content;
use fractal_api::r0::media::create_content::Parameters as CreateContentParameters;
use fractal_api::r0::media::create_content::Response as CreateContentResponse;
use fractal_api::r0::message::create_message_event::request as create_message_event;
use fractal_api::r0::message::create_message_event::Parameters as CreateMessageEventParameters;
use fractal_api::r0::message::create_message_event::Response as CreateMessageEventResponse;
use fractal_api::r0::pushrules::delete_room_rules::request as delete_room_rules;
use fractal_api::r0::pushrules::delete_room_rules::Parameters as DelRoomRulesParams;
use fractal_api::r0::pushrules::get_room_rules::request as get_room_rules;
use fractal_api::r0::pushrules::get_room_rules::Parameters as GetRoomRulesParams;
use fractal_api::r0::pushrules::set_room_rules::request as set_room_rules;
use fractal_api::r0::pushrules::set_room_rules::Parameters as SetRoomRulesParams;
use fractal_api::r0::read_marker::set_read_marker::request as set_read_marker;
use fractal_api::r0::read_marker::set_read_marker::Body as SetReadMarkerBody;
use fractal_api::r0::read_marker::set_read_marker::Parameters as SetReadMarkerParameters;
use fractal_api::r0::redact::redact_event::request as redact_event;
use fractal_api::r0::redact::redact_event::Body as RedactEventBody;
use fractal_api::r0::redact::redact_event::Parameters as RedactEventParameters;
use fractal_api::r0::redact::redact_event::Response as RedactEventResponse;
use fractal_api::r0::state::create_state_events_for_key::request as create_state_events_for_key;
use fractal_api::r0::state::create_state_events_for_key::Parameters as CreateStateEventsForKeyParameters;
use fractal_api::r0::state::get_state_events_for_key::request as get_state_events_for_key;
use fractal_api::r0::state::get_state_events_for_key::Parameters as GetStateEventsForKeyParameters;
use fractal_api::r0::sync::get_joined_members::request as get_joined_members;
use fractal_api::r0::sync::get_joined_members::Parameters as JoinedMembersParameters;
use fractal_api::r0::sync::get_joined_members::Response as JoinedMembersResponse;
use fractal_api::r0::sync::sync_events::Language;
use fractal_api::r0::tag::create_tag::request as create_tag;
use fractal_api::r0::tag::create_tag::Body as CreateTagBody;
use fractal_api::r0::tag::create_tag::Parameters as CreateTagParameters;
use fractal_api::r0::tag::delete_tag::request as delete_tag;
use fractal_api::r0::tag::delete_tag::Parameters as DeleteTagParameters;
use fractal_api::r0::typing::request as send_typing_notification;
use fractal_api::r0::typing::Body as TypingNotificationBody;
use fractal_api::r0::typing::Parameters as TypingNotificationParameters;
use fractal_api::r0::AccessToken;

use serde_json::value::to_raw_value;
use serde_json::Value as JsonValue;

use super::{
    dw_media, get_prev_batch_from, remove_matrix_access_token_if_present, ContentType, HandleError,
};
use crate::app::App;
use crate::util::i18n::i18n;
use crate::APPOP;

#[derive(Debug)]
pub enum RoomDetailError {
    MalformedKey,
    Reqwest(ReqwestError),
}

impl From<ReqwestError> for RoomDetailError {
    fn from(err: ReqwestError) -> Self {
        Self::Reqwest(err)
    }
}

impl HandleError for RoomDetailError {}

pub fn get_room_detail(
    base: Url,
    access_token: AccessToken,
    room_id: RoomId,
    key: String,
) -> Result<(RoomId, String, String), RoomDetailError> {
    let k = key.split('.').last().ok_or(RoomDetailError::MalformedKey)?;
    let params = GetStateEventsForKeyParameters { access_token };

    let request = get_state_events_for_key(base, &params, &room_id, &key)?;
    let response: JsonValue = HTTP_CLIENT.get_client().execute(request)?.json()?;

    let value = response[&k].as_str().map(Into::into).unwrap_or_default();

    Ok((room_id, key, value))
}

#[derive(Debug)]
pub enum RoomAvatarError {
    Reqwest(ReqwestError),
    Download(MediaError),
}

impl From<ReqwestError> for RoomAvatarError {
    fn from(err: ReqwestError) -> Self {
        Self::Reqwest(err)
    }
}

impl From<MediaError> for RoomAvatarError {
    fn from(err: MediaError) -> Self {
        Self::Download(err)
    }
}

impl HandleError for RoomAvatarError {}

pub fn get_room_avatar(
    session_client: MatrixClient,
    access_token: AccessToken,
    room_id: RoomId,
) -> Result<(RoomId, Option<Url>), RoomAvatarError> {
    let base = session_client.homeserver().clone();

    let params = GetStateEventsForKeyParameters { access_token };

    let request = get_state_events_for_key(base, &params, &room_id, "m.room.avatar")?;
    let response: JsonValue = HTTP_CLIENT.get_client().execute(request)?.json()?;

    let avatar = response["url"].as_str().and_then(|s| Url::parse(s).ok());
    let dest = cache_dir_path(None, &room_id.to_string()).ok();
    if let Some(ref avatar) = avatar {
        RUNTIME.handle().block_on(dw_media(
            session_client,
            avatar,
            ContentType::default_thumbnail(),
            dest,
        ))?;
    }

    Ok((room_id, avatar))
}

#[derive(Debug)]
pub enum RoomMembersError {
    Reqwest(ReqwestError),
    ParseUrl(UrlError),
}

impl From<ReqwestError> for RoomMembersError {
    fn from(err: ReqwestError) -> Self {
        Self::Reqwest(err)
    }
}

impl From<UrlError> for RoomMembersError {
    fn from(err: UrlError) -> Self {
        Self::ParseUrl(err)
    }
}

impl HandleError for RoomMembersError {}

pub fn get_room_members(
    base: Url,
    access_token: AccessToken,
    room_id: RoomId,
) -> Result<(RoomId, Vec<Member>), RoomMembersError> {
    let params = JoinedMembersParameters { access_token };

    let request = get_joined_members(base, &room_id, &params)?;
    let response: JoinedMembersResponse = HTTP_CLIENT.get_client().execute(request)?.json()?;

    let ms = response
        .joined
        .into_iter()
        .map(Member::try_from)
        .collect::<Result<_, UrlError>>()?;

    Ok((room_id, ms))
}

#[derive(Debug)]
pub enum RoomMessagesToError {
    MessageNotSent,
    Matrix(MatrixError),
    EventsDeserialization(IdError),
}

impl<T: Into<MatrixError>> From<T> for RoomMessagesToError {
    fn from(err: T) -> Self {
        Self::Matrix(err.into())
    }
}

impl HandleError for RoomMessagesToError {}

/* Load older messages starting by prev_batch
 * https://matrix.org/docs/spec/client_server/latest.html#get-matrix-client-r0-rooms-roomid-messages
 */
pub async fn get_room_messages(
    session_client: MatrixClient,
    room_id: RoomId,
    from: &str,
) -> Result<(Vec<Message>, RoomId, Option<String>), RoomMessagesToError> {
    let types = &["m.room.message".into(), "m.sticker".into()];

    let request = assign!(GetMessagesEventsRequest::backward(&room_id, from), {
        to: None,
        limit: globals::PAGE_LIMIT.into(),
        filter: Some(assign!(RoomEventFilter::empty(), {
            types: Some(types),
        })),
    });

    let response = session_client.room_messages(request).await?;

    let prev_batch = response.end;
    let evs = response
        .chunk
        .into_iter()
        .rev()
        .map(|ev| serde_json::to_value(ev.json().get()).unwrap());
    let list = Message::from_json_events(&room_id, evs)
        .map_err(RoomMessagesToError::EventsDeserialization)?;

    Ok((list, room_id, prev_batch))
}

pub async fn get_room_messages_from_msg(
    session_client: MatrixClient,
    room_id: RoomId,
    msg: Message,
) -> Result<(Vec<Message>, RoomId, Option<String>), RoomMessagesToError> {
    let event_id = msg.id.as_ref().ok_or(RoomMessagesToError::MessageNotSent)?;

    // first of all, we calculate the from param using the context api, then we call the
    // normal get_room_messages
    let from = get_prev_batch_from(session_client.clone(), &room_id, event_id).await?;

    get_room_messages(session_client, room_id, &from).await
}

#[derive(Debug)]
pub struct SendMsgError(String);

impl HandleError for SendMsgError {
    fn handle_error(&self) {
        error!("sending {}: retrying send", self.0);
        APPOP!(retry_send);
    }
}

pub fn send_msg(
    base: Url,
    access_token: AccessToken,
    msg: Message,
) -> Result<(String, Option<EventId>), SendMsgError> {
    let room_id: RoomId = msg.room.clone();

    let params = CreateMessageEventParameters { access_token };

    let mut body = json!({
        "body": msg.body,
        "msgtype": msg.mtype,
    });

    if let Some(u) = msg.url.as_ref() {
        body["url"] = json!(u);
    }

    if let (Some(f), Some(f_b)) = (msg.format.as_ref(), msg.formatted_body.as_ref()) {
        body["formatted_body"] = json!(f_b);
        body["format"] = json!(f);
    }

    let extra_content_map = msg
        .extra_content
        .as_ref()
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    for (k, v) in extra_content_map {
        body[k] = v;
    }

    let txn_id = msg.get_txn_id();

    create_message_event(base, &params, &body, &room_id, "m.room.message", &txn_id)
        .and_then(|request| {
            let response = HTTP_CLIENT
                .get_client()
                .execute(request)?
                .json::<CreateMessageEventResponse>()?;

            Ok((txn_id.clone(), response.event_id))
        })
        .or(Err(SendMsgError(txn_id)))
}

#[derive(Debug)]
pub struct SendTypingError(ReqwestError);

impl From<ReqwestError> for SendTypingError {
    fn from(err: ReqwestError) -> Self {
        Self(err)
    }
}

impl HandleError for SendTypingError {}

pub fn send_typing(
    base: Url,
    access_token: AccessToken,
    user_id: UserId,
    room_id: RoomId,
) -> Result<(), SendTypingError> {
    let params = TypingNotificationParameters { access_token };
    let body = TypingNotificationBody::Typing(Duration::from_secs(4));

    let request = send_typing_notification(base, &room_id, &user_id, &params, &body)?;
    HTTP_CLIENT.get_client().execute(request)?;

    Ok(())
}

#[derive(Debug)]
pub enum SendMsgRedactionError {
    MessageNotSent,
    Reqwest(ReqwestError),
}

impl From<ReqwestError> for SendMsgRedactionError {
    fn from(err: ReqwestError) -> Self {
        Self::Reqwest(err)
    }
}

impl HandleError for SendMsgRedactionError {
    fn handle_error(&self) {
        error!("Error deleting message: {:?}", self);
        let error = i18n("Error deleting message");
        APPOP!(show_error, (error));
    }
}

pub fn redact_msg(
    base: Url,
    access_token: AccessToken,
    msg: Message,
) -> Result<(EventId, Option<EventId>), SendMsgRedactionError> {
    let room_id = &msg.room;
    let txn_id = msg.get_txn_id();
    let event_id = msg
        .id
        .as_ref()
        .ok_or(SendMsgRedactionError::MessageNotSent)?;

    let params = RedactEventParameters { access_token };

    let body = RedactEventBody {
        reason: "Deletion requested by the sender".into(),
    };

    let request = redact_event(base, &params, &body, room_id, event_id, &txn_id)?;
    let response: RedactEventResponse = HTTP_CLIENT.get_client().execute(request)?.json()?;

    Ok((event_id.clone(), response.event_id))
}

#[derive(Debug)]
pub struct JoinRoomError(MatrixError);

impl From<MatrixError> for JoinRoomError {
    fn from(err: MatrixError) -> Self {
        Self(err)
    }
}

impl HandleError for JoinRoomError {
    fn handle_error(&self) {
        let (err_str, info) = match &self.0 {
            MatrixError::RumaResponse(RumaResponseError::Http(ServerError::Known(error))) => {
                (error.message.clone(), Some(error.message.clone()))
            }
            error => (error.to_string(), None),
        };

        error!(
            "{}",
            remove_matrix_access_token_if_present(&err_str).unwrap_or(err_str)
        );
        let error = i18n("Can’t join the room, try again.");
        let state = AppState::NoRoom;
        APPOP!(show_error_with_info, (error, info));
        APPOP!(set_state, (state));
    }
}

pub async fn join_room(
    session_client: MatrixClient,
    room_id_or_alias_id: &RoomIdOrAliasId,
) -> Result<RoomId, JoinRoomError> {
    Ok(session_client
        .join_room_by_id_or_alias(room_id_or_alias_id, Default::default())
        .await?
        .room_id)
}

#[derive(Debug)]
pub struct LeaveRoomError(MatrixError);

impl From<MatrixError> for LeaveRoomError {
    fn from(err: MatrixError) -> Self {
        Self(err)
    }
}

impl HandleError for LeaveRoomError {}

pub async fn leave_room(
    session_client: MatrixClient,
    room_id: &RoomId,
) -> Result<(), LeaveRoomError> {
    session_client.leave_room(room_id).await?;

    Ok(())
}

#[derive(Debug)]
pub struct MarkedAsReadError(ReqwestError);

impl From<ReqwestError> for MarkedAsReadError {
    fn from(err: ReqwestError) -> Self {
        Self(err)
    }
}

impl HandleError for MarkedAsReadError {}

pub fn mark_as_read(
    base: Url,
    access_token: AccessToken,
    room_id: RoomId,
    event_id: EventId,
) -> Result<(RoomId, EventId), MarkedAsReadError> {
    let params = SetReadMarkerParameters { access_token };

    let body = SetReadMarkerBody {
        fully_read: event_id.clone(),
        read: Some(event_id.clone()),
    };

    let request = set_read_marker(base, &params, &body, &room_id)?;
    HTTP_CLIENT.get_client().execute(request)?;

    Ok((room_id, event_id))
}
#[derive(Debug)]
pub struct SetRoomNameError(ReqwestError);

impl From<ReqwestError> for SetRoomNameError {
    fn from(err: ReqwestError) -> Self {
        Self(err)
    }
}

impl HandleError for SetRoomNameError {}

pub fn set_room_name(
    base: Url,
    access_token: AccessToken,
    room_id: RoomId,
    name: String,
) -> Result<(), SetRoomNameError> {
    let params = CreateStateEventsForKeyParameters { access_token };

    let body = json!({
        "name": name,
    });

    let request = create_state_events_for_key(base, &params, &body, &room_id, "m.room.name")?;
    HTTP_CLIENT.get_client().execute(request)?;

    Ok(())
}

#[derive(Debug)]
pub struct SetRoomTopicError(ReqwestError);

impl From<ReqwestError> for SetRoomTopicError {
    fn from(err: ReqwestError) -> Self {
        Self(err)
    }
}

impl HandleError for SetRoomTopicError {}

pub fn set_room_topic(
    base: Url,
    access_token: AccessToken,
    room_id: RoomId,
    topic: String,
) -> Result<(), SetRoomTopicError> {
    let params = CreateStateEventsForKeyParameters { access_token };

    let body = json!({
        "topic": topic,
    });

    let request = create_state_events_for_key(base, &params, &body, &room_id, "m.room.topic")?;
    HTTP_CLIENT.get_client().execute(request)?;

    Ok(())
}

#[derive(Debug)]
pub enum SetRoomAvatarError {
    Io(IoError),
    Reqwest(ReqwestError),
    ParseUrl(UrlError),
}

impl From<ReqwestError> for SetRoomAvatarError {
    fn from(err: ReqwestError) -> Self {
        Self::Reqwest(err)
    }
}

impl From<AttachedFileError> for SetRoomAvatarError {
    fn from(err: AttachedFileError) -> Self {
        match err {
            AttachedFileError::Io(err) => Self::Io(err),
            AttachedFileError::Reqwest(err) => Self::Reqwest(err),
            AttachedFileError::ParseUrl(err) => Self::ParseUrl(err),
        }
    }
}

impl HandleError for SetRoomAvatarError {}

pub fn set_room_avatar(
    base: Url,
    access_token: AccessToken,
    room_id: RoomId,
    avatar: PathBuf,
) -> Result<(), SetRoomAvatarError> {
    let params = CreateStateEventsForKeyParameters {
        access_token: access_token.clone(),
    };

    let upload_file_response = upload_file(base.clone(), access_token, &avatar)?;

    let body = json!({ "url": upload_file_response.content_uri.as_str() });
    let request = create_state_events_for_key(base, &params, &body, &room_id, "m.room.avatar")?;
    HTTP_CLIENT.get_client().execute(request)?;

    Ok(())
}

#[derive(Debug)]
pub enum AttachedFileError {
    Io(IoError),
    Reqwest(ReqwestError),
    ParseUrl(UrlError),
}

impl From<ReqwestError> for AttachedFileError {
    fn from(err: ReqwestError) -> Self {
        Self::Reqwest(err)
    }
}

impl From<IoError> for AttachedFileError {
    fn from(err: IoError) -> Self {
        Self::Io(err)
    }
}

impl From<UrlError> for AttachedFileError {
    fn from(err: UrlError) -> Self {
        Self::ParseUrl(err)
    }
}

impl HandleError for AttachedFileError {
    fn handle_error(&self) {
        let err_str = format!("{:?}", self);
        error!(
            "attaching {}: retrying send",
            remove_matrix_access_token_if_present(&err_str).unwrap_or(err_str)
        );
        APPOP!(retry_send);
    }
}

pub fn upload_file(
    base: Url,
    access_token: AccessToken,
    fname: &Path,
) -> Result<CreateContentResponse, AttachedFileError> {
    let params_upload = CreateContentParameters {
        access_token,
        filename: None,
    };

    let contents = fs::read(fname)?;
    let request = create_content(base, &params_upload, contents)?;

    HTTP_CLIENT
        .get_client()
        .execute(request)?
        .json()
        .map_err(Into::into)
}

#[derive(Debug, Clone, Copy)]
pub enum RoomType {
    Public,
    Private,
}

#[derive(Debug)]
pub struct NewRoomError(MatrixError);

impl From<MatrixError> for NewRoomError {
    fn from(err: MatrixError) -> Self {
        Self(err)
    }
}

impl HandleError for NewRoomError {
    fn handle_error(&self) {
        let err_str = format!("{:?}", self);
        error!(
            "{}",
            remove_matrix_access_token_if_present(&err_str).unwrap_or(err_str)
        );

        let error = i18n("Can’t create the room, try again");
        let state = AppState::NoRoom;
        APPOP!(show_error, (error));
        APPOP!(set_state, (state));
    }
}

pub async fn new_room(
    session_client: MatrixClient,
    name: String,
    privacy: RoomType,
) -> Result<Room, NewRoomError> {
    let (visibility, preset) = match privacy {
        RoomType::Public => (Visibility::Public, RoomPreset::PublicChat),
        RoomType::Private => (Visibility::Private, RoomPreset::PrivateChat),
    };

    let request = assign!(CreateRoomRequest::new(), {
        name: Some(&name),
        visibility: visibility,
        preset: Some(preset),
    });

    let response = session_client.create_room(request).await?;

    Ok(Room {
        name: Some(name),
        ..Room::new(response.room_id, RoomMembership::Joined(RoomTag::None))
    })
}

#[derive(Debug)]
pub enum DirectChatError {
    Matrix(MatrixError),
    EventsDeserialization,
}

impl From<MatrixError> for DirectChatError {
    fn from(err: MatrixError) -> Self {
        Self::Matrix(err)
    }
}

impl HandleError for DirectChatError {
    fn handle_error(&self) {
        error!("Can't set m.direct: {:?}", self);
        let err_str = format!("{:?}", self);
        error!(
            "{}",
            remove_matrix_access_token_if_present(&err_str).unwrap_or(err_str)
        );

        let error = i18n("Can’t create the room, try again");
        let state = AppState::NoRoom;
        APPOP!(show_error, (error));
        APPOP!(set_state, (state));
    }
}

async fn update_direct_chats(
    session_client: MatrixClient,
    user_id: &UserId,
    room_id: RoomId,
    user: Member,
) -> Result<(), DirectChatError> {
    let event_type = EventType::Direct;

    let request = GetGlobalAccountDataRequest::new(user_id, event_type.as_ref());
    let response = session_client.send(request).await?;

    let mut directs = match response
        .account_data
        .deserialize()
        .map(|data| data.content())
    {
        Ok(AnyBasicEventContent::Direct(directs)) => directs,
        _ => return Err(DirectChatError::EventsDeserialization),
    };

    directs.entry(user.uid).or_default().push(room_id);

    let request = SetGlobalAccountDataRequest::new(
        to_raw_value(&directs).or(Err(DirectChatError::EventsDeserialization))?,
        event_type.as_ref(),
        user_id,
    );

    session_client.send(request).await?;

    Ok(())
}

pub async fn direct_chat(
    session_client: MatrixClient,
    user_id: &UserId,
    user: Member,
) -> Result<Room, DirectChatError> {
    let invite = &[user.uid.clone()];
    let initial_state = &[AnyInitialStateEvent::RoomHistoryVisibility(
        InitialStateEvent {
            state_key: Default::default(),
            content: HistoryVisibilityEventContent::new(HistoryVisibility::Invited),
        },
    )];

    let request = assign!(CreateRoomRequest::new(), {
        invite,
        visibility: Visibility::Private,
        preset: Some(RoomPreset::PrivateChat),
        is_direct: true,
        initial_state,
    });

    let response = session_client.create_room(request).await?;

    update_direct_chats(
        session_client.clone(),
        user_id,
        response.room_id.clone(),
        user.clone(),
    )
    .await?;

    Ok(Room {
        name: user.alias,
        direct: true,
        ..Room::new(response.room_id, RoomMembership::Joined(RoomTag::None))
    })
}

#[derive(Debug)]
pub struct AddedToFavError(ReqwestError);

impl From<ReqwestError> for AddedToFavError {
    fn from(err: ReqwestError) -> Self {
        Self(err)
    }
}

impl HandleError for AddedToFavError {}

pub fn add_to_fav(
    base: Url,
    access_token: AccessToken,
    user_id: UserId,
    room_id: RoomId,
    tofav: bool,
) -> Result<(RoomId, bool), AddedToFavError> {
    let request = if tofav {
        let params = CreateTagParameters { access_token };
        let body = CreateTagBody { order: Some(0.5) };
        create_tag(base, &user_id, &room_id, "m.favourite", &params, &body)
    } else {
        let params = DeleteTagParameters { access_token };
        delete_tag(base, &user_id, &room_id, "m.favourite", &params)
    }?;

    HTTP_CLIENT.get_client().execute(request)?;

    Ok((room_id, tofav))
}

#[derive(Debug)]
pub struct InviteError(MatrixError);

impl From<MatrixError> for InviteError {
    fn from(err: MatrixError) -> Self {
        Self(err)
    }
}

impl HandleError for InviteError {}

pub async fn invite(
    session_client: MatrixClient,
    room_id: &RoomId,
    user_id: &UserId,
) -> Result<(), InviteError> {
    session_client.invite_user_by_id(room_id, user_id).await?;

    Ok(())
}

#[derive(Debug)]
pub struct ChangeLanguageError(ReqwestError);

impl From<ReqwestError> for ChangeLanguageError {
    fn from(err: ReqwestError) -> Self {
        Self(err)
    }
}

impl HandleError for ChangeLanguageError {
    fn handle_error(&self) {
        let err_str = format!("{:?}", self);
        error!(
            "Error forming url to set room language: {}",
            remove_matrix_access_token_if_present(&err_str).unwrap_or(err_str)
        );
    }
}

pub fn set_language(
    access_token: AccessToken,
    base: Url,
    user_id: UserId,
    room_id: RoomId,
    input_language: String,
) -> Result<(), ChangeLanguageError> {
    let params = SetRoomAccountDataParameters { access_token };

    let body = json!(Language { input_language });

    let request = set_room_account_data(
        base,
        &params,
        &body,
        &user_id,
        &room_id,
        "org.gnome.fractal.language",
    )?;

    HTTP_CLIENT.get_client().execute(request)?;

    Ok(())
}

#[derive(Debug)]
pub enum RoomNotify {
    All,
    DontNotify,
    NotSet,
}

#[derive(Debug)]
pub struct PushRulesError(ReqwestError);

impl From<ReqwestError> for PushRulesError {
    fn from(err: ReqwestError) -> Self {
        Self(err)
    }
}

impl HandleError for PushRulesError {
    fn handle_error(&self) {
        error!("PushRules: {}", self.0);
    }
}

pub fn get_pushrules(
    base: Url,
    access_token: AccessToken,
    room_id: RoomId,
) -> Result<RoomNotify, PushRulesError> {
    let params = GetRoomRulesParams { access_token };

    let request = get_room_rules(base, &params, &room_id)?;

    let mut value = RoomNotify::NotSet;

    let response: Result<JsonValue, _> = HTTP_CLIENT.get_client().execute(request)?.json();

    if let Ok(js) = response {
        if let Some(array) = js["actions"].as_array() {
            for v in array {
                match v.as_str().unwrap_or_default() {
                    "notify" => value = RoomNotify::All,
                    "dont_notify" => value = RoomNotify::DontNotify,
                    _ => {}
                };
            }
        }
    }

    Ok(value)
}

pub fn set_pushrules(
    base: Url,
    access_token: AccessToken,
    room_id: RoomId,
    notify: RoomNotify,
) -> Result<(), PushRulesError> {
    if let RoomNotify::NotSet = notify {
        return delete_pushrules(base, access_token, room_id);
    }

    let params = SetRoomRulesParams { access_token };
    let js = match notify {
        RoomNotify::DontNotify => json!({"actions": ["dont_notify"]}),
        RoomNotify::All => json!({
            "actions": ["notify", {"set_tweak": "sound", "value": "default"}]
        }),
        _ => json!({}),
    };

    let request = set_room_rules(base, &params, &room_id, &js)?;

    HTTP_CLIENT.get_client().execute(request)?;
    Ok(())
}

pub fn delete_pushrules(
    base: Url,
    access_token: AccessToken,
    room_id: RoomId,
) -> Result<(), PushRulesError> {
    let params = DelRoomRulesParams { access_token };
    let request = delete_room_rules(base, &params, &room_id)?;
    HTTP_CLIENT.get_client().execute(request)?;
    Ok(())
}

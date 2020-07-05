use fractal_api::identifiers::{EventId, RoomId};
use std::io;
use std::time::SystemTimeError;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct StandardErrorResponse {
    pub errcode: String,
    pub error: String,
}

type MatrixErrorCode = String;

#[macro_export]
macro_rules! derror {
    ($from: path, $to: path) => {
        impl From<$from> for Error {
            fn from(_: $from) -> Error {
                $to
            }
        }
    };
}

#[derive(Debug)]
pub enum Error {
    BackendError,
    CacheError,
    ReqwestError(fractal_api::reqwest::Error),
    NetworkError(fractal_api::reqwest::StatusCode),
    MatrixError(MatrixErrorCode, String),
    SendMsgError(String),
    SendMsgRedactionError(EventId),
    TokenUsed,
    Denied,
}

impl From<fractal_api::reqwest::Error> for Error {
    fn from(err: fractal_api::reqwest::Error) -> Error {
        Error::ReqwestError(err)
    }
}

impl From<StandardErrorResponse> for Error {
    fn from(resp: StandardErrorResponse) -> Error {
        Error::MatrixError(resp.errcode, resp.error)
    }
}

derror!(fractal_api::url::ParseError, Error::BackendError);
derror!(io::Error, Error::BackendError);
derror!(glib::error::Error, Error::BackendError);
derror!(regex::Error, Error::BackendError);
derror!(fractal_api::identifiers::Error, Error::BackendError);
derror!(SystemTimeError, Error::BackendError);

derror!(serde_json::Error, Error::CacheError);

#[derive(Debug)]
pub enum BKError {
    LoginError(Error),
    SendTypingError(Error),
    InviteError(Error),
    ChangeLanguageError(Error),
    NameError(Error),
    AvatarError(Error),
    MarkedAsReadError(Error),
    UserSearchError(Error),
    LogoutError(Error),
    LeaveRoomError(Error),
    DirectoryProtocolsError(Error),
    RoomMembersError(Error),
    AddedToFavError(Error),
    GetThreePIDError(Error),
    AddThreePIDError(Error),
    SubmitPhoneTokenError(Error),
    SetUserNameError(Error),
    ChangePasswordError(Error),
    AccountDestructionError(Error),
    DeleteThreePIDError(Error),
    GetTokenPhoneError(Error),
    GetTokenEmailError(Error),
    SetRoomNameError(Error),
    SetRoomTopicError(Error),
    SetUserAvatarError(Error),
    SetRoomAvatarError(Error),
    RoomMessagesToError(Error),
    MediaError(Error),
    SentMsgRedactionError(Error),
    JoinRoomError(Error),
    DirectorySearchError(Error),
    NewRoomError(Error, RoomId),
    RoomDetailError(Error),
    RoomAvatarError(Error),
    SentMsgError(Error),
    AttachedFileError(Error),
    RoomsError(Error),
    UpdateRoomsError(Error),
    RoomMessagesError(Error),
    RoomElementError(Error),
    SyncError(Error, u64),
}
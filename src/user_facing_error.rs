use gettextrs::gettext;
use matrix_sdk::{
    ruma::api::{
        client::error::ErrorKind::{Forbidden, LimitExceeded, UserDeactivated},
        error::{FromHttpResponseError, ServerError},
    },
    store::OpenStoreError,
    ClientBuildError, Error, HttpError,
};

pub trait UserFacingError {
    fn to_user_facing(self) -> String;
}

impl UserFacingError for HttpError {
    fn to_user_facing(self) -> String {
        match self {
            HttpError::Reqwest(error) => {
                // TODO: Add more information based on the error
                if error.is_timeout() {
                    gettext("The connection timed out. Try again later.")
                } else {
                    gettext("Unable to connect to the homeserver.")
                }
            }
            HttpError::ClientApi(FromHttpResponseError::Http(ServerError::Known(error))) => {
                match error.kind {
                    Forbidden => gettext("The provided username or password is invalid."),
                    UserDeactivated => gettext("The user is deactivated."),
                    LimitExceeded { retry_after_ms } => {
                        if let Some(ms) = retry_after_ms {
                            gettext!(
                                "You exceeded the homeserver’s rate limit, retry in {} seconds.",
                                ms.as_secs()
                            )
                        } else {
                            gettext("You exceeded the homeserver’s rate limit, try again later.")
                        }
                    }
                    _ => {
                        // TODO: The server may not give us pretty enough error message. We should
                        // add our own error message.
                        error.message
                    }
                }
            }
            _ => gettext("An unknown connection error occurred."),
        }
    }
}

impl UserFacingError for Error {
    fn to_user_facing(self) -> String {
        match self {
            Error::DecryptorError(_) => gettext("Could not decrypt the event"),
            Error::Http(http_error) => http_error.to_user_facing(),
            _ => gettext("An unknown error occurred."),
        }
    }
}

impl UserFacingError for OpenStoreError {
    fn to_user_facing(self) -> String {
        gettext("Could not open the store.")
    }
}

impl UserFacingError for ClientBuildError {
    fn to_user_facing(self) -> String {
        match self {
            ClientBuildError::Url(_) => gettext("This is not a valid URL"),
            ClientBuildError::Http(err) => err.to_user_facing(),
            ClientBuildError::SledStore(err) => err.to_user_facing(),
            _ => gettext("An unknown error occurred."),
        }
    }
}

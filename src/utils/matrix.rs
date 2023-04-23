//! Collection of methods related to the Matrix specification.

use std::path::Path;

use matrix_sdk::{config::RequestConfig, Client, ClientBuildError};
use ruma::{
    events::{room::message::MessageType, AnyMessageLikeEventContent, AnySyncTimelineEvent},
    ServerName,
};
use thiserror::Error;
use url::Url;

use crate::{gettext_f, secret::StoredSession, user_facing_error::UserFacingError};

/// The result of a password validation.
#[derive(Debug, Default, Clone, Copy)]
pub struct PasswordValidity {
    /// Whether the password includes at least one lowercase letter.
    pub has_lowercase: bool,
    /// Whether the password includes at least one uppercase letter.
    pub has_uppercase: bool,
    /// Whether the password includes at least one number.
    pub has_number: bool,
    /// Whether the password includes at least one symbol.
    pub has_symbol: bool,
    /// Whether the password is at least 8 characters long.
    pub has_length: bool,
    /// The percentage of checks passed for the password, between 0 and 100.
    ///
    /// If progress is 100, the password is valid.
    pub progress: u32,
}

impl PasswordValidity {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Validate a password according to the Matrix specification.
///
/// A password should include a lower-case letter, an upper-case letter, a
/// number and a symbol and be at a minimum 8 characters in length.
///
/// See: <https://spec.matrix.org/v1.1/client-server-api/#notes-on-password-management>
pub fn validate_password(password: &str) -> PasswordValidity {
    let mut validity = PasswordValidity::new();

    for char in password.chars() {
        if char.is_numeric() {
            validity.has_number = true;
        } else if char.is_lowercase() {
            validity.has_lowercase = true;
        } else if char.is_uppercase() {
            validity.has_uppercase = true;
        } else {
            validity.has_symbol = true;
        }
    }

    validity.has_length = password.len() >= 8;

    let mut passed = 0;
    if validity.has_number {
        passed += 1;
    }
    if validity.has_lowercase {
        passed += 1;
    }
    if validity.has_uppercase {
        passed += 1;
    }
    if validity.has_symbol {
        passed += 1;
    }
    if validity.has_length {
        passed += 1;
    }
    validity.progress = passed * 100 / 5;

    validity
}

/// Extract the body from the given event.
///
/// Only returns the body for messages.
///
/// If it's a media message, this will return a localized body.
pub fn get_event_body(event: &AnySyncTimelineEvent, sender_name: &str) -> Option<String> {
    let AnySyncTimelineEvent::MessageLike(event) = event else {
        return None;
    };

    match event.original_content()? {
        AnyMessageLikeEventContent::RoomMessage(message) => {
            let body = match message.msgtype {
                MessageType::Audio(_) => {
                    gettext_f("{user} sent an audio file.", &[("user", sender_name)])
                }
                MessageType::Emote(content) => gettext_f(
                    "{user}: {message}",
                    &[("user", sender_name), ("message", &content.body)],
                ),
                MessageType::File(_) => gettext_f("{user} sent a file.", &[("user", sender_name)]),
                MessageType::Image(_) => {
                    gettext_f("{user} sent an image.", &[("user", sender_name)])
                }
                MessageType::Location(_) => {
                    gettext_f("{user} sent their location.", &[("user", sender_name)])
                }
                MessageType::Notice(content) => gettext_f(
                    "{user}: {message}",
                    &[("user", sender_name), ("message", &content.body)],
                ),
                MessageType::ServerNotice(content) => gettext_f(
                    "{user}: {message}",
                    &[("user", sender_name), ("message", &content.body)],
                ),
                MessageType::Text(content) => gettext_f(
                    "{user}: {message}",
                    &[("user", sender_name), ("message", &content.body)],
                ),
                MessageType::Video(_) => {
                    gettext_f("{user} sent a video.", &[("user", sender_name)])
                }
                MessageType::VerificationRequest(_) => gettext_f(
                    "{user} sent a verification request.",
                    &[("user", sender_name)],
                ),
                _ => unimplemented!(),
            };
            Some(body)
        }
        AnyMessageLikeEventContent::Sticker(_) => Some(gettext_f(
            "{user} sent a sticker.",
            &[("user", sender_name)],
        )),
        _ => None,
    }
}

/// A homeserver URL or a server name.
pub enum HomeserverOrServerName<'a> {
    /// A homeserver URL.
    Homeserver(&'a Url),

    /// A server name.
    ServerName(&'a ServerName),
}

/// All errors that can occur when setting up the Matrix client.
#[derive(Error, Debug)]
pub enum ClientSetupError {
    #[error(transparent)]
    Client(#[from] ClientBuildError),
    #[error(transparent)]
    Sdk(#[from] matrix_sdk::Error),
}

impl UserFacingError for ClientSetupError {
    fn to_user_facing(self) -> String {
        match self {
            ClientSetupError::Client(err) => err.to_user_facing(),
            ClientSetupError::Sdk(err) => err.to_user_facing(),
        }
    }
}

/// Create a [`Client`] with the given parameters.
pub async fn client(
    homeserver: HomeserverOrServerName<'_>,
    use_discovery: bool,
    path: &Path,
    passphrase: &str,
) -> Result<Client, ClientBuildError> {
    let builder = match homeserver {
        HomeserverOrServerName::Homeserver(url) => Client::builder().homeserver_url(url),
        HomeserverOrServerName::ServerName(server_name) => {
            Client::builder().server_name(server_name)
        }
    };

    builder
        .sqlite_store(path, Some(passphrase))
        // force_auth option to solve an issue with some servers configuration to require
        // auth for profiles:
        // https://gitlab.gnome.org/GNOME/fractal/-/issues/934
        .request_config(RequestConfig::new().retry_limit(2).force_auth())
        .respect_login_well_known(use_discovery)
        .build()
        .await
}

/// Create a [`Client`] with the given stored session.
pub async fn client_with_stored_session(
    session: &StoredSession,
) -> Result<Client, ClientSetupError> {
    let homeserver = HomeserverOrServerName::Homeserver(&session.homeserver);
    let client = client(homeserver, false, &session.path, &session.secret.passphrase).await?;

    client
        .restore_session(matrix_sdk::Session {
            user_id: session.user_id.clone(),
            device_id: session.device_id.clone(),
            access_token: session.secret.access_token.clone(),
            refresh_token: None,
        })
        .await?;

    Ok(client)
}

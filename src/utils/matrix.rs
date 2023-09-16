//! Collection of methods related to the Matrix specification.

use std::fmt::Write;

use gtk::prelude::*;
use html2pango::html_escape;
use html5gum::{HtmlString, Token, Tokenizer};
use matrix_sdk::{config::RequestConfig, Client, ClientBuildError};
use ruma::{
    events::{room::message::MessageType, AnyMessageLikeEventContent, AnySyncTimelineEvent},
    matrix_uri::MatrixId,
    MatrixToUri, MatrixUri,
};
use thiserror::Error;

use super::media::filename_for_mime;
use crate::{
    components::{Pill, DEFAULT_PLACEHOLDER},
    gettext_f,
    prelude::*,
    secret::StoredSession,
    session::model::{Room, Session},
    spawn_tokio,
};

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

/// Create a [`Client`] with the given stored session.
pub async fn client_with_stored_session(
    session: StoredSession,
) -> Result<Client, ClientSetupError> {
    let (homeserver, path, passphrase, data) = session.into_parts();

    let client = Client::builder()
        .homeserver_url(homeserver)
        .sqlite_store(path, Some(&passphrase))
        // force_auth option to solve an issue with some servers configuration to require
        // auth for profiles:
        // https://gitlab.gnome.org/GNOME/fractal/-/issues/934
        .request_config(RequestConfig::new().retry_limit(2).force_auth())
        .build()
        .await?;

    client.restore_session(data).await?;

    Ok(client)
}

/// Fetch the content of the media message in the given message.
///
/// Compatible messages:
///
/// - File.
/// - Image.
/// - Video.
/// - Audio.
///
/// Returns `Ok((filename, binary_content))` on success.
///
/// Returns `Err` if an error occurred while fetching the content. Panics on
/// an incompatible event.
pub async fn get_media_content(
    client: Client,
    message: MessageType,
) -> Result<(String, Vec<u8>), matrix_sdk::Error> {
    let media = client.media();

    match message {
        MessageType::File(content) => {
            let filename = content
                .filename
                .as_ref()
                .filter(|name| !name.is_empty())
                .or(Some(&content.body))
                .filter(|name| !name.is_empty())
                .cloned()
                .unwrap_or_else(|| {
                    filename_for_mime(
                        content
                            .info
                            .as_ref()
                            .and_then(|info| info.mimetype.as_deref()),
                        None,
                    )
                });
            let handle = spawn_tokio!(async move { media.get_file(content, true).await });
            let data = handle.await.unwrap()?.unwrap();
            Ok((filename, data))
        }
        MessageType::Image(content) => {
            let filename = if content.body.is_empty() {
                filename_for_mime(
                    content
                        .info
                        .as_ref()
                        .and_then(|info| info.mimetype.as_deref()),
                    Some(mime::IMAGE),
                )
            } else {
                content.body.clone()
            };
            let handle = spawn_tokio!(async move { media.get_file(content, true).await });
            let data = handle.await.unwrap()?.unwrap();
            Ok((filename, data))
        }
        MessageType::Video(content) => {
            let filename = if content.body.is_empty() {
                filename_for_mime(
                    content
                        .info
                        .as_ref()
                        .and_then(|info| info.mimetype.as_deref()),
                    Some(mime::VIDEO),
                )
            } else {
                content.body.clone()
            };
            let handle = spawn_tokio!(async move { media.get_file(content, true).await });
            let data = handle.await.unwrap()?.unwrap();
            Ok((filename, data))
        }
        MessageType::Audio(content) => {
            let filename = if content.body.is_empty() {
                filename_for_mime(
                    content
                        .info
                        .as_ref()
                        .and_then(|info| info.mimetype.as_deref()),
                    Some(mime::AUDIO),
                )
            } else {
                content.body.clone()
            };
            let handle = spawn_tokio!(async move { media.get_file(content, true).await });
            let data = handle.await.unwrap()?.unwrap();
            Ok((filename, data))
        }
        _ => {
            panic!("Trying to get the media content of a message of incompatible type");
        }
    }
}

/// Extract mentions from the given string.
///
/// Returns a new string with placeholders and the corresponding widgets and the
/// string they are replacing.
pub fn extract_mentions(s: &str, room: &Room) -> (String, Vec<(Pill, String)>) {
    let session = room.session();
    let mut mentions = Vec::new();
    let mut mention = None;
    let mut new_string = String::new();

    for token in Tokenizer::new(s).infallible() {
        match token {
            Token::StartTag(tag) => {
                if tag.name == HtmlString(b"a".to_vec()) && !tag.self_closing {
                    if let Some(pill) = tag
                        .attributes
                        .get(&HtmlString(b"href".to_vec()))
                        .map(|href| String::from_utf8_lossy(href))
                        .and_then(|s| parse_pill(&s, room, &session))
                    {
                        mention = Some((pill, String::new()));
                        new_string.push_str(DEFAULT_PLACEHOLDER);
                        continue;
                    }
                }

                mention = None;

                // Restore HTML.
                write!(new_string, "<{}", String::from_utf8_lossy(&tag.name)).unwrap();
                for (attr_name, attr_value) in &tag.attributes {
                    write!(
                        new_string,
                        r#" {}="{}""#,
                        String::from_utf8_lossy(attr_name),
                        String::from_utf8_lossy(attr_value),
                    )
                    .unwrap();
                }
                if tag.self_closing {
                    write!(new_string, " /").unwrap();
                }
                write!(new_string, ">").unwrap();
            }
            Token::String(s) => {
                if let Some((_, string)) = &mut mention {
                    write!(string, "{}", String::from_utf8_lossy(&s)).unwrap();
                    continue;
                }

                write!(new_string, "{}", html_escape(&String::from_utf8_lossy(&s))).unwrap();
            }
            Token::EndTag(tag) => {
                if let Some(mention) = mention.take() {
                    mentions.push(mention);
                    continue;
                }

                write!(new_string, "</{}>", String::from_utf8_lossy(&tag.name)).unwrap();
            }
            _ => {}
        }
    }

    (new_string, mentions)
}

/// Try to parse the given string to a Matrix URI and generate a pill for it.
fn parse_pill(s: &str, room: &Room, session: &Session) -> Option<Pill> {
    let uri = html_escape::decode_html_entities(s);

    let id = if let Ok(mx_uri) = MatrixUri::parse(&uri) {
        mx_uri.id().to_owned()
    } else if let Ok(mx_to_uri) = MatrixToUri::parse(&uri) {
        mx_to_uri.id().to_owned()
    } else {
        return None;
    };

    match id {
        MatrixId::Room(room_id) => session
            .room_list()
            .get(&room_id)
            .map(|room| Pill::for_room(&room)),
        MatrixId::RoomAlias(room_alias) => {
            // TODO: Handle non-canonical aliases.
            session
                .client()
                .rooms()
                .iter()
                .find_map(|matrix_room| {
                    matrix_room
                        .canonical_alias()
                        .filter(|alias| alias == &room_alias)
                        .and_then(|_| session.room_list().get(matrix_room.room_id()))
                })
                .map(|room| Pill::for_room(&room))
        }
        MatrixId::User(user_id) => {
            // We should have a strong reference to the list wherever we show a user pill so
            // we can use `get_or_create_members()`.
            let user = room.get_or_create_members().get_or_create(user_id).upcast();
            Some(Pill::for_user(&user))
        }
        _ => None,
    }
}

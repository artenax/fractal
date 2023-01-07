use std::{collections::HashMap, ffi::OsStr, fmt, path::PathBuf, string::FromUtf8Error};

use gettextrs::gettext;
use log::error;
use matrix_sdk::ruma::{DeviceId, OwnedDeviceId, OwnedUserId, UserId};
use oo7::{Item, Keyring};
use serde::{Deserialize, Serialize};
use serde_json::error::Error as JsonError;
use thiserror::Error;
use url::Url;

use crate::{config::APP_ID, gettext_f, user_facing_error::UserFacingError};

const SCHEMA_ATTRIBUTE: &str = "xdg:schema";

/// Any error that can happen when interacting with the secret service.
#[derive(Debug, Error)]
pub enum SecretError {
    /// A corrupted session was found.
    #[error("{0}")]
    CorruptSession(String, Item),

    /// An error occurred interacting with the secret service.
    #[error(transparent)]
    Oo7(#[from] oo7::Error),
}

impl SecretError {
    /// Split `self` between its message and its optional `Item`.
    pub fn into_parts(self) -> (String, Option<Item>) {
        match self {
            SecretError::CorruptSession(message, item) => (message, Some(item)),
            SecretError::Oo7(error) => (error.to_user_facing(), None),
        }
    }
}

impl UserFacingError for oo7::Error {
    fn to_user_facing(self) -> String {
        match self {
            oo7::Error::Portal(error) => error.to_user_facing(),
            oo7::Error::DBus(error) => error.to_user_facing(),
        }
    }
}

impl UserFacingError for oo7::portal::Error {
    fn to_user_facing(self) -> String {
        match self {
            oo7::portal::Error::FileHeaderMismatch(_) |
            oo7::portal::Error::VersionMismatch(_) |
            oo7::portal::Error::NoData |
            oo7::portal::Error::MacError |
            oo7::portal::Error::HashedAttributeMac(_) |
            oo7::portal::Error::GVariantDeserialization(_) |
            oo7::portal::Error::SaltSizeMismatch(_, _) => gettext(
                "The secret storage file is corrupted.",
            ),
            oo7::portal::Error::NoParentDir(_) |
            oo7::portal::Error::NoDataDir => gettext(
                "Could not access the secret storage file location.",
            ),
            oo7::portal::Error::Io(_) => gettext(
                "An unknown error occurred when accessing the secret storage file.",
            ),
            oo7::portal::Error::TargetFileChanged(_) => gettext(
                "The secret storage file has been changed by another process.",
            ),
            oo7::portal::Error::PortalBus(_) => gettext(
                "An unknown error occurred when interacting with the D-Bus Secret Portal backend.",
            ),
            oo7::portal::Error::CancelledPortalRequest => gettext(
                "The request to the Flatpak Secret Portal was cancelled. Make sure to accept any prompt asking to access it.",
            ),
            oo7::portal::Error::PortalNotAvailable => gettext(
                "The Flatpak Secret Portal is not available. Make sure xdg-desktop-portal is installed, and it is at least at version 1.5.0.",
            ),
            oo7::portal::Error::WeakKey(_) => gettext(
                "The Flatpak Secret Portal provided a key that is too weak to be secure.",
            ),
            // Can only occur when using the `replace_item_index` or `delete_item_index` methods.
            oo7::portal::Error::InvalidItemIndex(_) => unreachable!(),
        }
    }
}

impl UserFacingError for oo7::dbus::Error {
    fn to_user_facing(self) -> String {
        match self {
            oo7::dbus::Error::Deleted => gettext(
                "The item was deleted.",
            ),
            oo7::dbus::Error::Dismissed => gettext(
                "The request to the D-Bus Secret Service was cancelled. Make sure to accept any prompt asking to access it.",
            ),
            oo7::dbus::Error::NotFound(_) => gettext(
                "Could not access the default collection. Make sure a keyring was created and set as default.",
            ),
            oo7::dbus::Error::Zbus(_) |
            oo7::dbus::Error::IO(_) => gettext(
                "An unknown error occurred when interacting with the D-Bus Secret Service.",
            ),
        }
    }
}

#[derive(Clone)]
pub struct StoredSession {
    pub homeserver: Url,
    pub user_id: OwnedUserId,
    pub device_id: OwnedDeviceId,
    pub path: PathBuf,
    pub secret: Secret,
}

impl fmt::Debug for StoredSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredSession")
            .field("homeserver", &self.homeserver)
            .field("user_id", &self.user_id)
            .field("device_id", &self.device_id)
            .field("path", &self.path)
            .finish()
    }
}

impl StoredSession {
    /// Build self from a secret.
    pub async fn try_from_secret_item(item: Item) -> Result<Self, SecretError> {
        let attr = item.attributes().await?;

        let homeserver = match attr.get("homeserver") {
            Some(string) => match Url::parse(string) {
                Ok(homeserver) => homeserver,
                Err(err) => {
                    error!(
                        "Could not parse 'homeserver' attribute in stored session: {:?}",
                        err
                    );
                    return Err(SecretError::CorruptSession(
                        gettext("Malformed homeserver in stored session"),
                        item,
                    ));
                }
            },
            None => {
                return Err(SecretError::CorruptSession(
                    gettext("Could not find homeserver in stored session"),
                    item,
                ));
            }
        };
        let user_id = match attr.get("user") {
            Some(string) => match UserId::parse(string.as_str()) {
                Ok(user_id) => user_id,
                Err(err) => {
                    error!(
                        "Could not parse 'user' attribute in stored session: {:?}",
                        err
                    );
                    return Err(SecretError::CorruptSession(
                        gettext("Malformed user ID in stored session"),
                        item,
                    ));
                }
            },
            None => {
                return Err(SecretError::CorruptSession(
                    gettext("Could not find user ID in stored session"),
                    item,
                ));
            }
        };
        let device_id = match attr.get("device-id") {
            Some(string) => <&DeviceId>::from(string.as_str()).to_owned(),
            None => {
                return Err(SecretError::CorruptSession(
                    gettext("Could not find device ID in stored session"),
                    item,
                ));
            }
        };
        let path = match attr.get("db-path") {
            Some(string) => PathBuf::from(string),
            None => {
                return Err(SecretError::CorruptSession(
                    gettext("Could not find database path in stored session"),
                    item,
                ));
            }
        };
        let secret = match item.secret().await {
            Ok(secret) => match Secret::from_utf8(&secret) {
                Ok(secret) => secret,
                Err(err) => {
                    error!("Could not parse secret in stored session: {:?}", err);
                    return Err(SecretError::CorruptSession(
                        gettext("Malformed secret in stored session"),
                        item,
                    ));
                }
            },
            Err(err) => {
                error!("Could not get secret in stored session: {:?}", err);
                return Err(SecretError::CorruptSession(
                    gettext("Could not get secret in stored session"),
                    item,
                ));
            }
        };

        Ok(Self {
            homeserver,
            user_id,
            device_id,
            path,
            secret,
        })
    }

    /// Get the attributes from `self`.
    pub fn attributes(&self) -> HashMap<&str, &str> {
        HashMap::from([
            ("homeserver", self.homeserver.as_str()),
            ("user", self.user_id.as_str()),
            ("device-id", self.device_id.as_str()),
            ("db-path", self.path.to_str().unwrap()),
            (SCHEMA_ATTRIBUTE, APP_ID),
        ])
    }

    /// Get the unique ID for this `StoredSession`.
    ///
    /// This is the name of the folder where the DB is stored.
    pub fn id(&self) -> &str {
        self.path
            .iter()
            .next_back()
            .and_then(OsStr::to_str)
            .unwrap()
    }
}

/// A possible error value when converting a `Secret` from a UTF-8 byte vector.
#[derive(Debug)]
pub enum FromUtf8SecretError {
    Str(FromUtf8Error),
    Json(JsonError),
}

impl From<FromUtf8Error> for FromUtf8SecretError {
    fn from(err: FromUtf8Error) -> Self {
        Self::Str(err)
    }
}

impl From<JsonError> for FromUtf8SecretError {
    fn from(err: JsonError) -> Self {
        Self::Json(err)
    }
}

/// A `Secret` that can be stored in the `SecretService`.
#[derive(Clone, Deserialize, Serialize)]
pub struct Secret {
    pub access_token: String,
    pub passphrase: String,
}

impl Secret {
    /// Converts a vector of bytes to a `Secret`.
    pub fn from_utf8(slice: &[u8]) -> Result<Self, FromUtf8SecretError> {
        let s = String::from_utf8(slice.to_owned())?;
        Ok(serde_json::from_str(&s)?)
    }
}

/// Retrieves all sessions stored to the `SecretService`
pub async fn restore_sessions() -> Result<Vec<StoredSession>, SecretError> {
    let keyring = Keyring::new().await?;

    let mut items = keyring
        .search_items(HashMap::from([(SCHEMA_ATTRIBUTE, APP_ID)]))
        .await?;

    if items.is_empty() {
        // If the keyring uses the file (portal) backend, look for all items,
        // because libsecret didn't store the secrets with the schema attribute.
        if let Keyring::File(_) = keyring {
            items = keyring.items().await?;

            if !items.is_empty() {
                // Migrate those secrets to use the schema attribute.
                for item in items {
                    let attributes = item.attributes().await?;
                    let secret = item.secret().await?;
                    let user_id = match attributes.get("user") {
                        Some(user_id) => user_id,
                        None => continue,
                    };

                    item.delete().await?;

                    let attr = attributes
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .chain([(SCHEMA_ATTRIBUTE, APP_ID)])
                        .collect::<HashMap<_, _>>();

                    keyring
                        .create_item(
                            &gettext_f(
                                // Translators: Do NOT translate the content between '{' and '}',
                                // this is a variable name.
                                "Fractal: Matrix credentials for {user_id}",
                                &[("user_id", user_id.as_str())],
                            ),
                            attr,
                            secret,
                            true,
                        )
                        .await?;
                }

                // Get the migrated items to build the sessions.
                items = keyring
                    .search_items(HashMap::from([(SCHEMA_ATTRIBUTE, APP_ID)]))
                    .await?;
            }
        }
    }

    let mut sessions = Vec::with_capacity(items.len());

    for item in items {
        sessions.push(StoredSession::try_from_secret_item(item).await?);
    }

    Ok(sessions)
}

/// Writes a session to the `SecretService`, overwriting any previously stored
/// session with the same `homeserver`, `username` and `device-id`.
pub async fn store_session(session: &StoredSession) -> Result<(), SecretError> {
    let keyring = Keyring::new().await?;

    let attributes = session.attributes();
    let secret = serde_json::to_string(&session.secret).unwrap();

    keyring
        .create_item(
            &gettext_f(
                // Translators: Do NOT translate the content between '{' and '}', this is a
                // variable name.
                "Fractal: Matrix credentials for {user_id}",
                &[("user_id", session.user_id.as_str())],
            ),
            attributes,
            secret,
            true,
        )
        .await?;

    Ok(())
}

/// Removes a session from the `SecretService`
pub async fn remove_session(session: &StoredSession) -> Result<(), SecretError> {
    let keyring = Keyring::new().await?;

    let attributes = session.attributes();

    keyring.delete(attributes).await?;

    Ok(())
}

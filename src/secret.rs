use std::{collections::HashMap, ffi::OsStr, fmt, fs, path::PathBuf, string::FromUtf8Error};

use gettextrs::gettext;
use gtk::glib;
use log::{debug, error, warn};
use matrix_sdk::ruma::{DeviceId, OwnedDeviceId, OwnedUserId, UserId};
use once_cell::sync::Lazy;
use oo7::{Item, Keyring};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{Deserialize, Serialize};
use serde_json::error::Error as JsonError;
use thiserror::Error;
use url::Url;

use crate::{
    config::{APP_ID, PROFILE},
    gettext_f, spawn_tokio,
    user_facing_error::UserFacingError,
    utils::matrix,
};

pub const CURRENT_VERSION: u8 = 3;
const SCHEMA_ATTRIBUTE: &str = "xdg:schema";

static DATA_PATH: Lazy<PathBuf> = Lazy::new(|| glib::user_data_dir().join(PROFILE.as_str()));

/// Any error that can happen when interacting with the secret service.
#[derive(Debug, Error)]
pub enum SecretError {
    /// A session with an unsupported version was found.
    #[error("Session found with unsupported version {version}")]
    UnsupportedVersion {
        version: u8,
        item: Item,
        attributes: HashMap<String, String>,
    },

    /// A session with an old version was found.
    #[error("Session found with old version")]
    OldVersion { item: Item, session: StoredSession },

    /// A corrupted session was found.
    #[error("{error}")]
    CorruptSession { error: String, item: Item },

    /// An error occurred interacting with the secret service.
    #[error(transparent)]
    Oo7(#[from] oo7::Error),

    /// Trying to restore a session with the wrong profile.
    #[error("Session found for wrong profile")]
    WrongProfile,
}

impl SecretError {
    /// Split `self` between its message and its optional `Item`.
    pub fn into_parts(self) -> (String, Option<Item>) {
        match self {
            SecretError::UnsupportedVersion { version, item, .. } => (
                gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    "Found stored session with unsupported version {version_nb}",
                    &[("version_nb", &version.to_string())],
                ),
                Some(item),
            ),
            SecretError::CorruptSession { error, item } => (error, Some(item)),
            SecretError::Oo7(error) => (error.to_user_facing(), None),
            error => (error.to_string(), None),
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
    pub version: u8,
}

impl fmt::Debug for StoredSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredSession")
            .field("homeserver", &self.homeserver)
            .field("user_id", &self.user_id)
            .field("device_id", &self.device_id)
            .field("path", &self.path)
            .field("version", &self.version)
            .finish()
    }
}

impl StoredSession {
    /// Build self from a secret.
    pub async fn try_from_secret_item(item: Item) -> Result<Self, SecretError> {
        let attr = item.attributes().await?;

        let version = match attr.get("version") {
            Some(string) => match string.parse::<u8>() {
                Ok(version) => version,
                Err(error) => {
                    error!("Could not parse 'version' attribute in stored session: {error}");
                    return Err(SecretError::CorruptSession {
                        error: gettext("Malformed version in stored session"),
                        item,
                    });
                }
            },
            None => 0,
        };
        if version > CURRENT_VERSION {
            return Err(SecretError::UnsupportedVersion {
                version,
                item,
                attributes: attr,
            });
        }

        // TODO: Remove this and request profile in Keyring::search_items when we remove
        // migration.
        match attr.get("profile") {
            // Ignore the item if it's for another profile.
            Some(profile) if *profile != PROFILE.as_str() => return Err(SecretError::WrongProfile),
            // It's an error if the version is at least 2 but there is no profile.
            // Versions older than 2 will be migrated.
            None if version >= 2 => {
                return Err(SecretError::CorruptSession {
                    error: gettext("Could not find profile in stored session"),
                    item,
                });
            }
            // No issue for other cases.
            _ => {}
        };

        let homeserver = match attr.get("homeserver") {
            Some(string) => match Url::parse(string) {
                Ok(homeserver) => homeserver,
                Err(error) => {
                    error!("Could not parse 'homeserver' attribute in stored session: {error}");
                    return Err(SecretError::CorruptSession {
                        error: gettext("Malformed homeserver in stored session"),
                        item,
                    });
                }
            },
            None => {
                return Err(SecretError::CorruptSession {
                    error: gettext("Could not find homeserver in stored session"),
                    item,
                });
            }
        };
        let user_id = match attr.get("user") {
            Some(string) => match UserId::parse(string.as_str()) {
                Ok(user_id) => user_id,
                Err(error) => {
                    error!("Could not parse 'user' attribute in stored session: {error}");
                    return Err(SecretError::CorruptSession {
                        error: gettext("Malformed user ID in stored session"),
                        item,
                    });
                }
            },
            None => {
                return Err(SecretError::CorruptSession {
                    error: gettext("Could not find user ID in stored session"),
                    item,
                });
            }
        };
        let device_id = match attr.get("device-id") {
            Some(string) => <&DeviceId>::from(string.as_str()).to_owned(),
            None => {
                return Err(SecretError::CorruptSession {
                    error: gettext("Could not find device ID in stored session"),
                    item,
                });
            }
        };
        let path = match attr.get("db-path") {
            Some(string) => PathBuf::from(string),
            None => {
                return Err(SecretError::CorruptSession {
                    error: gettext("Could not find database path in stored session"),
                    item,
                });
            }
        };
        let secret = match item.secret().await {
            Ok(secret) => {
                if version == 0 {
                    match Secret::from_utf8(&secret) {
                        Ok(secret) => secret,
                        Err(error) => {
                            error!("Could not parse secret in stored session: {error:?}");
                            return Err(SecretError::CorruptSession {
                                error: gettext("Malformed secret in stored session"),
                                item,
                            });
                        }
                    }
                } else {
                    match rmp_serde::from_slice::<Secret>(&secret) {
                        Ok(secret) => secret,
                        Err(error) => {
                            error!("Could not parse secret in stored session: {error}");
                            return Err(SecretError::CorruptSession {
                                error: gettext("Malformed secret in stored session"),
                                item,
                            });
                        }
                    }
                }
            }
            Err(error) => {
                error!("Could not get secret in stored session: {error}");
                return Err(SecretError::CorruptSession {
                    error: gettext("Could not get secret in stored session"),
                    item,
                });
            }
        };

        let session = Self {
            homeserver,
            user_id,
            device_id,
            path,
            secret,
            version,
        };

        if version < CURRENT_VERSION {
            Err(SecretError::OldVersion { item, session })
        } else {
            Ok(session)
        }
    }

    /// Construct a `StoredSession` from the given login data.
    pub fn with_login_data(homeserver: Url, data: matrix_sdk::Session) -> Self {
        let matrix_sdk::Session {
            access_token,
            user_id,
            device_id,
            ..
        } = data;

        let path = DATA_PATH.join(glib::uuid_string_random().as_str());

        let passphrase = thread_rng()
            .sample_iter(Alphanumeric)
            .take(30)
            .map(char::from)
            .collect();

        let secret = Secret {
            access_token,
            passphrase,
        };

        Self {
            homeserver,
            user_id,
            device_id,
            path,
            secret,
            version: CURRENT_VERSION,
        }
    }

    /// Split this `StoredSession` into parts.
    pub fn into_parts(self) -> (Url, PathBuf, String, matrix_sdk::Session) {
        let Self {
            homeserver,
            user_id,
            device_id,
            path,
            secret: Secret {
                access_token,
                passphrase,
            },
            ..
        } = self;

        let data = matrix_sdk::Session {
            access_token,
            user_id,
            device_id,
            refresh_token: None,
        };

        (homeserver, path, passphrase, data)
    }

    /// Get the attributes from `self`.
    pub fn attributes(&self) -> HashMap<&str, String> {
        HashMap::from([
            ("homeserver", self.homeserver.to_string()),
            ("user", self.user_id.to_string()),
            ("device-id", self.device_id.to_string()),
            ("db-path", self.path.to_str().unwrap().to_owned()),
            ("version", self.version.to_string()),
            ("profile", PROFILE.to_string()),
            (SCHEMA_ATTRIBUTE, APP_ID.to_owned()),
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

    /// Write this session to the `SecretService`, overwriting any previously
    /// stored session with the same attributes.
    pub async fn store(&self) -> Result<(), SecretError> {
        let keyring = Keyring::new().await?;

        let attrs = self.attributes();
        let attributes = attrs.iter().map(|(k, v)| (*k, v.as_ref())).collect();
        let secret = rmp_serde::to_vec_named(&self.secret).unwrap();

        keyring
            .create_item(
                &gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    "Fractal: Matrix credentials for {user_id}",
                    &[("user_id", self.user_id.as_str())],
                ),
                attributes,
                secret,
                true,
            )
            .await?;

        Ok(())
    }

    /// Delete this session from the system.
    pub async fn delete(self, item: Option<Item>, logout: bool) {
        debug!(
            "Removing stored session {} with version {} for Matrix user {}…",
            self.id(),
            self.version,
            self.user_id,
        );

        spawn_tokio!(async move {
            if logout {
                debug!("Logging out session");
                match matrix::client_with_stored_session(self.clone()).await {
                    Ok(client) => {
                        if let Err(error) = client.logout().await {
                            error!("Failed to log out session: {error}");
                        }
                    }
                    Err(error) => {
                        error!("Failed to build client to log out session: {error}")
                    }
                }
            }

            if let Some(item) = item {
                if let Err(error) = item.delete().await {
                    error!("Failed to delete session item from Secret Service: {error}");
                };
            } else if let Err(error) = self.delete_from_secret_service().await {
                error!("Failed to delete session data from Secret Service: {error}");
            }

            if let Err(error) = fs::remove_dir_all(self.path) {
                error!("Failed to remove session database: {error}");
            }
        })
        .await
        .unwrap();
    }

    /// Remove this session from the `SecretService`
    async fn delete_from_secret_service(&self) -> Result<(), SecretError> {
        let keyring = Keyring::new().await?;

        let attrs = self.attributes();
        let attributes = attrs.iter().map(|(k, v)| (*k, v.as_ref())).collect();

        keyring.delete(attributes).await?;

        Ok(())
    }

    /// Migrate this session to version 3.
    ///
    /// This implies moving the database under the profile's directory.
    pub async fn migrate_to_v3(mut self, item: Item) {
        warn!(
            "Session {} with version {} found for user {}, migrating to version 3…",
            self.id(),
            self.version,
            self.user_id,
        );

        let target_path = DATA_PATH.join(self.id());

        if self.path == target_path {
            return;
        }

        spawn_tokio!(async move {
            debug!("Moving database to: {}", target_path.to_string_lossy());

            if let Err(error) = fs::create_dir_all(&target_path) {
                error!("Failed to create new directory: {error}");
            }

            if let Err(error) = fs::rename(&self.path, &target_path) {
                error!("Failed to move database: {error}");
            }

            self.path = target_path;
            self.version = 3;

            if let Err(error) = item.delete().await {
                error!("Failed to remove outdated session: {error}");
            }

            if let Err(error) = self.store().await {
                error!("Failed to store updated session: {error}");
            }
        })
        .await
        .unwrap();
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

    let items = keyring
        .search_items(HashMap::from([(SCHEMA_ATTRIBUTE, APP_ID)]))
        .await?;

    let mut sessions = Vec::with_capacity(items.len());

    for item in items {
        match StoredSession::try_from_secret_item(item).await {
            Ok(session) => sessions.push(session),
            Err(SecretError::WrongProfile) => {}
            Err(error) => return Err(error),
        }
    }

    Ok(sessions)
}

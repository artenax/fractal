//! Collection of common methods and types.

pub mod macros;
pub mod media;
pub mod sourceview;
pub mod template_callbacks;

use std::path::PathBuf;

use futures::{
    future::{self, Either, Future},
    pin_mut,
};
use gtk::{
    gio::{self, prelude::*},
    glib::{self, closure, Object},
};
use matrix_sdk::ruma::{EventId, OwnedEventId, OwnedTransactionId, TransactionId, UInt};
use once_cell::sync::Lazy;
use regex::Regex;
use ruma::{
    exports::percent_encoding::percent_decode_str, matrix_uri::MatrixId, IdParseError,
    MatrixIdError, MatrixToError, OwnedServerName, RoomAliasId, RoomId, ServerName, UserId,
};
use url::form_urlencoded;

/// Returns an expression that is the and’ed result of the given boolean
/// expressions.
#[allow(dead_code)]
pub fn and_expr<E: AsRef<gtk::Expression>>(a_expr: E, b_expr: E) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool, _, _>(
        &[a_expr, b_expr],
        closure!(|_: Option<Object>, a: bool, b: bool| { a && b }),
    )
}

/// Returns an expression that is the or’ed result of the given boolean
/// expressions.
pub fn or_expr<E: AsRef<gtk::Expression>>(a_expr: E, b_expr: E) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool, _, _>(
        &[a_expr, b_expr],
        closure!(|_: Option<Object>, a: bool, b: bool| { a || b }),
    )
}

/// Returns an expression that is the inverted result of the given boolean
/// expressions.
#[allow(dead_code)]
pub fn not_expr<E: AsRef<gtk::Expression>>(a_expr: E) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool, _, _>(
        &[a_expr],
        closure!(|_: Option<Object>, a: bool| { !a }),
    )
}

/// Get the cache directory.
///
/// If it doesn't exist, this method creates it.
pub fn cache_dir() -> PathBuf {
    let mut path = glib::user_cache_dir();
    path.push("fractal");

    if !path.exists() {
        let dir = gio::File::for_path(path.clone());
        dir.make_directory_with_parents(gio::Cancellable::NONE)
            .unwrap();
    }

    path
}

/// Converts a `UInt` to `i32`.
///
/// Returns `-1` if the conversion didn't work.
pub fn uint_to_i32(u: Option<UInt>) -> i32 {
    u.and_then(|ui| {
        let u: Option<u16> = ui.try_into().ok();
        u
    })
    .map(|u| {
        let i: i32 = u.into();
        i
    })
    .unwrap_or(-1)
}

/// Generate temporary IDs for pending events.
///
/// Returns a `(transaction_id, event_id)` tuple. The `event_id` is derived from
/// the `transaction_id`.
pub fn pending_event_ids() -> (OwnedTransactionId, OwnedEventId) {
    let txn_id = TransactionId::new();
    let event_id = EventId::parse(format!("${}:fractal.gnome.org", txn_id)).unwrap();
    (txn_id, event_id)
}

pub enum TimeoutFuture {
    Timeout,
}

/// Executes the given future with the given timeout.
///
/// If the future didn't resolve before the timeout was reached, this returns
/// an `Err(TimeoutFuture)`.
pub async fn timeout_future<T>(
    timeout: std::time::Duration,
    fut: impl Future<Output = T>,
) -> Result<T, TimeoutFuture> {
    let timeout = glib::timeout_future(timeout);
    pin_mut!(fut);

    match future::select(fut, timeout).await {
        Either::Left((x, _)) => Ok(x),
        _ => Err(TimeoutFuture::Timeout),
    }
}

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

/// Replace variables in the given string with the given dictionary.
///
/// The expected format to replace is `{name}`, where `name` is the first string
/// in the dictionary entry tuple.
pub fn freplace(s: String, args: &[(&str, &str)]) -> String {
    let mut s = s;

    for (k, v) in args {
        s = s.replace(&format!("{{{}}}", k), v);
    }

    s
}

/// Check if the given hostname is reachable.
pub async fn check_if_reachable(hostname: &impl AsRef<str>) -> bool {
    let address = gio::NetworkAddress::parse_uri(hostname.as_ref(), 80).unwrap();
    let monitor = gio::NetworkMonitor::default();
    match monitor.can_reach_future(&address).await {
        Ok(()) => true,
        Err(error) => {
            log::error!(
                "Homeserver {} isn't reachable: {}",
                hostname.as_ref(),
                error
            );
            false
        }
    }
}

/// Regex that matches a string that only includes emojis.
pub static EMOJI_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)
        ^
        [\p{White_Space}\p{Emoji_Component}]*
        [\p{Emoji}--\p{Decimal_Number}]+
        [\p{White_Space}\p{Emoji}\p{Emoji_Component}--\p{Decimal_Number}]*
        $
        # That string is made of at least one emoji, except digits, possibly more,
        # possibly with modifiers, possibly with spaces, but nothing else
        ",
    )
    .unwrap()
});

const MATRIX_TO_BASE_URL: &str = "https://matrix.to/#/";

/// Parse a matrix.to URI.
///
/// Ruma's parsing fails with non-percent-encoded identifiers, which is the
/// format of permalinks provided by Element Web.
pub fn parse_matrix_to_uri(uri: &str) -> Result<(MatrixId, Vec<OwnedServerName>), IdParseError> {
    let s = uri
        .strip_prefix(MATRIX_TO_BASE_URL)
        .ok_or(MatrixToError::WrongBaseUrl)?;
    let s = s.strip_suffix('/').unwrap_or(s);

    let mut parts = s.split('?');
    let ids_part = parts.next().ok_or(MatrixIdError::NoIdentifier)?;
    let mut ids = ids_part.split('/');

    let first = ids.next().ok_or(MatrixIdError::NoIdentifier)?;
    let first_id = percent_decode_str(first).decode_utf8()?;

    let id: MatrixId = match first_id.as_bytes()[0] {
        b'!' => {
            let room_id = RoomId::parse(&first_id)?;

            if let Some(second) = ids.next() {
                let second_id = percent_decode_str(second).decode_utf8()?;
                let event_id = EventId::parse(&second_id)?;
                (room_id, event_id).into()
            } else {
                room_id.into()
            }
        }
        b'#' => {
            let room_id = RoomAliasId::parse(&first_id)?;

            if let Some(second) = ids.next() {
                let second_id = percent_decode_str(second).decode_utf8()?;
                let event_id = EventId::parse(&second_id)?;
                (room_id, event_id).into()
            } else {
                room_id.into()
            }
        }
        b'@' => UserId::parse(&first_id)?.into(),
        b'$' => return Err(MatrixIdError::MissingRoom.into()),
        _ => return Err(MatrixIdError::UnknownIdentifier.into()),
    };

    if ids.next().is_some() {
        return Err(MatrixIdError::TooManyIdentifiers.into());
    }

    let via = parts
        .next()
        .map(|query| {
            let query = html_escape::decode_html_entities(query);
            let query_parts = form_urlencoded::parse(query.as_bytes());
            query_parts
                .filter_map(|(key, value)| (key == "via").then(|| ServerName::parse(&value)))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    if parts.next().is_some() {
        return Err(MatrixToError::InvalidUrl.into());
    }

    Ok((id, via))
}

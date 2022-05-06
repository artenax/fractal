use sourceview::prelude::BufferExt;

/// FIXME: This should be addressed in ruma directly
#[macro_export]
macro_rules! fn_event {
    ( $event:ident, $fun:ident ) => {
        match &$event {
            AnyRoomEvent::Message(event) => event.$fun(),
            AnyRoomEvent::State(event) => event.$fun(),
            AnyRoomEvent::RedactedMessage(event) => event.$fun(),
            AnyRoomEvent::RedactedState(event) => event.$fun(),
        }
    };
}

/// FIXME: This should be addressed in ruma directly
#[macro_export]
macro_rules! event_from_sync_event {
    ( $event:ident, $room_id:ident) => {
        match $event {
            AnySyncRoomEvent::MessageLike(event) => {
                AnyRoomEvent::Message(event.into_full_event($room_id.clone()))
            }
            AnySyncRoomEvent::State(event) => {
                AnyRoomEvent::State(event.into_full_event($room_id.clone()))
            }
            AnySyncRoomEvent::RedactedMessage(event) => {
                AnyRoomEvent::RedactedMessage(event.into_full_event($room_id.clone()))
            }
            AnySyncRoomEvent::RedactedState(event) => {
                AnyRoomEvent::RedactedState(event.into_full_event($room_id.clone()))
            }
        }
    };
}

/// Spawn a future on the default `MainContext`
///
/// This was taken from `gtk-macros`
/// but allows setting optionally the priority
///
/// FIXME: this should maybe be upstreamed
#[macro_export]
macro_rules! spawn {
    ($future:expr) => {
        let ctx = glib::MainContext::default();
        ctx.spawn_local($future);
    };
    ($priority:expr, $future:expr) => {
        let ctx = glib::MainContext::default();
        ctx.spawn_local_with_priority($priority, $future);
    };
}

/// Spawn a future on the tokio runtime
#[macro_export]
macro_rules! spawn_tokio {
    ($future:expr) => {
        $crate::RUNTIME.spawn($future)
    };
}

use std::{convert::TryInto, path::PathBuf, str::FromStr};

use gettextrs::gettext;
use gtk::{
    gio::{self, prelude::*},
    glib::{self, closure, Object},
};
use matrix_sdk::ruma::{
    events::room::MediaSource, EventId, OwnedEventId, OwnedTransactionId, TransactionId, UInt,
};
use mime::Mime;

// Returns an expression that is the and’ed result of the given boolean
// expressions.
#[allow(dead_code)]
pub fn and_expr<E: AsRef<gtk::Expression>>(a_expr: E, b_expr: E) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool, _, _>(
        &[a_expr, b_expr],
        closure!(|_: Option<Object>, a: bool, b: bool| { a && b }),
    )
}

// Returns an expression that is the or’ed result of the given boolean
// expressions.
pub fn or_expr<E: AsRef<gtk::Expression>>(a_expr: E, b_expr: E) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool, _, _>(
        &[a_expr, b_expr],
        closure!(|_: Option<Object>, a: bool, b: bool| { a || b }),
    )
}

// Returns an expression that is the inverted result of the given boolean
// expressions.
#[allow(dead_code)]
pub fn not_expr<E: AsRef<gtk::Expression>>(a_expr: E) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool, _, _>(
        &[a_expr],
        closure!(|_: Option<Object>, a: bool| { !a }),
    )
}

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

pub fn setup_style_scheme(buffer: &sourceview::Buffer) {
    let manager = adw::StyleManager::default();

    buffer.set_style_scheme(style_scheme().as_ref());

    manager.connect_dark_notify(glib::clone!(@weak buffer => move |_| {
        buffer.set_style_scheme(style_scheme().as_ref());
    }));
}

pub fn style_scheme() -> Option<sourceview::StyleScheme> {
    let manager = adw::StyleManager::default();
    let scheme_name = if manager.is_dark() {
        "Adwaita-dark"
    } else {
        "Adwaita"
    };

    sourceview::StyleSchemeManager::default().scheme(scheme_name)
}

/// Get the unique id of the given `MediaSource`.
///
/// It is built from the underlying `MxcUri` and can be safely used in a
/// filename.
///
/// The id is not guaranteed to be unique for malformed `MxcUri`s.
pub fn media_type_uid(media_type: Option<MediaSource>) -> String {
    if let Some(mxc) = media_type
        .map(|media_type| match media_type {
            MediaSource::Plain(uri) => uri,
            MediaSource::Encrypted(file) => file.url,
        })
        .filter(|mxc| mxc.is_valid())
    {
        format!("{}_{}", mxc.server_name().unwrap(), mxc.media_id().unwrap())
    } else {
        "media_uid".to_owned()
    }
}

/// Get a default filename for a mime type.
///
/// Tries to guess the file extension, but it might not find it.
///
/// If the mime type is unknown, it uses the name for `fallback`. The fallback
/// mime types that are recognized are `mime::IMAGE`, `mime::VIDEO`
/// and `mime::AUDIO`, other values will behave the same as `None`.
pub fn filename_for_mime(mime_type: Option<&str>, fallback: Option<mime::Name>) -> String {
    let (type_, extension) = if let Some(mime) = mime_type.and_then(|m| Mime::from_str(m).ok()) {
        let extension =
            mime_guess::get_mime_extensions(&mime).map(|extensions| extensions[0].to_owned());

        (Some(mime.type_().as_str().to_owned()), extension)
    } else {
        (fallback.map(|type_| type_.as_str().to_owned()), None)
    };

    let name = match type_.as_deref() {
        // Translators: Default name for image files.
        Some("image") => gettext("image"),
        // Translators: Default name for video files.
        Some("video") => gettext("video"),
        // Translators: Default name for audio files.
        Some("audio") => gettext("audio"),
        // Translators: Default name for files.
        _ => gettext("file"),
    };

    extension
        .map(|extension| format!("{}.{}", name, extension))
        .unwrap_or(name)
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

use futures::{
    future::{self, Either, Future},
    pin_mut,
};

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

pub struct TemplateCallbacks {}

#[gtk::template_callbacks(functions)]
impl TemplateCallbacks {
    #[template_callback]
    fn string_not_empty(string: Option<&str>) -> bool {
        !string.unwrap_or_default().is_empty()
    }

    #[template_callback]
    fn object_is_some(obj: Option<glib::Object>) -> bool {
        obj.is_some()
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

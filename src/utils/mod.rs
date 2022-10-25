//! Collection of common methods and types.

pub mod macros;
pub mod matrix;
pub mod media;
pub mod notifications;
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
use matrix_sdk::ruma::UInt;
use once_cell::sync::Lazy;
use regex::Regex;

/// Returns an expression that is the and’ed result of the given boolean
/// expressions.
#[allow(dead_code)]
pub fn and_expr<E: AsRef<gtk::Expression>>(a_expr: E, b_expr: E) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool>(
        &[a_expr, b_expr],
        closure!(|_: Option<Object>, a: bool, b: bool| { a && b }),
    )
}

/// Returns an expression that is the or’ed result of the given boolean
/// expressions.
pub fn or_expr<E: AsRef<gtk::Expression>>(a_expr: E, b_expr: E) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool>(
        &[a_expr, b_expr],
        closure!(|_: Option<Object>, a: bool, b: bool| { a || b }),
    )
}

/// Returns an expression that is the inverted result of the given boolean
/// expressions.
#[allow(dead_code)]
pub fn not_expr<E: AsRef<gtk::Expression>>(a_expr: E) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool>(&[a_expr], closure!(|_: Option<Object>, a: bool| { !a }))
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

//! Collection of common methods and types.

mod expression_list_model;
pub mod macros;
pub mod matrix;
pub mod media;
pub mod notifications;
pub mod sourceview;
pub mod template_callbacks;

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use futures_util::{
    future::{self, Either, Future},
    pin_mut,
};
use gtk::{
    gio::{self, prelude::*},
    glib::{self, closure, Object},
};
use matrix_sdk::ruma::UInt;
use once_cell::sync::{Lazy, OnceCell};
use regex::Regex;
use tracing::error;

pub use self::expression_list_model::ExpressionListModel;
use crate::RUNTIME;

/// Returns an expression that is the and’ed result of the given boolean
/// expressions.
#[allow(dead_code)]
pub fn and_expr(
    a_expr: impl AsRef<gtk::Expression>,
    b_expr: impl AsRef<gtk::Expression>,
) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool>(
        &[a_expr.as_ref(), b_expr.as_ref()],
        closure!(|_: Option<Object>, a: bool, b: bool| { a && b }),
    )
}

/// Returns an expression that is the or’ed result of the given boolean
/// expressions.
pub fn or_expr(
    a_expr: impl AsRef<gtk::Expression>,
    b_expr: impl AsRef<gtk::Expression>,
) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool>(
        &[a_expr.as_ref(), b_expr.as_ref()],
        closure!(|_: Option<Object>, a: bool, b: bool| { a || b }),
    )
}

/// Returns an expression that is the inverted result of the given boolean
/// expressions.
#[allow(dead_code)]
pub fn not_expr<E: AsRef<gtk::Expression>>(a_expr: E) -> gtk::ClosureExpression {
    gtk::ClosureExpression::new::<bool>(&[a_expr], closure!(|_: Option<Object>, a: bool| { !a }))
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
        s = s.replace(&format!("{{{k}}}"), v);
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
            error!("Homeserver {} isn't reachable: {error}", hostname.as_ref());
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

/// Inner to manage a bound object.
#[derive(Debug)]
pub struct BoundObjectInner<T: glib::ObjectType> {
    obj: T,
    signal_handler_ids: Vec<glib::SignalHandlerId>,
}

/// Wrapper to manage a bound object.
///
/// This keeps a strong reference to the object.
#[derive(Debug)]
pub struct BoundObject<T: glib::ObjectType> {
    inner: RefCell<Option<BoundObjectInner<T>>>,
}

impl<T: glib::ObjectType> BoundObject<T> {
    /// Creates a new empty `BoundObjectWeakRef`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the given object and signal handlers IDs.
    ///
    /// Calls `disconnect_signals` first to drop the previous strong reference
    /// and disconnect the previous signal handlers.
    pub fn set(&self, obj: T, signal_handler_ids: Vec<glib::SignalHandlerId>) {
        self.disconnect_signals();

        let inner = BoundObjectInner {
            obj,
            signal_handler_ids,
        };

        self.inner.replace(Some(inner));
    }

    /// Get the object, if any.
    pub fn obj(&self) -> Option<T> {
        self.inner.borrow().as_ref().map(|inner| inner.obj.clone())
    }

    /// Disconnect the signal handlers and drop the strong reference.
    pub fn disconnect_signals(&self) {
        if let Some(inner) = self.inner.take() {
            for signal_handler_id in inner.signal_handler_ids {
                inner.obj.disconnect(signal_handler_id)
            }
        }
    }
}

impl<T: glib::ObjectType> Default for BoundObject<T> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}

/// Wrapper to manage a bound object.
///
/// This keeps a weak reference to the object.
#[derive(Debug)]
pub struct BoundObjectWeakRef<T: glib::ObjectType> {
    weak_obj: glib::WeakRef<T>,
    signal_handler_ids: RefCell<Vec<glib::SignalHandlerId>>,
}

impl<T: glib::ObjectType> BoundObjectWeakRef<T> {
    /// Creates a new empty `BoundObjectWeakRef` with the given object and
    /// signal handlers IDs.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the given object and signal handlers IDs.
    ///
    /// Calls `disconnect_signals` first to remove the previous weak reference
    /// and disconnect the previous signal handlers.
    pub fn set(&self, obj: &T, signal_handler_ids: Vec<glib::SignalHandlerId>) {
        self.disconnect_signals();

        self.weak_obj.set(Some(obj));
        self.signal_handler_ids.replace(signal_handler_ids);
    }

    /// Get a strong reference to the object.
    pub fn obj(&self) -> Option<T> {
        self.weak_obj.upgrade()
    }

    /// Disconnect the signal handlers and drop the weak reference.
    pub fn disconnect_signals(&self) {
        let signal_handler_ids = self.signal_handler_ids.take();

        if let Some(obj) = self.weak_obj.upgrade() {
            for signal_handler_id in signal_handler_ids {
                obj.disconnect(signal_handler_id)
            }
        }

        self.weak_obj.set(None);
    }
}

impl<T: glib::ObjectType> Default for BoundObjectWeakRef<T> {
    fn default() -> Self {
        Self {
            weak_obj: Default::default(),
            signal_handler_ids: Default::default(),
        }
    }
}

/// Helper type to keep track of ongoing async actions that can succeed in
/// different functions.
///
/// This type can only have one strong reference and many weak references.
///
/// The strong reference should be dropped in the first function where the
/// action succeeds. Then other functions can drop the weak references when
/// they can't be upgraded.
#[derive(Debug)]
pub struct OngoingAsyncAction<T> {
    strong: Rc<AsyncAction<T>>,
}

impl<T> OngoingAsyncAction<T> {
    /// Create a new async action that sets the given value.
    ///
    /// Returns both a strong and a weak reference.
    pub fn set(value: T) -> (Self, WeakOngoingAsyncAction<T>) {
        let strong = Rc::new(AsyncAction::Set(value));
        let weak = Rc::downgrade(&strong);
        (Self { strong }, WeakOngoingAsyncAction { weak })
    }

    /// Create a new async action that removes a value.
    ///
    /// Returns both a strong and a weak reference.
    pub fn remove() -> (Self, WeakOngoingAsyncAction<T>) {
        let strong = Rc::new(AsyncAction::Remove);
        let weak = Rc::downgrade(&strong);
        (Self { strong }, WeakOngoingAsyncAction { weak })
    }

    /// Create a new weak reference to this async action.
    pub fn downgrade(&self) -> WeakOngoingAsyncAction<T> {
        let weak = Rc::downgrade(&self.strong);
        WeakOngoingAsyncAction { weak }
    }

    /// The inner action.
    pub fn action(&self) -> &AsyncAction<T> {
        &self.strong
    }

    /// Get the inner value, if any.
    pub fn as_value(&self) -> Option<&T> {
        self.strong.as_value()
    }
}

/// A weak reference to an `OngoingAsyncAction`.
#[derive(Debug, Clone)]
pub struct WeakOngoingAsyncAction<T> {
    weak: Weak<AsyncAction<T>>,
}

impl<T> WeakOngoingAsyncAction<T> {
    /// Whether this async action is still ongoing (i.e. whether the strong
    /// reference still exists).
    pub fn is_ongoing(&self) -> bool {
        self.weak.strong_count() > 0
    }
}

/// An async action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncAction<T> {
    /// An async action is ongoing to set this value.
    Set(T),

    /// An async action is ongoing to remove a value.
    Remove,
}

impl<T> AsyncAction<T> {
    /// Get the inner value, if any.
    pub fn as_value(&self) -> Option<&T> {
        match self {
            Self::Set(value) => Some(value),
            _ => None,
        }
    }
}

/// A type that requires the tokio runtime to be running when dropped.
///
/// This is basically usable as a [`OnceCell`].
#[derive(Debug, Clone)]
pub struct TokioDrop<T>(OnceCell<T>);

impl<T> TokioDrop<T> {
    /// Create a new empty `TokioDrop`;
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets a reference to the underlying value.
    ///
    /// Returns `None` if the cell is empty.
    pub fn get(&self) -> Option<&T> {
        self.0.get()
    }

    /// Sets the contents of this cell to `value`.
    ///
    /// Returns `Ok(())` if the cell was empty and `Err(value)` if it was full.
    pub fn set(&self, value: T) -> Result<(), T> {
        self.0.set(value)
    }
}

impl<T> Default for TokioDrop<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T> Drop for TokioDrop<T> {
    fn drop(&mut self) {
        let _guard = RUNTIME.enter();

        if let Some(inner) = self.0.take() {
            drop(inner)
        }
    }
}

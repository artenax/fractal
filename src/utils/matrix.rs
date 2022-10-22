//! Collection of methods related to the Matrix specification.

use matrix_sdk::ruma::{EventId, OwnedEventId, OwnedTransactionId, TransactionId};

/// Generate temporary IDs for pending events.
///
/// Returns a `(transaction_id, event_id)` tuple. The `event_id` is derived from
/// the `transaction_id`.
pub fn pending_event_ids() -> (OwnedTransactionId, OwnedEventId) {
    let txn_id = TransactionId::new();
    let event_id = EventId::parse(format!("${}:fractal.gnome.org", txn_id)).unwrap();
    (txn_id, event_id)
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

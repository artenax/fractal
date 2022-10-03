//! Collection of GTK template callbacks.

use gtk::glib;

/// Struct used as a collection of GTK template callbacks.
pub struct TemplateCallbacks {}

#[gtk::template_callbacks(functions)]
impl TemplateCallbacks {
    /// Returns `true` when the given string is not empty.
    #[template_callback]
    pub fn string_not_empty(string: Option<&str>) -> bool {
        !string.unwrap_or_default().is_empty()
    }

    /// Returns `true` when the given `Option<glib::Object>` is `Some`.
    #[template_callback]
    pub fn object_is_some(obj: Option<glib::Object>) -> bool {
        obj.is_some()
    }

    /// Inverts the given boolean.
    #[template_callback]
    pub fn invert_boolean(boolean: bool) -> bool {
        !boolean
    }
}

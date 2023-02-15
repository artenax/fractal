//! Collection of macros.

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

/// Show a toast with the given message on the ancestor window of `widget`.
///
/// The simplest way to use this macros is for displaying a simple message. It
/// can be anything that implements `AsRef<str>`.
///
/// ```ignore
/// toast!(widget, gettext("Something happened"));
/// ```
///
/// This macro also supports replacing named variables with their value. It
/// supports both the `var` and the `var = expr` syntax. In this case the
/// message and the variables must be `String`s.
///
/// ```ignore
/// toast!(
///     widget,
///     gettext("Error number {n}: {msg}"),
///     n = error_nb.to_string(),
///     msg,
/// );
/// ```
///
/// To add `Pill`s to the toast, you can precede a [`Room`] or [`User`] with
/// `@`.
///
/// ```ignore
/// let room = Room::new(session, room_id);
/// let member = Member::new(room, user_id);
///
/// toast!(
///     widget,
///     gettext("Could not contact {user} in {room}",
///     @user = member,
///     @room,
/// );
/// ```
///
/// For this macro to work, the ancestor window be a [`Window`](crate::Window)
/// or an [`adw::PreferencesWindow`].
///
/// [`Room`]: crate::session::room::Room
/// [`User`]: crate::session::user::User
#[macro_export]
macro_rules! toast {
    ($widget:expr, $message:expr) => {
        {
            $crate::_add_toast!($widget, adw::Toast::new($message.as_ref()));
        }
    };
    ($widget:expr, $message:expr, $($tail:tt)+) => {
        {
            let (string_vars, pill_vars) = $crate::_toast_accum!([], [], $($tail)+);
            let string_dict: Vec<_> = string_vars
                .iter()
                .map(|(key, val): &(&str, String)| (key.as_ref(), val.as_ref()))
                .collect();
            let message = $crate::utils::freplace($message.into(), &*string_dict);

            let toast = if pill_vars.is_empty() {
                adw::Toast::new($message.as_ref())
            } else {
                let pill_vars = std::collections::HashMap::<&str, $crate::components::Pill>::from(pill_vars);
                let mut swapped_label = String::new();
                let mut widgets = Vec::with_capacity(pill_vars.len());
                let mut last_end = 0;

                let mut matches = pill_vars
                    .keys()
                    .map(|key: &&str| {
                        message
                            .match_indices(&format!("{{{key}}}"))
                            .map(|(start, _)| (start, key))
                            .collect::<Vec<_>>()
                    })
                    .flatten()
                    .collect::<Vec<_>>();
                matches.sort_unstable();

                for (start, key) in matches {
                    swapped_label.push_str(&message[last_end..start]);
                    swapped_label.push_str($crate::components::DEFAULT_PLACEHOLDER);
                    last_end = start + key.len() + 2;
                    widgets.push(pill_vars.get(key).unwrap().clone())
                }
                swapped_label.push_str(&message[last_end..message.len()]);

                let widget = $crate::components::LabelWithWidgets::with_label_and_widgets(
                    &swapped_label,
                    widgets,
                );

                adw::Toast::builder()
                    .custom_title(&widget)
                    .build()
            };

            $crate::_add_toast!($widget, toast);
        }
    };
}
#[doc(hidden)]
#[macro_export]
macro_rules! _toast_accum {
    ([$($string_vars:tt)*], [$($pill_vars:tt)*], $var:ident, $($tail:tt)*) => {
        $crate::_toast_accum!([$($string_vars)* (stringify!($var), $var),], [$($pill_vars)*], $($tail)*)
    };
    ([$($string_vars:tt)*], [$($pill_vars:tt)*], $var:ident = $val:expr, $($tail:tt)*) => {
        $crate::_toast_accum!([$($string_vars)* (stringify!($var), $val),], [$($pill_vars)*], $($tail)*)
    };
    ([$($string_vars:tt)*], [$($pill_vars:tt)*], @$var:ident, $($tail:tt)*) => {
        {
            let pill: $crate::components::Pill = $var.to_pill();
            $crate::_toast_accum!([$($string_vars)*], [$($pill_vars)* (stringify!($var), pill),], $($tail)*)
        }
    };
    ([$($string_vars:tt)*], [$($pill_vars:tt)*], @$var:ident = $val:expr, $($tail:tt)*) => {
        {
            let pill: $crate::components::Pill = $val.to_pill();
            $crate::_toast_accum!([$($string_vars)*], [$($pill_vars)* (stringify!($var), pill),], $($tail)*)
        }
    };
    ([$($string_vars:tt)*], [$($pill_vars:tt)*],) => { ([$($string_vars)*], [$($pill_vars)*]) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! _add_toast {
    ($widget:expr, $toast:expr) => {{
        use gtk::prelude::WidgetExt;
        if let Some(root) = $widget.root() {
            if let Some(window) = root.downcast_ref::<$crate::Window>() {
                window.add_toast($toast);
            } else if let Some(window) = root.downcast_ref::<adw::PreferencesWindow>() {
                use adw::prelude::PreferencesWindowExt;
                window.add_toast($toast);
            } else {
                panic!("Trying to display a toast when the parent doesn't support it");
            }
        }
    }};
}

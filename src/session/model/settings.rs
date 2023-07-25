use gtk::{glib, prelude::*, subclass::prelude::*};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::Application;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSessionSettings {
    /// Custom servers to explore.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    explore_custom_servers: Vec<String>,

    /// Whether notifications are enabled for this session.
    #[serde(
        default = "ruma::serde::default_true",
        skip_serializing_if = "ruma::serde::is_true"
    )]
    notifications_enabled: bool,
}

impl Default for StoredSessionSettings {
    fn default() -> Self {
        Self {
            explore_custom_servers: Default::default(),
            notifications_enabled: true,
        }
    }
}

mod imp {
    use std::cell::RefCell;

    use once_cell::sync::{Lazy, OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct SessionSettings {
        /// The ID of the session these settings are for.
        pub session_id: OnceCell<String>,
        /// The stored settings.
        pub stored_settings: RefCell<StoredSessionSettings>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SessionSettings {
        const NAME: &'static str = "SessionSettings";
        type Type = super::SessionSettings;
    }

    impl ObjectImpl for SessionSettings {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::builder("session-id")
                        .construct_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("notifications-enabled")
                        .default_value(true)
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "session-id" => obj.set_session_id(value.get().ok()),
                "notifications-enabled" => obj.set_notifications_enabled(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "session-id" => obj.session_id().to_value(),
                "notifications-enabled" => obj.notifications_enabled().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// The settings of a `Session`.
    pub struct SessionSettings(ObjectSubclass<imp::SessionSettings>);
}

impl SessionSettings {
    /// Create a new `SessionSettings` for the given session ID.
    pub fn new(session_id: &str) -> Self {
        glib::Object::builder()
            .property("session-id", session_id)
            .build()
    }

    /// Save the settings in the GSettings.
    fn save(&self) {
        let mut sessions = sessions();
        let stored_settings = self.imp().stored_settings.borrow().clone();

        sessions.insert(self.session_id().to_owned(), stored_settings);
        let sessions = sessions.into_iter().collect::<Vec<_>>();

        if let Err(error) = Application::default()
            .settings()
            .set_string("sessions", &serde_json::to_string(&sessions).unwrap())
        {
            error!("Failed to save session settings: {error}");
        }
    }

    /// Delete the settings from the GSettings.
    pub fn delete(&self) {
        let mut sessions = sessions();

        sessions.remove(self.session_id());

        if let Err(error) = Application::default()
            .settings()
            .set_string("sessions", &serde_json::to_string(&sessions).unwrap())
        {
            error!("Failed to delete session settings: {error}");
        }
    }

    /// The ID of the session these settings are for.
    pub fn session_id(&self) -> &str {
        self.imp().session_id.get().unwrap()
    }

    /// Set the ID of the session these settings are for.
    fn set_session_id(&self, session_id: Option<String>) {
        let session_id = match session_id {
            Some(s) => s,
            None => return,
        };

        let imp = self.imp();
        imp.session_id.set(session_id.clone()).unwrap();

        if let Some(session_settings) = sessions()
            .into_iter()
            .find_map(|(s_id, session)| (s_id == session_id).then_some(session))
        {
            // Restore the settings.
            imp.stored_settings.replace(session_settings);
        } else {
            // This is a new session, add it to the list of sessions.
            self.save();
        }
    }

    pub fn explore_custom_servers(&self) -> Vec<String> {
        self.imp()
            .stored_settings
            .borrow()
            .explore_custom_servers
            .clone()
    }

    pub fn set_explore_custom_servers(&self, servers: Vec<String>) {
        if self.explore_custom_servers() == servers {
            return;
        }

        self.imp()
            .stored_settings
            .borrow_mut()
            .explore_custom_servers = servers;
        self.save();
    }

    /// Whether notifications are enabled for this session.
    pub fn notifications_enabled(&self) -> bool {
        self.imp().stored_settings.borrow().notifications_enabled
    }

    /// Set whether notifications are enabled for this session.
    pub fn set_notifications_enabled(&self, enabled: bool) {
        if self.notifications_enabled() == enabled {
            return;
        }

        self.imp()
            .stored_settings
            .borrow_mut()
            .notifications_enabled = enabled;
        self.save();
        self.notify("notifications-enabled");
    }
}

/// Get map of session stored in the GSettings.
fn sessions() -> IndexMap<String, StoredSessionSettings> {
    let serialized = Application::default().settings().string("sessions");

    match serde_json::from_str::<Vec<(String, StoredSessionSettings)>>(&serialized) {
        Ok(stored_settings) => stored_settings.into_iter().collect(),
        Err(error) => {
            error!("Failed to load profile settings, fallback to default settings: {error}");
            Default::default()
        }
    }
}

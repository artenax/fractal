use gtk::{glib, prelude::*, subclass::prelude::*};
use log::error;
use serde::{Deserialize, Serialize};

use crate::Application;

#[derive(Serialize, Deserialize)]
struct StoredSessionSettings {
    /// The ID of the session these settings are for.
    session_id: String,

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

mod imp {
    use std::cell::{Cell, RefCell};

    use once_cell::sync::{Lazy, OnceCell};

    use super::*;

    #[derive(Debug)]
    pub struct SessionSettings {
        /// The ID of the session these settings are for.
        pub session_id: OnceCell<String>,

        /// Custom servers to explore.
        pub explore_custom_servers: RefCell<Vec<String>>,

        /// Whether notifications are enabled for this session.
        pub notifications_enabled: Cell<bool>,
    }

    impl Default for SessionSettings {
        fn default() -> Self {
            Self {
                session_id: Default::default(),
                explore_custom_servers: Default::default(),
                notifications_enabled: Cell::new(true),
            }
        }
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
    pub fn new(session_id: &str) -> Self {
        glib::Object::builder()
            .property("session-id", &session_id)
            .build()
    }

    /// The ID of the session these settings are for.
    pub fn session_id(&self) -> &str {
        self.imp().session_id.get().unwrap()
    }

    /// Set the ID of the session these settings are for.
    fn set_session_id(&self, session_id: Option<String>) {
        let priv_ = self.imp();

        let session_id = match session_id {
            Some(s) => s,
            None => return,
        };

        let app_settings = Application::default().settings();
        let sessions =
            serde_json::from_str::<Vec<StoredSessionSettings>>(&app_settings.string("sessions"))
                .unwrap_or_default();

        let index = sessions
            .iter()
            .enumerate()
            .find_map(|(idx, settings)| (settings.session_id == session_id).then_some(idx));

        priv_.session_id.set(session_id).unwrap();

        if let Some(settings) = index.and_then(|idx| sessions.into_iter().nth(idx)) {
            self.update_from_stored_settings(settings);
        } else {
            self.store_settings();
        }
    }

    fn update_from_stored_settings(&self, settings: StoredSessionSettings) {
        let priv_ = self.imp();
        let StoredSessionSettings {
            session_id: _,
            explore_custom_servers,
            notifications_enabled,
        } = settings;

        *priv_.explore_custom_servers.borrow_mut() = explore_custom_servers;
        priv_.notifications_enabled.set(notifications_enabled);
    }

    fn as_stored_settings(&self) -> StoredSessionSettings {
        StoredSessionSettings {
            session_id: self.session_id().to_owned(),
            explore_custom_servers: self.explore_custom_servers(),
            notifications_enabled: self.notifications_enabled(),
        }
    }

    fn store_settings(&self) {
        let new_settings = self.as_stored_settings();

        let app_settings = Application::default().settings();
        let mut sessions =
            serde_json::from_str::<Vec<StoredSessionSettings>>(&app_settings.string("sessions"))
                .unwrap_or_default();

        let index = sessions.iter().enumerate().find_map(|(idx, settings)| {
            (settings.session_id == new_settings.session_id).then_some(idx)
        });
        if let Some(index) = index {
            sessions[index] = new_settings;
        } else {
            sessions.push(new_settings);
        }

        if let Err(error) =
            app_settings.set_string("sessions", &serde_json::to_string(&sessions).unwrap())
        {
            error!("Error storing settings for session: {error}");
        }
    }

    pub fn delete_settings(&self) {
        let app_settings = Application::default().settings();
        if let Ok(sessions) =
            serde_json::from_str::<Vec<StoredSessionSettings>>(&app_settings.string("sessions"))
        {
            let session_id = self.session_id();
            let mut found = false;
            let sessions = sessions
                .into_iter()
                .filter(|settings| {
                    if settings.session_id == session_id {
                        found = true;
                        false
                    } else {
                        true
                    }
                })
                .collect::<Vec<_>>();

            if found {
                if let Err(error) =
                    app_settings.set_string("sessions", &serde_json::to_string(&sessions).unwrap())
                {
                    log::error!("Error deleting settings for session: {error}");
                }
            }
        }
    }

    pub fn explore_custom_servers(&self) -> Vec<String> {
        self.imp().explore_custom_servers.borrow().clone()
    }

    pub fn set_explore_custom_servers(&self, servers: Vec<String>) {
        if self.explore_custom_servers() == servers {
            return;
        }

        self.imp().explore_custom_servers.replace(servers);
        self.store_settings();
    }

    /// Whether notifications are enabled for this session.
    pub fn notifications_enabled(&self) -> bool {
        self.imp().notifications_enabled.get()
    }

    /// Set whether notifications are enabled for this session.
    pub fn set_notifications_enabled(&self, enabled: bool) {
        if self.notifications_enabled() == enabled {
            return;
        }

        self.imp().notifications_enabled.replace(enabled);
        self.store_settings();
        self.notify("notifications-enabled");
    }
}

use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use log::{debug, warn};
use ruma::{
    api::client::push::get_notifications::v3::Notification, EventId, OwnedEventId, OwnedRoomId,
    RoomId,
};

use super::{Room, Session};
use crate::{
    application::AppShowRoomPayload, prelude::*, utils::matrix::get_event_body, Application,
};

mod imp {
    use std::{cell::RefCell, collections::HashMap};

    use glib::WeakRef;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Notifications {
        pub session: WeakRef<Session>,
        /// A map of room ID to list of event IDs for which a notification was
        /// sent to the system.
        pub list: RefCell<HashMap<OwnedRoomId, Vec<OwnedEventId>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Notifications {
        const NAME: &'static str = "Notifications";
        type Type = super::Notifications;
    }

    impl ObjectImpl for Notifications {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Session>("session")
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "session" => self.obj().set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session" => self.obj().session().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// The notifications of a `Session`.
    pub struct Notifications(ObjectSubclass<imp::Notifications>);
}

impl Notifications {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Set the current session.
    pub fn set_session(&self, session: Option<&Session>) {
        let imp = self.imp();

        if self.session().as_ref() == session {
            return;
        }

        imp.session.set(session);
        self.notify("session");
    }

    /// Ask the system to show the given notification, if applicable.
    ///
    /// The notification won't be shown if the application is active and this
    /// session is displayed.
    pub fn show(&self, matrix_notification: Notification) {
        let Some(session) = self.session() else {
            return;
        };

        // Don't show notifications if they are disabled.
        if !session.settings().notifications_enabled() {
            return;
        }

        let app = Application::default();
        let window = app.main_window();
        let session_id = session.session_id();

        // Don't show notifications for the current session if the window is active.
        if window.is_active() && window.current_session_id().as_deref() == Some(session_id) {
            return;
        }

        let Some(room) = session.room_list().get(&matrix_notification.room_id) else {
            warn!(
                "Could not display notification for missing room {}",
                matrix_notification.room_id
            );
            return;
        };

        let event = match matrix_notification.event.deserialize() {
            Ok(event) => event,
            Err(error) => {
                warn!(
                    "Could not display notification for unrecognized event in room {}: {error}",
                    matrix_notification.room_id
                );
                return;
            }
        };

        let sender_name = room
            .members()
            .member_by_id(event.sender().to_owned())
            .display_name();

        let body = match get_event_body(&event, &sender_name) {
            Some(body) => body,
            None => {
                debug!("Received notification for event of unexpected type {event:?}",);
                return;
            }
        };

        let room_id = room.room_id();
        let event_id = event.event_id();

        let notification = gio::Notification::new(&room.display_name());
        notification.set_priority(gio::NotificationPriority::High);

        let payload = AppShowRoomPayload {
            session_id: session_id.to_owned(),
            room_id: room_id.to_owned(),
        };

        notification
            .set_default_action_and_target_value("app.show-room", Some(&payload.to_variant()));
        notification.set_body(Some(&body));

        if let Some(icon) = room.avatar_data().as_notification_icon() {
            notification.set_icon(&icon);
        }

        let id = notification_id(session_id, room_id, event_id);
        Application::default().send_notification(Some(&id), &notification);

        self.imp()
            .list
            .borrow_mut()
            .entry(room_id.to_owned())
            .or_default()
            .push(event_id.to_owned());
    }

    /// Ask the system to remove the known notifications for the given room.
    ///
    /// Only the notifications that were shown since the application's startup
    /// are known, older ones might still be present.
    pub fn withdraw_all_for_room(&self, room: &Room) {
        let Some(session) = self.session() else {
            return;
        };

        let room_id = room.room_id();
        if let Some(notifications) = self.imp().list.borrow_mut().remove(room_id) {
            let app = Application::default();

            for event_id in notifications {
                let id = notification_id(session.session_id(), room_id, &event_id);
                app.withdraw_notification(&id);
            }
        }
    }

    /// Ask the system to remove all the known notifications for this session.
    ///
    /// Only the notifications that were shown since the application's startup
    /// are known, older ones might still be present.
    pub fn clear(&self) {
        let Some(session) = self.session() else {
            return;
        };

        let app = Application::default();

        for (room_id, notifications) in self.imp().list.take() {
            for event_id in notifications {
                let id = notification_id(session.session_id(), &room_id, &event_id);
                app.withdraw_notification(&id);
            }
        }
    }
}

impl Default for Notifications {
    fn default() -> Self {
        Self::new()
    }
}

fn notification_id(session_id: &str, room_id: &RoomId, event_id: &EventId) -> String {
    format!("{session_id}//{room_id}//{event_id}")
}

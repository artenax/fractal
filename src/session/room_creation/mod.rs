use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{gdk, glib, glib::clone, CompositeTemplate};
use log::error;
use matrix_sdk::{
    ruma::{
        api::client::{
            error::ErrorKind,
            room::{create_room, Visibility},
        },
        assign,
    },
    Error,
};
use ruma::events::{room::encryption::RoomEncryptionEventContent, InitialStateEvent};

use crate::{
    components::SpinnerButton,
    session::{user::UserExt, Session},
    spawn, spawn_tokio,
    window::Window,
    UserFacingError,
};

// MAX length of room addresses
const MAX_BYTES: usize = 255;

mod imp {
    use glib::{object::WeakRef, subclass::InitializingObject};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/room-creation.ui")]
    pub struct RoomCreation {
        pub session: WeakRef<Session>,
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub create_button: TemplateChild<SpinnerButton>,
        #[template_child]
        pub content: TemplateChild<gtk::Box>,
        #[template_child]
        pub room_name: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub room_topic: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub visibility_private: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub encryption: TemplateChild<gtk::Switch>,
        #[template_child]
        pub room_address: TemplateChild<gtk::Entry>,
        #[template_child]
        pub server_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub room_address_error_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub room_address_error: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoomCreation {
        const NAME: &'static str = "RoomCreation";
        type Type = super::RoomCreation;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);

            klass.add_binding(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                |obj, _| {
                    obj.close();
                    true
                },
                None,
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for RoomCreation {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
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

    impl WidgetImpl for RoomCreation {}
    impl WindowImpl for RoomCreation {}
    impl AdwWindowImpl for RoomCreation {}
}

glib::wrapper! {
    /// Preference Window to display and update room details.
    pub struct RoomCreation(ObjectSubclass<imp::RoomCreation>)
        @extends gtk::Widget, gtk::Window, adw::Window, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl RoomCreation {
    pub fn new(parent_window: Option<&impl IsA<gtk::Window>>, session: &Session) -> Self {
        glib::Object::builder()
            .property("transient-for", parent_window)
            .property("session", session)
            .build()
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

        if let Some(user) = session.as_ref().and_then(|session| session.user()) {
            imp.server_name
                .set_label(&format!(":{}", user.user_id().server_name()));
        }

        imp.session.set(session);
        self.notify("session");
    }

    /// Create the room, if it is allowed.
    #[template_callback]
    fn create_room(&self) {
        let imp = self.imp();

        if !self.can_create_room() {
            return;
        }

        imp.create_button.set_loading(true);
        imp.content.set_sensitive(false);

        let Some(session) = self.session() else {
            return;
        };
        let client = session.client();

        let name = Some(imp.room_name.text().to_string());
        let topic = Some(imp.room_topic.text().to_string()).filter(|s| !s.is_empty());

        let mut request = assign!(
            create_room::v3::Request::new(),
            {
                name,
                topic,
            }
        );

        if imp.visibility_private.is_active() {
            // The room is private.
            request.visibility = Visibility::Private;

            if imp.encryption.is_active() {
                let event =
                    InitialStateEvent::new(RoomEncryptionEventContent::with_recommended_defaults());
                request.initial_state = vec![event.to_raw_any()];
            }
        } else {
            // The room is public.
            request.visibility = Visibility::Public;
            request.room_alias_name = Some(imp.room_address.text().to_string());
        };

        let handle = spawn_tokio!(async move { client.create_room(request).await });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                    Ok(matrix_room) => {
                        if let Some(session) = obj.session() {
                            let Some(window) = obj.transient_for().and_downcast::<Window>() else {
                                return;
                            };
                            let room = session.room_list().get_wait(matrix_room.room_id()).await;
                            window.session_view().select_room(room);
                        }
                        obj.close();
                    },
                    Err(error) => {
                        error!("Couldn’t create a new room: {error}");
                        obj.handle_error(error);
                    },
                };
            })
        );
    }

    /// Display the error that occurred during creation.
    fn handle_error(&self, error: Error) {
        let imp = self.imp();

        imp.create_button.set_loading(false);
        imp.content.set_sensitive(true);

        // Handle the room address already taken error.
        if let Some(kind) = error.client_api_error_kind() {
            if *kind == ErrorKind::RoomInUse {
                imp.room_address.add_css_class("error");
                imp.room_address_error
                    .set_text(&gettext("The address is already taken."));
                imp.room_address_error_revealer.set_reveal_child(true);

                return;
            }
        }

        imp.toast_overlay
            .add_toast(adw::Toast::new(&error.to_user_facing()));
    }

    /// Check whether a room can be created with the current input.
    ///
    /// This will also change the UI elements to reflect why the room can't be
    /// created.
    fn can_create_room(&self) -> bool {
        let imp = self.imp();
        let mut can_create = true;

        if imp.room_name.text().is_empty() {
            can_create = false;
        }

        // Only public rooms have an address.
        if imp.visibility_private.is_active() {
            return can_create;
        }

        let room_address = imp.room_address.text();

        // We don't allow #, : in the room address
        let address_has_error = if room_address.contains(':') {
            imp.room_address_error
                .set_text(&gettext("Can’t contain “:”"));
            can_create = false;
            true
        } else if room_address.contains('#') {
            imp.room_address_error
                .set_text(&gettext("Can’t contain “#”"));
            can_create = false;
            true
        } else if room_address.len() > MAX_BYTES {
            imp.room_address_error
                .set_text(&gettext("Too long. Use a shorter address."));
            can_create = false;
            true
        } else if room_address.is_empty() {
            can_create = false;
            false
        } else {
            false
        };

        // TODO: should we immediately check if the address is available, like element
        // is doing?

        if address_has_error {
            imp.room_address.add_css_class("error");
        } else {
            imp.room_address.remove_css_class("error");
        }
        imp.room_address_error_revealer
            .set_reveal_child(address_has_error);

        can_create
    }

    /// Validate the form and change the corresponding UI elements.
    #[template_callback]
    fn validate_form(&self) {
        self.imp()
            .create_button
            .set_sensitive(self.can_create_room());
    }
}

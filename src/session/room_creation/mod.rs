use std::convert::{TryFrom, TryInto};

use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gdk, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};
use log::error;
use matrix_sdk::{
    ruma::{
        api::{
            client::{
                error::ErrorKind as RumaClientErrorKind,
                room::{create_room, Visibility},
            },
            error::{FromHttpResponseError, ServerError},
        },
        assign,
        identifiers::{Error, RoomName},
    },
    HttpError,
};

use crate::{
    components::SpinnerButton,
    session::{user::UserExt, Session},
    spawn, spawn_tokio, UserFacingError,
};

// MAX length of room addresses
const MAX_BYTES: usize = 255;

mod imp {
    use std::cell::RefCell;

    use glib::{object::WeakRef, subclass::InitializingObject};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/room-creation.ui")]
    pub struct RoomCreation {
        pub session: RefCell<Option<WeakRef<Session>>>,
        #[template_child]
        pub content: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub create_button: TemplateChild<SpinnerButton>,
        #[template_child]
        pub cancel_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub room_name: TemplateChild<gtk::Entry>,
        #[template_child]
        pub private_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub room_address: TemplateChild<gtk::Entry>,
        #[template_child]
        pub room_name_error_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub room_name_error: TemplateChild<gtk::Label>,
        #[template_child]
        pub room_address_error_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub room_address_error: TemplateChild<gtk::Label>,
        #[template_child]
        pub server_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub error_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub error_label_revealer: TemplateChild<gtk::Revealer>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoomCreation {
        const NAME: &'static str = "RoomCreation";
        type Type = super::RoomCreation;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            SpinnerButton::static_type();
            Self::bind_template(klass);

            klass.add_binding(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                |obj, _| {
                    obj.cancel();
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
                vec![glib::ParamSpecObject::new(
                    "session",
                    "Session",
                    "The session",
                    Session::static_type(),
                    glib::ParamFlags::READWRITE,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "session" => obj.set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session" => obj.session().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.cancel_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.cancel();
                }));

            self.create_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.create_room();
                }));

            self.room_name
                .connect_text_notify(clone!(@weak obj = > move |_| {
                    obj.validate_input();
                }));

            self.room_address
                .connect_text_notify(clone!(@weak obj = > move |_| {
                    obj.validate_input();
                }));
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

impl RoomCreation {
    pub fn new(parent_window: Option<&impl IsA<gtk::Window>>, session: &Session) -> Self {
        glib::Object::new(&[("transient-for", &parent_window), ("session", session)])
            .expect("Failed to create RoomCreation")
    }

    pub fn session(&self) -> Option<Session> {
        self.imp()
            .session
            .borrow()
            .as_ref()
            .and_then(|session| session.upgrade())
    }

    fn set_session(&self, session: Option<Session>) {
        let priv_ = self.imp();

        if self.session() == session {
            return;
        }

        if let Some(user) = session.as_ref().and_then(|session| session.user()) {
            priv_
                .server_name
                .set_label(&[":", user.user_id().server_name().as_str()].concat());
        }

        priv_
            .session
            .replace(session.map(|session| session.downgrade()));
        self.notify("session");
    }

    fn create_room(&self) -> Option<()> {
        let priv_ = self.imp();

        priv_.create_button.set_loading(true);
        priv_.content.set_sensitive(false);
        priv_.cancel_button.set_sensitive(false);
        priv_.error_label_revealer.set_reveal_child(false);

        let client = self.session()?.client();

        let room_name = priv_.room_name.text().to_string();

        let visibility = if priv_.private_button.is_active() {
            Visibility::Private
        } else {
            Visibility::Public
        };

        let room_address = if !priv_.private_button.is_active() {
            Some(format!("#{}", priv_.room_address.text().as_str()))
        } else {
            None
        };

        let handle = spawn_tokio!(async move {
            // We don't allow invalid room names to be entered by the user
            let name = room_name.as_str().try_into().unwrap();

            let request = assign!(create_room::v3::Request::new(),
            {
                name: Some(name),
                visibility,
                room_alias_name: room_address.as_deref()
            });
            client.create_room(request).await
        });

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                match handle.await.unwrap() {
                        Ok(response) => {
                            if let Some(session) = obj.session() {
                                let room = session.room_list().get_wait(response.room_id).await;
                                session.select_room(room);
                            }
                            obj.close();
                        },
                        Err(error) => {
                            error!("Couldn’t create a new room: {}", error);
                            obj.handle_error(error);
                        },
                };
            })
        );

        None
    }

    /// Display the error that occurred during creation
    fn handle_error(&self, error: HttpError) {
        let priv_ = self.imp();

        priv_.create_button.set_loading(false);
        priv_.content.set_sensitive(true);
        priv_.cancel_button.set_sensitive(true);

        // Treat the room address already taken error special
        if let HttpError::ClientApi(FromHttpResponseError::Http(ServerError::Known(
            ref client_error,
        ))) = error
        {
            if client_error.kind == RumaClientErrorKind::RoomInUse {
                priv_.room_address.add_css_class("error");
                priv_
                    .room_address_error
                    .set_text(&gettext("The address is already taken."));
                priv_.room_address_error_revealer.set_reveal_child(true);

                return;
            }
        }

        priv_.error_label.set_label(&error.to_user_facing());

        priv_.error_label_revealer.set_reveal_child(true);
    }

    fn validate_input(&self) {
        let priv_ = self.imp();

        // Validate room name
        let (is_name_valid, has_error) =
            match <&RoomName>::try_from(priv_.room_name.text().as_str()) {
                Ok(_) => (true, false),
                Err(Error::EmptyRoomName) => (false, false),
                Err(Error::MaximumLengthExceeded) => {
                    priv_
                        .room_name_error
                        .set_text(&gettext("Too long. Use a shorter name."));
                    (false, true)
                }
                Err(_) => unimplemented!(),
            };

        if has_error {
            priv_.room_name.add_css_class("error");
        } else {
            priv_.room_name.remove_css_class("error");
        }

        priv_.room_name_error_revealer.set_reveal_child(has_error);

        // Validate room address

        // Only public rooms have a address
        if priv_.private_button.is_active() {
            priv_.create_button.set_sensitive(is_name_valid);
            return;
        }

        let room_address = priv_.room_address.text();

        // We don't allow #, : in the room address
        let (is_address_valid, has_error) = if room_address.find(':').is_some() {
            priv_
                .room_address_error
                .set_text(&gettext("Can't contain `:`"));
            (false, true)
        } else if room_address.find('#').is_some() {
            priv_
                .room_address_error
                .set_text(&gettext("Can't contain `#`"));
            (false, true)
        } else if room_address.len() > MAX_BYTES {
            priv_
                .room_address_error
                .set_text(&gettext("Too long. Use a shorter address."));
            (false, true)
        } else if room_address.is_empty() {
            (false, false)
        } else {
            (true, false)
        };

        // TODO: should we immediately check if the address is available, like element
        // is doing?

        if has_error {
            priv_.room_address.add_css_class("error");
        } else {
            priv_.room_address.remove_css_class("error");
        }

        priv_
            .room_address_error_revealer
            .set_reveal_child(has_error);
        priv_
            .create_button
            .set_sensitive(is_name_valid && is_address_valid);
    }

    fn cancel(&self) {
        if self.imp().cancel_button.is_sensitive() {
            self.close();
        }
    }
}

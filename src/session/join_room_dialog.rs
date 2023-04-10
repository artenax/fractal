use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{gdk, glib, CompositeTemplate};
use ruma::{
    matrix_uri::MatrixId, MatrixToUri, MatrixUri, OwnedRoomOrAliasId, OwnedServerName,
    RoomOrAliasId,
};

use crate::session::Session;

mod imp {
    use glib::{object::WeakRef, subclass::InitializingObject};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/join-room-dialog.ui")]
    pub struct JoinRoomDialog {
        pub session: WeakRef<Session>,
        #[template_child]
        pub entry: TemplateChild<gtk::Entry>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for JoinRoomDialog {
        const NAME: &'static str = "JoinRoomDialog";
        type Type = super::JoinRoomDialog;
        type ParentType = adw::MessageDialog;

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

    impl ObjectImpl for JoinRoomDialog {
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

    impl WidgetImpl for JoinRoomDialog {}
    impl WindowImpl for JoinRoomDialog {}

    impl MessageDialogImpl for JoinRoomDialog {
        fn response(&self, response: &str) {
            self.obj().join_room();

            self.parent_response(response)
        }
    }
}

glib::wrapper! {
    /// Dialog to join a room.
    pub struct JoinRoomDialog(ObjectSubclass<imp::JoinRoomDialog>)
        @extends gtk::Widget, gtk::Window, adw::MessageDialog, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl JoinRoomDialog {
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

        imp.session.set(session);
        self.notify("session");
    }

    /// Handle when the entry text changed.
    #[template_callback]
    fn entry_changed(&self, entry: &gtk::Entry) {
        let Some(session) = self.session() else {
            self.set_response_enabled("join", false);
            return;
        };

        let Some((room_id, _)) = parse_room(&entry.text()) else {
            self.set_response_enabled("join", false);
            return;
        };

        self.set_response_enabled("join", true);

        if session.room_list().find_joined_room(&room_id).is_some() {
            self.set_response_label("join", &gettext("_View"));
        } else {
            self.set_response_label("join", &gettext("_Join"));
        }
    }

    /// Join the room that was entered, if it is valid.
    fn join_room(&self) {
        let Some(session) = self.session() else {
            return;
        };

        let Some((room_id, via)) = parse_room(&self.imp().entry.text()) else {
            return;
        };

        if let Some(room) = session.room_list().find_joined_room(&room_id) {
            session.select_room(Some(room));
        } else {
            session.room_list().join_by_id_or_alias(room_id, via)
        }
    }
}

fn parse_room(room: &str) -> Option<(OwnedRoomOrAliasId, Vec<OwnedServerName>)> {
    MatrixUri::parse(room)
        .ok()
        .and_then(|uri| match uri.id() {
            MatrixId::Room(room_id) => Some((room_id.clone().into(), uri.via().to_owned())),
            MatrixId::RoomAlias(room_alias) => {
                Some((room_alias.clone().into(), uri.via().to_owned()))
            }
            _ => None,
        })
        .or_else(|| {
            MatrixToUri::parse(room)
                .ok()
                .and_then(|uri| match uri.id() {
                    MatrixId::Room(room_id) => Some((room_id.clone().into(), uri.via().to_owned())),
                    MatrixId::RoomAlias(room_alias) => {
                        Some((room_alias.clone().into(), uri.via().to_owned()))
                    }
                    _ => None,
                })
        })
        .or_else(|| {
            RoomOrAliasId::parse(room)
                .ok()
                .map(|room_id| (room_id, vec![]))
        })
}

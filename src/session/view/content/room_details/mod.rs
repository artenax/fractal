mod general_page;
mod history_viewer;
mod invite_subpage;
mod member_page;

use std::convert::From;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{glib, CompositeTemplate};

pub use self::{
    general_page::GeneralPage,
    history_viewer::{AudioHistoryViewer, FileHistoryViewer, MediaHistoryViewer},
    invite_subpage::InviteSubpage,
    member_page::MemberPage,
};
use crate::session::model::Room;

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub enum SubpageName {
    Members,
    Invite,
    MediaHistory,
    FileHistory,
    AudioHistory,
}

impl glib::variant::StaticVariantType for SubpageName {
    fn static_variant_type() -> std::borrow::Cow<'static, glib::VariantTy> {
        String::static_variant_type()
    }
}

impl glib::variant::FromVariant for SubpageName {
    fn from_variant(variant: &glib::variant::Variant) -> Option<Self> {
        match variant.str()? {
            "members" => Some(Self::Members),
            "invite" => Some(Self::Invite),
            "media-history" => Some(Self::MediaHistory),
            "file-history" => Some(Self::FileHistory),
            "audio-history" => Some(Self::AudioHistory),
            _ => None,
        }
    }
}

mod imp {
    use std::{cell::RefCell, collections::HashMap};

    use glib::subclass::InitializingObject;
    use once_cell::unsync::OnceCell;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/session/view/content/room_details/mod.ui")]
    pub struct RoomDetails {
        /// The room to show the details for.
        pub room: OnceCell<Room>,
        /// The subpages that are loaded.
        ///
        /// We keep them around to avoid reloading them if the user reopens the
        /// same subpage.
        pub subpages: RefCell<HashMap<SubpageName, adw::NavigationPage>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoomDetails {
        const NAME: &'static str = "RoomDetails";
        type Type = super::RoomDetails;
        type ParentType = adw::PreferencesWindow;

        fn class_init(klass: &mut Self::Class) {
            GeneralPage::static_type();
            Self::bind_template(klass);

            klass.install_action(
                "details.show-subpage",
                Some("s"),
                move |widget, _, param| {
                    let subpage = param
                        .and_then(|variant| variant.get::<SubpageName>())
                        .expect("The parameter should be a valid subpage name");

                    widget.show_subpage(subpage, false);
                },
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for RoomDetails {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Room>("room")
                    .construct_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "room" => obj.set_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room" => self.room.get().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for RoomDetails {}
    impl WindowImpl for RoomDetails {}
    impl AdwWindowImpl for RoomDetails {}
    impl PreferencesWindowImpl for RoomDetails {}
}

glib::wrapper! {
    /// Preference Window to display and update room details.
    pub struct RoomDetails(ObjectSubclass<imp::RoomDetails>)
        @extends gtk::Widget, gtk::Window, adw::Window, gtk::Root, adw::PreferencesWindow, @implements gtk::Accessible;
}

impl RoomDetails {
    pub fn new(parent_window: &Option<gtk::Window>, room: &Room) -> Self {
        glib::Object::builder()
            .property("transient-for", parent_window)
            .property("room", room)
            .build()
    }

    /// The room backing all the details of the preference window.
    pub fn room(&self) -> &Room {
        // Use unwrap because room property is CONSTRUCT_ONLY.
        self.imp().room.get().unwrap()
    }

    /// Set the room backing all the details of the preference window.
    fn set_room(&self, room: Room) {
        self.imp().room.set(room).expect("Room already initialized");
        self.notify("room");
    }

    /// Show the subpage with the given name.
    fn show_subpage(&self, name: SubpageName, is_initial: bool) {
        let imp = self.imp();
        let room = self.room();

        let mut subpages = imp.subpages.borrow_mut();
        let subpage = subpages.entry(name).or_insert_with(|| match name {
            SubpageName::Members => MemberPage::new(room).upcast(),
            SubpageName::Invite => InviteSubpage::new(room).upcast(),
            SubpageName::MediaHistory => MediaHistoryViewer::new(room).upcast(),
            SubpageName::FileHistory => FileHistoryViewer::new(room).upcast(),
            SubpageName::AudioHistory => AudioHistoryViewer::new(room).upcast(),
        });

        if is_initial {
            subpage.set_can_pop(false);
        }

        self.push_subpage(subpage);
    }

    /// Show the given subpage as the initial page.
    pub fn show_initial_subpage(&self, name: SubpageName) {
        self.show_subpage(name, true);
    }
}

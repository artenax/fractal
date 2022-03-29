mod invite_subpage;
mod member_page;

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gdk,
    glib::{self, clone, closure},
    subclass::prelude::*,
    CompositeTemplate,
};
use matrix_sdk::ruma::events::RoomEventType;

pub use self::{invite_subpage::InviteSubpage, member_page::MemberPage};
use crate::{
    components::CustomEntry,
    session::{self, room::RoomAction, Room},
    utils::{and_expr, or_expr},
};

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::unsync::OnceCell;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/content-room-details.ui")]
    pub struct RoomDetails {
        pub room: OnceCell<Room>,
        pub avatar_chooser: OnceCell<gtk::FileChooserNative>,
        #[template_child]
        pub avatar_remove_button: TemplateChild<adw::Bin>,
        #[template_child]
        pub avatar_edit_button: TemplateChild<adw::Bin>,
        #[template_child]
        pub edit_toggle: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub room_name_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub room_topic_text_view: TemplateChild<gtk::TextView>,
        pub member_page: OnceCell<MemberPage>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoomDetails {
        const NAME: &'static str = "RoomDetails";
        type Type = super::RoomDetails;
        type ParentType = adw::PreferencesWindow;

        fn class_init(klass: &mut Self::Class) {
            CustomEntry::static_type();
            Self::bind_template(klass);
            klass.install_action("details.choose-avatar", None, move |widget, _, _| {
                widget.open_avatar_chooser()
            });
            klass.install_action("details.remove-avatar", None, move |widget, _, _| {
                widget.room().store_avatar(None)
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for RoomDetails {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "room",
                    "Room",
                    "The room backing all details of the preference window",
                    Room::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
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
                "room" => obj.set_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room" => self.room.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            let member_page = MemberPage::new(obj.room());
            obj.add(&member_page);
            self.member_page.set(member_page).unwrap();

            obj.init_avatar();
            obj.init_edit_toggle();
            obj.init_avatar_chooser();
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
        glib::Object::new(&[("transient-for", parent_window), ("room", room)])
            .expect("Failed to create RoomDetails")
    }

    pub fn room(&self) -> &Room {
        // Use unwrap because room property is CONSTRUCT_ONLY.
        self.imp().room.get().unwrap()
    }

    fn set_room(&self, room: Room) {
        self.imp().room.set(room).expect("Room already initialized");
    }

    fn init_avatar(&self) {
        let priv_ = self.imp();
        let avatar_remove_button = &priv_.avatar_remove_button;
        let avatar_edit_button = &priv_.avatar_edit_button;

        // Hide avatar controls when the user is not eligible to perform the actions.
        let room = self.room();

        let room_avatar_exists = room
            .property_expression("avatar")
            .chain_property::<session::Avatar>("image")
            .chain_closure::<bool>(closure!(
                |_: Option<glib::Object>, image: Option<gdk::Paintable>| { image.is_some() }
            ));

        let room_avatar_changeable =
            room.new_allowed_expr(RoomAction::StateEvent(RoomEventType::RoomAvatar));
        let room_avatar_removable = and_expr(&room_avatar_changeable, &room_avatar_exists);

        room_avatar_removable.bind(&avatar_remove_button.get(), "visible", gtk::Widget::NONE);
        room_avatar_changeable.bind(&avatar_edit_button.get(), "visible", gtk::Widget::NONE);
    }

    fn init_edit_toggle(&self) {
        let priv_ = self.imp();
        let edit_toggle = &priv_.edit_toggle;
        let label_enabled = gettext("Save Details");
        let label_disabled = gettext("Edit Details");

        edit_toggle.set_active(false);
        edit_toggle.set_label(&label_disabled);

        // Save changes of name and topic on toggle button release.
        edit_toggle.connect_toggled(clone!(@weak self as this => move |button| {
            if button.is_active() {
                button.set_label(&label_enabled);
                return;
            }
            button.set_label(&label_disabled);

            let priv_ = this.imp();
            let room = this.room();

            let room_name = priv_.room_name_entry.buffer().text();
            let topic_buffer = priv_.room_topic_text_view.buffer();
            let topic = topic_buffer.text(&topic_buffer.start_iter(), &topic_buffer.end_iter(), true);
            room.store_room_name(room_name);
            room.store_topic(topic.to_string());
        }));

        // End editing on enter.
        priv_
            .room_name_entry
            .connect_activate(clone!(@weak self as this => move |_entry| {
                this.imp().edit_toggle.set_active(false);
            }));

        // Hide edit controls when the user is not eligible to perform the actions.
        let room = self.room();
        let room_name_changeable =
            room.new_allowed_expr(RoomAction::StateEvent(RoomEventType::RoomName));
        let room_topic_changeable =
            room.new_allowed_expr(RoomAction::StateEvent(RoomEventType::RoomTopic));

        let edit_toggle_visible = or_expr(room_name_changeable, room_topic_changeable);
        edit_toggle_visible.bind(&edit_toggle.get(), "visible", gtk::Widget::NONE);
    }

    fn init_avatar_chooser(&self) {
        let avatar_chooser = gtk::FileChooserNative::new(
            Some(&gettext("Choose avatar")),
            Some(self),
            gtk::FileChooserAction::Open,
            None,
            None,
        );
        avatar_chooser.connect_response(clone!(@weak self as this => move |chooser, response| {
            let file = chooser.file().and_then(|f| f.path());
            if let (gtk::ResponseType::Accept, Some(file)) = (response, file) {
                log::debug!("Chose file {:?}", file);
                this.room().store_avatar(Some(file));
            }
        }));

        // We must keep a reference to FileChooserNative around as it is not
        // managed by GTK.
        self.imp()
            .avatar_chooser
            .set(avatar_chooser)
            .expect("File chooser already initialized");
    }

    fn avatar_chooser(&self) -> &gtk::FileChooserNative {
        self.imp().avatar_chooser.get().unwrap()
    }

    fn open_avatar_chooser(&self) {
        self.avatar_chooser().show();
    }

    pub fn present_invite_subpage(&self) {
        self.set_title(Some(&gettext("Invite new Members")));
        let subpage = InviteSubpage::new(self.room());
        self.present_subpage(&subpage);
    }

    pub fn close_invite_subpage(&self) {
        self.set_title(Some(&gettext("Room Details")));
        self.close_subpage();
    }

    pub fn member_page(&self) -> &MemberPage {
        self.imp().member_page.get().unwrap()
    }
}

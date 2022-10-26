use std::convert::From;

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gdk,
    glib::{self, clone, closure},
    CompositeTemplate,
};
use log::error;
use matrix_sdk::ruma::events::RoomEventType;

use crate::{
    components::CustomEntry,
    session::{self, room::RoomAction, Room},
    utils::{and_expr, or_expr},
};

mod imp {
    use std::cell::Cell;

    use glib::subclass::InitializingObject;
    use once_cell::unsync::OnceCell;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-room-details-general-page.ui")]
    pub struct GeneralPage {
        pub room: OnceCell<Room>,
        pub avatar_chooser: OnceCell<gtk::FileChooserNative>,
        #[template_child]
        pub avatar_remove_button: TemplateChild<adw::Bin>,
        #[template_child]
        pub avatar_edit_button: TemplateChild<adw::Bin>,
        #[template_child]
        pub edit_toggle: TemplateChild<gtk::Button>,
        #[template_child]
        pub room_name_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub room_topic_text_view: TemplateChild<gtk::TextView>,
        #[template_child]
        pub room_topic_entry: TemplateChild<CustomEntry>,
        #[template_child]
        pub room_topic_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub members_count: TemplateChild<gtk::Label>,
        pub edit_mode: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GeneralPage {
        const NAME: &'static str = "ContentRoomDetailsGeneralPage";
        type Type = super::GeneralPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
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

    impl ObjectImpl for GeneralPage {
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
            match pspec.name() {
                "room" => self.obj().set_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room" => self.room.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.init_avatar();
            obj.init_edit_toggle();

            let members = obj.room().members();
            members.connect_items_changed(clone!(@weak obj => move |members, _, _, _| {
                obj.member_count_changed(members.n_items());
            }));

            obj.member_count_changed(members.n_items());
        }
    }

    impl WidgetImpl for GeneralPage {}
    impl BinImpl for GeneralPage {}
}

glib::wrapper! {
    /// Preference Window to display and update room details.
    pub struct GeneralPage(ObjectSubclass<imp::GeneralPage>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl GeneralPage {
    pub fn new(room: &Room) -> Self {
        glib::Object::builder().property("room", room).build()
    }

    /// The room backing all the details of the preference window.
    pub fn room(&self) -> &Room {
        // Use unwrap because room property is CONSTRUCT_ONLY.
        self.imp().room.get().unwrap()
    }

    /// Set the room backing all the details of the preference window.
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

        edit_toggle.set_label(&label_disabled);

        // Save changes of name and topic on toggle button release.
        edit_toggle.connect_clicked(clone!(@weak self as this => move |button| {
            let priv_ = this.imp();
            if !priv_.edit_mode.get() {
                priv_.edit_mode.set(true);
                button.set_label(&label_enabled);
                priv_.room_topic_text_view.set_justification(gtk::Justification::Left);
                priv_.room_name_entry.set_xalign(0.0);
                priv_.room_name_entry.set_halign(gtk::Align::Center);
                priv_.room_name_entry.set_sensitive(true);
                priv_.room_name_entry.set_width_chars(25);
                priv_.room_topic_entry.set_sensitive(true);
                priv_.room_topic_label.show();
                return;
            }
            priv_.edit_mode.set(false);
            button.set_label(&label_disabled);
            priv_.room_topic_text_view.set_justification(gtk::Justification::Center);
            priv_.room_name_entry.set_xalign(0.5);
            priv_.room_name_entry.set_sensitive(false);
            priv_.room_name_entry.set_halign(gtk::Align::Fill);
            priv_.room_name_entry.set_width_chars(-1);
            priv_.room_topic_entry.set_sensitive(false);
            priv_.room_topic_label.hide();

            let room = this.room();

            let room_name = priv_.room_name_entry.buffer().text();
            let topic_buffer = priv_.room_topic_text_view.buffer();
            let topic = topic_buffer.text(&topic_buffer.start_iter(), &topic_buffer.end_iter(), true);
            room.store_room_name(room_name);
            room.store_topic(topic.to_string());
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

    fn avatar_chooser(&self) -> Option<&gtk::FileChooserNative> {
        if let Some(avatar_chooser) = self.imp().avatar_chooser.get() {
            Some(avatar_chooser)
        } else {
            let window = self.root()?.downcast::<adw::Window>().ok()?;

            let avatar_chooser = gtk::FileChooserNative::new(
                Some(&gettext("Choose avatar")),
                Some(&window),
                gtk::FileChooserAction::Open,
                None,
                None,
            );
            avatar_chooser.connect_response(
                clone!(@weak self as this => move |chooser, response| {
                    let file = chooser.file().and_then(|f| f.path());
                    if let (gtk::ResponseType::Accept, Some(file)) = (response, file) {
                        log::debug!("Chose file {:?}", file);
                        this.room().store_avatar(Some(file));
                    }
                }),
            );

            // We must keep a reference to FileChooserNative around as it is not
            // managed by GTK.
            self.imp()
                .avatar_chooser
                .set(avatar_chooser)
                .expect("File chooser already initialized");

            self.avatar_chooser()
        }
    }

    fn open_avatar_chooser(&self) {
        if let Some(avatar_chooser) = self.avatar_chooser() {
            avatar_chooser.show();
        } else {
            error!("Failed to create the FileChooserNative");
        }
    }

    fn member_count_changed(&self, n: u32) {
        self.imp().members_count.set_text(&format!("{}", n));
    }
}

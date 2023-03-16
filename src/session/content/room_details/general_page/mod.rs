use std::convert::From;

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    CompositeTemplate,
};
use log::error;
use matrix_sdk::room::Room as MatrixRoom;
use ruma::{
    assign,
    events::{room::avatar::ImageInfo, StateEventType},
    OwnedMxcUri,
};

use crate::{
    components::{CustomEntry, EditableAvatar},
    session::{room::RoomAction, Room},
    spawn, spawn_tokio, toast,
    utils::{
        media::{get_image_info, load_file},
        or_expr, OngoingAsyncAction,
    },
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::InitializingObject;
    use once_cell::unsync::OnceCell;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-room-details-general-page.ui")]
    pub struct GeneralPage {
        pub room: OnceCell<Room>,
        #[template_child]
        pub avatar: TemplateChild<EditableAvatar>,
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
        pub changing_avatar: RefCell<Option<OngoingAsyncAction<OwnedMxcUri>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GeneralPage {
        const NAME: &'static str = "ContentRoomDetailsGeneralPage";
        type Type = super::GeneralPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
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
        room.avatar().connect_notify_local(
            Some("url"),
            clone!(@weak self as obj => move |avatar, _| {
                obj.avatar_changed(avatar.url());
            }),
        );

        self.imp().room.set(room).expect("Room already initialized");
    }

    fn init_avatar(&self) {
        let avatar = &*self.imp().avatar;
        avatar.connect_edit_avatar(clone!(@weak self as obj => move |_, file| {
            spawn!(
                clone!(@weak obj => async move {
                    obj.change_avatar(file).await;
                })
            );
        }));
        avatar.connect_remove_avatar(clone!(@weak self as obj => move |_| {
            spawn!(
                clone!(@weak obj => async move {
                    obj.remove_avatar().await;
                })
            );
        }));

        // Hide avatar controls when the user is not eligible to perform the actions.
        let room = self.room();
        let room_avatar_changeable =
            room.new_allowed_expr(RoomAction::StateEvent(StateEventType::RoomAvatar));

        room_avatar_changeable.bind(avatar, "editable", gtk::Widget::NONE);
    }

    fn avatar_changed(&self, uri: Option<OwnedMxcUri>) {
        let imp = self.imp();

        if let Some(action) = imp.changing_avatar.borrow().as_ref() {
            if uri.as_ref() != action.as_value() {
                // This is not the change we expected, maybe another device did a change too.
                // Let's wait for another change.
                return;
            }
        } else {
            // No action is ongoing, we don't need to do anything.
            return;
        };

        // Reset the state.
        imp.changing_avatar.take();
        imp.avatar.success();
        if uri.is_none() {
            toast!(self, gettext("Avatar removed successfully"));
        } else {
            toast!(self, gettext("Avatar changed successfully"));
        }
    }

    async fn change_avatar(&self, file: gio::File) {
        let room = self.room();
        let MatrixRoom::Joined(matrix_room) = room.matrix_room() else {
            error!("Cannot change avatar of room not joined");
            return;
        };

        let imp = self.imp();
        let avatar = &imp.avatar;
        avatar.edit_in_progress();

        let (data, info) = match load_file(&file).await {
            Ok(res) => res,
            Err(error) => {
                error!("Could not load room avatar file: {error}");
                toast!(self, gettext("Could not load file"));
                avatar.reset();
                return;
            }
        };

        let base_image_info = get_image_info(&file).await;
        let image_info = assign!(ImageInfo::new(), {
            width: base_image_info.width,
            height: base_image_info.height,
            size: info.size.map(Into::into),
            mimetype: Some(info.mime.to_string()),
        });

        let client = room.session().client();
        let handle = spawn_tokio!(async move { client.media().upload(&info.mime, data).await });

        let uri = match handle.await.unwrap() {
            Ok(res) => res.content_uri,
            Err(error) => {
                error!("Could not upload room avatar: {}", error);
                toast!(self, gettext("Could not upload avatar"));
                avatar.reset();
                return;
            }
        };

        let (action, weak_action) = OngoingAsyncAction::set(uri.clone());
        imp.changing_avatar.replace(Some(action));

        let handle =
            spawn_tokio!(async move { matrix_room.set_avatar_url(&uri, Some(image_info)).await });

        // We don't need to handle the success of the request, we should receive the
        // change via sync.
        if let Err(error) = handle.await.unwrap() {
            // Because this action can finish in avatar_changed, we must only act if this is
            // still the current action.
            if weak_action.is_ongoing() {
                imp.changing_avatar.take();
                error!("Could not change room avatar: {error}");
                toast!(self, gettext("Could not change avatar"));
                avatar.reset();
            }
        }
    }

    async fn remove_avatar(&self) {
        let room = self.room();
        let MatrixRoom::Joined(matrix_room) = room.matrix_room() else {
            error!("Cannot remove avatar of room not joined");
            return;
        };

        let imp = self.imp();
        let avatar = &*imp.avatar;
        avatar.removal_in_progress();

        let (action, weak_action) = OngoingAsyncAction::remove();
        imp.changing_avatar.replace(Some(action));

        let handle = spawn_tokio!(async move { matrix_room.remove_avatar().await });

        // We don't need to handle the success of the request, we should receive the
        // change via sync.
        if let Err(error) = handle.await.unwrap() {
            // Because this action can finish in avatar_changed, we must only act if this is
            // still the current action.
            if weak_action.is_ongoing() {
                imp.changing_avatar.take();
                error!("Could not remove room avatar: {}", error);
                toast!(self, gettext("Could not remove avatar"));
                avatar.reset();
            }
        }
    }

    fn init_edit_toggle(&self) {
        let imp = self.imp();
        let edit_toggle = &imp.edit_toggle;
        let label_enabled = gettext("Save Details");
        let label_disabled = gettext("Edit Details");

        edit_toggle.set_label(&label_disabled);

        // Save changes of name and topic on toggle button release.
        edit_toggle.connect_clicked(clone!(@weak self as this => move |button| {
            let imp = this.imp();
            if !imp.edit_mode.get() {
                imp.edit_mode.set(true);
                button.set_label(&label_enabled);
                imp.room_topic_text_view.set_justification(gtk::Justification::Left);
                imp.room_name_entry.set_xalign(0.0);
                imp.room_name_entry.set_halign(gtk::Align::Center);
                imp.room_name_entry.set_sensitive(true);
                imp.room_name_entry.set_width_chars(25);
                imp.room_topic_entry.set_sensitive(true);
                imp.room_topic_label.show();
                return;
            }
            imp.edit_mode.set(false);
            button.set_label(&label_disabled);
            imp.room_topic_text_view.set_justification(gtk::Justification::Center);
            imp.room_name_entry.set_xalign(0.5);
            imp.room_name_entry.set_sensitive(false);
            imp.room_name_entry.set_halign(gtk::Align::Fill);
            imp.room_name_entry.set_width_chars(-1);
            imp.room_topic_entry.set_sensitive(false);
            imp.room_topic_label.hide();

            let room = this.room();

            let room_name = imp.room_name_entry.buffer().text().to_string();
            let topic_buffer = imp.room_topic_text_view.buffer();
            let topic = topic_buffer.text(&topic_buffer.start_iter(), &topic_buffer.end_iter(), true);
            room.store_room_name(room_name);
            room.store_topic(topic.to_string());
        }));

        // Hide edit controls when the user is not eligible to perform the actions.
        let room = self.room();
        let room_name_changeable =
            room.new_allowed_expr(RoomAction::StateEvent(StateEventType::RoomName));
        let room_topic_changeable =
            room.new_allowed_expr(RoomAction::StateEvent(StateEventType::RoomTopic));

        let edit_toggle_visible = or_expr(room_name_changeable, room_topic_changeable);
        edit_toggle_visible.bind(&edit_toggle.get(), "visible", gtk::Widget::NONE);
    }

    fn member_count_changed(&self, n: u32) {
        self.imp().members_count.set_text(&format!("{n}"));
    }
}

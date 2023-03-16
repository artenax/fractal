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
    components::{CustomEntry, EditableAvatar, SpinnerButton},
    session::{room::RoomAction, Room},
    spawn, spawn_tokio, toast,
    utils::{
        and_expr,
        media::{get_image_info, load_file},
        not_expr, or_expr,
        template_callbacks::TemplateCallbacks,
        OngoingAsyncAction,
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
        pub room_name_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub room_topic_text_view: TemplateChild<gtk::TextView>,
        #[template_child]
        pub room_topic_entry: TemplateChild<CustomEntry>,
        #[template_child]
        pub room_topic_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub edit_details_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub save_details_btn: TemplateChild<SpinnerButton>,
        #[template_child]
        pub members_count: TemplateChild<gtk::Label>,
        /// Whether edit mode is enabled.
        pub edit_mode_enabled: Cell<bool>,
        pub changing_avatar: RefCell<Option<OngoingAsyncAction<OwnedMxcUri>>>,
        pub changing_name: RefCell<Option<OngoingAsyncAction<String>>>,
        pub changing_topic: RefCell<Option<OngoingAsyncAction<String>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GeneralPage {
        const NAME: &'static str = "ContentRoomDetailsGeneralPage";
        type Type = super::GeneralPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
            TemplateCallbacks::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for GeneralPage {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Room>("room")
                        .construct_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("edit-mode-enabled")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "room" => obj.set_room(value.get().unwrap()),
                "edit-mode-enabled" => obj.set_edit_mode_enabled(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "room" => self.room.get().to_value(),
                "edit-mode-enabled" => obj.edit_mode_enabled().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.init_avatar();
            obj.init_edit_mode();

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

#[gtk::template_callbacks]
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
        room.connect_notify_local(
            Some("name"),
            clone!(@weak self as obj => move |room, _| {
                obj.name_changed(room.name());
            }),
        );
        room.connect_notify_local(
            Some("topic"),
            clone!(@weak self as obj => move |room, _| {
                obj.topic_changed(room.topic());
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

    /// Whether edit mode is enabled.
    pub fn edit_mode_enabled(&self) -> bool {
        self.imp().edit_mode_enabled.get()
    }

    pub fn set_edit_mode_enabled(&self, enabled: bool) {
        if self.edit_mode_enabled() == enabled {
            return;
        }

        self.enable_details(enabled);
        self.imp().edit_mode_enabled.set(enabled);
        self.notify("edit-mode-enabled");
    }

    fn enable_details(&self, enabled: bool) {
        let imp = self.imp();

        if enabled {
            imp.room_topic_text_view
                .set_justification(gtk::Justification::Left);
            imp.room_name_entry.set_xalign(0.0);
            imp.room_name_entry.set_halign(gtk::Align::Center);
            imp.room_name_entry.set_sensitive(true);
            imp.room_name_entry.set_width_chars(25);
            imp.room_topic_entry.set_sensitive(true);
            imp.room_topic_label.set_visible(true);
        } else {
            imp.room_topic_text_view
                .set_justification(gtk::Justification::Center);
            imp.room_name_entry.set_xalign(0.5);
            imp.room_name_entry.set_sensitive(false);
            imp.room_name_entry.set_halign(gtk::Align::Fill);
            imp.room_name_entry.set_width_chars(-1);
            imp.room_topic_entry.set_sensitive(false);
            imp.room_topic_label.set_visible(false);
        }
    }

    fn init_edit_mode(&self) {
        let imp = self.imp();

        self.enable_details(false);

        // Hide edit controls when the user is not eligible to perform the actions.
        let room = self.room();
        let room_name_changeable =
            room.new_allowed_expr(RoomAction::StateEvent(StateEventType::RoomName));
        let room_topic_changeable =
            room.new_allowed_expr(RoomAction::StateEvent(StateEventType::RoomTopic));
        let edit_mode_disabled = not_expr(self.property_expression("edit-mode-enabled"));

        let details_changeable = or_expr(room_name_changeable, room_topic_changeable);
        let edit_details_visible = and_expr(edit_mode_disabled, details_changeable);

        edit_details_visible.bind(&*imp.edit_details_btn, "visible", gtk::Widget::NONE);
    }

    /// Finish the details changes if none are ongoing.
    fn finish_details_changes(&self) {
        let imp = self.imp();

        if imp.changing_name.borrow().is_some() {
            return;
        }
        if imp.changing_topic.borrow().is_some() {
            return;
        }

        self.set_edit_mode_enabled(false);
        imp.save_details_btn.set_loading(false);
    }

    fn name_changed(&self, name: Option<String>) {
        let imp = self.imp();

        if let Some(action) = imp.changing_name.borrow().as_ref() {
            if name.as_ref() != action.as_value() {
                // This is not the change we expected, maybe another device did a change too.
                // Let's wait for another change.
                return;
            }
        } else {
            // No action is ongoing, we don't need to do anything.
            return;
        };

        toast!(self, gettext("Room name saved successfully"));

        // Reset state.
        imp.changing_name.take();
        self.finish_details_changes();
    }

    fn topic_changed(&self, topic: Option<String>) {
        let imp = self.imp();

        // It is not possible to remove a topic so we process the empty string as
        // `None`. We need to cancel that here.
        let topic = topic.unwrap_or_default();

        if let Some(action) = imp.changing_topic.borrow().as_ref() {
            if Some(&topic) != action.as_value() {
                // This is not the change we expected, maybe another device did a change too.
                // Let's wait for another change.
                return;
            }
        } else {
            // No action is ongoing, we don't need to do anything.
            return;
        };

        toast!(self, gettext("Room topic saved successfully"));

        // Reset state.
        imp.changing_topic.take();
        self.finish_details_changes();
    }

    #[template_callback]
    fn edit_details_clicked(&self) {
        self.set_edit_mode_enabled(true);
    }

    #[template_callback]
    fn save_details_clicked(&self) {
        self.imp().save_details_btn.set_loading(true);
        self.enable_details(false);

        spawn!(clone!(@weak self as obj => async move {
            obj.save_details().await;
        }));
        self.set_edit_mode_enabled(false);
    }

    async fn save_details(&self) {
        let imp = self.imp();
        let room = self.room();

        let raw_name = imp.room_name_entry.text().to_string();
        let trimmed_name = raw_name.trim();
        let name = if trimmed_name.is_empty() {
            None
        } else {
            Some(trimmed_name.to_owned())
        };

        let topic_buffer = imp.room_topic_text_view.buffer();
        let raw_topic = topic_buffer
            .text(&topic_buffer.start_iter(), &topic_buffer.end_iter(), false)
            .to_string();
        let topic = raw_topic.trim().to_owned();

        let name_changed = name != room.name();
        let topic_changed = topic != room.topic().unwrap_or_default();

        if !name_changed && !topic_changed {
            return;
        }

        let MatrixRoom::Joined(matrix_room) = room.matrix_room() else {
            error!("Cannot change name or topic of room not joined");
            return;
        };

        if name_changed {
            let matrix_room = matrix_room.clone();

            let (action, weak_action) = if let Some(name) = name.clone() {
                OngoingAsyncAction::set(name)
            } else {
                OngoingAsyncAction::remove()
            };
            imp.changing_name.replace(Some(action));

            let handle = spawn_tokio!(async move { matrix_room.set_name(name).await });

            // We don't need to handle the success of the request, we should receive the
            // change via sync.
            if let Err(error) = handle.await.unwrap() {
                // Because this action can finish in name_changed, we must only act if this is
                // still the current action.
                if weak_action.is_ongoing() {
                    imp.changing_name.take();
                    error!("Could not change room name: {error}");
                    toast!(self, gettext("Could not change room name"));
                    self.enable_details(true);
                    imp.save_details_btn.set_loading(false);
                    return;
                }
            }
        }

        if topic_changed {
            let matrix_room = matrix_room.clone();

            let (action, weak_action) = OngoingAsyncAction::set(topic.clone());
            imp.changing_topic.replace(Some(action));

            let handle = spawn_tokio!(async move { matrix_room.set_room_topic(&topic).await });

            // We don't need to handle the success of the request, we should receive the
            // change via sync.
            if let Err(error) = handle.await.unwrap() {
                // Because this action can finish in topic_changed, we must only act if this is
                // still the current action.
                if weak_action.is_ongoing() {
                    imp.changing_topic.take();
                    error!("Could not change room topic: {error}");
                    toast!(self, gettext("Could not change room topic"));
                    self.enable_details(true);
                    imp.save_details_btn.set_loading(false);
                }
            }
        }
    }

    fn member_count_changed(&self, n: u32) {
        self.imp().members_count.set_text(&format!("{n}"));
    }
}

mod invite_subpage;
mod member_page;

use std::convert::From;

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gdk,
    glib::{self, clone, closure},
    CompositeTemplate,
};
use matrix_sdk::ruma::events::RoomEventType;

pub use self::{invite_subpage::InviteSubpage, member_page::MemberPage};
use crate::{
    components::CustomEntry,
    session::{self, room::RoomAction, Room},
    utils::{and_expr, or_expr},
};

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "RoomDetailsPageName")]
pub enum PageName {
    General,
    Members,
    Invite,
}

impl Default for PageName {
    fn default() -> Self {
        Self::General
    }
}

impl glib::variant::StaticVariantType for PageName {
    fn static_variant_type() -> std::borrow::Cow<'static, glib::VariantTy> {
        String::static_variant_type()
    }
}

impl glib::variant::FromVariant for PageName {
    fn from_variant(variant: &glib::variant::Variant) -> Option<Self> {
        match variant.str()? {
            "general" => Some(PageName::General),
            "members" => Some(PageName::Members),
            "invite" => Some(PageName::Invite),
            _ => None,
        }
    }
}

impl glib::variant::ToVariant for PageName {
    fn to_variant(&self) -> glib::variant::Variant {
        match self {
            PageName::General => "general",
            PageName::Members => "members",
            PageName::Invite => "invite",
        }
        .to_variant()
    }
}

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };

    use glib::subclass::InitializingObject;
    use once_cell::unsync::OnceCell;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-room-details.ui")]
    pub struct RoomDetails {
        pub room: OnceCell<Room>,
        pub avatar_chooser: OnceCell<gtk::FileChooserNative>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
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
        pub list_stack_children: RefCell<HashMap<PageName, glib::WeakRef<gtk::Widget>>>,
        pub visible_page: Cell<PageName>,
        pub previous_visible_page: RefCell<Vec<PageName>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoomDetails {
        const NAME: &'static str = "RoomDetails";
        type Type = super::RoomDetails;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            CustomEntry::static_type();
            Self::bind_template(klass);
            klass.install_action("details.choose-avatar", None, move |widget, _, _| {
                widget.open_avatar_chooser()
            });
            klass.install_action("details.remove-avatar", None, move |widget, _, _| {
                widget.room().store_avatar(None)
            });
            klass.install_action("details.next-page", Some("s"), move |widget, _, param| {
                let page = param
                    .and_then(|variant| variant.get::<PageName>())
                    .expect("The parameter need to be a valid PageName");

                widget.next_page(page);
            });
            klass.install_action("details.previous-page", None, move |widget, _, _| {
                widget.previous_page();
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
                vec![
                    glib::ParamSpecObject::new(
                        "room",
                        "Room",
                        "The room backing all details of the preference window",
                        Room::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecEnum::new(
                        "visible-page",
                        "Visible Page",
                        "The page currently visible",
                        PageName::static_type(),
                        PageName::default() as i32,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                ]
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
                "visible-page" => obj.set_visible_page(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room" => self.room.get().to_value(),
                "visible-page" => obj.visible_page().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            obj.init_avatar();
            obj.init_edit_toggle();
            obj.init_avatar_chooser();
            obj.init_member_action();

            self.main_stack
                .connect_visible_child_notify(clone!(@weak obj => move |_| {
                    obj.notify("visible-page");
                }));

            let members = obj.room().members();
            members.connect_items_changed(clone!(@weak obj => move |members, _, _, _| {
                obj.member_count_changed(members.n_items());
            }));

            obj.member_count_changed(members.n_items());
        }
    }

    impl WidgetImpl for RoomDetails {}
    impl WindowImpl for RoomDetails {}
    impl AdwWindowImpl for RoomDetails {}
}

glib::wrapper! {
    /// Preference Window to display and update room details.
    pub struct RoomDetails(ObjectSubclass<imp::RoomDetails>)
        @extends gtk::Widget, gtk::Window, adw::Window, gtk::Root, @implements gtk::Accessible;
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

    pub fn visible_page(&self) -> PageName {
        self.imp().visible_page.get()
    }

    pub fn set_visible_page(&self, name: PageName) {
        let priv_ = self.imp();
        let prev_name = self.visible_page();
        let mut list_stack_children = priv_.list_stack_children.borrow_mut();

        if prev_name == name {
            return;
        }

        match name {
            PageName::General => {
                self.set_title(Some(&gettext("Room Details")));
                priv_.main_stack.set_visible_child_name("general");
            }
            PageName::Members => {
                let members_page = if let Some(members_page) = list_stack_children
                    .get(&PageName::Members)
                    .and_then(glib::object::WeakRef::upgrade)
                {
                    members_page
                } else {
                    let members_page = MemberPage::new(self.room()).upcast::<gtk::Widget>();
                    list_stack_children.insert(PageName::Members, members_page.downgrade());
                    self.imp().main_stack.add_child(&members_page);
                    members_page
                };

                self.set_title(Some(&gettext("Room Members")));
                priv_.main_stack.set_visible_child(&members_page);
            }
            PageName::Invite => {
                priv_.main_stack.set_visible_child_name("general");
                let invite_page = if let Some(invite_page) = list_stack_children
                    .get(&PageName::Invite)
                    .and_then(glib::object::WeakRef::upgrade)
                {
                    invite_page
                } else {
                    let invite_page = InviteSubpage::new(self.room()).upcast::<gtk::Widget>();
                    list_stack_children.insert(PageName::Invite, invite_page.downgrade());
                    priv_.main_stack.add_child(&invite_page);
                    invite_page
                };

                self.set_title(Some(&gettext("Invite new Members")));
                priv_.main_stack.set_visible_child(&invite_page);
            }
        }

        priv_.visible_page.set(name);
        self.notify("visible-page");
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

    fn member_count_changed(&self, n: u32) {
        self.imp().members_count.set_text(&format!("{}", n));
    }

    fn next_page(&self, next_page: PageName) {
        let priv_ = self.imp();
        let prev_page = self.visible_page();

        if prev_page == next_page {
            return;
        }

        priv_
            .main_stack
            .set_transition_type(gtk::StackTransitionType::SlideLeft);

        priv_.previous_visible_page.borrow_mut().push(prev_page);
        self.set_visible_page(next_page);
    }

    fn previous_page(&self) {
        let priv_ = self.imp();

        priv_
            .main_stack
            .set_transition_type(gtk::StackTransitionType::SlideRight);

        if let Some(prev_page) = priv_.previous_visible_page.borrow_mut().pop() {
            self.set_visible_page(prev_page);
        } else {
            // If there isn't any previous page close the dialog since it was opened on a
            // specific page
            self.close();
        };
    }
}

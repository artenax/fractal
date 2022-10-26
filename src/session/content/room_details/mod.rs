mod general_page;
mod invite_subpage;
mod member_page;

use std::convert::From;

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{glib, CompositeTemplate};
use log::warn;

pub use self::{general_page::GeneralPage, invite_subpage::InviteSubpage, member_page::MemberPage};
use crate::session::Room;

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "RoomDetailsPageName")]
pub enum PageName {
    None,
    General,
    Members,
    Invite,
}

impl Default for PageName {
    fn default() -> Self {
        Self::None
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
            "" => Some(PageName::None),
            _ => None,
        }
    }
}

impl glib::variant::ToVariant for PageName {
    fn to_variant(&self) -> glib::variant::Variant {
        match self {
            PageName::None => "",
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
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
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
            Self::bind_template(klass);

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
                    glib::ParamSpecObject::builder::<Room>("room")
                        .construct_only()
                        .build(),
                    glib::ParamSpecEnum::builder("visible-page", PageName::default())
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
                "visible-page" => obj.set_visible_page(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room" => self.room.get().to_value(),
                "visible-page" => self.obj().visible_page().to_value(),
                _ => unimplemented!(),
            }
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
    }

    /// The page that is currently visible.
    pub fn visible_page(&self) -> PageName {
        self.imp().visible_page.get()
    }

    /// Set the page that is currently visible.
    pub fn set_visible_page(&self, name: PageName) {
        let priv_ = self.imp();
        let prev_name = self.visible_page();
        let mut list_stack_children = priv_.list_stack_children.borrow_mut();

        if prev_name == name {
            return;
        }

        match name {
            PageName::General => {
                let general_page = if let Some(general_page) = list_stack_children
                    .get(&PageName::General)
                    .and_then(glib::object::WeakRef::upgrade)
                {
                    general_page
                } else {
                    let general_page = GeneralPage::new(self.room()).upcast::<gtk::Widget>();
                    list_stack_children.insert(PageName::General, general_page.downgrade());
                    self.imp().main_stack.add_child(&general_page);
                    general_page
                };

                self.set_title(Some(&gettext("Room Details")));
                priv_.main_stack.set_visible_child(&general_page);
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
            PageName::None => {
                warn!("Can't switch to PageName::None");
            }
        }

        priv_.visible_page.set(name);
        self.notify("visible-page");
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

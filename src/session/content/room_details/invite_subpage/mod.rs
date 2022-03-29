use adw::subclass::prelude::*;
use gtk::{gdk, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};

mod invitee;
use self::invitee::Invitee;
mod invitee_list;
mod invitee_row;
use self::{
    invitee_list::{InviteeList, InviteeListState},
    invitee_row::InviteeRow,
};
use crate::{
    components::{Pill, SpinnerButton},
    session::{content::RoomDetails, Room, User},
    spawn,
};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-invite-subpage.ui")]
    pub struct InviteSubpage {
        pub room: RefCell<Option<Room>>,
        #[template_child]
        pub list_view: TemplateChild<gtk::ListView>,
        #[template_child]
        pub text_buffer: TemplateChild<gtk::TextBuffer>,
        #[template_child]
        pub invite_button: TemplateChild<SpinnerButton>,
        #[template_child]
        pub cancel_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub text_view: TemplateChild<gtk::TextView>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub matching_page: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub no_matching_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub no_search_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub error_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub loading_page: TemplateChild<gtk::Spinner>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for InviteSubpage {
        const NAME: &'static str = "ContentInviteSubpage";
        type Type = super::InviteSubpage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            InviteeRow::static_type();
            Self::bind_template(klass);

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

    impl ObjectImpl for InviteSubpage {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "room",
                    "Room",
                    "The room users will be invited to",
                    Room::static_type(),
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
                "room" => obj.set_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room" => obj.room().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.cancel_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.close();
                }));

            self.text_buffer.connect_delete_range(clone!(@weak obj => move |_, start, end| {
                let mut current = start.to_owned();
                loop {
                    if let Some(anchor) = current.child_anchor() {
                        let user = anchor.widgets()[0].downcast_ref::<Pill>().unwrap().user().unwrap().downcast::<Invitee>().unwrap();
                        user.take_anchor();
                        user.set_invited(false);
                    }

                    current.forward_char();

                    if &current == end {
                        break;
                    }
                }
            }));

            self.text_buffer.connect_insert_text(
                clone!(@weak obj => move |text_buffer, location, text| {
                    let mut changed = false;

                    // We don't allow adding chars before and between pills
                    loop {
                        if location.child_anchor().is_some() {
                            changed = true;
                            if !location.forward_char() {
                                break;
                            }
                        } else {
                            break;
                        }
                    }

                    if changed {
                        text_buffer.place_cursor(location);
                        text_buffer.stop_signal_emission_by_name("insert-text");
                        text_buffer.insert(location, text);
                    }
                }),
            );

            self.invite_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.invite();
                }));

            self.list_view.connect_activate(|list_view, index| {
                let invitee = list_view
                    .model()
                    .unwrap()
                    .item(index)
                    .unwrap()
                    .downcast::<Invitee>()
                    .unwrap();

                invitee.set_invited(!invitee.is_invited());
            });
        }
    }

    impl WidgetImpl for InviteSubpage {}
    impl BinImpl for InviteSubpage {}
}

glib::wrapper! {
    /// Preference Window to display and update room details.
    pub struct InviteSubpage(ObjectSubclass<imp::InviteSubpage>)
        @extends gtk::Widget, gtk::Window, adw::Window, adw::Bin, @implements gtk::Accessible;
}

impl InviteSubpage {
    pub fn new(room: &Room) -> Self {
        glib::Object::new(&[("room", room)]).expect("Failed to create InviteSubpage")
    }

    pub fn room(&self) -> Option<Room> {
        self.imp().room.borrow().clone()
    }

    fn set_room(&self, room: Option<Room>) {
        let priv_ = self.imp();

        if self.room() == room {
            return;
        }

        if let Some(ref room) = room {
            let user_list = InviteeList::new(room);
            user_list.connect_invitee_added(clone!(@weak self as obj => move |_, invitee| {
                obj.add_user_pill(invitee);
            }));

            user_list.connect_invitee_removed(clone!(@weak self as obj => move |_, invitee| {
                obj.remove_user_pill(invitee);
            }));

            user_list.connect_notify_local(
                Some("state"),
                clone!(@weak self as obj => move |_, _| {
                    obj.update_view();
                }),
            );

            priv_
                .text_buffer
                .bind_property("text", &user_list, "search-term")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build();

            user_list
                .bind_property("has-selected", &*priv_.invite_button, "sensitive")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build();

            priv_
                .list_view
                .set_model(Some(&gtk::NoSelection::new(Some(&user_list))));
        } else {
            priv_.list_view.set_model(gtk::SelectionModel::NONE);
        }

        priv_.room.replace(room);
        self.notify("room");
    }

    fn close(&self) {
        let window = self.root().unwrap().downcast::<RoomDetails>().unwrap();
        window.close_invite_subpage();
    }

    fn add_user_pill(&self, user: &Invitee) {
        let priv_ = self.imp();

        let pill = Pill::for_user(user.upcast_ref());
        pill.set_margin_start(3);
        pill.set_margin_end(3);

        let (mut start_iter, mut end_iter) = priv_.text_buffer.bounds();

        // We don't allow adding chars before and between pills
        loop {
            if start_iter.child_anchor().is_some() {
                start_iter.forward_char();
            } else {
                break;
            }
        }

        priv_.text_buffer.delete(&mut start_iter, &mut end_iter);
        let anchor = priv_.text_buffer.create_child_anchor(&mut start_iter);
        priv_.text_view.add_child_at_anchor(&pill, &anchor);
        user.set_anchor(Some(anchor));

        priv_.text_view.grab_focus();
    }

    fn remove_user_pill(&self, user: &Invitee) {
        if let Some(anchor) = user.take_anchor() {
            if !anchor.is_deleted() {
                let text_buffer = &self.imp().text_buffer;
                let mut start_iter = text_buffer.iter_at_child_anchor(&anchor);
                let mut end_iter = start_iter;
                end_iter.forward_char();
                text_buffer.delete(&mut start_iter, &mut end_iter);
            }
        }
    }

    fn invitee_list(&self) -> Option<InviteeList> {
        self.imp()
            .list_view
            .model()?
            .downcast::<gtk::NoSelection>()
            .unwrap()
            .model()
            .unwrap()
            .downcast::<InviteeList>()
            .ok()
    }

    fn invite(&self) {
        self.imp().invite_button.set_loading(true);
        if let Some(room) = self.room() {
            if let Some(user_list) = self.invitee_list() {
                let invitees: Vec<User> = user_list
                    .invitees()
                    .into_iter()
                    .map(glib::object::Cast::upcast)
                    .collect();
                spawn!(clone!(@weak self as obj => async move {
                    room.invite(invitees.as_slice()).await;
                    obj.close();
                    obj.imp().invite_button.set_loading(false);
                }));
            }
        }
    }

    fn update_view(&self) {
        let priv_ = self.imp();
        match self
            .invitee_list()
            .expect("Can't update view without an InviteeList")
            .state()
        {
            InviteeListState::Initial => priv_.stack.set_visible_child(&*priv_.no_search_page),
            InviteeListState::Loading => priv_.stack.set_visible_child(&*priv_.loading_page),
            InviteeListState::NoMatching => priv_.stack.set_visible_child(&*priv_.no_matching_page),
            InviteeListState::Matching => priv_.stack.set_visible_child(&*priv_.matching_page),
            InviteeListState::Error => priv_.stack.set_visible_child(&*priv_.error_page),
        }
    }
}

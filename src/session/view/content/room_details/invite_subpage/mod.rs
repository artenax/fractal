use adw::subclass::prelude::*;
use gettextrs::ngettext;
use gtk::{gdk, glib, glib::clone, prelude::*, CompositeTemplate};

mod invitee;
use self::invitee::Invitee;
mod invitee_list;
mod invitee_row;
use self::{
    invitee_list::{InviteeList, InviteeListState},
    invitee_row::InviteeRow,
};
use crate::{
    components::{Pill, Spinner, SpinnerButton},
    prelude::*,
    session::model::{Room, User},
    spawn, toast,
};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_details/invite_subpage/mod.ui"
    )]
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
        pub loading_page: TemplateChild<Spinner>,
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
                vec![glib::ParamSpecObject::builder::<Room>("room")
                    .explicit_notify()
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
                "room" => self.obj().room().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            self.cancel_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.close();
                }));

            self.text_buffer.connect_delete_range(|_, start, end| {
                let mut current = start.to_owned();
                loop {
                    if let Some(anchor) = current.child_anchor() {
                        let user = anchor.widgets()[0]
                            .downcast_ref::<Pill>()
                            .unwrap()
                            .user()
                            .and_downcast::<Invitee>()
                            .unwrap();
                        user.take_anchor();
                        user.set_invited(false);
                    }

                    current.forward_char();

                    if &current == end {
                        break;
                    }
                }
            });

            self.text_buffer
                .connect_insert_text(|text_buffer, location, text| {
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
                });

            self.invite_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.invite();
                }));

            self.list_view.connect_activate(|list_view, index| {
                let invitee = list_view
                    .model()
                    .unwrap()
                    .item(index)
                    .and_downcast::<Invitee>()
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
        glib::Object::builder().property("room", room).build()
    }

    /// The room users will be invited to.
    pub fn room(&self) -> Option<Room> {
        self.imp().room.borrow().clone()
    }

    /// Set the room users will be invited to.
    fn set_room(&self, room: Option<Room>) {
        let imp = self.imp();

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

            imp.text_buffer
                .bind_property("text", &user_list, "search-term")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build();

            user_list
                .bind_property("has-selected", &*imp.invite_button, "sensitive")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build();

            imp.list_view
                .set_model(Some(&gtk::NoSelection::new(Some(user_list))));
        } else {
            imp.list_view.set_model(gtk::SelectionModel::NONE);
        }

        imp.room.replace(room);
        self.notify("room");
    }

    fn close(&self) {
        self.activate_action("details.previous-page", None).unwrap();
    }

    fn add_user_pill(&self, user: &Invitee) {
        let imp = self.imp();

        let pill = Pill::for_user(user.upcast_ref());
        pill.set_margin_start(3);
        pill.set_margin_end(3);

        let (mut start_iter, mut end_iter) = imp.text_buffer.bounds();

        // We don't allow adding chars before and between pills
        loop {
            if start_iter.child_anchor().is_some() {
                start_iter.forward_char();
            } else {
                break;
            }
        }

        imp.text_buffer.delete(&mut start_iter, &mut end_iter);
        let anchor = imp.text_buffer.create_child_anchor(&mut start_iter);
        imp.text_view.add_child_at_anchor(&pill, &anchor);
        user.set_anchor(Some(anchor));

        imp.text_view.grab_focus();
    }

    fn remove_user_pill(&self, user: &Invitee) {
        let Some(anchor) = user.take_anchor() else {
            return;
        };

        if !anchor.is_deleted() {
            let text_buffer = &self.imp().text_buffer;
            let mut start_iter = text_buffer.iter_at_child_anchor(&anchor);
            let mut end_iter = start_iter;
            end_iter.forward_char();
            text_buffer.delete(&mut start_iter, &mut end_iter);
        }
    }

    fn invitee_list(&self) -> Option<InviteeList> {
        self.imp()
            .list_view
            .model()
            .and_downcast::<gtk::NoSelection>()?
            .model()
            .and_downcast::<InviteeList>()
    }

    /// Invite the selected users to the room.
    fn invite(&self) {
        self.imp().invite_button.set_loading(true);

        spawn!(clone!(@weak self as obj => async move {
            obj.invite_inner().await;
        }));
    }

    async fn invite_inner(&self) {
        let Some(room) = self.room() else {
            return;
        };
        let Some(user_list) = self.invitee_list() else {
            return;
        };

        let invitees: Vec<User> = user_list
            .invitees()
            .into_iter()
            .map(glib::object::Cast::upcast)
            .collect();

        match room.invite(&invitees).await {
            Ok(()) => {
                self.close();
            }
            Err(failed_users) => {
                for invitee in &invitees {
                    if !failed_users.contains(&invitee) {
                        user_list.remove_invitee(&invitee.user_id())
                    }
                }

                let n = failed_users.len();
                let first_failed = failed_users.first().unwrap();

                toast!(
                    self,
                    ngettext(
                        // Translators: Do NOT translate the content between '{' and '}', these
                        // are variable names.
                        "Failed to invite {user} to {room}. Try again later.",
                        "Failed to invite {n} users to {room}. Try again later.",
                        n as u32,
                    ),
                    @user = first_failed,
                    @room,
                    n = n.to_string(),
                );
            }
        }

        self.imp().invite_button.set_loading(false);
    }

    fn update_view(&self) {
        let imp = self.imp();
        match self
            .invitee_list()
            .expect("Can't update view without an InviteeList")
            .state()
        {
            InviteeListState::Initial => imp.stack.set_visible_child(&*imp.no_search_page),
            InviteeListState::Loading => imp.stack.set_visible_child(&*imp.loading_page),
            InviteeListState::NoMatching => imp.stack.set_visible_child(&*imp.no_matching_page),
            InviteeListState::Matching => imp.stack.set_visible_child(&*imp.matching_page),
            InviteeListState::Error => imp.stack.set_visible_child(&*imp.error_page),
        }
    }
}

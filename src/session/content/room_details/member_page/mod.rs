use adw::{
    prelude::*,
    subclass::{bin::BinImpl, prelude::*},
};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone, closure},
    CompositeTemplate,
};
use log::warn;

mod member_menu;
mod members_list_view;

use members_list_view::{MembersListView, MembershipSubpageItem};
use ruma::events::room::power_levels::PowerLevelAction;

use self::member_menu::MemberMenu;
use crate::{
    prelude::*,
    session::{
        content::room_details::member_page::members_list_view::extra_lists::ExtraLists,
        room::{Member, Membership, PowerLevel},
        Room, User, UserActions,
    },
    spawn,
};

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };

    use glib::subclass::InitializingObject;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-member-page.ui")]
    pub struct MemberPage {
        pub room: RefCell<Option<Room>>,
        #[template_child]
        pub members_search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub list_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub invite_button: TemplateChild<gtk::Button>,
        pub member_menu: OnceCell<MemberMenu>,
        pub list_stack_children: RefCell<HashMap<Membership, glib::WeakRef<MembersListView>>>,
        pub state: Cell<Membership>,
        pub invite_action_watch: RefCell<Option<gtk::ExpressionWatch>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MemberPage {
        const NAME: &'static str = "ContentMemberPage";
        type Type = super::MemberPage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("member.verify", None, move |widget, _, _| {
                if let Some(member) = widget.member_menu().member() {
                    widget.verify_member(member);
                } else {
                    warn!("No member was selected to be verified");
                }
            });

            klass.install_action("members.subpage", Some("u"), move |widget, _, param| {
                let state = param.and_then(|variant| variant.get::<Membership>());

                if let Some(state) = state {
                    widget.set_state(state);
                }
            });

            klass.install_action("members.previous", None, move |widget, _, _| {
                if widget.state() == Membership::Join {
                    widget
                        .activate_action("details.previous-page", None)
                        .unwrap();
                } else {
                    widget.set_state(Membership::Join);
                }
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MemberPage {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Room>("room")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<MemberMenu>("member-menu")
                        .read_only()
                        .build(),
                    glib::ParamSpecEnum::builder::<Membership>("state")
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
                "state" => obj.set_state(value.get().unwrap()),

                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "room" => obj.room().to_value(),
                "member-menu" => obj.member_menu().to_value(),
                "state" => obj.state().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            if let Some(invite_action) = self.invite_action_watch.take() {
                invite_action.unwatch();
            }
        }
    }

    impl WidgetImpl for MemberPage {}
    impl BinImpl for MemberPage {}
}

glib::wrapper! {
    pub struct MemberPage(ObjectSubclass<imp::MemberPage>)
        @extends gtk::Widget, adw::Bin;
}

impl MemberPage {
    pub fn new(room: &Room) -> Self {
        glib::Object::builder().property("room", room).build()
    }

    /// The room backing all the details of the member page.
    pub fn room(&self) -> Option<Room> {
        self.imp().room.borrow().as_ref().cloned()
    }

    /// Set the room backing all the details of the member page.
    pub fn set_room(&self, room: Option<Room>) {
        let imp = self.imp();
        let prev_room = self.room();

        if prev_room == room {
            return;
        }

        if let Some(invite_action) = imp.invite_action_watch.take() {
            invite_action.unwatch();
        }

        if let Some(room) = room.as_ref() {
            self.init_members_list(room);
            self.init_invite_button(room);
            self.set_state(Membership::Join);
        }

        imp.room.replace(room);
        self.notify("room");
    }

    fn init_members_list(&self, room: &Room) {
        let imp = self.imp();

        // Sort the members list by power level, then display name.
        let sorter = gtk::MultiSorter::new();
        sorter.append(
            gtk::NumericSorter::builder()
                .expression(Member::this_expression("power-level"))
                .sort_order(gtk::SortType::Descending)
                .build(),
        );

        sorter.append(gtk::StringSorter::new(Some(Member::this_expression(
            "display-name",
        ))));

        let members = gtk::SortListModel::new(Some(room.members().clone()), Some(sorter));

        let joined_members = self.build_filtered_list(members.clone(), Membership::Join);
        let invited_members = self.build_filtered_list(members.clone(), Membership::Invite);
        let banned_members = self.build_filtered_list(members, Membership::Ban);

        let extra_list = ExtraLists::new(
            &MembershipSubpageItem::new(Membership::Invite, &invited_members),
            &MembershipSubpageItem::new(Membership::Ban, &banned_members),
        );
        let model_list = gio::ListStore::builder()
            .item_type(gio::ListModel::static_type())
            .build();
        model_list.append(&extra_list);
        model_list.append(&joined_members);

        let main_list = gtk::FlattenListModel::new(Some(model_list));

        let mut list_stack_children = imp.list_stack_children.borrow_mut();
        let joined_view = MembersListView::new(&main_list);
        imp.list_stack.add_child(&joined_view);
        list_stack_children.insert(Membership::Join, joined_view.downgrade());
        let invited_view = MembersListView::new(&invited_members);
        imp.list_stack.add_child(&invited_view);
        list_stack_children.insert(Membership::Invite, invited_view.downgrade());
        let banned_view = MembersListView::new(&banned_members);
        imp.list_stack.add_child(&banned_view);
        list_stack_children.insert(Membership::Ban, banned_view.downgrade());
    }

    /// The object holding information needed for the menu of each `MemberRow`.
    pub fn member_menu(&self) -> &MemberMenu {
        self.imp().member_menu.get_or_init(|| {
            let menu = MemberMenu::new();

            menu.connect_notify_local(
                Some("allowed-actions"),
                clone!(@weak self as obj => move |menu, _| {
                    obj.update_actions(menu.allowed_actions());
                }),
            );
            self.update_actions(menu.allowed_actions());
            menu
        })
    }

    fn update_actions(&self, allowed_actions: UserActions) {
        self.action_set_enabled(
            "member.verify",
            allowed_actions.contains(UserActions::VERIFY),
        );
    }

    fn verify_member(&self, member: Member) {
        // TODO: show the verification immediately when started
        spawn!(clone!(@weak self as obj => async move {
            member.upcast::<User>().verify_identity().await;
        }));
    }

    /// The membership state of the displayed members.
    pub fn state(&self) -> Membership {
        self.imp().state.get()
    }

    /// Set the membership state of the displayed members.
    pub fn set_state(&self, state: Membership) {
        let imp = self.imp();

        if self.state() == state {
            return;
        }

        if state == Membership::Join {
            imp.list_stack
                .set_transition_type(gtk::StackTransitionType::SlideRight)
        } else {
            imp.list_stack
                .set_transition_type(gtk::StackTransitionType::SlideLeft)
        }

        if let Some(window) = self.root().and_then(|w| w.downcast::<adw::Window>().ok()) {
            match state {
                Membership::Invite => window.set_title(Some(&gettext("Invited Room Members"))),
                Membership::Ban => window.set_title(Some(&gettext("Banned Room Members"))),
                _ => window.set_title(Some(&gettext("Room Members"))),
            }
        }

        if let Some(view) = imp
            .list_stack_children
            .borrow()
            .get(&state)
            .and_then(glib::WeakRef::upgrade)
        {
            imp.list_stack.set_visible_child(&view);
        }

        self.imp().state.set(state);
        self.notify("state");
    }

    fn build_filtered_list(
        &self,
        model: impl IsA<gio::ListModel>,
        state: Membership,
    ) -> gio::ListModel {
        let membership_expression = Member::this_expression("membership").chain_closure::<bool>(
            closure!(|_: Option<glib::Object>, this_state: Membership| this_state == state),
        );

        let membership_filter = gtk::BoolFilter::new(Some(&membership_expression));

        fn search_string(member: Member) -> String {
            format!(
                "{} {} {} {}",
                member.display_name(),
                member.user_id(),
                member.role(),
                member.power_level(),
            )
        }

        let member_expr = gtk::ClosureExpression::new::<String>(
            [
                Member::this_expression("display-name"),
                Member::this_expression("power-level"),
            ],
            closure!(
                |member: Option<Member>, _display_name: String, _power_level: PowerLevel| {
                    member.map(search_string).unwrap_or_default()
                }
            ),
        );
        let search_filter = gtk::StringFilter::builder()
            .match_mode(gtk::StringFilterMatchMode::Substring)
            .expression(&member_expr)
            .ignore_case(true)
            .build();

        self.imp()
            .members_search_entry
            .bind_property("text", &search_filter, "search")
            .flags(glib::BindingFlags::SYNC_CREATE)
            .build();

        let filter = gtk::EveryFilter::new();

        filter.append(membership_filter);
        filter.append(search_filter);

        let filter_model = gtk::FilterListModel::new(Some(model), Some(filter));
        filter_model.upcast()
    }

    fn init_invite_button(&self, room: &Room) {
        let invite_possible = room.own_user_is_allowed_to_expr(PowerLevelAction::Invite);

        let watch = invite_possible.watch(
            glib::Object::NONE,
            clone!(@weak self as obj => move || {
                obj.update_invite_button();
            }),
        );

        self.imp().invite_action_watch.replace(Some(watch));
        self.update_invite_button();
    }

    fn update_invite_button(&self) {
        if let Some(invite_action) = &*self.imp().invite_action_watch.borrow() {
            let allow_invite = invite_action
                .evaluate_as::<bool>()
                .expect("Created expression needs to be valid and a boolean");
            self.imp().invite_button.set_visible(allow_invite);
        };
    }
}

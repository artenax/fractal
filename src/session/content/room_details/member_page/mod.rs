use adw::{prelude::*, subclass::prelude::*};
use gtk::{
    glib::{self, clone, closure},
    CompositeTemplate,
};
use log::warn;

mod member_menu;
mod member_row;

use self::{member_menu::MemberMenu, member_row::MemberRow};
use crate::{
    components::{Avatar, Badge},
    ngettext_f,
    prelude::*,
    session::{
        content::RoomDetails,
        room::{Member, Membership, RoomAction},
        Room, User, UserActions,
    },
    spawn,
};

const MAX_LIST_HEIGHT: i32 = 300;

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-member-page.ui")]
    pub struct MemberPage {
        pub room: OnceCell<Room>,
        #[template_child]
        pub member_count: TemplateChild<gtk::Label>,
        #[template_child]
        pub invite_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub members_search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub members_list_view: TemplateChild<gtk::ListView>,
        #[template_child]
        pub members_scroll: TemplateChild<gtk::ScrolledWindow>,
        pub member_menu: OnceCell<MemberMenu>,
        #[template_child]
        pub invited_section: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub invited_list_view: TemplateChild<gtk::ListView>,
        #[template_child]
        pub invited_scroll: TemplateChild<gtk::ScrolledWindow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MemberPage {
        const NAME: &'static str = "ContentMemberPage";
        type Type = super::MemberPage;
        type ParentType = adw::PreferencesPage;

        fn class_init(klass: &mut Self::Class) {
            Avatar::static_type();
            Badge::static_type();
            MemberRow::static_type();
            Self::bind_template(klass);

            klass.install_action("member.verify", None, move |widget, _, _| {
                if let Some(member) = widget.member_menu().member() {
                    widget.verify_member(member);
                } else {
                    warn!("No member was selected to be verified");
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
                    glib::ParamSpecObject::new(
                        "room",
                        "Room",
                        "The room backing all details of the member page",
                        Room::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecObject::new(
                        "member-menu",
                        "Member Menu",
                        "The object holding information needed for the menu of each MemberRow",
                        MemberMenu::static_type(),
                        glib::ParamFlags::READABLE,
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
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room" => self.room.get().to_value(),
                "member-menu" => obj.member_menu().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            obj.init_members_list();
            obj.init_invited_list();
            obj.init_invite_button();
        }
    }
    impl WidgetImpl for MemberPage {}
    impl PreferencesPageImpl for MemberPage {}
}

glib::wrapper! {
    pub struct MemberPage(ObjectSubclass<imp::MemberPage>)
        @extends gtk::Widget, adw::PreferencesPage;
}

impl MemberPage {
    pub fn new(room: &Room) -> Self {
        glib::Object::new(&[("room", room)]).expect("Failed to create MemberPage")
    }

    pub fn room(&self) -> &Room {
        self.imp().room.get().unwrap()
    }

    fn set_room(&self, room: Room) {
        self.imp().room.set(room).expect("Room already initialized");
    }

    fn init_members_list(&self) {
        let priv_ = self.imp();
        let members = self.room().members();

        // Only keep the members that are in the join membership state
        let joined_expression = gtk::PropertyExpression::new(
            Member::static_type(),
            gtk::Expression::NONE,
            "membership",
        )
        .chain_closure::<bool>(closure!(
            |_: Option<glib::Object>, membership: Membership| { membership == Membership::Join }
        ));
        let joined_filter = gtk::BoolFilter::new(Some(joined_expression));
        let joined_members = gtk::FilterListModel::new(Some(members), Some(&joined_filter));

        // Set up the members count.
        self.member_count_changed(joined_members.n_items());
        joined_members.connect_items_changed(clone!(@weak self as obj => move |members, _, _, _| {
            obj.member_count_changed(members.n_items());
        }));

        // Sort the members list by power level, then display name.
        let sorter = gtk::MultiSorter::new();
        sorter.append(
            &gtk::NumericSorter::builder()
                .expression(&gtk::PropertyExpression::new(
                    Member::static_type(),
                    gtk::Expression::NONE,
                    "power-level",
                ))
                .sort_order(gtk::SortType::Descending)
                .build(),
        );
        sorter.append(&gtk::StringSorter::new(Some(
            &gtk::PropertyExpression::new(
                Member::static_type(),
                gtk::Expression::NONE,
                "display-name",
            ),
        )));
        let sorted_members = gtk::SortListModel::new(Some(&joined_members), Some(&sorter));

        fn search_string(member: Member) -> String {
            format!(
                "{} {} {} {}",
                member.display_name(),
                member.user_id(),
                member.role(),
                member.power_level(),
            )
        }

        let member_expr = gtk::ClosureExpression::new::<String, &[gtk::Expression], _>(
            &[],
            closure!(|member: Option<Member>| { member.map(search_string).unwrap_or_default() }),
        );
        let filter = gtk::StringFilter::builder()
            .match_mode(gtk::StringFilterMatchMode::Substring)
            .expression(&member_expr)
            .ignore_case(true)
            .build();
        priv_
            .members_search_entry
            .bind_property("text", &filter, "search")
            .flags(glib::BindingFlags::SYNC_CREATE)
            .build();

        let filter_model = gtk::FilterListModel::new(Some(&sorted_members), Some(&filter));
        let model = gtk::NoSelection::new(Some(&filter_model));
        priv_.members_list_view.set_model(Some(&model));
    }

    fn member_count_changed(&self, n: u32) {
        let priv_ = self.imp();
        priv_
            .member_count
            // Translators: Do NOT translate the content between '{' and '}', this is a variable
            // name.
            .set_text(&ngettext_f(
                "1 Member",
                "{n} Members",
                n,
                &[("n", &n.to_string())],
            ));
        // FIXME: This won't be needed when we can request the natural height
        // on AdwPreferencesPage
        // See: https://gitlab.gnome.org/GNOME/libadwaita/-/issues/77
        if n > 5 {
            priv_.members_scroll.set_min_content_height(MAX_LIST_HEIGHT);
        } else {
            priv_.members_scroll.set_min_content_height(-1);
        }
    }

    fn init_invited_list(&self) {
        let priv_ = self.imp();
        let members = self.room().members();

        // Only keep the members that are in the join membership state
        let invited_expression = gtk::PropertyExpression::new(
            Member::static_type(),
            gtk::Expression::NONE,
            "membership",
        )
        .chain_closure::<bool>(closure!(
            |_: Option<glib::Object>, membership: Membership| { membership == Membership::Invite }
        ));
        let invited_filter = gtk::BoolFilter::new(Some(invited_expression));
        let invited_members = gtk::FilterListModel::new(Some(members), Some(&invited_filter));

        // Set up the invited section visibility and the invited count.
        self.invited_count_changed(invited_members.n_items());
        invited_members.connect_items_changed(
            clone!(@weak self as obj => move |members, _, _, _| {
                obj.invited_count_changed(members.n_items());
            }),
        );

        // Sort the invited list by display name.
        let sorter = gtk::StringSorter::new(Some(&gtk::PropertyExpression::new(
            Member::static_type(),
            gtk::Expression::NONE,
            "display-name",
        )));
        let sorted_invited = gtk::SortListModel::new(Some(&invited_members), Some(&sorter));

        let model = gtk::NoSelection::new(Some(&sorted_invited));
        priv_.invited_list_view.set_model(Some(&model));
    }

    fn invited_count_changed(&self, n: u32) {
        let priv_ = self.imp();
        priv_.invited_section.set_visible(n > 0);
        priv_
            .invited_section
            // Translators: Do NOT translate the content between '{' and '}', this is a variable
            // name.
            .set_title(&ngettext_f(
                "1 Invited",
                "{n} Invited",
                n,
                &[("n", &n.to_string())],
            ));
        // FIXME: This won't be needed when we can request the natural height
        // on AdwPreferencesPage
        // See: https://gitlab.gnome.org/GNOME/libadwaita/-/issues/77
        if n > 5 {
            priv_.invited_scroll.set_min_content_height(MAX_LIST_HEIGHT);
        } else {
            priv_.invited_scroll.set_min_content_height(-1);
        }
    }

    fn init_invite_button(&self) {
        let invite_button = &*self.imp().invite_button;

        let invite_possible = self.room().new_allowed_expr(RoomAction::Invite);
        const NONE_OBJECT: Option<&glib::Object> = None;
        invite_possible.bind(invite_button, "sensitive", NONE_OBJECT);

        invite_button.connect_clicked(clone!(@weak self as obj => move |_| {
            let window = obj
            .root()
            .unwrap()
            .downcast::<RoomDetails>()
            .unwrap();
            window.present_invite_subpage();
        }));
    }

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
}

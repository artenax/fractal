mod category_row;
mod entry_row;
mod room_row;
mod row;
mod verification_row;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib, glib::clone, CompositeTemplate};
use tracing::error;

use self::{
    category_row::CategoryRow, entry_row::EntryRow, room_row::RoomRow, row::Row,
    verification_row::VerificationRow,
};
use crate::{
    components::Avatar,
    prelude::*,
    session::model::{
        Category, CategoryType, Entry, IdentityVerification, Room, RoomType, Selection,
        SidebarListModel, User,
    },
    Window,
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{signal::SignalHandlerId, subclass::InitializingObject};
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/session/view/sidebar/mod.ui")]
    pub struct Sidebar {
        #[template_child]
        pub scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub listview: TemplateChild<gtk::ListView>,
        #[template_child]
        pub room_search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub room_search: TemplateChild<gtk::SearchBar>,
        #[template_child]
        pub account_switcher_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub room_row_menu: TemplateChild<gio::MenuModel>,
        #[template_child]
        pub offline_banner: TemplateChild<adw::Banner>,
        pub room_row_popover: OnceCell<gtk::PopoverMenu>,
        pub user: RefCell<Option<User>>,
        /// The type of the source that activated drop mode.
        pub drop_source_type: Cell<Option<RoomType>>,
        /// The type of the drop target that is currently hovered.
        pub drop_active_target_type: Cell<Option<RoomType>>,
        /// The list model of this sidebar.
        pub list_model: glib::WeakRef<SidebarListModel>,
        pub bindings: RefCell<Vec<glib::Binding>>,
        pub offline_handler_id: RefCell<Option<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Sidebar {
        const NAME: &'static str = "Sidebar";
        type Type = super::Sidebar;
        type ParentType = adw::NavigationPage;

        fn class_init(klass: &mut Self::Class) {
            RoomRow::static_type();
            Row::static_type();
            Avatar::static_type();
            Self::bind_template(klass);
            klass.set_css_name("sidebar");
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Sidebar {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<User>("user")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<SidebarListModel>("list-model")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecEnum::builder::<CategoryType>("drop-source-type")
                        .read_only()
                        .build(),
                    glib::ParamSpecEnum::builder::<CategoryType>("drop-active-target-type")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "user" => obj.set_user(value.get().unwrap()),
                "list-model" => obj.set_list_model(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "user" => obj.user().to_value(),
                "list-model" => obj.list_model().to_value(),
                "drop-source-type" => obj
                    .drop_source_type()
                    .map(CategoryType::from)
                    .unwrap_or_default()
                    .to_value(),
                "drop-active-target-type" => obj
                    .drop_active_target_type()
                    .map(CategoryType::from)
                    .unwrap_or_default()
                    .to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let factory = gtk::SignalListItemFactory::new();
            factory.connect_setup(clone!(@weak obj => move |_, item| {
                let item = match item.downcast_ref::<gtk::ListItem>() {
                    Some(item) => item,
                    None => {
                        error!("List item factory did not receive a list item: {item:?}");
                        return;
                    }
                };
                let row = Row::new(&obj);
                item.set_child(Some(&row));
                item.bind_property("item", &row, "list-row").build();
            }));
            self.listview.set_factory(Some(&factory));

            self.listview.connect_activate(move |listview, pos| {
                let model: Option<Selection> = listview.model().and_downcast();
                let row: Option<gtk::TreeListRow> =
                    model.as_ref().and_then(|m| m.item(pos)).and_downcast();

                let (model, row) = match (model, row) {
                    (Some(model), Some(row)) => (model, row),
                    _ => return,
                };

                match row.item() {
                    Some(o) if o.is::<Category>() => row.set_expanded(!row.is_expanded()),
                    Some(o) if o.is::<Room>() => model.set_selected(pos),
                    Some(o) if o.is::<Entry>() => model.set_selected(pos),
                    Some(o) if o.is::<IdentityVerification>() => model.set_selected(pos),
                    _ => {}
                }
            });

            self.account_switcher_button.set_create_popup_func(clone!(@weak obj => move |btn| {
                if let Some(window) = obj.parent_window() {
                    let account_switcher = window.account_switcher();
                    // We need to remove the popover from the previous MenuButton, if any
                    if let Some(prev_parent) = account_switcher.parent().and_downcast::<gtk::MenuButton>() {
                        if &prev_parent == btn {
                            return;
                        } else {
                            prev_parent.set_popover(gtk::Widget::NONE);
                        }
                    }
                    btn.set_popover(Some(account_switcher));
                }
            }));

            // FIXME: Remove this hack once https://gitlab.gnome.org/GNOME/gtk/-/issues/4938 is resolved
            self.scrolled_window
                .vscrollbar()
                .first_child()
                .unwrap()
                .set_overflow(gtk::Overflow::Hidden);
        }
    }

    impl WidgetImpl for Sidebar {
        fn focus(&self, direction_type: gtk::DirectionType) -> bool {
            // WORKAROUND: This works around the tab behavior `gtk::ListViews have`
            // See: https://gitlab.gnome.org/GNOME/gtk/-/issues/4840
            let focus_child = self
                .obj()
                .focus_child()
                .and_then(|w| w.focus_child())
                .and_then(|w| w.focus_child());
            if focus_child.map_or(false, |w| w.is::<gtk::ListView>())
                && matches!(
                    direction_type,
                    gtk::DirectionType::TabForward | gtk::DirectionType::TabBackward
                )
            {
                false
            } else {
                self.parent_focus(direction_type)
            }
        }
    }

    impl NavigationPageImpl for Sidebar {}
}

glib::wrapper! {
    pub struct Sidebar(ObjectSubclass<imp::Sidebar>)
        @extends gtk::Widget, adw::NavigationPage, @implements gtk::Accessible;
}

impl Sidebar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn room_search_bar(&self) -> gtk::SearchBar {
        self.imp().room_search.clone()
    }

    /// The list model of this sidebar.
    pub fn list_model(&self) -> Option<SidebarListModel> {
        self.imp().list_model.upgrade()
    }

    /// Set the list model of the sidebar.
    pub fn set_list_model(&self, list_model: Option<SidebarListModel>) {
        if self.list_model() == list_model {
            return;
        }

        let imp = self.imp();

        for binding in imp.bindings.take() {
            binding.unbind();
        }

        if let Some(list_model) = &list_model {
            let bindings = vec![
                self.bind_property(
                    "drop-source-type",
                    list_model.item_list(),
                    "show-all-for-category",
                )
                .sync_create()
                .build(),
                list_model
                    .string_filter()
                    .bind_property("search", &*imp.room_search_entry, "text")
                    .sync_create()
                    .bidirectional()
                    .build(),
            ];

            imp.bindings.replace(bindings);
        }

        imp.listview
            .set_model(list_model.as_ref().map(|m| m.selection_model()));
        imp.list_model.set(list_model.as_ref());
        self.notify("list-model");
    }

    /// The logged-in user.
    pub fn user(&self) -> Option<User> {
        self.imp().user.borrow().clone()
    }

    /// Set the logged-in user.
    fn set_user(&self, user: Option<User>) {
        let prev_user = self.user();
        if prev_user == user {
            return;
        }

        if let Some(prev_user) = prev_user {
            if let Some(handler_id) = self.imp().offline_handler_id.take() {
                prev_user.session().disconnect(handler_id);
            }
        }

        if let Some(user) = user.as_ref() {
            let session = user.session();
            let handler_id = session.connect_notify_local(
                Some("offline"),
                clone!(@weak self as obj => move |session, _| {
                    obj.imp().offline_banner.set_revealed(session.is_offline());
                }),
            );
            self.imp().offline_banner.set_revealed(session.is_offline());

            self.imp().offline_handler_id.replace(Some(handler_id));
        }

        self.imp().user.replace(user);
        self.notify("user");
    }

    /// The type of the source that activated drop mode.
    pub fn drop_source_type(&self) -> Option<RoomType> {
        self.imp().drop_source_type.get()
    }

    /// Set the type of the source that activated drop mode.
    fn set_drop_source_type(&self, source_type: Option<RoomType>) {
        let imp = self.imp();

        if self.drop_source_type() == source_type {
            return;
        }

        imp.drop_source_type.set(source_type);

        if source_type.is_some() {
            imp.listview.add_css_class("drop-mode");
        } else {
            imp.listview.remove_css_class("drop-mode");
        }

        self.notify("drop-source-type");
    }

    /// The type of the drop target that is currently hovered.
    pub fn drop_active_target_type(&self) -> Option<RoomType> {
        self.imp().drop_active_target_type.get()
    }

    /// Set the type of the drop target that is currently hovered.
    fn set_drop_active_target_type(&self, target_type: Option<RoomType>) {
        if self.drop_active_target_type() == target_type {
            return;
        }

        self.imp().drop_active_target_type.set(target_type);
        self.notify("drop-active-target-type");
    }

    pub fn room_row_popover(&self) -> &gtk::PopoverMenu {
        let imp = self.imp();
        imp.room_row_popover
            .get_or_init(|| gtk::PopoverMenu::from_model(Some(&*imp.room_row_menu)))
    }

    /// Returns the parent `Window` containing the `Sidebar`
    fn parent_window(&self) -> Option<Window> {
        self.root().and_downcast()
    }
}

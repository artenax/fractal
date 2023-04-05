mod category;
mod category_row;
mod category_type;
mod entry;
mod entry_row;
mod entry_type;
mod item_list;
mod room_row;
mod row;
mod selection;
mod sidebar_item;
mod verification_row;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{
    gio, glib,
    glib::{clone, closure},
    CompositeTemplate,
};
use log::error;

pub use self::{
    category::Category,
    category_type::CategoryType,
    entry::Entry,
    entry_type::EntryType,
    item_list::ItemList,
    sidebar_item::{SidebarItem, SidebarItemExt, SidebarItemImpl},
};
use self::{
    category_row::CategoryRow, entry_row::EntryRow, room_row::RoomRow, row::Row,
    selection::Selection, verification_row::VerificationRow,
};
use crate::{
    components::Avatar,
    session::{
        room::{Room, RoomType},
        user::UserExt,
        verification::IdentityVerification,
        User,
    },
    Window,
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{signal::SignalHandlerId, subclass::InitializingObject};
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/sidebar.ui")]
    pub struct Sidebar {
        pub compact: Cell<bool>,
        pub selected_item: RefCell<Option<glib::Object>>,
        #[template_child]
        pub headerbar: TemplateChild<adw::HeaderBar>,
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
        pub drop_binding: RefCell<Option<glib::Binding>>,
        pub offline_handler_id: RefCell<Option<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Sidebar {
        const NAME: &'static str = "Sidebar";
        type Type = super::Sidebar;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            RoomRow::static_type();
            Row::static_type();
            Avatar::static_type();
            Self::bind_template(klass);
            klass.set_css_name("sidebar");

            klass.install_action(
                "sidebar.set-drop-source-type",
                Some("u"),
                move |obj, _, variant| {
                    obj.set_drop_source_type(
                        variant
                            .and_then(|variant| variant.get::<Option<u32>>().flatten())
                            .and_then(|u| RoomType::try_from(u).ok()),
                    );
                },
            );
            klass.install_action("sidebar.update-drop-targets", None, move |obj, _, _| {
                if obj.drop_source_type().is_some() {
                    obj.update_drop_targets();
                }
            });
            klass.install_action(
                "sidebar.set-active-drop-category",
                Some("mu"),
                move |obj, _, variant| {
                    obj.update_active_drop_targets(
                        variant
                            .and_then(|variant| variant.get::<Option<u32>>().flatten())
                            .and_then(|u| RoomType::try_from(u).ok()),
                    );
                },
            );
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
                    glib::ParamSpecBoolean::builder("compact")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<ItemList>("item-list")
                        .write_only()
                        .build(),
                    glib::ParamSpecObject::builder::<glib::Object>("selected-item")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecEnum::builder_with_default(
                        "drop-source-type",
                        CategoryType::None,
                    )
                    .read_only()
                    .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "compact" => obj.set_compact(value.get().unwrap()),
                "user" => obj.set_user(value.get().unwrap()),
                "item-list" => obj.set_item_list(value.get().unwrap()),
                "selected-item" => obj.set_selected_item(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "compact" => obj.compact().to_value(),
                "user" => obj.user().to_value(),
                "selected-item" => obj.selected_item().to_value(),
                "drop-source-type" => obj
                    .drop_source_type()
                    .map(CategoryType::from)
                    .unwrap_or(CategoryType::None)
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
                let model: Option<Selection> = listview.model().and_then(|o| o.downcast().ok());
                let row: Option<gtk::TreeListRow> = model
                    .as_ref()
                    .and_then(|m| m.item(pos))
                    .and_then(|o| o.downcast().ok());

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
                    if let Some(prev_parent) = account_switcher.parent().and_then(|btn| btn.downcast::<gtk::MenuButton>().ok()) {
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
    impl BinImpl for Sidebar {}
}

glib::wrapper! {
    pub struct Sidebar(ObjectSubclass<imp::Sidebar>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Sidebar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Whether a compact view is used.
    pub fn compact(&self) -> bool {
        self.imp().compact.get()
    }

    /// Set whether a compact view is used.
    pub fn set_compact(&self, compact: bool) {
        self.imp().compact.set(compact)
    }

    /// The selected item in this sidebar.
    pub fn selected_item(&self) -> Option<glib::Object> {
        self.imp().selected_item.borrow().clone()
    }

    pub fn room_search_bar(&self) -> gtk::SearchBar {
        self.imp().room_search.clone()
    }

    /// Set the list of items in the sidebar.
    pub fn set_item_list(&self, item_list: Option<ItemList>) {
        let imp = self.imp();

        if let Some(binding) = imp.drop_binding.take() {
            binding.unbind();
        }

        let item_list = match item_list {
            Some(item_list) => item_list,
            None => {
                imp.listview.set_model(gtk::SelectionModel::NONE);
                return;
            }
        };

        imp.drop_binding.replace(Some(
            self.bind_property("drop-source-type", &item_list, "show-all-for-category")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build(),
        ));

        let tree_model = gtk::TreeListModel::new(item_list, false, true, |item| {
            item.clone().downcast::<gio::ListModel>().ok()
        });

        let room_expression = gtk::ClosureExpression::new::<String>(
            &[] as &[gtk::Expression],
            closure!(|row: gtk::TreeListRow| {
                row.item()
                    .and_then(|o| o.downcast::<Room>().ok())
                    .map_or(String::new(), |o| o.display_name())
            }),
        );
        let filter = gtk::StringFilter::builder()
            .match_mode(gtk::StringFilterMatchMode::Substring)
            .expression(&room_expression)
            .ignore_case(true)
            .build();
        imp.room_search_entry
            .bind_property("text", &filter, "search")
            .flags(glib::BindingFlags::SYNC_CREATE)
            .build();
        let filter_model = gtk::FilterListModel::new(Some(tree_model), Some(filter));

        let selection = Selection::new(Some(&filter_model));
        self.bind_property("selected-item", &selection, "selected-item")
            .flags(glib::BindingFlags::SYNC_CREATE | glib::BindingFlags::BIDIRECTIONAL)
            .build();

        imp.listview.set_model(Some(&selection));
    }

    /// Set the selected item in this sidebar.
    pub fn set_selected_item(&self, selected_item: Option<glib::Object>) {
        if self.selected_item() == selected_item {
            return;
        }

        self.imp().selected_item.replace(selected_item);
        self.notify("selected-item");
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
        self.update_drop_targets();
    }

    /// Update the disabled or empty state of drop targets.
    fn update_drop_targets(&self) {
        let mut child = self.imp().listview.first_child();

        while let Some(widget) = child {
            if let Some(row) = widget
                .first_child()
                .and_then(|widget| widget.downcast::<Row>().ok())
            {
                if let Some(source_type) = self.drop_source_type() {
                    if row
                        .room_type()
                        .filter(|row_type| source_type.can_change_to(row_type))
                        .is_some()
                    {
                        row.remove_css_class("drop-disabled");

                        if row
                            .item()
                            .and_then(|object| object.downcast::<Category>().ok())
                            .filter(|category| category.is_empty())
                            .is_some()
                        {
                            row.add_css_class("drop-empty");
                        } else {
                            row.remove_css_class("drop-empty");
                        }
                    } else {
                        let is_forget_entry = row
                            .entry_type()
                            .filter(|entry_type| entry_type == &EntryType::Forget)
                            .is_some();
                        if is_forget_entry && source_type == RoomType::Left {
                            row.remove_css_class("drop-disabled");
                        } else {
                            row.add_css_class("drop-disabled");
                            row.remove_css_class("drop-empty");
                        }
                    }
                } else {
                    // Clear style
                    row.remove_css_class("drop-disabled");
                    row.remove_css_class("drop-empty");
                    row.remove_css_class("drop-active");
                };

                if let Some(category_row) = row
                    .child()
                    .and_then(|child| child.downcast::<CategoryRow>().ok())
                {
                    category_row.set_show_label_for_category(
                        self.drop_source_type()
                            .map(CategoryType::from)
                            .unwrap_or(CategoryType::None),
                    );
                }
            }
            child = widget.next_sibling();
        }
    }

    /// Update the active state of drop targets.
    fn update_active_drop_targets(&self, target_type: Option<RoomType>) {
        let mut child = self.imp().listview.first_child();

        while let Some(widget) = child {
            if let Some((row, row_type)) = widget
                .first_child()
                .and_then(|widget| widget.downcast::<Row>().ok())
                .and_then(|row| {
                    let row_type = row.room_type()?;
                    Some((row, row_type))
                })
            {
                if target_type
                    .filter(|target_type| target_type == &row_type)
                    .is_some()
                {
                    row.add_css_class("drop-active");
                } else {
                    row.remove_css_class("drop-active");
                }
            }
            child = widget.next_sibling();
        }
    }

    pub fn room_row_popover(&self) -> &gtk::PopoverMenu {
        let imp = self.imp();
        imp.room_row_popover
            .get_or_init(|| gtk::PopoverMenu::from_model(Some(&*imp.room_row_menu)))
    }

    /// Returns the parent `Window` containing the `Sidebar`
    fn parent_window(&self) -> Option<Window> {
        self.root()?.downcast().ok()
    }
}

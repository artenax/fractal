use adw::{prelude::*, subclass::prelude::*};
use gtk::{gdk, glib, glib::clone};

use super::{CategoryType, EntryType};
use crate::{
    session::{
        room::{Room, RoomType},
        sidebar::{
            Category, CategoryRow, Entry, EntryRow, RoomRow, Sidebar, SidebarItem, VerificationRow,
        },
        verification::IdentityVerification,
    },
    utils::BoundObjectWeakRef,
};

mod imp {
    use std::cell::RefCell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Row {
        pub sidebar: BoundObjectWeakRef<Sidebar>,
        pub list_row: RefCell<Option<gtk::TreeListRow>>,
        pub bindings: RefCell<Vec<glib::Binding>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Row {
        const NAME: &'static str = "SidebarRow";
        type Type = super::Row;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("sidebar-row");
        }
    }

    impl ObjectImpl for Row {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<glib::Object>("item")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<gtk::TreeListRow>("list-row")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<Sidebar>("sidebar")
                        .construct_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "list-row" => obj.set_list_row(value.get().unwrap()),
                "sidebar" => obj.set_sidebar(value.get().ok().as_ref()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "item" => obj.item().to_value(),
                "list-row" => obj.list_row().to_value(),
                "sidebar" => obj.sidebar().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Set up drop controller
            let drop = gtk::DropTarget::builder()
                .actions(gdk::DragAction::MOVE)
                .formats(&gdk::ContentFormats::for_type(Room::static_type()))
                .build();
            drop.connect_accept(clone!(@weak obj => @default-return false, move |_, drop| {
                obj.drop_accept(drop)
            }));
            drop.connect_leave(clone!(@weak obj => move |_| {
                obj.drop_leave();
            }));
            drop.connect_drop(
                clone!(@weak obj => @default-return false, move |_, v, _, _| {
                    obj.drop_end(v)
                }),
            );
            obj.add_controller(drop);
        }

        fn dispose(&self) {
            self.sidebar.disconnect_signals();
        }
    }

    impl WidgetImpl for Row {}
    impl BinImpl for Row {}
}

glib::wrapper! {
    pub struct Row(ObjectSubclass<imp::Row>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Row {
    pub fn new(sidebar: &Sidebar) -> Self {
        glib::Object::builder()
            .property("sidebar", sidebar)
            .property("focusable", true)
            .build()
    }

    /// The ancestor sidebar of this row.
    pub fn sidebar(&self) -> Sidebar {
        self.imp().sidebar.obj().unwrap()
    }

    /// Set the ancestor sidebar of this row.
    fn set_sidebar(&self, sidebar: Option<&Sidebar>) {
        let Some(sidebar) = sidebar else {
            return;
        };

        let drop_source_type_handler = sidebar.connect_notify_local(
            Some("drop-source-type"),
            clone!(@weak self as obj => move |_, _| {
                obj.update_for_drop_source_type();
            }),
        );

        let drop_active_target_type_handler = sidebar.connect_notify_local(
            Some("drop-active-target-type"),
            clone!(@weak self as obj => move |_, _| {
                obj.update_for_drop_active_target_type();
            }),
        );

        self.imp().sidebar.set(
            sidebar,
            vec![drop_source_type_handler, drop_active_target_type_handler],
        );
    }

    /// The sidebar item of this row.
    pub fn item(&self) -> Option<SidebarItem> {
        self.list_row()
            .and_then(|r| r.item())
            .and_then(|obj| obj.downcast().ok())
    }

    /// The list row to track for expander state.
    pub fn list_row(&self) -> Option<gtk::TreeListRow> {
        self.imp().list_row.borrow().clone()
    }

    /// Set the list row to track for expander state.
    pub fn set_list_row(&self, list_row: Option<gtk::TreeListRow>) {
        let imp = self.imp();

        if self.list_row() == list_row {
            return;
        }

        for binding in imp.bindings.take() {
            binding.unbind();
        }

        let row = if let Some(row) = list_row.clone() {
            imp.list_row.replace(list_row);
            row
        } else {
            return;
        };

        let mut bindings = vec![];
        if let Some(item) = self.item() {
            if let Some(category) = item.downcast_ref::<Category>() {
                let child =
                    if let Some(Ok(child)) = self.child().map(|w| w.downcast::<CategoryRow>()) {
                        child
                    } else {
                        let child = CategoryRow::new();
                        self.set_child(Some(&child));
                        child
                    };
                child.set_category(Some(category.clone()));

                bindings.push(
                    row.bind_property("expanded", &child, "expanded")
                        .flags(glib::BindingFlags::SYNC_CREATE)
                        .build(),
                );
            } else if let Some(room) = item.downcast_ref::<Room>() {
                let child = if let Some(Ok(child)) = self.child().map(|w| w.downcast::<RoomRow>()) {
                    child
                } else {
                    let child = RoomRow::new();
                    self.set_child(Some(&child));
                    child
                };

                child.set_room(Some(room.clone()));
            } else if let Some(entry) = item.downcast_ref::<Entry>() {
                let child = if let Some(Ok(child)) = self.child().map(|w| w.downcast::<EntryRow>())
                {
                    child
                } else {
                    let child = EntryRow::new();
                    self.set_child(Some(&child));
                    child
                };

                if entry.type_() == EntryType::Forget {
                    self.add_css_class("forget");
                }

                child.set_entry(Some(entry.clone()));
            } else if let Some(verification) = item.downcast_ref::<IdentityVerification>() {
                let child = if let Some(Ok(child)) =
                    self.child().map(|w| w.downcast::<VerificationRow>())
                {
                    child
                } else {
                    let child = VerificationRow::new();
                    self.set_child(Some(&child));
                    child
                };

                child.set_identity_verification(Some(verification.clone()));
            } else {
                panic!("Wrong row item: {item:?}");
            }

            self.update_for_drop_source_type();
        }

        imp.bindings.replace(bindings);

        self.notify("item");
        self.notify("list-row");
    }

    /// Get the `RoomType` of this item.
    ///
    /// If this is not a `Category` or one of its children, returns `None`.
    pub fn room_type(&self) -> Option<RoomType> {
        let item = self.item()?;

        if let Some(room) = item.downcast_ref::<Room>() {
            Some(room.category())
        } else {
            item.downcast_ref::<Category>()
                .and_then(|category| RoomType::try_from(category.type_()).ok())
        }
    }

    /// Get the `EntryType` of this item.
    ///
    /// If this is not a `Entry`, returns `None`.
    pub fn entry_type(&self) -> Option<EntryType> {
        let item = self.item()?;
        item.downcast_ref::<Entry>().map(|entry| entry.type_())
    }

    /// Handle the drag-n-drop hovering this row.
    fn drop_accept(&self, drop: &gdk::Drop) -> bool {
        let room = drop
            .drag()
            .map(|drag| drag.content())
            .and_then(|content| content.value(Room::static_type()).ok())
            .and_then(|value| value.get::<Room>().ok());
        if let Some(room) = room {
            if let Some(target_type) = self.room_type() {
                if room.category().can_change_to(target_type) {
                    self.sidebar()
                        .set_drop_active_target_type(Some(target_type));
                    return true;
                }
            } else if let Some(entry_type) = self.entry_type() {
                if room.category() == RoomType::Left && entry_type == EntryType::Forget {
                    self.add_css_class("drop-active");
                    self.sidebar().set_drop_active_target_type(None);
                    return true;
                }
            }
        }
        false
    }

    /// Handle the drag-n-drop leaving this row.
    fn drop_leave(&self) {
        self.remove_css_class("drop-active");
        self.sidebar().set_drop_active_target_type(None);
    }

    /// Handle the drop on this row.
    fn drop_end(&self, value: &glib::Value) -> bool {
        let mut ret = false;
        if let Ok(room) = value.get::<Room>() {
            if let Some(target_type) = self.room_type() {
                if room.category().can_change_to(target_type) {
                    room.set_category(target_type);
                    ret = true;
                }
            } else if let Some(entry_type) = self.entry_type() {
                if room.category() == RoomType::Left && entry_type == EntryType::Forget {
                    room.forget();
                    ret = true;
                }
            }
        }
        self.sidebar().set_drop_source_type(None);
        ret
    }

    /// Update the disabled or empty state of this drop target.
    fn update_for_drop_source_type(&self) {
        let source_type = self.sidebar().drop_source_type();

        if let Some(source_type) = source_type {
            if self
                .room_type()
                .map_or(false, |row_type| source_type.can_change_to(row_type))
            {
                self.remove_css_class("drop-disabled");

                if self
                    .item()
                    .and_then(|object| object.downcast::<Category>().ok())
                    .map_or(false, |category| category.is_empty())
                {
                    self.add_css_class("drop-empty");
                } else {
                    self.remove_css_class("drop-empty");
                }
            } else {
                let is_forget_entry = self
                    .entry_type()
                    .map_or(false, |entry_type| entry_type == EntryType::Forget);
                if is_forget_entry && source_type == RoomType::Left {
                    self.remove_css_class("drop-disabled");
                } else {
                    self.add_css_class("drop-disabled");
                    self.remove_css_class("drop-empty");
                }
            }
        } else {
            // Clear style
            self.remove_css_class("drop-disabled");
            self.remove_css_class("drop-empty");
            self.remove_css_class("drop-active");
        };

        if let Some(category_row) = self
            .child()
            .and_then(|child| child.downcast::<CategoryRow>().ok())
        {
            category_row.set_show_label_for_category(
                source_type.map(CategoryType::from).unwrap_or_default(),
            );
        }
    }

    /// Update the active state of this drop target.
    fn update_for_drop_active_target_type(&self) {
        let Some(room_type) = self.room_type() else {
            return;
        };
        let target_type = self.sidebar().drop_active_target_type();

        if target_type.map_or(false, |target_type| target_type == room_type) {
            self.add_css_class("drop-active");
        } else {
            self.remove_css_class("drop-active");
        }
    }
}

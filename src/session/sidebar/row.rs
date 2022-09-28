use std::convert::TryFrom;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gdk, glib, glib::clone};

use super::EntryType;
use crate::session::{
    room::{Room, RoomType},
    sidebar::{
        Category, CategoryRow, Entry, EntryRow, RoomRow, Sidebar, SidebarItem, VerificationRow,
    },
    verification::IdentityVerification,
};

mod imp {
    use std::cell::RefCell;

    use glib::WeakRef;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Row {
        pub sidebar: WeakRef<Sidebar>,
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
                    glib::ParamSpecObject::new(
                        "item",
                        "Item",
                        "The sidebar item of this row",
                        glib::Object::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "list-row",
                        "List Row",
                        "The list row to track for expander state",
                        gtk::TreeListRow::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecObject::new(
                        "sidebar",
                        "sidebar",
                        "The ancestor sidebar of this row",
                        Sidebar::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
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
                "list-row" => obj.set_list_row(value.get().unwrap()),
                "sidebar" => self.sidebar.set(value.get().ok().as_ref()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "item" => obj.item().to_value(),
                "list-row" => obj.list_row().to_value(),
                "sidebar" => obj.sidebar().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

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
            obj.add_controller(&drop);
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
        glib::Object::new(&[("sidebar", sidebar)]).expect("Failed to create Row")
    }

    pub fn sidebar(&self) -> Sidebar {
        self.imp().sidebar.upgrade().unwrap()
    }

    pub fn item(&self) -> Option<SidebarItem> {
        self.list_row()
            .and_then(|r| r.item())
            .and_then(|obj| obj.downcast().ok())
    }

    pub fn list_row(&self) -> Option<gtk::TreeListRow> {
        self.imp().list_row.borrow().clone()
    }

    pub fn set_list_row(&self, list_row: Option<gtk::TreeListRow>) {
        let priv_ = self.imp();

        if self.list_row() == list_row {
            return;
        }

        for binding in priv_.bindings.take() {
            binding.unbind();
        }

        let row = if let Some(row) = list_row.clone() {
            priv_.list_row.replace(list_row);
            row
        } else {
            return;
        };

        let mut bindings = vec![];
        if let Some(item) = self.item() {
            if let Some(list_item) = self.parent() {
                bindings.push(
                    item.bind_property("visible", &list_item, "visible")
                        .flags(glib::BindingFlags::SYNC_CREATE)
                        .build(),
                );
            }

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

                if let Some(list_item) = self.parent() {
                    list_item.set_css_classes(&["category"]);
                }
            } else if let Some(room) = item.downcast_ref::<Room>() {
                let child = if let Some(Ok(child)) = self.child().map(|w| w.downcast::<RoomRow>()) {
                    child
                } else {
                    let child = RoomRow::new();
                    self.set_child(Some(&child));
                    child
                };

                child.set_room(Some(room.clone()));

                if let Some(list_item) = self.parent() {
                    list_item.set_css_classes(&["room"]);
                }
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

                if let Some(list_item) = self.parent() {
                    list_item.set_css_classes(&["entry"]);
                }
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

                if let Some(list_item) = self.parent() {
                    list_item.set_css_classes(&["room"]);
                }
            } else {
                panic!("Wrong row item: {:?}", item);
            }
            self.activate_action("sidebar.update-drop-targets", None)
                .unwrap();
        }

        priv_.bindings.replace(bindings);

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

    fn drop_accept(&self, drop: &gdk::Drop) -> bool {
        let room = drop
            .drag()
            .map(|drag| drag.content())
            .and_then(|content| content.value(Room::static_type()).ok())
            .and_then(|value| value.get::<Room>().ok());
        if let Some(room) = room {
            if let Some(target_type) = self.room_type() {
                if room.category().can_change_to(&target_type) {
                    self.activate_action(
                        "sidebar.set-active-drop-category",
                        Some(&Some(u32::from(target_type)).to_variant()),
                    )
                    .unwrap();
                    return true;
                }
            } else if let Some(entry_type) = self.entry_type() {
                if room.category() == RoomType::Left && entry_type == EntryType::Forget {
                    self.parent().unwrap().add_css_class("drop-active");
                    self.activate_action("sidebar.set-active-drop-category", None)
                        .unwrap();
                    return true;
                }
            }
        }
        false
    }

    fn drop_leave(&self) {
        self.parent().unwrap().remove_css_class("drop-active");
        self.activate_action("sidebar.set-active-drop-category", None)
            .unwrap();
    }

    fn drop_end(&self, value: &glib::Value) -> bool {
        let mut ret = false;
        if let Ok(room) = value.get::<Room>() {
            if let Some(target_type) = self.room_type() {
                if room.category().can_change_to(&target_type) {
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
        self.activate_action("sidebar.set-drop-source-type", None)
            .unwrap();
        ret
    }
}

use std::convert::TryFrom;

use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};

use crate::session::{
    room::RoomType,
    room_list::RoomList,
    sidebar::{Category, CategoryType, Entry, EntryType},
    verification::VerificationList,
};

mod imp {
    use std::cell::Cell;

    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct ItemList {
        pub list: OnceCell<[(glib::Object, Cell<bool>); 8]>,
        pub room_list: OnceCell<RoomList>,
        pub verification_list: OnceCell<VerificationList>,
        /// The `CategoryType` to show all compatible categories for.
        ///
        /// Uses `RoomType::can_change_to` to find compatible categories.
        pub show_all_for_category: Cell<CategoryType>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ItemList {
        const NAME: &'static str = "ItemList";
        type Type = super::ItemList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for ItemList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::new(
                        "room-list",
                        "Room list",
                        "The list of rooms",
                        RoomList::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecObject::new(
                        "verification-list",
                        "Verification list",
                        "The list of verification requests",
                        VerificationList::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecEnum::new(
                        "show-all-for-category",
                        "Show All For Category",
                        "The `CategoryType` to show all compatible categories for",
                        CategoryType::static_type(),
                        CategoryType::None as i32,
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
                "room-list" => obj.set_room_list(value.get().unwrap()),
                "verification-list" => obj.set_verification_list(value.get().unwrap()),
                "show-all-for-category" => obj.set_show_all_for_category(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room-list" => obj.room_list().to_value(),
                "verification-list" => obj.verification_list().to_value(),
                "show-all-for-category" => obj.show_all_for_category().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            let room_list = obj.room_list();
            let verification_list = obj.verification_list();

            let list = [
                Entry::new(EntryType::Explore).upcast::<glib::Object>(),
                Category::new(CategoryType::VerificationRequest, verification_list)
                    .upcast::<glib::Object>(),
                Category::new(CategoryType::Invited, room_list).upcast::<glib::Object>(),
                Category::new(CategoryType::Favorite, room_list).upcast::<glib::Object>(),
                Category::new(CategoryType::Normal, room_list).upcast::<glib::Object>(),
                Category::new(CategoryType::LowPriority, room_list).upcast::<glib::Object>(),
                Category::new(CategoryType::Left, room_list).upcast::<glib::Object>(),
                Entry::new(EntryType::Forget).upcast::<glib::Object>(),
            ];

            for (index, item) in list.iter().enumerate() {
                if let Some(category) = item.downcast_ref::<Category>() {
                    category.connect_notify_local(
                        Some("empty"),
                        clone!(@weak obj => move |_, _| {
                            obj.update_item(index);
                        }),
                    );
                }
            }

            let list = list.map(|item| {
                let visible = if let Some(category) = item.downcast_ref::<Category>() {
                    !category.is_empty()
                } else {
                    item.downcast_ref::<Entry>()
                        .filter(|entry| entry.type_() == EntryType::Forget)
                        .is_none()
                };
                (item, Cell::new(visible))
            });

            self.list.set(list).unwrap();
        }
    }

    impl ListModelImpl for ItemList {
        fn item_type(&self, _list_model: &Self::Type) -> glib::Type {
            glib::Object::static_type()
        }
        fn n_items(&self, _list_model: &Self::Type) -> u32 {
            self.list
                .get()
                .unwrap()
                .iter()
                .filter(|(_, visible)| visible.get())
                .count() as u32
        }
        fn item(&self, _list_model: &Self::Type, position: u32) -> Option<glib::Object> {
            self.list
                .get()
                .unwrap()
                .iter()
                .filter_map(
                    |(item, visible)| {
                        if visible.get() {
                            Some(item)
                        } else {
                            None
                        }
                    },
                )
                .nth(position as usize)
                .cloned()
        }
    }
}

glib::wrapper! {
    /// Fixed list of all subcomponents in the sidebar.
    ///
    /// ItemList implements the ListModel interface and yields the subcomponents
    /// from the sidebar, namely Entries and Categories.
    pub struct ItemList(ObjectSubclass<imp::ItemList>)
        @implements gio::ListModel;
}

impl ItemList {
    pub fn new(room_list: &RoomList, verification_list: &VerificationList) -> Self {
        glib::Object::new(&[
            ("room-list", room_list),
            ("verification-list", verification_list),
        ])
        .expect("Failed to create ItemList")
    }

    fn update_item(&self, position: usize) {
        let priv_ = self.imp();
        let (item, old_visible) = priv_.list.get().unwrap().get(position).unwrap();

        let visible = if let Some(category) = item.downcast_ref::<Category>() {
            !category.is_empty()
                || RoomType::try_from(self.show_all_for_category())
                    .ok()
                    .and_then(|room_type| {
                        RoomType::try_from(category.type_())
                            .ok()
                            .filter(|category| room_type.can_change_to(category))
                    })
                    .is_some()
        } else if item
            .downcast_ref::<Entry>()
            .filter(|entry| entry.type_() == EntryType::Forget)
            .is_some()
        {
            self.show_all_for_category() == CategoryType::Left
        } else {
            return;
        };

        if visible != old_visible.get() {
            old_visible.set(visible);
            let hidden_before_position = priv_
                .list
                .get()
                .unwrap()
                .iter()
                .take(position)
                .filter(|(_, visible)| !visible.get())
                .count();
            let real_position = position - hidden_before_position;

            let (removed, added) = if visible { (0, 1) } else { (1, 0) };

            self.items_changed(real_position as u32, removed, added);
        }
    }

    pub fn show_all_for_category(&self) -> CategoryType {
        self.imp().show_all_for_category.get()
    }

    pub fn set_show_all_for_category(&self, category: CategoryType) {
        let priv_ = self.imp();

        if category == self.show_all_for_category() {
            return;
        }

        priv_.show_all_for_category.set(category);
        for i in 0..priv_.list.get().unwrap().len() {
            self.update_item(i);
        }

        self.notify("show-all-for-category");
    }

    fn set_room_list(&self, room_list: RoomList) {
        self.imp().room_list.set(room_list).unwrap();
    }

    fn set_verification_list(&self, verification_list: VerificationList) {
        self.imp().verification_list.set(verification_list).unwrap();
    }

    pub fn room_list(&self) -> &RoomList {
        self.imp().room_list.get().unwrap()
    }

    pub fn verification_list(&self) -> &VerificationList {
        self.imp().verification_list.get().unwrap()
    }
}

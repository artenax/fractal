use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};

use super::{Category, CategoryType, Entry, EntryType, SidebarItem, SidebarItemExt};
use crate::session::{room_list::RoomList, verification::VerificationList};

mod imp {
    use std::cell::Cell;

    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct ItemList {
        pub list: OnceCell<[SidebarItem; 9]>,
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
                    glib::ParamSpecObject::builder::<RoomList>("room-list")
                        .construct_only()
                        .build(),
                    glib::ParamSpecObject::builder::<VerificationList>("verification-list")
                        .construct_only()
                        .build(),
                    glib::ParamSpecEnum::builder_with_default(
                        "show-all-for-category",
                        CategoryType::None,
                    )
                    .explicit_notify()
                    .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "room-list" => obj.set_room_list(value.get().unwrap()),
                "verification-list" => obj.set_verification_list(value.get().unwrap()),
                "show-all-for-category" => obj.set_show_all_for_category(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "room-list" => obj.room_list().to_value(),
                "verification-list" => obj.verification_list().to_value(),
                "show-all-for-category" => obj.show_all_for_category().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let room_list = obj.room_list();
            let verification_list = obj.verification_list();

            let list: [SidebarItem; 9] = [
                Entry::new(EntryType::Explore).upcast(),
                Category::new(CategoryType::VerificationRequest, verification_list).upcast(),
                Category::new(CategoryType::Invited, room_list).upcast(),
                Category::new(CategoryType::Favorite, room_list).upcast(),
                Category::new(CategoryType::Direct, room_list).upcast(),
                Category::new(CategoryType::Normal, room_list).upcast(),
                Category::new(CategoryType::LowPriority, room_list).upcast(),
                Category::new(CategoryType::Left, room_list).upcast(),
                Entry::new(EntryType::Forget).upcast(),
            ];

            self.list.set(list.clone()).unwrap();

            for item in list.iter() {
                if let Some(category) = item.downcast_ref::<Category>() {
                    category.connect_notify_local(
                        Some("empty"),
                        clone!(@weak obj => move |category, _| {
                            obj.update_item(category);
                        }),
                    );
                }
                obj.update_item(item);
            }
        }
    }

    impl ListModelImpl for ItemList {
        fn item_type(&self) -> glib::Type {
            SidebarItem::static_type()
        }

        fn n_items(&self) -> u32 {
            self.list
                .get()
                .unwrap()
                .iter()
                .filter(|item| item.visible())
                .count() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.list
                .get()
                .unwrap()
                .iter()
                .filter(|item| item.visible())
                .nth(position as usize)
                .cloned()
                .map(|item| item.upcast())
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
        glib::Object::builder()
            .property("room-list", room_list)
            .property("verification-list", verification_list)
            .build()
    }

    /// The `CategoryType` to show all compatible categories for.
    ///
    /// The UI is updated to show possible actions for the list items according
    /// to the `CategoryType`.
    pub fn show_all_for_category(&self) -> CategoryType {
        self.imp().show_all_for_category.get()
    }

    /// Set the `CategoryType` to show all compatible categories for.
    pub fn set_show_all_for_category(&self, category: CategoryType) {
        let imp = self.imp();

        if category == self.show_all_for_category() {
            return;
        }

        imp.show_all_for_category.set(category);
        for item in imp.list.get().unwrap().iter() {
            self.update_item(item)
        }

        self.notify("show-all-for-category");
    }

    /// Set the list of rooms.
    fn set_room_list(&self, room_list: RoomList) {
        self.imp().room_list.set(room_list).unwrap();
    }

    /// Set the list of verification requests.
    fn set_verification_list(&self, verification_list: VerificationList) {
        self.imp().verification_list.set(verification_list).unwrap();
    }

    /// The list of rooms.
    pub fn room_list(&self) -> &RoomList {
        self.imp().room_list.get().unwrap()
    }

    /// The list of verification requests.
    pub fn verification_list(&self) -> &VerificationList {
        self.imp().verification_list.get().unwrap()
    }

    fn update_item(&self, item: &impl IsA<SidebarItem>) {
        let imp = self.imp();
        let item = item.upcast_ref::<SidebarItem>();

        let old_visible = item.visible();
        let old_pos = imp
            .list
            .get()
            .unwrap()
            .iter()
            .position(|obj| item == obj)
            .unwrap();

        item.update_visibility(self.show_all_for_category());

        let visible = item.visible();

        if visible != old_visible {
            let hidden_before_position = imp
                .list
                .get()
                .unwrap()
                .iter()
                .take(old_pos)
                .filter(|item| !item.visible())
                .count();
            let real_position = old_pos - hidden_before_position;

            let (removed, added) = if visible { (0, 1) } else { (1, 0) };
            self.items_changed(real_position as u32, removed, added);
        }
    }
}

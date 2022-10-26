use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};

use super::{CategoryType, SidebarItem, SidebarItemExt, SidebarItemImpl};
use crate::session::{
    room::{Room, RoomType},
    room_list::RoomList,
};

mod imp {
    use std::cell::Cell;

    use once_cell::unsync::OnceCell;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Category {
        pub model: OnceCell<gio::ListModel>,
        pub type_: Cell<CategoryType>,
        pub is_empty: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Category {
        const NAME: &'static str = "Category";
        type Type = super::Category;
        type ParentType = SidebarItem;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for Category {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecEnum::builder("type", CategoryType::default())
                        .construct_only()
                        .build(),
                    glib::ParamSpecString::builder("display-name")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<gio::ListModel>("model")
                        .construct_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("empty").read_only().build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "type" => self.type_.set(value.get().unwrap()),
                "model" => self.obj().set_model(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "type" => obj.type_().to_value(),
                "display-name" => obj.display_name().to_value(),
                "model" => obj.model().to_value(),
                "empty" => obj.is_empty().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for Category {
        fn item_type(&self) -> glib::Type {
            SidebarItem::static_type()
        }

        fn n_items(&self) -> u32 {
            self.model.get().unwrap().n_items()
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.model.get().unwrap().item(position)
        }
    }

    impl SidebarItemImpl for Category {
        fn update_visibility(&self, for_category: CategoryType) {
            let obj = self.obj();

            obj.set_visible(
                !obj.is_empty()
                    || RoomType::try_from(for_category)
                        .ok()
                        .and_then(|room_type| {
                            RoomType::try_from(obj.type_())
                                .ok()
                                .filter(|category| room_type.can_change_to(category))
                        })
                        .is_some(),
            )
        }
    }
}

glib::wrapper! {
    /// A list of Items in the same category implementing ListModel.
    ///
    /// This struct is used in ItemList for the sidebar.
    pub struct Category(ObjectSubclass<imp::Category>)
        @extends SidebarItem,
        @implements gio::ListModel;
}

impl Category {
    pub fn new(type_: CategoryType, model: &impl IsA<gio::ListModel>) -> Self {
        glib::Object::builder()
            .property("type", &type_)
            .property("model", model)
            .build()
    }

    /// The type of this category.
    pub fn type_(&self) -> CategoryType {
        self.imp().type_.get()
    }

    /// The display name of this category.
    pub fn display_name(&self) -> String {
        self.type_().to_string()
    }

    /// The filter list model on this category.
    pub fn model(&self) -> Option<&gio::ListModel> {
        self.imp().model.get()
    }

    /// Set the filter list model of this category.
    fn set_model(&self, model: gio::ListModel) {
        let type_ = self.type_();

        // Special case room lists so that they are sorted and in the right category
        let model = if model.is::<RoomList>() {
            let filter = gtk::CustomFilter::new(move |o| {
                o.downcast_ref::<Room>()
                    .filter(|r| CategoryType::from(r.category()) == type_)
                    .is_some()
            });
            let filter_model = gtk::FilterListModel::new(Some(&model), Some(&filter));

            let sorter = gtk::NumericSorter::builder()
                .expression(Room::this_expression("latest-unread"))
                .sort_order(gtk::SortType::Descending)
                .build();
            let sort_model = gtk::SortListModel::new(Some(&filter_model), Some(&sorter));
            sort_model.upcast()
        } else {
            model
        };

        model.connect_items_changed(
            clone!(@weak self as obj => move |model, pos, removed, added| {
                obj.items_changed(pos, removed, added);
                obj.set_is_empty(model.n_items() == 0);
            }),
        );

        self.set_is_empty(model.n_items() == 0);
        self.imp().model.set(model).unwrap();
    }

    /// Set whether this category is empty.
    fn set_is_empty(&self, is_empty: bool) {
        if is_empty == self.is_empty() {
            return;
        }

        self.imp().is_empty.set(is_empty);
        self.notify("empty");
    }

    /// Whether this category is empty.
    pub fn is_empty(&self) -> bool {
        self.imp().is_empty.get()
    }
}

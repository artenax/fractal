use gtk::{glib, prelude::*, subclass::prelude::*};

use super::{item_list::ItemList, selection::Selection};
use crate::session::model::Room;

mod imp {
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct SidebarListModel {
        /// The list of items in the sidebar.
        pub item_list: OnceCell<ItemList>,
        /// The tree list model.
        pub tree_model: OnceCell<gtk::TreeListModel>,
        /// The string filter.
        pub string_filter: gtk::StringFilter,
        /// The selection model.
        pub selection_model: Selection,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SidebarListModel {
        const NAME: &'static str = "SidebarListModel";
        type Type = super::SidebarListModel;
    }

    impl ObjectImpl for SidebarListModel {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<ItemList>("item-list")
                        .construct_only()
                        .build(),
                    glib::ParamSpecObject::builder::<gtk::TreeListModel>("tree-model")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<gtk::StringFilter>("string-filter")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<Selection>("selection-model")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "item-list" => obj.set_item_list(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "item-list" => obj.item_list().to_value(),
                "tree-model" => obj.tree_model().to_value(),
                "string-filter" => obj.string_filter().to_value(),
                "selection-model" => obj.selection_model().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// A wrapper for the sidebar list model of a `Session`.
    ///
    /// It allows to keep the state for selection and filtering.
    pub struct SidebarListModel(ObjectSubclass<imp::SidebarListModel>);
}

impl SidebarListModel {
    /// Create a new `SidebarListModel`.
    pub fn new(item_list: &ItemList) -> Self {
        glib::Object::builder()
            .property("item-list", item_list)
            .build()
    }

    /// The list of items in the sidebar.
    pub fn item_list(&self) -> &ItemList {
        self.imp().item_list.get().unwrap()
    }

    /// Set the list of items in the sidebar.
    fn set_item_list(&self, item_list: ItemList) {
        let imp = self.imp();

        imp.item_list.set(item_list.clone()).unwrap();

        let tree_model =
            gtk::TreeListModel::new(item_list, false, true, |item| item.clone().downcast().ok());
        imp.tree_model.set(tree_model.clone()).unwrap();

        let room_expression =
            gtk::TreeListRow::this_expression("item").chain_property::<Room>("display-name");
        imp.string_filter
            .set_match_mode(gtk::StringFilterMatchMode::Substring);
        imp.string_filter.set_expression(Some(&room_expression));
        imp.string_filter.set_ignore_case(true);
        // Default to an empty string to be able to bind to GtkEditable::text.
        imp.string_filter.set_search(Some(""));

        let filter_model =
            gtk::FilterListModel::new(Some(tree_model), Some(imp.string_filter.clone()));

        imp.selection_model.set_model(Some(&filter_model));
    }

    /// The tree list model.
    pub fn tree_model(&self) -> &gtk::TreeListModel {
        self.imp().tree_model.get().unwrap()
    }

    /// The string filter.
    pub fn string_filter(&self) -> &gtk::StringFilter {
        &self.imp().string_filter
    }

    /// The selection model.
    pub fn selection_model(&self) -> &Selection {
        &self.imp().selection_model
    }
}

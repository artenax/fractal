use adw::subclass::prelude::BinImpl;
use gettextrs::gettext;
use gtk::{self, accessible, glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::session::model::{Category, CategoryType};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/session/view/sidebar/category_row.ui")]
    pub struct CategoryRow {
        /// The category of this row.
        pub category: RefCell<Option<Category>>,
        /// The expanded state of this row.
        pub expanded: Cell<bool>,
        /// The `CategoryType` to show a label for during a drag-and-drop
        /// operation.
        pub show_label_for_category: Cell<CategoryType>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CategoryRow {
        const NAME: &'static str = "SidebarCategoryRow";
        type Type = super::CategoryRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_css_name("category");
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CategoryRow {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Category>("category")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("expanded")
                        .default_value(true)
                        .explicit_notify()
                        .construct()
                        .build(),
                    glib::ParamSpecString::builder("label").read_only().build(),
                    glib::ParamSpecEnum::builder::<CategoryType>("show-label-for-category")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "category" => obj.set_category(value.get().unwrap()),
                "expanded" => obj.set_expanded(value.get().unwrap()),
                "show-label-for-category" => obj.set_show_label_for_category(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "category" => obj.category().to_value(),
                "expanded" => obj.expanded().to_value(),
                "label" => obj.label().to_value(),
                "show-label-for-category" => obj.show_label_for_category().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for CategoryRow {}
    impl BinImpl for CategoryRow {}
}

glib::wrapper! {
    pub struct CategoryRow(ObjectSubclass<imp::CategoryRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl CategoryRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The category represented by this row.
    pub fn category(&self) -> Option<Category> {
        self.imp().category.borrow().clone()
    }

    /// Set the category represented by this row.
    pub fn set_category(&self, category: Option<Category>) {
        if self.category() == category {
            return;
        }

        self.imp().category.replace(category);
        self.notify("category");
        self.notify("label");
    }

    /// The expanded state of this row.
    fn expanded(&self) -> bool {
        self.imp().expanded.get()
    }

    /// Set the expanded state of this row.
    fn set_expanded(&self, expanded: bool) {
        if self.expanded() == expanded {
            return;
        }

        if expanded {
            self.set_state_flags(gtk::StateFlags::CHECKED, false);
        } else {
            self.unset_state_flags(gtk::StateFlags::CHECKED);
        }

        self.set_expanded_accessibility_state(expanded);
        self.imp().expanded.set(expanded);
        self.notify("expanded");
    }

    fn set_expanded_accessibility_state(&self, expanded: bool) {
        if let Some(row) = self.parent() {
            row.update_state(&[accessible::State::Expanded(Some(expanded))]);
        }
    }

    /// The label to show for this row.
    pub fn label(&self) -> Option<String> {
        let to_type = self.category()?.type_();
        let from_type = self.show_label_for_category();

        let label = match from_type {
            CategoryType::Invited => match to_type {
                CategoryType::Favorite => gettext("Join Room as Favorite"),
                CategoryType::Normal => gettext("Join Room"),
                CategoryType::LowPriority => gettext("Join Room as Low Priority"),
                CategoryType::Left => gettext("Reject Invite"),
                CategoryType::Direct => gettext("Join Room as People"),
                _ => to_type.to_string(),
            },
            CategoryType::Favorite => match to_type {
                CategoryType::Normal => gettext("Move to Rooms"),
                CategoryType::LowPriority => gettext("Move to Low Priority"),
                CategoryType::Left => gettext("Leave Room"),
                CategoryType::Direct => gettext("Move to People"),
                _ => to_type.to_string(),
            },
            CategoryType::Normal => match to_type {
                CategoryType::Favorite => gettext("Move to Favorites"),
                CategoryType::LowPriority => gettext("Move to Low Priority"),
                CategoryType::Left => gettext("Leave Room"),
                CategoryType::Direct => gettext("Move to People"),
                _ => to_type.to_string(),
            },
            CategoryType::LowPriority => match to_type {
                CategoryType::Favorite => gettext("Move to Favorites"),
                CategoryType::Normal => gettext("Move to Rooms"),
                CategoryType::Left => gettext("Leave Room"),
                CategoryType::Direct => gettext("Move to People"),
                _ => to_type.to_string(),
            },
            CategoryType::Left => match to_type {
                CategoryType::Favorite => gettext("Rejoin Room as Favorites"),
                CategoryType::Normal => gettext("Rejoin Room"),
                CategoryType::LowPriority => gettext("Rejoin Room as Low Priority"),
                CategoryType::Direct => gettext("Rejoin Room as People"),
                _ => to_type.to_string(),
            },
            CategoryType::Direct => match to_type {
                CategoryType::Favorite => gettext("Move to Favorites"),
                CategoryType::Normal => gettext("Move to Rooms"),
                CategoryType::LowPriority => gettext("Move to Low Priority"),
                CategoryType::Left => gettext("Leave Room"),
                _ => to_type.to_string(),
            },
            _ => to_type.to_string(),
        };

        Some(label)
    }

    /// The `CategoryType` to show a label for.
    ///
    /// This will change the label according to the action that can be performed
    /// when changing from the `CategoryType` to this row's `Category`.
    pub fn show_label_for_category(&self) -> CategoryType {
        self.imp().show_label_for_category.get()
    }

    /// Set the `CategoryType` to show a label for.
    pub fn set_show_label_for_category(&self, category: CategoryType) {
        if category == self.show_label_for_category() {
            return;
        }

        self.imp().show_label_for_category.set(category);

        self.notify("show-label-for-category");
        self.notify("label");
    }
}

use gtk::{glib, prelude::*, subclass::prelude::*};

use super::CategoryType;

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct CategoryFilter {
        /// The expression to watch.
        pub expression: RefCell<Option<gtk::Expression>>,
        /// The category type to filter.
        pub category_type: Cell<CategoryType>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CategoryFilter {
        const NAME: &'static str = "CategoryFilter";
        type Type = super::CategoryFilter;
        type ParentType = gtk::Filter;
    }

    impl ObjectImpl for CategoryFilter {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    gtk::ParamSpecExpression::builder("expression")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecEnum::builder::<CategoryType>("category-type")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "expression" => obj.set_expression(value.get().unwrap()),
                "category-type" => obj.set_category_type(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "expression" => obj.expression().to_value(),
                "category-type" => obj.category_type().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl FilterImpl for CategoryFilter {
        fn strictness(&self) -> gtk::FilterMatch {
            if self.category_type.get() == CategoryType::None {
                return gtk::FilterMatch::All;
            }

            if self.expression.borrow().is_none() {
                return gtk::FilterMatch::None;
            }

            gtk::FilterMatch::Some
        }

        fn match_(&self, item: &glib::Object) -> bool {
            let category_type = self.category_type.get();
            if category_type == CategoryType::None {
                return true;
            }

            let Some(value) = self
                .expression
                .borrow()
                .as_ref()
                .and_then(|e| e.evaluate(Some(item)))
                .map(|v| v.get::<CategoryType>().unwrap())
            else {
                return false;
            };

            value == category_type
        }
    }
}

glib::wrapper! {
    /// A filter by `CategoryType`.
    pub struct CategoryFilter(ObjectSubclass<imp::CategoryFilter>)
        @extends gtk::Filter;
}

impl CategoryFilter {
    pub fn new(expression: impl AsRef<gtk::Expression>, category_type: CategoryType) -> Self {
        glib::Object::builder()
            .property("expression", expression.as_ref())
            .property("category-type", category_type)
            .build()
    }

    /// The expression to watch.
    pub fn expression(&self) -> Option<gtk::Expression> {
        self.imp().expression.borrow().clone()
    }

    /// Set the expression to watch.
    ///
    /// This expression must return a [`CategoryType`].
    pub fn set_expression(&self, expression: Option<gtk::Expression>) {
        let prev_expression = self.expression();

        if prev_expression.is_none() && expression.is_none() {
            return;
        }

        let change = if self.category_type() == CategoryType::None {
            None
        } else if prev_expression.is_none() {
            Some(gtk::FilterChange::LessStrict)
        } else if expression.is_none() {
            Some(gtk::FilterChange::MoreStrict)
        } else {
            Some(gtk::FilterChange::Different)
        };

        self.imp().expression.replace(expression);
        if let Some(change) = change {
            self.changed(change)
        }
        self.notify("expression");
    }

    /// The category type to filter.
    pub fn category_type(&self) -> CategoryType {
        self.imp().category_type.get()
    }

    /// Set the category type to filter.
    pub fn set_category_type(&self, category_type: CategoryType) {
        let prev_category_type = self.category_type();

        if prev_category_type == category_type {
            return;
        }

        let change = if self.expression().is_none() {
            None
        } else if prev_category_type == CategoryType::None {
            Some(gtk::FilterChange::MoreStrict)
        } else if category_type == CategoryType::None {
            Some(gtk::FilterChange::LessStrict)
        } else {
            Some(gtk::FilterChange::Different)
        };

        self.imp().category_type.set(category_type);
        if let Some(change) = change {
            self.changed(change)
        }
        self.notify("category-type");
    }
}

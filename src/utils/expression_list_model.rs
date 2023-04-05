use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use log::error;

use crate::utils::BoundObject;

mod imp {
    use std::cell::RefCell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ExpressionListModel {
        pub model: BoundObject<gio::ListModel>,
        pub expression: RefCell<Option<gtk::Expression>>,
        pub watches: RefCell<Vec<gtk::ExpressionWatch>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ExpressionListModel {
        const NAME: &'static str = "ExpressionListModel";
        type Type = super::ExpressionListModel;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for ExpressionListModel {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<gio::ListModel>("model")
                        .explicit_notify()
                        .build(),
                    gtk::ParamSpecExpression::builder("expression")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "model" => obj.set_model(value.get().unwrap()),
                "expression" => obj.set_expression(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "model" => obj.model().to_value(),
                "expression" => obj.expression().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            self.model.disconnect_signals();

            for watch in self.watches.take() {
                watch.unwatch()
            }
        }
    }

    impl ListModelImpl for ExpressionListModel {
        fn item_type(&self) -> glib::Type {
            self.model
                .obj()
                .map(|m| m.item_type())
                .unwrap_or_else(glib::Object::static_type)
        }

        fn n_items(&self) -> u32 {
            self.model.obj().map(|m| m.n_items()).unwrap_or_default()
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.model.obj().and_then(|m| m.item(position))
        }
    }
}

glib::wrapper! {
    /// A list model that signals an item as changed when the expression's value changes.
    pub struct ExpressionListModel(ObjectSubclass<imp::ExpressionListModel>)
        @implements gio::ListModel;
}

impl ExpressionListModel {
    pub fn new(model: impl IsA<gio::ListModel>, expression: impl AsRef<gtk::Expression>) -> Self {
        glib::Object::builder()
            .property("model", model.upcast())
            .property("expression", expression.as_ref())
            .build()
    }

    /// The underlying model.
    pub fn model(&self) -> Option<gio::ListModel> {
        self.imp().model.obj()
    }

    /// Set the underlying model.
    pub fn set_model(&self, model: Option<gio::ListModel>) {
        let imp = self.imp();

        if imp.model.obj() == model {
            return;
        }

        let removed = self.n_items();

        imp.model.disconnect_signals();
        for watch in imp.watches.take() {
            watch.unwatch();
        }

        let added = if let Some(model) = model {
            let items_changed_handler = model.connect_items_changed(
                clone!(@weak self as obj => move |_, pos, removed, added| {
                    obj.watch_items(pos, removed, added);
                    obj.items_changed(pos, removed, added);
                }),
            );

            let added = model.n_items();
            imp.model.set(model, vec![items_changed_handler]);

            self.watch_items(0, removed, added);
            added
        } else {
            0
        };

        self.items_changed(0, removed, added);
        self.notify("model");
    }

    /// The watched expression.
    pub fn expression(&self) -> Option<gtk::Expression> {
        self.imp().expression.borrow().clone()
    }

    /// Set the watched expression.
    pub fn set_expression(&self, expression: Option<gtk::Expression>) {
        if self.expression().is_none() && expression.is_none() {
            return;
        }

        let imp = self.imp();

        // Reset expression watches.
        for watch in imp.watches.take() {
            watch.unwatch();
        }

        imp.expression.replace(expression);

        // Watch items again.
        let added = self.n_items();
        self.watch_items(0, 0, added);

        self.notify("expression");
    }

    /// Watch and unwatch items according to changes in the underlying model.
    fn watch_items(&self, pos: u32, removed: u32, added: u32) {
        let Some(expression) = self.expression() else {
            return;
        };
        let Some(model) = self.model() else {
            return;
        };
        let imp = self.imp();

        let mut new_watches = Vec::with_capacity(added as usize);
        for item_pos in pos..pos + added {
            let Some(item) = model.item(item_pos) else {
                error!("Out of bounds item");
                break;
            };

            new_watches.push(expression.watch(
                Some(&item),
                clone!(@weak self as obj, @weak item => move || {
                    obj.item_expr_changed(&item);
                }),
            ));
        }

        let mut watches = imp.watches.borrow_mut();
        let removed_range = (pos as usize)..((pos + removed) as usize);
        for watch in watches.splice(removed_range, new_watches) {
            watch.unwatch()
        }
    }

    fn item_expr_changed(&self, item: &glib::Object) {
        let Some(model) = self.model() else {
            return;
        };

        for (pos, obj) in model.snapshot().iter().enumerate() {
            if obj == item {
                self.items_changed(pos as u32, 1, 1);
                break;
            }
        }
    }
}

use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};

mod imp {
    use std::cell::{Cell, RefCell};

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Selection {
        pub model: RefCell<Option<gio::ListModel>>,
        pub selected: Cell<u32>,
        pub selected_item: RefCell<Option<glib::Object>>,
        pub signal_handler: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Selection {
        const NAME: &'static str = "SidebarSelection";
        type Type = super::Selection;
        type Interfaces = (gio::ListModel, gtk::SelectionModel);

        fn new() -> Self {
            Self {
                selected: Cell::new(gtk::INVALID_LIST_POSITION),
                ..Default::default()
            }
        }
    }

    impl ObjectImpl for Selection {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<gio::ListModel>("model")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecUInt::builder("selected")
                        .default_value(gtk::INVALID_LIST_POSITION)
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<glib::Object>("selected-item")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "model" => {
                    let model: Option<gio::ListModel> = value.get().unwrap();
                    obj.set_model(model.as_ref());
                }
                "selected" => obj.set_selected(value.get().unwrap()),
                "selected-item" => obj.set_selected_item(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "model" => obj.model().to_value(),
                "selected" => obj.selected().to_value(),
                "selected-item" => obj.selected_item().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for Selection {
        fn item_type(&self) -> glib::Type {
            gtk::TreeListRow::static_type()
        }
        fn n_items(&self) -> u32 {
            self.model
                .borrow()
                .as_ref()
                .map(|m| m.n_items())
                .unwrap_or(0)
        }
        fn item(&self, position: u32) -> Option<glib::Object> {
            self.model.borrow().as_ref().and_then(|m| m.item(position))
        }
    }

    impl SelectionModelImpl for Selection {
        fn selection_in_range(&self, _position: u32, _n_items: u32) -> gtk::Bitset {
            let bitset = gtk::Bitset::new_empty();
            let selected = self.selected.get();

            if selected != gtk::INVALID_LIST_POSITION {
                bitset.add(selected);
            }

            bitset
        }

        fn is_selected(&self, position: u32) -> bool {
            self.selected.get() == position
        }
    }
}

glib::wrapper! {
    pub struct Selection(ObjectSubclass<imp::Selection>)
        @implements gio::ListModel, gtk::SelectionModel;
}

impl Selection {
    pub fn new<P: IsA<gio::ListModel>>(model: Option<&P>) -> Selection {
        let model = model.map(|m| m.clone().upcast());
        glib::Object::builder().property("model", &model).build()
    }

    /// The underlying model.
    pub fn model(&self) -> Option<gio::ListModel> {
        self.imp().model.borrow().clone()
    }

    /// The position of the selected item.
    pub fn selected(&self) -> u32 {
        self.imp().selected.get()
    }

    /// The selected item.
    pub fn selected_item(&self) -> Option<glib::Object> {
        self.imp().selected_item.borrow().clone()
    }

    /// Set the underlying model.
    pub fn set_model<P: IsA<gio::ListModel>>(&self, model: Option<&P>) {
        let imp = self.imp();

        let _guard = self.freeze_notify();

        let model = model.map(|m| m.clone().upcast());

        let old_model = self.model();
        if old_model == model {
            return;
        }

        let n_items_before = old_model
            .map(|model| {
                if let Some(id) = imp.signal_handler.take() {
                    model.disconnect(id);
                }
                model.n_items()
            })
            .unwrap_or(0);

        if let Some(model) = model {
            imp.signal_handler.replace(Some(model.connect_items_changed(
                clone!(@weak self as obj => move |m, p, r, a| {
                        obj.items_changed_cb(m, p, r, a);
                }),
            )));

            self.items_changed_cb(&model, 0, n_items_before, model.n_items());

            imp.model.replace(Some(model));
        } else {
            imp.model.replace(None);

            if self.selected() != gtk::INVALID_LIST_POSITION {
                imp.selected.replace(gtk::INVALID_LIST_POSITION);
                self.notify("selected");
            }
            if self.selected_item().is_some() {
                imp.selected_item.replace(None);
                self.notify("selected-item");
            }

            self.items_changed(0, n_items_before, 0);
        }

        self.notify("model");
    }

    /// Set the selected item by its position.
    pub fn set_selected(&self, position: u32) {
        let imp = self.imp();

        let old_selected = self.selected();
        if old_selected == position {
            return;
        }

        let selected_item = self
            .model()
            .and_then(|m| m.item(position))
            .and_then(|o| o.downcast::<gtk::TreeListRow>().ok())
            .and_then(|r| r.item());

        let selected = if selected_item.is_none() {
            gtk::INVALID_LIST_POSITION
        } else {
            position
        };

        if old_selected == selected {
            return;
        }

        imp.selected.replace(selected);
        imp.selected_item.replace(selected_item);

        if old_selected == gtk::INVALID_LIST_POSITION {
            self.selection_changed(selected, 1);
        } else if selected == gtk::INVALID_LIST_POSITION {
            self.selection_changed(old_selected, 1);
        } else if selected < old_selected {
            self.selection_changed(selected, old_selected - selected + 1);
        } else {
            self.selection_changed(old_selected, selected - old_selected + 1);
        }

        self.notify("selected");
        self.notify("selected-item");
    }

    /// Set the selected item.
    fn set_selected_item(&self, item: Option<glib::Object>) {
        let imp = self.imp();

        let selected_item = self.selected_item();
        if selected_item == item {
            return;
        }

        let old_selected = self.selected();

        let mut selected = gtk::INVALID_LIST_POSITION;

        if item.is_some() {
            if let Some(model) = self.model() {
                for i in 0..model.n_items() {
                    let current_item = model
                        .item(i)
                        .and_then(|o| o.downcast::<gtk::TreeListRow>().ok())
                        .and_then(|r| r.item());
                    if current_item == item {
                        selected = i;
                        break;
                    }
                }
            }
        }

        imp.selected_item.replace(item);

        if old_selected != selected {
            imp.selected.replace(selected);

            if old_selected == gtk::INVALID_LIST_POSITION {
                self.selection_changed(selected, 1);
            } else if selected == gtk::INVALID_LIST_POSITION {
                self.selection_changed(old_selected, 1);
            } else if selected < old_selected {
                self.selection_changed(selected, old_selected - selected + 1);
            } else {
                self.selection_changed(old_selected, selected - old_selected + 1);
            }
            self.notify("selected");
        }

        self.notify("selected-item");
    }

    fn items_changed_cb(&self, model: &gio::ListModel, position: u32, removed: u32, added: u32) {
        let imp = self.imp();

        let _guard = self.freeze_notify();

        let selected = self.selected();
        let selected_item = self.selected_item();

        if selected_item.is_none() || selected < position {
            // unchanged
        } else if selected != gtk::INVALID_LIST_POSITION && selected >= position + removed {
            imp.selected.replace(selected + added - removed);
            self.notify("selected");
        } else {
            for i in 0..=added {
                if i == added {
                    // the item really was deleted
                    imp.selected.replace(gtk::INVALID_LIST_POSITION);
                    self.notify("selected");
                } else {
                    let item = model
                        .item(position + i)
                        .and_then(|o| o.downcast::<gtk::TreeListRow>().ok())
                        .and_then(|r| r.item());
                    if item == selected_item {
                        // the item moved
                        if selected != position + i {
                            imp.selected.replace(position + i);
                            self.notify("selected");
                        }
                        break;
                    }
                }
            }
        }

        self.items_changed(position, removed, added);
    }
}

use gtk::{gdk, gio, glib, glib::clone, prelude::*, subclass::prelude::*};

use crate::utils::BoundObjectWeakRef;

pub type CreateWidgetFromObjectFn = dyn Fn(&glib::Object) -> gtk::Widget + 'static;

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    pub struct OverlappingBox {
        /// The child widgets.
        pub widgets: RefCell<Vec<gtk::Widget>>,

        /// The size of the widgets.
        pub widgets_sizes: RefCell<Vec<(i32, i32)>>,

        /// The maximum number of children to display.
        ///
        /// `0` means that all children are displayed.
        pub max_children: Cell<u32>,

        /// The size by which the widgets overlap.
        pub overlap: Cell<u32>,

        /// The orientation of the box.
        pub orientation: Cell<gtk::Orientation>,

        /// The list model that is bound, if any.
        pub bound_model: RefCell<Option<BoundObjectWeakRef<gio::ListModel>>>,

        /// The method used to create widgets from the items of the list model,
        /// if any.
        pub create_widget_func: RefCell<Option<Box<CreateWidgetFromObjectFn>>>,
    }

    impl Default for OverlappingBox {
        fn default() -> Self {
            Self {
                widgets: Default::default(),
                widgets_sizes: Default::default(),
                max_children: Default::default(),
                overlap: Default::default(),
                orientation: gtk::Orientation::Horizontal.into(),
                bound_model: Default::default(),
                create_widget_func: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for OverlappingBox {
        const NAME: &'static str = "OverlappingBox";
        type Type = super::OverlappingBox;
        type ParentType = gtk::Widget;
        type Interfaces = (gtk::Buildable, gtk::Orientable);
    }

    impl ObjectImpl for OverlappingBox {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecUInt::builder("max-children")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecUInt::builder("overlap")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecOverride::for_interface::<gtk::Orientable>("orientation"),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "max-children" => obj.set_max_children(value.get().unwrap()),
                "overlap" => obj.set_overlap(value.get().unwrap()),
                "orientation" => obj.set_orientation(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "max-children" => obj.max_children().to_value(),
                "overlap" => obj.overlap().to_value(),
                "orientation" => obj.orientation().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            for widget in self.widgets.borrow().iter() {
                widget.unparent();
            }

            if let Some(bound_model) = self.bound_model.take() {
                bound_model.disconnect_signals()
            }
        }
    }

    impl WidgetImpl for OverlappingBox {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let mut size = 0;
            let overlap = self.overlap.get() as i32;
            let self_orientation = self.obj().orientation();

            for child in self.widgets.borrow().iter() {
                if !child.should_layout() {
                    continue;
                }

                let (_, child_size, ..) = child.measure(orientation, -1);

                if orientation == self_orientation {
                    size += child_size - overlap;
                } else {
                    size = size.max(child_size);
                }
            }

            if orientation == self_orientation {
                // The last child doesn't have an overlap.
                if size > 0 {
                    size += overlap;
                }
            }

            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let overlap = self.overlap.get() as i32;
            let self_orientation = self.obj().orientation();
            let mut pos = 0;

            for child in self.widgets.borrow().iter() {
                if !child.should_layout() {
                    continue;
                }

                let (_, child_height, ..) = child.measure(gtk::Orientation::Vertical, -1);
                let (_, child_width, ..) = child.measure(gtk::Orientation::Horizontal, -1);

                let (x, y) = if self_orientation == gtk::Orientation::Horizontal {
                    let x = pos;
                    pos += child_width - overlap;

                    // Center the child on the opposite orientation.
                    let y = (height - child_height) / 2;

                    (x, y)
                } else {
                    let y = pos;
                    pos += child_height - overlap;

                    // Center the child on the opposite orientation.
                    let x = (width - child_width) / 2;
                    (y, x)
                };

                let allocation = gdk::Rectangle::new(x, y, child_width, child_height);

                child.size_allocate(&allocation, -1);
            }
        }
    }

    impl BuildableImpl for OverlappingBox {}

    impl OrientableImpl for OverlappingBox {}
}

glib::wrapper! {
    /// A box that has multiple widgets overlapping.
    ///
    /// Note that this works only with children with a fixed size.
    pub struct OverlappingBox(ObjectSubclass<imp::OverlappingBox>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::Orientable;
}

impl OverlappingBox {
    /// Create an empty `OverlappingBox`.
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The maximum number of children to display.
    ///
    /// `0` means that all children are displayed.
    pub fn max_children(&self) -> u32 {
        self.imp().max_children.get()
    }

    /// Set the maximum number of children to display.
    pub fn set_max_children(&self, max_children: u32) {
        let old_max_children = self.max_children();

        if old_max_children == max_children {
            return;
        }

        let imp = self.imp();
        imp.max_children.set(max_children);
        self.notify("max-children");

        if max_children != 0 && self.children_nb() > max_children as usize {
            // We have more children than we should, remove them.
            let children = imp.widgets.borrow_mut().split_off(max_children as usize);
            for widget in children {
                widget.unparent()
            }
        } else if max_children == 0 || (old_max_children != 0 && max_children > old_max_children) {
            let Some(model) = imp.bound_model.borrow().as_ref().and_then(|s| s.obj()) else {
                return;
            };

            let diff = model.n_items() - old_max_children;
            if diff > 0 {
                // We could have more children, create them.
                self.handle_items_changed(&model, old_max_children, 0, diff);
            }
        }
    }

    /// The size by which the widgets overlap.
    pub fn overlap(&self) -> u32 {
        self.imp().overlap.get()
    }

    /// Set the size by which the widgets overlap.
    pub fn set_overlap(&self, overlap: u32) {
        if self.overlap() == overlap {
            return;
        }

        self.imp().overlap.set(overlap);
        self.notify("overlap");
        self.queue_resize();
    }

    /// The orientation of the box.
    pub fn orientation(&self) -> gtk::Orientation {
        self.imp().orientation.get()
    }

    /// Set the orientation of the box.
    pub fn set_orientation(&self, orientation: gtk::Orientation) {
        if self.orientation() == orientation {
            return;
        }

        self.imp().orientation.set(orientation);
        self.notify("orientation");
        self.queue_resize();
    }

    /// The number of children in this box.
    pub fn children_nb(&self) -> usize {
        self.imp().widgets.borrow().len()
    }

    /// Bind a `ListModel` to this box.
    ///
    /// The contents of the box are cleared and then filled with widgets that
    /// represent items from the model. The box is updated whenever the model
    /// changes. If the model is `None`, the box is left empty.
    pub fn bind_model<P: Fn(&glib::Object) -> gtk::Widget + 'static>(
        &self,
        model: Option<&impl glib::IsA<gio::ListModel>>,
        create_widget_func: P,
    ) {
        let imp = self.imp();

        if let Some(bound_model) = imp.bound_model.take() {
            bound_model.disconnect_signals()
        }
        for child in self.imp().widgets.take() {
            child.unparent();
        }
        imp.create_widget_func.take();

        let Some(model) = model else {
            return;
        };

        let signal_handler_id = model.connect_items_changed(
            clone!(@weak self as obj => move |model, position, removed, added| {
                obj.handle_items_changed(model, position, removed, added)
            }),
        );

        imp.bound_model.replace(Some(BoundObjectWeakRef::new(
            model.upcast_ref(),
            vec![signal_handler_id],
        )));

        imp.create_widget_func
            .replace(Some(Box::new(create_widget_func)));

        self.handle_items_changed(model, 0, 0, model.n_items())
    }

    fn handle_items_changed(
        &self,
        model: &impl glib::IsA<gio::ListModel>,
        position: u32,
        mut removed: u32,
        added: u32,
    ) {
        let max_children = self.max_children();
        if max_children != 0 && position >= max_children {
            // No changes here.
            return;
        }

        let imp = self.imp();
        let mut widgets = imp.widgets.borrow_mut();
        let create_widget_func_option = imp.create_widget_func.borrow();
        let create_widget_func = create_widget_func_option.as_ref().unwrap();

        while removed > 0 {
            if position as usize >= widgets.len() {
                break;
            }

            let widget = widgets.remove(position as usize);
            widget.unparent();
            removed -= 1;
        }

        for i in position..(position + added) {
            if max_children != 0 && i >= max_children {
                break;
            }

            let item = model.item(i).unwrap();
            let widget = create_widget_func(&item);
            widget.set_parent(self);
            widgets.insert(i as usize, widget)
        }

        self.queue_resize();
    }
}

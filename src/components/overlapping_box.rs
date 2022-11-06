use gtk::{gdk, glib, prelude::*, subclass::prelude::*};

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[derive(Debug)]
    pub struct OverlappingBox {
        /// The child widgets.
        pub widgets: RefCell<Vec<gtk::Widget>>,

        /// The size of the widgets.
        pub widgets_sizes: RefCell<Vec<(i32, i32)>>,

        /// The size by which the widgets overlap.
        pub overlap: Cell<u32>,

        /// The orientation of the box.
        pub orientation: Cell<gtk::Orientation>,
    }

    impl Default for OverlappingBox {
        fn default() -> Self {
            Self {
                widgets: Default::default(),
                widgets_sizes: Default::default(),
                overlap: Default::default(),
                orientation: gtk::Orientation::Horizontal.into(),
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
                "overlap" => obj.set_overlap(value.get().unwrap()),
                "orientation" => obj.set_orientation(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "overlap" => obj.overlap().to_value(),
                "orientation" => obj.orientation().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            for widget in self.widgets.borrow().iter() {
                widget.unparent();
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

    impl BuildableImpl for OverlappingBox {
        fn add_child(&self, builder: &gtk::Builder, child: &glib::Object, type_: Option<&str>) {
            if let Some(child) = child.downcast_ref::<gtk::Widget>() {
                self.obj().append(child);
            } else {
                self.parent_add_child(builder, child, type_)
            }
        }
    }

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
        glib::Object::new(&[])
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

    /// The children of this box.
    pub fn children(&self) -> Vec<gtk::Widget> {
        self.imp().widgets.borrow().to_owned()
    }

    /// Add a child at the end of this box.
    pub fn append<P: IsA<gtk::Widget>>(&self, child: &P) {
        self.imp().widgets.borrow_mut().push(child.clone().upcast());
        child.set_parent(self);
        self.queue_resize();
    }

    /// Add a child at the beginning of this box.
    pub fn prepend<P: IsA<gtk::Widget>>(&self, child: &P) {
        self.imp()
            .widgets
            .borrow_mut()
            .insert(0, child.clone().upcast());
        child.set_parent(self);
        self.queue_resize();
    }

    /// Remove the child at the given index.
    pub fn remove(&self, index: usize) {
        let child = self.imp().widgets.borrow_mut().remove(index);
        child.unparent();
        self.queue_resize();
    }

    /// Only keep the first `len` children and drop the rest.
    pub fn truncate_children(&self, len: usize) {
        let children = self.imp().widgets.borrow_mut().split_off(len);
        for child in children {
            child.unparent();
        }
        self.queue_resize();
    }

    /// Remove all the children of this box.
    pub fn remove_all(&self) {
        for child in self.imp().widgets.take() {
            child.unparent();
        }
        self.queue_resize();
    }
}

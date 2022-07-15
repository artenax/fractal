use std::cmp::max;

use gtk::{glib, glib::clone, pango, prelude::*, subclass::prelude::*};

pub const DEFAULT_PLACEHOLDER: &str = "<widget>";
const PANGO_SCALE: i32 = 1024;
const OBJECT_REPLACEMENT_CHARACTER: &str = "\u{FFFC}";
fn pango_pixels(d: i32) -> i32 {
    (d + 512) >> 10
}

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct LabelWithWidgets {
        pub widgets: RefCell<Vec<gtk::Widget>>,
        pub widgets_sizes: RefCell<Vec<(i32, i32)>>,
        pub label: gtk::Label,
        pub placeholder: RefCell<Option<String>>,
        pub text: RefCell<Option<String>>,
        pub ellipsize: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LabelWithWidgets {
        const NAME: &'static str = "LabelWithWidgets";
        type Type = super::LabelWithWidgets;
        type ParentType = gtk::Widget;
        type Interfaces = (gtk::Buildable,);
    }

    impl ObjectImpl for LabelWithWidgets {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::new(
                        "label",
                        "Label",
                        "The label",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecString::new(
                        "placeholder",
                        "Placeholder",
                        "The placeholder that is replaced with widgets",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecString::new(
                        "use-markup",
                        "Use Markup",
                        "Whether the label's text is interpreted as Pango markup.",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecString::new(
                        "ellipsize",
                        "Ellipsize",
                        "Whether the label's text should be ellipsized.",
                        None,
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
                "label" => obj.set_label(value.get().unwrap()),
                "placeholder" => obj.set_placeholder(value.get().unwrap()),
                "use-markup" => obj.set_use_markup(value.get().unwrap()),
                "ellipsize" => obj.set_ellipsize(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "label" => obj.label().to_value(),
                "placeholder" => obj.placeholder().to_value(),
                "use-markup" => obj.uses_markup().to_value(),
                "ellipsize" => obj.ellipsize().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            let label = &self.label;
            label.set_parent(obj);
            label.set_wrap(true);
            label.set_wrap_mode(pango::WrapMode::WordChar);
            label.set_xalign(0.0);
            label.set_valign(gtk::Align::Start);
            label.add_css_class("line-height");
            label.connect_notify_local(
                Some("label"),
                clone!(@weak obj => move |_, _| {
                    obj.invalidate_child_widgets();
                }),
            );
        }

        fn dispose(&self, _obj: &Self::Type) {
            self.label.unparent();
            for widget in self.widgets.borrow().iter() {
                widget.unparent();
            }
        }
    }

    impl WidgetImpl for LabelWithWidgets {
        fn measure(
            &self,
            _widget: &Self::Type,
            orientation: gtk::Orientation,
            for_size: i32,
        ) -> (i32, i32, i32, i32) {
            let (mut minimum, mut natural, mut minimum_baseline, mut natural_baseline) =
                if self.label.should_layout() {
                    self.label.measure(orientation, for_size)
                } else {
                    (-1, -1, -1, -1)
                };

            for child in self.widgets.borrow().iter() {
                if self.label.should_layout() {
                    let (child_min, child_nat, child_min_baseline, child_nat_baseline) =
                        child.measure(orientation, for_size);

                    minimum = max(minimum, child_min);
                    natural = max(natural, child_nat);

                    if child_min_baseline > -1 {
                        minimum_baseline = max(minimum_baseline, child_min_baseline);
                    }
                    if child_nat_baseline > -1 {
                        natural_baseline = max(natural_baseline, child_nat_baseline);
                    }
                }
            }
            (minimum, natural, minimum_baseline, natural_baseline)
        }

        fn size_allocate(&self, widget: &Self::Type, width: i32, height: i32, baseline: i32) {
            // The order of the widget allocation is important.
            widget.allocate_shapes();
            self.label.allocate(width, height, baseline, None);
            widget.allocate_children();
        }

        fn request_mode(&self, _widget: &Self::Type) -> gtk::SizeRequestMode {
            self.label.request_mode()
        }
    }

    impl BuildableImpl for LabelWithWidgets {
        fn add_child(
            &self,
            buildable: &Self::Type,
            builder: &gtk::Builder,
            child: &glib::Object,
            type_: Option<&str>,
        ) {
            if let Some(child) = child.downcast_ref::<gtk::Widget>() {
                buildable.append_child(child);
            } else {
                self.parent_add_child(buildable, builder, child, type_)
            }
        }
    }
}

glib::wrapper! {
    /// A Label that can have multiple widgets placed inside the text.
    ///
    /// By default the string "<widget>" will be used as location to place the
    /// child widgets. You can set your own placeholder if you need.
    pub struct LabelWithWidgets(ObjectSubclass<imp::LabelWithWidgets>)
        @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable;
}

impl LabelWithWidgets {
    /// Create an empty `LabelWithWidget`.
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create LabelWithWidgets")
    }

    /// Create a `LabelWithWidget` with the given label and widgets.
    pub fn with_label_and_widgets<P: IsA<gtk::Widget>>(label: &str, widgets: Vec<P>) -> Self {
        let obj: Self =
            glib::Object::new(&[("label", &label)]).expect("Failed to create LabelWithWidgets");
        // FIXME: use a property for widgets
        obj.set_widgets(widgets);
        obj
    }

    pub fn append_child<P: IsA<gtk::Widget>>(&self, child: &P) {
        self.imp().widgets.borrow_mut().push(child.clone().upcast());
        child.set_parent(self);
        self.invalidate_child_widgets();
    }

    pub fn set_widgets<P: IsA<gtk::Widget>>(&self, widgets: Vec<P>) {
        let priv_ = self.imp();

        for widget in priv_.widgets.take() {
            widget.unparent();
        }

        priv_
            .widgets
            .borrow_mut()
            .append(&mut widgets.into_iter().map(|w| w.upcast()).collect());

        for child in priv_.widgets.borrow().iter() {
            child.set_parent(self);
        }
        self.invalidate_child_widgets();
    }

    pub fn widgets(&self) -> Vec<gtk::Widget> {
        self.imp().widgets.borrow().to_owned()
    }

    pub fn set_label(&self, label: Option<String>) {
        let priv_ = self.imp();

        if priv_.text.borrow().as_ref() == label.as_ref() {
            return;
        }

        priv_.text.replace(label);
        self.update_label();
        self.notify("label");
    }

    pub fn label(&self) -> Option<String> {
        self.imp().text.borrow().to_owned()
    }

    pub fn set_placeholder(&self, placeholder: Option<String>) {
        let priv_ = self.imp();

        if priv_.placeholder.borrow().as_ref() == placeholder.as_ref() {
            return;
        }

        priv_.placeholder.replace(placeholder);
        self.update_label();
        self.notify("placeholder");
    }

    pub fn placeholder(&self) -> Option<String> {
        self.imp().placeholder.borrow().to_owned()
    }

    fn invalidate_child_widgets(&self) {
        self.imp().widgets_sizes.borrow_mut().clear();
        self.queue_resize();
    }

    fn allocate_shapes(&self) {
        let priv_ = self.imp();
        let mut widgets_sizes = priv_.widgets_sizes.borrow_mut();

        let mut child_size_changed = false;
        for (i, child) in priv_.widgets.borrow().iter().enumerate() {
            let (_, natural_size) = child.preferred_size();
            let width = natural_size.width();
            let height = natural_size.height();
            if let Some((old_width, old_height)) = widgets_sizes.get(i) {
                if old_width != &width || old_height != &height {
                    let _ = std::mem::replace(&mut widgets_sizes[i], (width, height));
                    child_size_changed = true;
                }
            } else {
                widgets_sizes.insert(i, (width, height));
                child_size_changed = true;
            }
        }

        if !child_size_changed {
            return;
        }

        let attrs = pango::AttrList::new();
        for (i, (start_index, _)) in priv_
            .label
            .text()
            .as_str()
            .match_indices(OBJECT_REPLACEMENT_CHARACTER)
            .enumerate()
        {
            if let Some((width, height)) = widgets_sizes.get(i) {
                let logical_rect = pango::Rectangle::new(
                    0,
                    -(height - (height / 4)) * PANGO_SCALE,
                    width * PANGO_SCALE,
                    height * PANGO_SCALE,
                );

                let mut shape = pango::AttrShape::new(&logical_rect, &logical_rect);
                shape.set_start_index(start_index as u32);
                shape.set_end_index((start_index + OBJECT_REPLACEMENT_CHARACTER.len()) as u32);
                attrs.insert(shape);
            } else {
                break;
            }
        }
        priv_.label.set_attributes(Some(&attrs));
    }

    fn allocate_children(&self) {
        let priv_ = self.imp();
        let widgets = priv_.widgets.borrow();
        let widgets_sizes = priv_.widgets_sizes.borrow();

        let mut run_iter = priv_.label.layout().iter().unwrap();
        let mut i = 0;
        loop {
            if let Some(run) = run_iter.run_readonly() {
                if run
                    .item()
                    .analysis()
                    .extra_attrs()
                    .iter()
                    .any(|attr| attr.type_() == pango::AttrType::Shape)
                {
                    if let Some(widget) = widgets.get(i) {
                        let (width, height) = widgets_sizes[i];
                        let (_, extents) = run_iter.run_extents();

                        let (offset_x, offset_y) = priv_.label.layout_offsets();
                        let allocation = gtk::Allocation::new(
                            pango_pixels(extents.x()) + offset_x,
                            pango_pixels(extents.y()) + offset_y,
                            width,
                            height,
                        );
                        widget.size_allocate(&allocation, -1);
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            if !run_iter.next_run() {
                break;
            }
        }
    }

    pub fn uses_markup(&self) -> bool {
        self.imp().label.uses_markup()
    }

    /// Sets whether the text of the label contains markup.
    pub fn set_use_markup(&self, use_markup: bool) {
        self.imp().label.set_use_markup(use_markup);
    }

    /// Whether the text of the label is ellipsized.
    pub fn ellipsize(&self) -> bool {
        self.imp().ellipsize.get()
    }

    /// Sets whether the text of the label should be ellipsized.
    pub fn set_ellipsize(&self, ellipsize: bool) {
        if self.ellipsize() == ellipsize {
            return;
        }

        self.imp().ellipsize.set(true);
        self.update_label();
        self.notify("ellipsize");
    }

    fn update_label(&self) {
        let priv_ = self.imp();
        if self.ellipsize() {
            // Workaround: if both wrap and ellipsize are set, and there are
            // widgets inserted, GtkLabel reports an erroneous minimum width.
            priv_.label.set_wrap(false);
            priv_.label.set_ellipsize(pango::EllipsizeMode::End);

            if let Some(label) = priv_.text.borrow().as_ref() {
                let placeholder = priv_.placeholder.borrow();
                let placeholder = placeholder.as_deref().unwrap_or(DEFAULT_PLACEHOLDER);
                let label = label.replace(placeholder, OBJECT_REPLACEMENT_CHARACTER);
                let label = if let Some(pos) = label.find('\n') {
                    format!("{}…", &label[0..pos])
                } else {
                    label
                };
                priv_.label.set_label(&label);
            }
        } else {
            priv_.label.set_wrap(true);
            priv_.label.set_ellipsize(pango::EllipsizeMode::None);

            if let Some(label) = priv_.text.borrow().as_ref() {
                let placeholder = priv_.placeholder.borrow();
                let placeholder = placeholder.as_deref().unwrap_or(DEFAULT_PLACEHOLDER);
                let label = label.replace(placeholder, OBJECT_REPLACEMENT_CHARACTER);
                priv_.label.set_label(&label);
            }
        }
        self.invalidate_child_widgets();
    }
}

impl Default for LabelWithWidgets {
    fn default() -> Self {
        Self::new()
    }
}

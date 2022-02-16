use gtk::{glib, subclass::prelude::*};

type WidgetBuilderFn = Box<dyn Fn(&super::Toast) -> Option<gtk::Widget> + 'static>;

mod imp {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    pub struct Toast {
        pub widget_builder: RefCell<Option<WidgetBuilderFn>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Toast {
        const NAME: &'static str = "ComponentsToast";
        type Type = super::Toast;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for Toast {}
}

glib::wrapper! {
    /// A `Toast` that can be shown in the UI.
    pub struct Toast(ObjectSubclass<imp::Toast>);
}

impl Toast {
    pub fn new<F: Fn(&Self) -> Option<gtk::Widget> + 'static>(f: F) -> Self {
        let obj: Self = glib::Object::new(&[]).expect("Failed to create Toast");
        obj.set_widget_builder(f);
        obj
    }

    /// Set a function that builds the widget used to display this error in the
    /// UI
    pub fn set_widget_builder<F: Fn(&Self) -> Option<gtk::Widget> + 'static>(&self, f: F) {
        self.imp().widget_builder.replace(Some(Box::new(f)));
    }

    /// Produces a widget via the function set in `Self::set_widget_builder()`
    pub fn widget(&self) -> Option<gtk::Widget> {
        let widget_builder = self.imp().widget_builder.borrow();
        let widget_builder = widget_builder.as_ref()?;
        widget_builder(self)
    }
}

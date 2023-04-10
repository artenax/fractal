use gtk::{glib, prelude::*, subclass::prelude::*};

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Spinner {
        inner: gtk::Spinner,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Spinner {
        const NAME: &'static str = "Spinner";
        type Type = super::Spinner;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }
    }

    impl ObjectImpl for Spinner {
        fn constructed(&self) {
            self.parent_constructed();
            self.inner.set_parent(&*self.obj());
        }

        fn dispose(&self) {
            self.inner.unparent();
        }
    }

    impl WidgetImpl for Spinner {
        fn map(&self) {
            self.parent_map();
            self.inner.start();
        }

        fn unmap(&self) {
            self.inner.stop();
            self.parent_unmap();
        }
    }
}

glib::wrapper! {
    pub struct Spinner(ObjectSubclass<imp::Spinner>)
        @extends gtk::Widget;
}

impl Default for Spinner {
    fn default() -> Self {
        glib::Object::new()
    }
}

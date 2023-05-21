use adw::{prelude::*, subclass::prelude::*};
use gtk::{glib, CompositeTemplate};

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/components/toastable_window.ui")]
    pub struct ToastableWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ToastableWindow {
        const NAME: &'static str = "ToastableWindow";
        const ABSTRACT: bool = true;
        type Type = super::ToastableWindow;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ToastableWindow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<gtk::Widget>("child-content").build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "child-content" => obj.set_child_content(value.get().ok().as_ref()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "child-content" => obj.child_content().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for ToastableWindow {}
    impl WindowImpl for ToastableWindow {}
    impl AdwWindowImpl for ToastableWindow {}
}

glib::wrapper! {
    /// A window that can display toasts.
    pub struct ToastableWindow(ObjectSubclass<imp::ToastableWindow>)
        @extends gtk::Widget, gtk::Window, adw::Window, gtk::Root, @implements gtk::Accessible;
}

pub trait ToastableWindowExt: 'static {
    /// Get the content of this window.
    fn child_content(&self) -> Option<gtk::Widget>;

    /// Set content of this window.
    ///
    /// Use this instead of `set_child` or `set_content`, otherwise it will
    /// panic.
    fn set_child_content(&self, content: Option<&gtk::Widget>);

    /// Add a toast.
    fn add_toast(&self, toast: adw::Toast);
}

impl<O: IsA<ToastableWindow>> ToastableWindowExt for O {
    fn child_content(&self) -> Option<gtk::Widget> {
        self.upcast_ref().imp().toast_overlay.child()
    }

    fn set_child_content(&self, content: Option<&gtk::Widget>) {
        self.upcast_ref().imp().toast_overlay.set_child(content);
    }

    fn add_toast(&self, toast: adw::Toast) {
        self.upcast_ref().imp().toast_overlay.add_toast(toast);
    }
}

/// Public trait that must be implemented for everything that derives from
/// `ToastableWindow`.
pub trait ToastableWindowImpl: adw::subclass::prelude::WindowImpl {}

unsafe impl<T> IsSubclassable<T> for ToastableWindow where T: ToastableWindowImpl {}

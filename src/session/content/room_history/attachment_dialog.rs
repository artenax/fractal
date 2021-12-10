use gtk::{gdk, gio, glib, prelude::*, subclass::prelude::*, CompositeTemplate};
use once_cell::sync::Lazy;

mod imp {
    use std::cell::RefCell;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/attachment-dialog.ui")]
    pub struct AttachmentDialog {
        pub file: RefCell<Option<gio::File>>,
        pub texture: RefCell<Option<gdk::Texture>>,
        #[template_child]
        pub preview: TemplateChild<gtk::Image>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AttachmentDialog {
        const NAME: &'static str = "AttachmentDialog";
        type Type = super::AttachmentDialog;
        type ParentType = gtk::Window;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("attachment-dialog.send", None, move |window, _, _| {
                window.emit_by_name::<()>("send", &[]);
                window.close();
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AttachmentDialog {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: Lazy<Vec<glib::subclass::Signal>> = Lazy::new(|| {
                vec![
                    glib::subclass::Signal::builder("send", &[], glib::Type::UNIT.into())
                        .flags(glib::SignalFlags::RUN_FIRST)
                        .build(),
                ]
            });
            SIGNALS.as_ref()
        }
    }
    impl WidgetImpl for AttachmentDialog {}
    impl WindowImpl for AttachmentDialog {}
}

glib::wrapper! {
    pub struct AttachmentDialog(ObjectSubclass<imp::AttachmentDialog>)
        @extends gtk::Widget, gtk::Window;
}

impl AttachmentDialog {
    pub fn new(window: &gtk::Window) -> Self {
        glib::Object::new(&[("transient-for", window)]).unwrap()
    }

    pub fn set_texture(&self, texture: &gdk::Texture) {
        let priv_ = self.imp();
        priv_.stack.set_visible_child_name("preview");

        priv_
            .preview
            .set_paintable(Some(texture.upcast_ref::<gdk::Paintable>()));
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later
use gtk::{gdk, glib, glib::subclass, prelude::*, subclass::prelude::*};
use matrix_sdk::encryption::verification::QrVerificationData;

mod camera;
mod camera_paintable;
mod qr_code_detector;

pub use camera::Camera;

mod imp {
    use std::cell::RefCell;

    use adw::subclass::prelude::*;
    use gtk::CompositeTemplate;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(resource = "/org/gnome/Fractal/qr-code-scanner.ui")]
    pub struct QrCodeScanner {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub picture: TemplateChild<gtk::Picture>,
        pub handler: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for QrCodeScanner {
        const NAME: &'static str = "QrCodeScanner";
        type Type = super::QrCodeScanner;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }
    impl ObjectImpl for QrCodeScanner {
        fn signals() -> &'static [subclass::Signal] {
            static SIGNALS: Lazy<Vec<subclass::Signal>> = Lazy::new(|| {
                vec![subclass::Signal::builder("code-detected")
                    .param_types([QrVerificationDataBoxed::static_type()])
                    .run_first()
                    .build()]
            });
            SIGNALS.as_ref()
        }
    }
    impl WidgetImpl for QrCodeScanner {
        fn unmap(&self) {
            self.parent_unmap();
            self.obj().stop();
        }
    }
    impl BinImpl for QrCodeScanner {}
}

glib::wrapper! {
    pub struct QrCodeScanner(ObjectSubclass<imp::QrCodeScanner>) @extends gtk::Widget, adw::Bin;
}

impl QrCodeScanner {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    pub fn stop(&self) {
        let imp = self.imp();

        if let Some(paintable) = imp.picture.paintable() {
            imp.picture.set_paintable(gdk::Paintable::NONE);
            if let Some(handler) = imp.handler.take() {
                paintable.disconnect(handler);
            }
        }
    }

    pub async fn start(&self) {
        let imp = self.imp();
        let camera = camera::Camera::default();

        if let Some(paintable) = camera.paintable().await {
            self.stop();

            imp.picture.set_paintable(Some(&paintable));

            let callback = glib::clone!(@weak self as obj => @default-return None, move |args: &[glib::Value]| {
                let code = args.get(1).unwrap().get::<QrVerificationDataBoxed>().unwrap();
                obj.emit_by_name::<()>("code-detected", &[&code]);

                None
            });
            let handler = paintable.connect_local("code-detected", false, callback);

            imp.handler.replace(Some(handler));
            imp.stack.set_visible_child_name("camera");
        } else {
            imp.stack.set_visible_child_name("no-camera");
        }
    }

    pub fn connect_code_detected<F: Fn(&Self, QrVerificationData) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("code-detected", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            let data = values[1].get::<QrVerificationDataBoxed>().unwrap();

            f(&obj, data.0);

            None
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, glib::Boxed)]
#[boxed_type(name = "QrVerificationDataBoxed")]
struct QrVerificationDataBoxed(QrVerificationData);

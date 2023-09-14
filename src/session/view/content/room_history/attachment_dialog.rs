use std::cell::Cell;

use gtk::{gdk, gio, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};
use once_cell::sync::Lazy;

use crate::components::MediaContentViewer;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_history/attachment_dialog.ui"
    )]
    pub struct AttachmentDialog {
        pub send: Cell<bool>,
        #[template_child]
        pub send_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub media: TemplateChild<MediaContentViewer>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AttachmentDialog {
        const NAME: &'static str = "AttachmentDialog";
        type Type = super::AttachmentDialog;
        type ParentType = gtk::Window;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("attachment-dialog.send", None, move |window, _, _| {
                window.imp().send.set(true);
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
            static SIGNALS: Lazy<Vec<glib::subclass::Signal>> =
                Lazy::new(|| vec![glib::subclass::Signal::builder("send").run_first().build()]);
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
    pub fn for_image(transient_for: &gtk::Window, title: &str, image: &gdk::Texture) -> Self {
        let obj: Self = glib::Object::builder()
            .property("transient-for", transient_for)
            .property("title", title)
            .build();
        obj.imp().media.view_image(image);
        obj.imp().send_button.grab_focus();
        obj
    }

    pub fn for_file(transient_for: &gtk::Window, title: &str, file: &gio::File) -> Self {
        let obj: Self = glib::Object::builder()
            .property("transient-for", transient_for)
            .property("title", title)
            .build();
        obj.imp().media.view_file(file.to_owned());
        obj.imp().send_button.grab_focus();
        obj
    }

    pub fn for_location(
        transient_for: &gtk::Window,
        title: &str,
        geo_uri: &geo_uri::GeoUri,
    ) -> Self {
        let obj: Self = glib::Object::builder()
            .property("transient-for", transient_for)
            .property("title", title)
            .build();
        obj.imp().media.view_location(geo_uri);
        obj.imp().send_button.grab_focus();
        obj
    }

    /// Show the dialog asynchronously.
    ///
    /// Returns `gtk::ResponseType::Ok` if the user clicked on send, otherwise
    /// returns `gtk::ResponseType::Cancel`.
    pub async fn run_future(&self) -> gtk::ResponseType {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let sender = Cell::new(Some(sender));

        let handler_id = self.connect_close_request(
            clone!(@weak self as obj => @default-return glib::Propagation::Proceed, move |_| {
                if let Some(sender) = sender.take() {
                    let response = if obj.imp().send.get() {
                        gtk::ResponseType::Ok
                    } else {
                        gtk::ResponseType::Cancel
                    };

                    sender.send(response).unwrap();
                }
                glib::Propagation::Proceed
            }),
        );

        self.present();
        let res = receiver.await.unwrap();

        self.disconnect(handler_id);

        res
    }
}

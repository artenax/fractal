use adw::{prelude::*, subclass::prelude::*};
use gtk::{gdk, gio, glib, glib::clone, CompositeTemplate};
use log::warn;
use matrix_sdk::ruma::events::{room::message::MessageType, AnyMessageLikeEventContent};

use super::room::EventActions;
use crate::{
    components::{ContentType, MediaContentViewer},
    session::room::SupportedEvent,
    spawn,
    utils::cache_dir,
    Window,
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{object::WeakRef, subclass::InitializingObject};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/media-viewer.ui")]
    pub struct MediaViewer {
        pub fullscreened: Cell<bool>,
        pub event: RefCell<Option<WeakRef<SupportedEvent>>>,
        pub body: RefCell<Option<String>>,
        #[template_child]
        pub flap: TemplateChild<adw::Flap>,
        #[template_child]
        pub menu: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub media: TemplateChild<MediaContentViewer>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MediaViewer {
        const NAME: &'static str = "MediaViewer";
        type Type = super::MediaViewer;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);

            klass.install_action("media-viewer.close", None, move |obj, _, _| {
                if obj.fullscreened() {
                    obj.activate_action("win.toggle-fullscreen", None).unwrap();
                }

                obj.imp().media.stop_playback();
                obj.activate_action("session.show-content", None).unwrap();
            });
            klass.add_binding_action(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                "media-viewer.close",
                None,
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MediaViewer {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::new(
                        "fullscreened",
                        "Fullscreened",
                        "Whether the viewer is fullscreen",
                        false,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecObject::new(
                        "event",
                        "Event",
                        "The media event to display",
                        SupportedEvent::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecString::new(
                        "body",
                        "Body",
                        "The body of the media event",
                        None,
                        glib::ParamFlags::READABLE,
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
                "fullscreened" => obj.set_fullscreened(value.get().unwrap()),
                "event" => obj.set_event(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "fullscreened" => obj.fullscreened().to_value(),
                "event" => obj.event().to_value(),
                "body" => obj.body().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.menu
                .set_menu_model(Some(Self::Type::event_media_menu_model()));

            // Bind `fullscreened` to the window property of the same name.
            obj.connect_notify_local(Some("root"), |obj, _| {
                if let Some(window) = obj.root().and_then(|root| root.downcast::<Window>().ok()) {
                    window
                        .bind_property("fullscreened", obj, "fullscreened")
                        .flags(glib::BindingFlags::SYNC_CREATE)
                        .build();
                }
            });
        }
    }

    impl WidgetImpl for MediaViewer {}
    impl BinImpl for MediaViewer {}
}

glib::wrapper! {
    pub struct MediaViewer(ObjectSubclass<imp::MediaViewer>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl MediaViewer {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create MediaViewer")
    }

    pub fn event(&self) -> Option<SupportedEvent> {
        self.imp()
            .event
            .borrow()
            .as_ref()
            .and_then(|event| event.upgrade())
    }

    pub fn set_event(&self, event: Option<SupportedEvent>) {
        if event == self.event() {
            return;
        }

        self.imp()
            .event
            .replace(event.map(|event| event.downgrade()));
        self.build();
        self.notify("event");
    }

    pub fn body(&self) -> Option<String> {
        self.imp().body.borrow().clone()
    }

    pub fn set_body(&self, body: Option<String>) {
        if body == self.body() {
            return;
        }

        self.imp().body.replace(body);
        self.notify("body");
    }

    pub fn fullscreened(&self) -> bool {
        self.imp().fullscreened.get()
    }

    pub fn set_fullscreened(&self, fullscreened: bool) {
        let priv_ = self.imp();

        if fullscreened == self.fullscreened() {
            return;
        }

        priv_.fullscreened.set(fullscreened);

        if fullscreened {
            // Upscale the media on fullscreen
            priv_.media.set_halign(gtk::Align::Fill);
            priv_.flap.set_fold_policy(adw::FlapFoldPolicy::Always);
        } else {
            priv_.media.set_halign(gtk::Align::Center);
            priv_.flap.set_fold_policy(adw::FlapFoldPolicy::Never);
        }

        self.notify("fullscreened");
    }

    fn build(&self) {
        self.imp().media.show_loading();

        if let Some(event) = self.event() {
            self.set_event_actions(Some(event.upcast_ref()));
            if let Some(AnyMessageLikeEventContent::RoomMessage(content)) = event.content() {
                match content.msgtype {
                    MessageType::Image(image) => {
                        self.set_body(Some(image.body));

                        spawn!(
                            glib::PRIORITY_LOW,
                            clone!(@weak self as obj => async move {
                                let priv_ = obj.imp();

                                match event.get_media_content().await {
                                    Ok((_, _, data)) => {
                                        match gdk::Texture::from_bytes(&glib::Bytes::from(&data)) {
                                            Ok(texture) => {
                                                priv_.media.view_image(&texture);
                                                return;
                                            }
                                            Err(error) => warn!("Could not load GdkTexture from file: {}", error),
                                        }
                                    }
                                    Err(error) => warn!("Could not retrieve image file: {}", error),
                                }

                                priv_.media.show_fallback(ContentType::Image);
                            })
                        );
                    }
                    MessageType::Video(video) => {
                        self.set_body(Some(video.body));

                        spawn!(
                            glib::PRIORITY_LOW,
                            clone!(@weak self as obj => async move {
                                let priv_ = obj.imp();

                                match event.get_media_content().await {
                                    Ok((uid, filename, data)) => {
                                        // The GStreamer backend of GtkVideo doesn't work with input streams so
                                        // we need to store the file.
                                        // See: https://gitlab.gnome.org/GNOME/gtk/-/issues/4062
                                        let mut path = cache_dir();
                                        path.push(format!("{}_{}", uid, filename));
                                        let file = gio::File::for_path(path);
                                        file.replace_contents(
                                            &data,
                                            None,
                                            false,
                                            gio::FileCreateFlags::REPLACE_DESTINATION,
                                            gio::Cancellable::NONE,
                                        )
                                        .unwrap();

                                        priv_.media.view_file(file);
                                    }
                                    Err(error) => {
                                        warn!("Could not retrieve video file: {}", error);
                                        priv_.media.show_fallback(ContentType::Video);
                                    }
                                }
                            })
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    fn reveal_headerbar(&self, reveal: bool) {
        if self.fullscreened() {
            self.imp().flap.set_reveal_flap(reveal);
        }
    }

    #[template_callback]
    fn handle_motion(&self, _x: f64, y: f64) {
        if y <= 50.0 {
            self.reveal_headerbar(true);
        }
    }

    #[template_callback]
    fn handle_touch(&self) {
        self.reveal_headerbar(true);
    }

    #[template_callback]
    fn handle_click(&self, n_pressed: i32) {
        if n_pressed == 2 {
            self.activate_action("win.toggle-fullscreen", None).unwrap();
        }
    }
}

impl EventActions for MediaViewer {}

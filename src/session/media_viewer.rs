use adw::{prelude::*, subclass::prelude::*};
use gtk::{gdk, gio, glib, glib::clone, graphene, CompositeTemplate};
use log::warn;
use matrix_sdk::{room::timeline::TimelineItemContent, ruma::events::room::message::MessageType};

use super::room::{EventActions, EventTexture};
use crate::{
    components::{ContentType, ImagePaintable, MediaContentViewer, ScaleRevealer},
    session::room::Event,
    spawn,
    utils::cache_dir,
    Window,
};

const ANIMATION_DURATION: u32 = 250;
const CANCEL_SWIPE_ANIMATION_DURATION: u32 = 400;

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{object::WeakRef, subclass::InitializingObject};
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/media-viewer.ui")]
    pub struct MediaViewer {
        pub fullscreened: Cell<bool>,
        pub event: WeakRef<Event>,
        pub body: RefCell<Option<String>>,
        pub animation: OnceCell<adw::TimedAnimation>,
        pub swipe_tracker: OnceCell<adw::SwipeTracker>,
        pub swipe_progress: Cell<f64>,
        #[template_child]
        pub flap: TemplateChild<adw::Flap>,
        #[template_child]
        pub header_bar: TemplateChild<gtk::HeaderBar>,
        #[template_child]
        pub menu: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub revealer: TemplateChild<ScaleRevealer>,
        #[template_child]
        pub media: TemplateChild<MediaContentViewer>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MediaViewer {
        const NAME: &'static str = "MediaViewer";
        type Type = super::MediaViewer;
        type ParentType = gtk::Widget;
        type Interfaces = (adw::Swipeable,);

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);

            klass.set_css_name("media-viewer");
            klass.install_action("media-viewer.close", None, move |obj, _, _| {
                obj.close();
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
                    glib::ParamSpecBoolean::builder("fullscreened")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<Event>("event")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecString::builder("body").read_only().build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "fullscreened" => obj.set_fullscreened(value.get().unwrap()),
                "event" => obj.set_event(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "fullscreened" => obj.fullscreened().to_value(),
                "event" => obj.event().to_value(),
                "body" => obj.body().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            let target = adw::CallbackAnimationTarget::new(clone!(@weak obj => move |value| {
                // This is needed to fade the header bar content
                obj.imp().header_bar.set_opacity(value);

                obj.queue_draw();
            }));
            let animation = adw::TimedAnimation::new(&*obj, 0.0, 1.0, ANIMATION_DURATION, target);
            self.animation.set(animation).unwrap();

            let swipe_tracker = adw::SwipeTracker::new(&*obj);
            swipe_tracker.set_orientation(gtk::Orientation::Vertical);
            swipe_tracker.connect_update_swipe(clone!(@weak obj => move |_, progress| {
                obj.imp().header_bar.set_opacity(0.0);
                obj.imp().swipe_progress.set(progress);
                obj.queue_allocate();
                obj.queue_draw();
            }));
            swipe_tracker.connect_end_swipe(clone!(@weak obj => move |_, _, to| {
                if to == 0.0 {
                    let target = adw::CallbackAnimationTarget::new(clone!(@weak obj => move |value| {
                        obj.imp().swipe_progress.set(value);
                        obj.queue_allocate();
                        obj.queue_draw();
                    }));
                    let swipe_progress = obj.imp().swipe_progress.get();
                    let animation = adw::TimedAnimation::new(
                        &obj,
                        swipe_progress,
                        0.0,
                        CANCEL_SWIPE_ANIMATION_DURATION,
                        target,
                    );
                    animation.set_easing(adw::Easing::EaseOutCubic);
                    animation.connect_done(clone!(@weak obj => move |_| {
                        obj.imp().header_bar.set_opacity(1.0);
                    }));
                    animation.play();
                } else {
                    obj.close();
                    obj.imp().header_bar.set_opacity(1.0);
                }
            }));
            self.swipe_tracker.set(swipe_tracker).unwrap();

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

            self.revealer
                .connect_transition_done(clone!(@weak obj => move |revealer| {
                    if !revealer.reveals_child() {
                        obj.set_visible(false);
                    }
                }));
        }

        fn dispose(&self) {
            self.flap.unparent();
        }
    }

    impl WidgetImpl for MediaViewer {
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let swipe_y_offset = -height as f64 * self.swipe_progress.get();
            let allocation = gtk::Allocation::new(0, swipe_y_offset as i32, width, height);
            self.flap.size_allocate(&allocation, baseline);
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let progress = {
                let swipe_progress = 1.0 - self.swipe_progress.get().abs();
                let animation_progress = self.animation.get().unwrap().value();
                swipe_progress.min(animation_progress)
            };

            if progress > 0.0 {
                let background_color = gdk::RGBA::new(0.0, 0.0, 0.0, 1.0 * progress as f32);
                let bounds = graphene::Rect::new(0.0, 0.0, obj.width() as f32, obj.height() as f32);
                snapshot.append_color(&background_color, &bounds);
            }

            obj.snapshot_child(&*self.flap, snapshot);
        }
    }

    impl SwipeableImpl for MediaViewer {
        fn cancel_progress(&self) -> f64 {
            0.0
        }

        fn distance(&self) -> f64 {
            self.obj().height() as f64
        }

        fn progress(&self) -> f64 {
            self.swipe_progress.get()
        }

        fn snap_points(&self) -> Vec<f64> {
            vec![-1.0, 0.0, 1.0]
        }

        fn swipe_area(&self, _: adw::NavigationDirection, _: bool) -> gdk::Rectangle {
            gdk::Rectangle::new(0, 0, self.obj().width(), self.obj().height())
        }
    }
}

glib::wrapper! {
    pub struct MediaViewer(ObjectSubclass<imp::MediaViewer>)
        @extends gtk::Widget, @implements gtk::Accessible, adw::Swipeable;
}

#[gtk::template_callbacks]
impl MediaViewer {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Reveal this widget by transitioning from `source_widget`.
    pub fn reveal(&self, source_widget: &impl IsA<gtk::Widget>) {
        let imp = self.imp();

        self.set_visible(true);

        imp.swipe_progress.set(0.0);
        imp.revealer.set_source_widget(Some(source_widget));
        imp.revealer.set_reveal_child(true);

        let animation = imp.animation.get().unwrap();
        animation.set_value_from(animation.value());
        animation.set_value_to(1.0);
        animation.play();
    }

    /// The media event to display.
    pub fn event(&self) -> Option<Event> {
        self.imp().event.upgrade()
    }

    /// Set the media event to display.
    pub fn set_event(&self, event: Option<Event>) {
        if event == self.event() {
            return;
        }

        self.imp().event.set(event.as_ref());
        self.build();
        self.notify("event");
    }

    /// The body of the media event.
    pub fn body(&self) -> Option<String> {
        self.imp().body.borrow().clone()
    }

    /// Set the body of the media event.
    fn set_body(&self, body: Option<String>) {
        if body == self.body() {
            return;
        }

        self.imp().body.replace(body);
        self.notify("body");
    }

    /// Whether the viewer is fullscreened.
    pub fn fullscreened(&self) -> bool {
        self.imp().fullscreened.get()
    }

    /// Set whether the viewer is fullscreened.
    pub fn set_fullscreened(&self, fullscreened: bool) {
        let imp = self.imp();

        if fullscreened == self.fullscreened() {
            return;
        }

        imp.fullscreened.set(fullscreened);

        if fullscreened {
            // Upscale the media on fullscreen
            imp.media.set_halign(gtk::Align::Fill);
            imp.flap.set_fold_policy(adw::FlapFoldPolicy::Always);
        } else {
            imp.media.set_halign(gtk::Align::Center);
            imp.flap.set_fold_policy(adw::FlapFoldPolicy::Never);
        }

        self.notify("fullscreened");
    }

    fn build(&self) {
        self.imp().media.show_loading();

        if let Some(event) = self.event() {
            self.set_event_actions(Some(event.upcast_ref()));
            if let TimelineItemContent::Message(content) = event.content() {
                match content.msgtype() {
                    MessageType::Image(image) => {
                        let image = image.clone();
                        self.set_body(Some(image.body));

                        spawn!(
                            glib::PRIORITY_LOW,
                            clone!(@weak self as obj => async move {
                                let imp = obj.imp();

                                match event.get_media_content().await {
                                    Ok((_, _, data)) => {
                                        match ImagePaintable::from_bytes(&glib::Bytes::from(&data), image.info.and_then(|info| info.mimetype).as_deref()) {
                                            Ok(texture) => {
                                                imp.media.view_image(&texture);
                                                return;
                                            }
                                            Err(error) => warn!("Could not load GdkTexture from file: {}", error),
                                        }
                                    }
                                    Err(error) => warn!("Could not retrieve image file: {}", error),
                                }

                                imp.media.show_fallback(ContentType::Image);
                            })
                        );
                    }
                    MessageType::Video(video) => {
                        self.set_body(Some(video.body.clone()));

                        spawn!(
                            glib::PRIORITY_LOW,
                            clone!(@weak self as obj => async move {
                                let imp = obj.imp();

                                match event.get_media_content().await {
                                    Ok((uid, filename, data)) => {
                                        // The GStreamer backend of GtkVideo doesn't work with input streams so
                                        // we need to store the file.
                                        // See: https://gitlab.gnome.org/GNOME/gtk/-/issues/4062
                                        let mut path = cache_dir();
                                        path.push(format!("{uid}_{filename}"));
                                        let file = gio::File::for_path(path);
                                        file.replace_contents(
                                            &data,
                                            None,
                                            false,
                                            gio::FileCreateFlags::REPLACE_DESTINATION,
                                            gio::Cancellable::NONE,
                                        )
                                        .unwrap();

                                        imp.media.view_file(file);
                                    }
                                    Err(error) => {
                                        warn!("Could not retrieve video file: {}", error);
                                        imp.media.show_fallback(ContentType::Video);
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

    fn close(&self) {
        if self.fullscreened() {
            self.activate_action("win.toggle-fullscreen", None).unwrap();
        }

        self.imp().media.stop_playback();
        self.imp().revealer.set_reveal_child(false);

        let animation = self.imp().animation.get().unwrap();

        animation.set_value_from(animation.value());
        animation.set_value_to(0.0);
        animation.play();
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

impl EventActions for MediaViewer {
    fn texture(&self) -> Option<EventTexture> {
        self.imp().media.texture().map(EventTexture::Original)
    }
}

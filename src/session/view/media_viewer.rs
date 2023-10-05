use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{gdk, gio, glib, glib::clone, graphene, CompositeTemplate};
use matrix_sdk::ruma::events::room::message::MessageType;
use ruma::OwnedEventId;
use tracing::{error, warn};

use crate::{
    components::{ContentType, ImagePaintable, MediaContentViewer, ScaleRevealer},
    prelude::*,
    session::model::Room,
    spawn, spawn_tokio, toast,
    utils::{matrix::get_media_content, media::save_to_file},
    Window,
};

const ANIMATION_DURATION: u32 = 250;
const CANCEL_SWIPE_ANIMATION_DURATION: u32 = 400;

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };

    use glib::{object::WeakRef, subclass::InitializingObject};
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/session/view/media_viewer.ui")]
    pub struct MediaViewer {
        pub fullscreened: Cell<bool>,
        /// The room containing the media message.
        pub room: WeakRef<Room>,
        /// The ID of the event containing the media message.
        pub event_id: RefCell<Option<OwnedEventId>>,
        /// The media message to display.
        pub message: RefCell<Option<MessageType>>,
        pub body: RefCell<Option<String>>,
        pub animation: OnceCell<adw::TimedAnimation>,
        pub swipe_tracker: OnceCell<adw::SwipeTracker>,
        pub swipe_progress: Cell<f64>,
        #[template_child]
        pub toolbar_view: TemplateChild<adw::ToolbarView>,
        #[template_child]
        pub header_bar: TemplateChild<gtk::HeaderBar>,
        #[template_child]
        pub menu: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub revealer: TemplateChild<ScaleRevealer>,
        #[template_child]
        pub media: TemplateChild<MediaContentViewer>,
        pub actions_expression_watches: RefCell<HashMap<&'static str, gtk::ExpressionWatch>>,
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

            // Menu actions
            klass.install_action("media-viewer.copy-image", None, move |obj, _, _| {
                obj.copy_image();
            });

            klass.install_action("media-viewer.save-image", None, move |obj, _, _| {
                spawn!(clone!(@weak obj => async move {
                    obj.save_file().await;
                }));
            });

            klass.install_action("media-viewer.save-video", None, move |obj, _, _| {
                spawn!(clone!(@weak obj => async move {
                    obj.save_file().await;
                }));
            });

            klass.install_action("media-viewer.save-audio", None, move |obj, _, _| {
                spawn!(clone!(@weak obj => async move {
                    obj.save_file().await;
                }));
            });

            klass.install_action("media-viewer.permalink", None, move |obj, _, _| {
                spawn!(clone!(@weak obj => async move {
                    obj.copy_permalink().await;
                }));
            });
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
                    glib::ParamSpecObject::builder::<Room>("room")
                        .read_only()
                        .build(),
                    glib::ParamSpecString::builder("event-id")
                        .read_only()
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
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "fullscreened" => obj.fullscreened().to_value(),
                "room" => obj.room().to_value(),
                "event-id" => obj.event_id().as_ref().map(|e| e.as_str()).to_value(),
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

            // Bind `fullscreened` to the window property of the same name.
            obj.connect_notify_local(Some("root"), |obj, _| {
                if let Some(window) = obj.root().and_downcast::<Window>() {
                    window
                        .bind_property("fullscreened", obj, "fullscreened")
                        .sync_create()
                        .build();
                }
            });

            self.revealer
                .connect_transition_done(clone!(@weak obj => move |revealer| {
                    if !revealer.reveals_child() {
                        obj.set_visible(false);
                    }
                }));

            obj.update_menu_actions();
        }

        fn dispose(&self) {
            self.toolbar_view.unparent();

            for expr_watch in self.actions_expression_watches.take().values() {
                expr_watch.unwatch();
            }
        }
    }

    impl WidgetImpl for MediaViewer {
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let swipe_y_offset = -height as f64 * self.swipe_progress.get();
            let allocation = gtk::Allocation::new(0, swipe_y_offset as i32, width, height);
            self.toolbar_view.size_allocate(&allocation, baseline);
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

            obj.snapshot_child(&*self.toolbar_view, snapshot);
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

    /// The room containing the media message.
    pub fn room(&self) -> Option<Room> {
        self.imp().room.upgrade()
    }

    /// The ID of the event containing the media message.
    pub fn event_id(&self) -> Option<OwnedEventId> {
        self.imp().event_id.borrow().clone()
    }

    /// The media message to display.
    pub fn message(&self) -> Option<MessageType> {
        self.imp().message.borrow().clone()
    }

    /// Set the media message to display in the given room.
    pub fn set_message(&self, room: &Room, event_id: OwnedEventId, message: MessageType) {
        let imp = self.imp();

        imp.room.set(Some(room));
        imp.event_id.replace(Some(event_id));
        imp.message.replace(Some(message));

        self.update_menu_actions();
        self.build();
        self.notify("room");
        self.notify("event-id");
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
            imp.toolbar_view
                .set_top_bar_style(adw::ToolbarStyle::Raised);
        } else {
            imp.media.set_halign(gtk::Align::Center);
            imp.toolbar_view.set_top_bar_style(adw::ToolbarStyle::Flat);
        }

        self.notify("fullscreened");
    }

    /// Update the actions of the menu according to the current message.
    fn update_menu_actions(&self) {
        let imp = self.imp();

        let borrowed_message = imp.message.borrow();
        let message = borrowed_message.as_ref();
        let has_image = message
            .map(|m| matches!(m, MessageType::Image(_)))
            .unwrap_or_default();
        let has_video = message
            .map(|m| matches!(m, MessageType::Video(_)))
            .unwrap_or_default();
        let has_audio = message
            .map(|m| matches!(m, MessageType::Audio(_)))
            .unwrap_or_default();

        let has_event_id = imp.event_id.borrow().is_some();

        self.action_set_enabled("media-viewer.copy-image", has_image);
        self.action_set_enabled("media-viewer.save-image", has_image);
        self.action_set_enabled("media-viewer.save-video", has_video);
        self.action_set_enabled("media-viewer.save-audio", has_audio);
        self.action_set_enabled("media-viewer.permalink", has_event_id);
    }

    fn build(&self) {
        self.imp().media.show_loading();

        let Some(room) = self.room() else {
            return;
        };
        let Some(message) = self.message() else {
            return;
        };

        // self.set_event_actions(Some(&event));
        let client = room.session().client();

        match &message {
            MessageType::Image(image) => {
                let image = image.clone();
                self.set_body(Some(image.body));

                spawn!(
                    glib::Priority::LOW,
                    clone!(@weak self as obj => async move {
                        let imp = obj.imp();

                        match get_media_content(client, message).await {
                            Ok(( _, data)) => {
                                match ImagePaintable::from_bytes(&glib::Bytes::from(&data), image.info.and_then(|info| info.mimetype).as_deref()) {
                                    Ok(texture) => {
                                        imp.media.view_image(&texture);
                                        return;
                                    }
                                    Err(error) => warn!("Could not load GdkTexture from file: {error}"),
                                }
                            }
                            Err(error) => warn!("Could not retrieve image file: {error}"),
                        }

                        imp.media.show_fallback(ContentType::Image);
                    })
                );
            }
            MessageType::Video(video) => {
                self.set_body(Some(video.body.clone()));

                spawn!(
                    glib::Priority::LOW,
                    clone!(@weak self as obj => async move {
                        let imp = obj.imp();

                        match get_media_content(client, message).await {
                            Ok(( _, data)) => {
                                // The GStreamer backend of GtkVideo doesn't work with input streams so
                                // we need to store the file.
                                // See: https://gitlab.gnome.org/GNOME/gtk/-/issues/4062
                                let (file, _) = gio::File::new_tmp(Option::<String>::None).unwrap();
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
                                warn!("Could not retrieve video file: {error}");
                                imp.media.show_fallback(ContentType::Video);
                            }
                        }
                    })
                );
            }
            _ => {}
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
            self.imp().toolbar_view.set_reveal_top_bars(reveal);
        }
    }

    fn toggle_headerbar(&self) {
        let revealed = self.imp().toolbar_view.reveals_top_bars();
        self.reveal_headerbar(!revealed);
    }

    #[template_callback]
    fn handle_motion(&self, _x: f64, y: f64) {
        if y <= 50.0 {
            self.reveal_headerbar(true);
        }
    }

    #[template_callback]
    fn handle_click(&self, n_pressed: i32) {
        if self.fullscreened() && n_pressed == 1 {
            self.toggle_headerbar();
        } else if n_pressed == 2 {
            self.activate_action("win.toggle-fullscreen", None).unwrap();
        }
    }

    /// Copy the current image to the clipboard.
    fn copy_image(&self) {
        let Some(texture) = self.imp().media.texture() else {
            return;
        };
        self.clipboard().set_texture(&texture);
        toast!(self, gettext("Image copied to clipboard"));
    }

    /// Save the current file to the clipboard.
    async fn save_file(&self) {
        let Some(room) = self.room() else {
            return;
        };
        let Some(message) = self.message() else {
            return;
        };
        let client = room.session().client();

        let (filename, data) = match get_media_content(client, message).await {
            Ok(res) => res,
            Err(error) => {
                error!("Could not get event file: {error}");
                toast!(self, error.to_user_facing());

                return;
            }
        };

        save_to_file(self, data, filename).await;
    }

    /// Copy the permalink of the event of the media message to the clipboard.
    async fn copy_permalink(&self) {
        let Some(room) = self.room() else {
            return;
        };
        let Some(event_id) = self.event_id() else {
            return;
        };
        let matrix_room = room.matrix_room();

        let handle =
            spawn_tokio!(async move { matrix_room.matrix_to_event_permalink(event_id).await });

        match handle.await.unwrap() {
            Ok(permalink) => {
                self.clipboard().set_text(&permalink.to_string());
                toast!(self, gettext("Permalink copied to clipboard"));
            }
            Err(error) => {
                error!("Could not get permalink: {error}");
                toast!(self, gettext("Failed to copy the permalink"));
            }
        }
    }
}

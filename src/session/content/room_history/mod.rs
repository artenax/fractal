mod attachment_dialog;
mod divider_row;
mod item_row;
mod message_row;
mod state_row;
mod verification_info_bar;

use std::str::FromStr;

use adw::subclass::prelude::*;
use ashpd::{
    desktop::location::{Accuracy, LocationProxy},
    WindowIdentifier,
};
use futures::TryFutureExt;
use gettextrs::gettext;
use gtk::{
    gdk, gio, glib,
    glib::{clone, signal::Inhibit},
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
};
use log::warn;
use matrix_sdk::ruma::events::room::message::{
    EmoteMessageEventContent, FormattedBody, MessageType, RoomMessageEventContent,
    TextMessageEventContent,
};
use ruma::events::{room::message::LocationMessageEventContent, AnyMessageLikeEventContent};
use sourceview::prelude::*;

use self::{
    attachment_dialog::AttachmentDialog, divider_row::DividerRow, item_row::ItemRow,
    state_row::StateRow, verification_info_bar::VerificationInfoBar,
};
use crate::{
    components::{CustomEntry, DragOverlay, Pill, ReactionChooser, RoomTitle, Toast},
    i18n::gettext_f,
    session::{
        content::{MarkdownPopover, RoomDetails},
        room::{Event, Room, RoomType, Timeline, TimelineItem, TimelineState},
        user::UserExt,
    },
    spawn,
    utils::filename_for_mime,
    window::Window,
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{signal::SignalHandlerId, subclass::InitializingObject};
    use once_cell::unsync::OnceCell;

    use super::*;
    use crate::{components::Toast, window::Window, Application};

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-room-history.ui")]
    pub struct RoomHistory {
        pub compact: Cell<bool>,
        pub room: RefCell<Option<Room>>,
        pub category_handler: RefCell<Option<SignalHandlerId>>,
        pub empty_timeline_handler: RefCell<Option<SignalHandlerId>>,
        pub state_timeline_handler: RefCell<Option<SignalHandlerId>>,
        pub md_enabled: Cell<bool>,
        pub is_auto_scrolling: Cell<bool>,
        pub sticky: Cell<bool>,
        pub item_context_menu: OnceCell<gtk::PopoverMenu>,
        pub item_reaction_chooser: ReactionChooser,
        #[template_child]
        pub headerbar: TemplateChild<adw::HeaderBar>,
        #[template_child]
        pub room_title: TemplateChild<RoomTitle>,
        #[template_child]
        pub room_menu: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub listview: TemplateChild<gtk::ListView>,
        #[template_child]
        pub content: TemplateChild<gtk::Widget>,
        #[template_child]
        pub scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub scroll_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub scroll_btn_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub message_entry: TemplateChild<sourceview::View>,
        #[template_child]
        pub markdown_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub loading: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub error: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        pub is_loading: Cell<bool>,
        #[template_child]
        pub drag_overlay: TemplateChild<DragOverlay>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoomHistory {
        const NAME: &'static str = "ContentRoomHistory";
        type Type = super::RoomHistory;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            CustomEntry::static_type();
            ItemRow::static_type();
            MarkdownPopover::static_type();
            VerificationInfoBar::static_type();
            Timeline::static_type();
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Group);
            klass.install_action(
                "room-history.send-text-message",
                None,
                move |widget, _, _| {
                    widget.send_text_message();
                },
            );
            klass.install_action("room-history.leave", None, move |widget, _, _| {
                widget.leave();
            });

            klass.install_action("room-history.try-again", None, move |widget, _, _| {
                widget.try_again();
            });

            klass.install_action("room-history.details", None, move |widget, _, _| {
                widget.open_room_details("general");
            });
            klass.install_action("room-history.invite-members", None, move |widget, _, _| {
                widget.open_invite_members();
            });

            klass.install_action("room-history.scroll-down", None, move |widget, _, _| {
                widget.scroll_down();
            });

            klass.install_action("room-history.select-file", None, move |widget, _, _| {
                widget.select_file();
            });

            klass.install_action("room-history.open-emoji", None, move |widget, _, _| {
                widget.open_emoji();
            });

            klass.install_action("room-history.send-location", None, move |widget, _, _| {
                spawn!(clone!(@weak widget => async move {
                    let toast_error = match widget.send_location().await {
                        // Do nothing if the request was canceled by the user
                        Err(ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)) => {
                            log::error!("Location request was cancelled by the user");
                            Some(gettext("The location request has been cancelled."))
                        },
                        Err(err) => {
                            log::error!("Failed to send location {}", err);
                            Some(gettext("Failed to retrieve current location."))
                        }
                        _ => None,
                    };

                    if let Some(window) = widget
                        .root()
                        .as_ref()
                        .and_then(|root| root.downcast_ref::<Window>())
                    {
                        if let Some(message) = toast_error {
                            window.add_toast(&Toast::new(&message));
                        }
                    }
                }));
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for RoomHistory {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::new(
                        "compact",
                        "Compact",
                        "Whether a compact view is used",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecObject::new(
                        "room",
                        "Room",
                        "The room currently shown",
                        Room::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecBoolean::new(
                        "empty",
                        "Empty",
                        "Whether there is currently a room shown",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "markdown-enabled",
                        "Markdown enabled",
                        "Whether outgoing messages should be interpreted as markdown",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "sticky",
                        "Sticky",
                        "Whether the room history should stick to the newest message in the timeline",
                        true,
                        glib::ParamFlags::READWRITE,
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
                "compact" => {
                    let compact = value.get().unwrap();
                    self.compact.set(compact);
                }
                "room" => {
                    let room = value.get().unwrap();
                    obj.set_room(room);
                }
                "markdown-enabled" => {
                    let md_enabled = value.get().unwrap();
                    self.md_enabled.set(md_enabled);
                    self.markdown_button.set_icon_name(if md_enabled {
                        "format-indent-more-symbolic"
                    } else {
                        "format-justify-left-symbolic"
                    });
                }
                "sticky" => obj.set_sticky(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "compact" => self.compact.get().to_value(),
                "room" => obj.room().to_value(),
                "empty" => obj.room().is_none().to_value(),
                "markdown-enabled" => self.md_enabled.get().to_value(),
                "sticky" => obj.sticky().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            let factory = gtk::SignalListItemFactory::new();
            factory.connect_setup(clone!(@weak obj => move |_, item| {
                let row = ItemRow::new(&obj);
                item.set_child(Some(&row));
                ItemRow::this_expression("item").chain_property::<TimelineItem>("activatable").bind(item, "activatable", Some(&row));
                item.bind_property("item", &row, "item").build();
                item.set_selectable(false);
            }));
            self.listview.set_factory(Some(&factory));

            // Needed to use the natural height of GtkPictures
            self.listview
                .set_vscroll_policy(gtk::ScrollablePolicy::Natural);

            self.listview
                .connect_activate(clone!(@weak obj => move |listview, pos| {
                    if let Some(event) = listview
                        .model()
                        .and_then(|model| model.item(pos))
                        .as_ref()
                        .and_then(|o| o.downcast_ref::<Event>())
                    {
                        if let Some(room) = obj.room() {
                            room.session().show_media(event);
                        }
                    }
                }));

            obj.set_sticky(true);
            let adj = self.listview.vadjustment().unwrap();

            adj.connect_value_changed(clone!(@weak obj => move |adj| {
                let priv_ = obj.imp();

                if priv_.is_auto_scrolling.get() {
                    if adj.value() + adj.page_size() == adj.upper() {
                        priv_.is_auto_scrolling.set(false);
                        obj.set_sticky(true);
                    }
                } else {
                    obj.set_sticky(adj.value() + adj.page_size() == adj.upper());
                }
                obj.start_loading();
            }));
            adj.connect_upper_notify(clone!(@weak obj => move |_| {
                if obj.sticky() {
                    obj.scroll_down();
                }
                obj.start_loading();
            }));

            let key_events = gtk::EventControllerKey::new();
            self.message_entry.add_controller(&key_events);
            self.message_entry
                .connect_paste_clipboard(clone!(@weak obj => move |entry| {
                    let formats = obj.clipboard().formats();

                    // We only handle files and supported images.
                    if formats.contains_type(gio::File::static_type()) || formats.contains_type(gdk::Texture::static_type()) {
                        entry.stop_signal_emission_by_name("paste-clipboard");
                        spawn!(
                            clone!(@weak obj => async move {
                                obj.read_clipboard().await;
                        }));
                    }
                }));

            key_events
                .connect_key_pressed(clone!(@weak obj => @default-return Inhibit(false), move |_, key, _, modifier| {
                if !modifier.contains(gdk::ModifierType::SHIFT_MASK) && (key == gdk::Key::Return || key == gdk::Key::KP_Enter) {
                    obj.activate_action("room-history.send-text-message", None).unwrap();
                    Inhibit(true)
                } else {
                    Inhibit(false)
                }
            }));

            let buffer = self
                .message_entry
                .buffer()
                .downcast::<sourceview::Buffer>()
                .unwrap();

            buffer.connect_text_notify(clone!(@weak obj => move |buffer| {
               let (start_iter, end_iter) = buffer.bounds();
               obj.action_set_enabled("room-history.send-text-message", start_iter != end_iter);
            }));
            crate::utils::setup_style_scheme(&buffer);

            let (start_iter, end_iter) = buffer.bounds();
            obj.action_set_enabled("room-history.send-text-message", start_iter != end_iter);

            let md_lang = sourceview::LanguageManager::default().language("markdown");
            buffer.set_language(md_lang.as_ref());
            obj.bind_property("markdown-enabled", &buffer, "highlight-syntax")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build();

            let settings = Application::default().settings();
            settings
                .bind("markdown-enabled", obj, "markdown-enabled")
                .build();

            obj.setup_drop_target();

            self.parent_constructed(obj);
        }
    }

    impl WidgetImpl for RoomHistory {}
    impl BinImpl for RoomHistory {}
}

glib::wrapper! {
    pub struct RoomHistory(ObjectSubclass<imp::RoomHistory>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl RoomHistory {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create RoomHistory")
    }

    pub fn set_room(&self, room: Option<Room>) {
        let priv_ = self.imp();

        if self.room() == room {
            return;
        }

        if let Some(room) = self.room() {
            if let Some(category_handler) = priv_.category_handler.take() {
                room.disconnect(category_handler);
            }

            if let Some(empty_timeline_handler) = priv_.empty_timeline_handler.take() {
                room.timeline().disconnect(empty_timeline_handler);
            }

            if let Some(state_timeline_handler) = priv_.state_timeline_handler.take() {
                room.timeline().disconnect(state_timeline_handler);
            }
        }

        if let Some(ref room) = room {
            let handler_id = room.connect_notify_local(
                Some("category"),
                clone!(@weak self as obj => move |_, _| {
                        obj.update_room_state();
                }),
            );

            priv_.category_handler.replace(Some(handler_id));

            let handler_id = room.timeline().connect_notify_local(
                Some("empty"),
                clone!(@weak self as obj => move |_, _| {
                        obj.update_view();
                }),
            );

            priv_.empty_timeline_handler.replace(Some(handler_id));

            let handler_id = room.timeline().connect_notify_local(
                Some("state"),
                clone!(@weak self as obj => move |_, _| {
                        obj.update_view();
                }),
            );

            priv_.state_timeline_handler.replace(Some(handler_id));

            room.load_members();
        }

        // TODO: use gtk::MultiSelection to allow selection
        let model = room
            .as_ref()
            .map(|room| gtk::NoSelection::new(Some(room.timeline())));

        priv_.listview.set_model(model.as_ref());
        priv_.is_loading.set(false);
        priv_.message_entry.grab_focus();
        priv_.room.replace(room);
        self.update_view();
        self.start_loading();
        self.update_room_state();
        self.notify("room");
        self.notify("empty");
    }

    pub fn room(&self) -> Option<Room> {
        self.imp().room.borrow().clone()
    }

    pub fn send_text_message(&self) {
        let priv_ = self.imp();
        let buffer = priv_.message_entry.buffer();
        let (start_iter, end_iter) = buffer.bounds();
        let body_len = buffer.text(&start_iter, &end_iter, true).len();

        let is_markdown = priv_.md_enabled.get();
        let mut has_mentions = false;
        let mut plain_body = String::with_capacity(body_len);
        // formatted_body is Markdown if is_markdown is true, and HTML if false.
        let mut formatted_body = String::with_capacity(body_len);
        // uncopied_text_location is the start of the text we haven't copied to
        // plain_body and formatted_body.
        let mut uncopied_text_location = start_iter;

        let mut iter = start_iter;
        loop {
            if let Some(anchor) = iter.child_anchor() {
                let widgets = anchor.widgets();
                let pill = widgets.first().unwrap().downcast_ref::<Pill>().unwrap();
                let (url, label) = pill
                    .user()
                    .map(|user| {
                        (
                            user.user_id().matrix_to_uri().to_string(),
                            user.display_name(),
                        )
                    })
                    .or_else(|| {
                        pill.room().map(|room| {
                            (
                                // No server name needed. matrix.to URIs for mentions aren't
                                // routable
                                room.room_id().matrix_to_uri([]).to_string(),
                                room.display_name(),
                            )
                        })
                    })
                    .unwrap();

                // Add more uncopied characters from message
                let some_text = buffer.text(&uncopied_text_location, &iter, false);
                plain_body.push_str(&some_text);
                formatted_body.push_str(&some_text);
                uncopied_text_location = iter;

                // Add mention
                has_mentions = true;
                plain_body.push_str(&label);
                formatted_body.push_str(&if is_markdown {
                    format!("[{}]({})", label, url)
                } else {
                    format!("<a href='{}'>{}</a>", url, label)
                });
            }
            if !iter.forward_char() {
                // Add remaining uncopied characters
                let some_text = buffer.text(&uncopied_text_location, &iter, false);
                plain_body.push_str(&some_text);
                formatted_body.push_str(&some_text);
                break;
            }
        }

        let is_emote = plain_body.starts_with("/me ");
        if is_emote {
            plain_body.replace_range(.."/me ".len(), "");
            formatted_body.replace_range(.."/me ".len(), "");
        }

        let html_body = if is_markdown {
            FormattedBody::markdown(formatted_body).map(|b| b.body)
        } else if has_mentions {
            // Already formatted with HTML
            Some(formatted_body)
        } else {
            None
        };

        let content = RoomMessageEventContent::new(if is_emote {
            MessageType::Emote(if let Some(html_body) = html_body {
                EmoteMessageEventContent::html(plain_body, html_body)
            } else {
                EmoteMessageEventContent::plain(plain_body)
            })
        } else {
            MessageType::Text(if let Some(html_body) = html_body {
                TextMessageEventContent::html(plain_body, html_body)
            } else {
                TextMessageEventContent::plain(plain_body)
            })
        });

        self.room().unwrap().send_room_message_event(content);
        buffer.set_text("");
    }

    pub fn leave(&self) {
        if let Some(room) = &*self.imp().room.borrow() {
            room.set_category(RoomType::Left);
        }
    }

    /// Opens the room details on the page with the given name.
    pub fn open_room_details(&self, page_name: &str) {
        if let Some(room) = self.room() {
            let window = RoomDetails::new(&self.parent_window(), &room);
            window.set_property("visible-page-name", page_name);
            window.show();
        }
    }

    pub fn open_invite_members(&self) {
        if let Some(room) = self.room() {
            let window = RoomDetails::new(&self.parent_window(), &room);
            window.set_property("visible-page-name", "members");
            window.present_invite_subpage();
            window.show();
        }
    }

    fn update_room_state(&self) {
        let priv_ = self.imp();

        if let Some(room) = &*priv_.room.borrow() {
            if room.category() == RoomType::Left {
                self.action_set_enabled("room-history.leave", false);
                priv_.room_menu.hide();
            } else {
                self.action_set_enabled("room-history.leave", true);
                priv_.room_menu.show();
            }
        }
    }

    fn update_view(&self) {
        let priv_ = self.imp();

        if let Some(room) = &*priv_.room.borrow() {
            if room.timeline().is_empty() {
                if room.timeline().state() == TimelineState::Error {
                    priv_.stack.set_visible_child(&*priv_.error);
                } else {
                    priv_.stack.set_visible_child(&*priv_.loading);
                }
            } else {
                priv_.stack.set_visible_child(&*priv_.content);
            }
        }
    }

    fn need_messages(&self) -> bool {
        let adj = self.imp().listview.vadjustment().unwrap();
        // Load more messages when the user gets close to the end of the known room
        // history Use the page size twice to detect if the user gets close to
        // the endload_timeline
        adj.value() < adj.page_size() * 2.0 || adj.upper() <= adj.page_size() / 2.0
    }

    fn start_loading(&self) {
        let priv_ = self.imp();
        if !priv_.is_loading.get() {
            let room = if let Some(room) = self.room() {
                room
            } else {
                return;
            };

            if !self.need_messages() && !room.timeline().is_empty() {
                return;
            }

            priv_.is_loading.set(true);

            let obj_weak = self.downgrade();
            spawn!(async move {
                loop {
                    // We don't want to hold a strong ref to `obj` on `await`
                    let need = if let Some(obj) = obj_weak.upgrade() {
                        if obj.room().as_ref() == Some(&room) {
                            obj.need_messages() || room.timeline().is_empty()
                        } else {
                            return;
                        }
                    } else {
                        return;
                    };

                    if need {
                        if !room.timeline().load().await {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                // Remove the task
                if let Some(obj) = obj_weak.upgrade() {
                    obj.imp().is_loading.set(false);
                }
            });
        }
    }

    /// Returns the parent GtkWindow containing this widget.
    fn parent_window(&self) -> Option<gtk::Window> {
        self.root()?.downcast().ok()
    }

    pub fn sticky(&self) -> bool {
        self.imp().sticky.get()
    }

    pub fn set_sticky(&self, sticky: bool) {
        let priv_ = self.imp();

        if self.sticky() == sticky {
            return;
        }

        priv_.scroll_btn_revealer.set_reveal_child(!sticky);

        priv_.sticky.set(sticky);
        self.notify("sticky");
    }

    /// Scroll to the newest message in the timeline
    pub fn scroll_down(&self) {
        let priv_ = self.imp();

        priv_.is_auto_scrolling.set(true);

        priv_
            .scrolled_window
            .emit_by_name::<bool>("scroll-child", &[&gtk::ScrollType::End, &false]);
    }

    fn try_again(&self) {
        self.start_loading();
    }

    fn open_emoji(&self) {
        self.imp().message_entry.emit_insert_emoji();
    }

    async fn send_location(&self) -> ashpd::Result<()> {
        if let Some(room) = self.room() {
            let connection = ashpd::zbus::Connection::session().await?;
            let proxy = LocationProxy::new(&connection).await?;
            let identifier = WindowIdentifier::default();

            let session = proxy
                .create_session(Some(0), Some(0), Some(Accuracy::Exact))
                .await?;

            // We want to be listening for new locations whenever the session is up
            // otherwise we might lose the first response and will have to wait for a future
            // update by geoclue
            let (_, location) = futures::try_join!(
                proxy.start(&session, &identifier).into_future(),
                proxy.receive_location_updated().into_future()
            )?;

            let geo_uri = format!("geo:{},{}", location.latitude(), location.longitude());

            let window = self.root().unwrap().downcast::<gtk::Window>().unwrap();
            let dialog = AttachmentDialog::for_location(&window, "Your Location", &geo_uri);
            if dialog.run_future().await != gtk::ResponseType::Ok {
                return Ok(());
            }

            let iso8601_datetime =
                glib::DateTime::from_unix_local(location.timestamp().as_secs() as i64)
                    .expect("Valid location timestamp");
            let location_body = gettext_f(
                "User Location {geo_uri} at {iso8601_datetime}",
                &[
                    ("geo_uri", &geo_uri),
                    (
                        "iso8601_datetime",
                        iso8601_datetime.format_iso8601().unwrap().as_str(),
                    ),
                ],
            );
            room.send_room_message_event(AnyMessageLikeEventContent::RoomMessage(
                RoomMessageEventContent::new(MessageType::Location(
                    LocationMessageEventContent::new(location_body, geo_uri),
                )),
            ));
        }
        Ok(())
    }

    async fn send_image(&self, image: gdk::Texture) {
        let window = self.root().unwrap().downcast::<gtk::Window>().unwrap();
        let filename = filename_for_mime(Some(mime::IMAGE_PNG.as_ref()), None);
        let dialog = AttachmentDialog::for_image(&window, &filename, &image);

        if dialog.run_future().await != gtk::ResponseType::Ok {
            return;
        }

        if let Some(room) = self.room() {
            room.send_attachment(
                image.save_to_png_bytes().to_vec(),
                mime::IMAGE_PNG,
                &filename,
            );
        }
    }

    pub fn select_file(&self) {
        let window = self.root().unwrap().downcast::<gtk::Window>().unwrap();
        let dialog = gtk::FileChooserNative::new(
            None,
            Some(&window),
            gtk::FileChooserAction::Open,
            None,
            None,
        );
        dialog.set_modal(true);

        dialog.connect_response(
            glib::clone!(@weak self as obj, @strong dialog => move |_, response| {
                dialog.destroy();
                if response == gtk::ResponseType::Accept {
                    let file = dialog.file().unwrap();

                    crate::spawn!(glib::clone!(@weak obj, @strong file => async move {
                        obj.send_file(file).await;
                    }));
                }
            }),
        );

        dialog.show();
    }

    async fn send_file(&self, file: gio::File) {
        let attributes: &[&str] = &[
            *gio::FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE,
            *gio::FILE_ATTRIBUTE_STANDARD_DISPLAY_NAME,
        ];

        // Read mime type.
        let info = file
            .query_info_future(
                &attributes.join(","),
                gio::FileQueryInfoFlags::NONE,
                glib::PRIORITY_DEFAULT,
            )
            .await
            .ok();

        let mime = info
            .as_ref()
            .and_then(|info| info.content_type())
            .and_then(|content_type| mime::Mime::from_str(&content_type).ok())
            .unwrap_or(mime::APPLICATION_OCTET_STREAM);
        let filename = info.map(|info| info.display_name()).map_or_else(
            || filename_for_mime(Some(mime.as_ref()), None),
            |name| name.to_string(),
        );

        match file.load_contents_future().await {
            Ok((bytes, _tag)) => {
                let window = self.root().unwrap().downcast::<gtk::Window>().unwrap();
                let dialog = AttachmentDialog::for_file(&window, &filename, &file);

                if dialog.run_future().await != gtk::ResponseType::Ok {
                    return;
                }

                if let Some(room) = self.room() {
                    room.send_attachment(bytes.clone(), mime.clone(), &filename);
                }
            }
            Err(err) => {
                warn!("Could not read file: {}", err);

                if let Some(window) = self
                    .root()
                    .as_ref()
                    .and_then(|root| root.downcast_ref::<Window>())
                {
                    window.add_toast(&Toast::new(&gettext("Error reading file")));
                }
            }
        }
    }

    fn setup_drop_target(&self) {
        let priv_ = imp::RoomHistory::from_instance(self);

        let target = gtk::DropTarget::new(
            gio::File::static_type(),
            gdk::DragAction::COPY | gdk::DragAction::MOVE,
        );

        target.connect_drop(
            clone!(@weak self as obj => @default-return false, move |_, value, _, _| {
                match value.get::<gio::File>() {
                    Ok(file) => {
                        spawn!(clone!(@weak obj => async move {
                            obj.send_file(file).await;
                        }));
                        true
                    }
                    Err(error) => {
                        warn!("Could not get file from drop: {error:?}");

                        if let Some(window) = obj
                            .root()
                            .as_ref()
                            .and_then(|root| root.downcast_ref::<Window>())
                        {
                            window.add_toast(
                                &Toast::new(&gettext("Error getting file from drop"))
                            );
                        }

                        false
                    }
                }
            }),
        );

        priv_.drag_overlay.set_drop_target(&target);
    }

    async fn read_clipboard(&self) {
        let clipboard = self.clipboard();
        let formats = clipboard.formats();

        if formats.contains_type(gdk::Texture::static_type()) {
            // There is an image in the clipboard.
            match clipboard
                .read_value_future(gdk::Texture::static_type(), glib::PRIORITY_DEFAULT)
                .await
            {
                Ok(value) => match value.get::<gdk::Texture>() {
                    Ok(texture) => {
                        self.send_image(texture).await;
                        return;
                    }
                    Err(error) => warn!("Could not get GdkTexture from value: {error:?}"),
                },
                Err(error) => warn!("Could not get GdkTexture from the clipboard: {error:?}"),
            }

            if let Some(window) = self
                .root()
                .as_ref()
                .and_then(|root| root.downcast_ref::<Window>())
            {
                window.add_toast(&Toast::new(&gettext("Error getting image from clipboard")));
            }
        } else if formats.contains_type(gio::File::static_type()) {
            // There is a file in the clipboard.
            match clipboard
                .read_value_future(gio::File::static_type(), glib::PRIORITY_DEFAULT)
                .await
            {
                Ok(value) => match value.get::<gio::File>() {
                    Ok(file) => {
                        self.send_file(file).await;
                        return;
                    }
                    Err(error) => warn!("Could not get file from value: {error:?}"),
                },
                Err(error) => warn!("Could not get file from the clipboard: {error:?}"),
            }

            if let Some(window) = self
                .root()
                .as_ref()
                .and_then(|root| root.downcast_ref::<Window>())
            {
                window.add_toast(&Toast::new(&gettext("Error getting file from clipboard")));
            }
        }
    }

    pub fn handle_paste_action(&self) {
        spawn!(glib::clone!(@weak self as obj => async move {
            obj.read_clipboard().await;
        }));
    }

    pub fn item_context_menu(&self) -> &gtk::PopoverMenu {
        self.imp()
            .item_context_menu
            .get_or_init(|| gtk::PopoverMenu::from_model(gio::MenuModel::NONE))
    }

    pub fn item_reaction_chooser(&self) -> &ReactionChooser {
        &self.imp().item_reaction_chooser
    }
}

impl Default for RoomHistory {
    fn default() -> Self {
        Self::new()
    }
}

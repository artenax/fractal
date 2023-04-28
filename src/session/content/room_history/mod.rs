mod attachment_dialog;
mod completion;
mod divider_row;
mod item_row;
mod message_row;
mod read_receipts_list;
mod state_row;
mod typing_row;
mod verification_info_bar;

use std::time::Duration;

use adw::subclass::prelude::*;
use ashpd::{
    desktop::location::{Accuracy, LocationProxy},
    WindowIdentifier,
};
use futures::TryFutureExt;
use geo_uri::GeoUri;
use gettextrs::gettext;
use gtk::{
    gdk, gio, glib,
    glib::{clone, signal::Inhibit, FromVariant},
    prelude::*,
    CompositeTemplate,
};
use log::{debug, error, warn};
use matrix_sdk::{
    attachment::{AttachmentInfo, BaseFileInfo, BaseImageInfo},
    ruma::{
        events::{
            room::message::{EmoteMessageEventContent, FormattedBody, MessageType},
            AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent,
        },
        EventId,
    },
};
use ruma::{
    api::client::receipt::create_receipt::v3::ReceiptType,
    events::{
        receipt::ReceiptThread,
        room::{
            message::{ForwardThread, LocationMessageEventContent, RoomMessageEventContent},
            power_levels::PowerLevelAction,
        },
        AnyMessageLikeEventContent,
    },
    OwnedEventId,
};
use sourceview::prelude::*;

use self::{
    attachment_dialog::AttachmentDialog, completion::CompletionPopover, divider_row::DividerRow,
    item_row::ItemRow, message_row::content::MessageContent, read_receipts_list::ReadReceiptsList,
    state_row::StateRow, typing_row::TypingRow, verification_info_bar::VerificationInfoBar,
};
use crate::{
    components::{
        CustomEntry, DragOverlay, LabelWithWidgets, Pill, ReactionChooser, RoomTitle, Spinner,
    },
    i18n::gettext_f,
    session::{
        content::{room_details, RoomDetails},
        room::{Event, EventKey, Room, RoomType, Timeline, TimelineState},
        user::UserExt,
    },
    spawn, spawn_tokio, toast,
    utils::{
        media::{filename_for_mime, get_audio_info, get_image_info, get_video_info, load_file},
        template_callbacks::TemplateCallbacks,
    },
};

/// The time to wait before considering that scrolling has ended.
const SCROLL_TIMEOUT: Duration = Duration::from_millis(500);
/// The time to wait before considering that messages on a screen where read.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Default, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(i32)]
#[enum_type(name = "RelatedEventType")]
pub enum RelatedEventType {
    #[default]
    None = 0,
    Reply = 1,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{signal::SignalHandlerId, subclass::InitializingObject};
    use once_cell::unsync::OnceCell;

    use super::*;
    use crate::Application;

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
        pub completion: CompletionPopover,
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
        pub loading: TemplateChild<Spinner>,
        #[template_child]
        pub error: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        pub is_loading: Cell<bool>,
        #[template_child]
        pub drag_overlay: TemplateChild<DragOverlay>,
        pub invite_action_watch: RefCell<Option<gtk::ExpressionWatch>>,
        #[template_child]
        pub related_event_header: TemplateChild<LabelWithWidgets>,
        #[template_child]
        pub related_event_content: TemplateChild<MessageContent>,
        pub related_event_type: Cell<RelatedEventType>,
        pub related_event: RefCell<Option<Event>>,
        pub scroll_timeout: RefCell<Option<glib::SourceId>>,
        pub read_timeout: RefCell<Option<glib::SourceId>>,
        /// Whether we should load more history when the timeline is ready.
        pub load_when_timeline_ready: Cell<bool>,
        /// The GtkSelectionModel used in the listview.
        // TODO: use gtk::MultiSelection to allow selection
        pub selection_model: OnceCell<gtk::NoSelection>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoomHistory {
        const NAME: &'static str = "ContentRoomHistory";
        type Type = super::RoomHistory;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            CustomEntry::static_type();
            ItemRow::static_type();
            VerificationInfoBar::static_type();
            Timeline::static_type();
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
            TemplateCallbacks::bind_template_callbacks(klass);
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

            klass.install_action("room-history.permalink", None, move |widget, _, _| {
                spawn!(clone!(@weak widget => async move {
                    widget.permalink().await;
                }));
            });

            klass.install_action("room-history.details", None, move |widget, _, _| {
                widget.open_room_details(room_details::PageName::General);
            });
            klass.install_action("room-history.invite-members", None, move |widget, _, _| {
                widget.open_room_details(room_details::PageName::Invite);
            });

            klass.install_action("room-history.scroll-down", None, move |widget, _, _| {
                widget.scroll_down();
            });

            klass.install_action("room-history.select-file", None, move |widget, _, _| {
                spawn!(clone!(@weak widget => async move {
                    widget.select_file().await;
                }));
            });

            klass.install_action("room-history.open-emoji", None, move |widget, _, _| {
                widget.open_emoji();
            });

            klass.install_action("room-history.send-location", None, move |widget, _, _| {
                spawn!(clone!(@weak widget => async move {
                    let toast_error = match widget.send_location().await {
                        // Do nothing if the request was cancelled by the user
                        Err(ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)) => {
                            error!("Location request was cancelled by the user");
                            Some(gettext("The location request has been cancelled."))
                        },
                        Err(error) => {
                            error!("Failed to send location {error}");
                            Some(gettext("Failed to retrieve current location."))
                        }
                        _ => None,
                    };

                    if let Some(message) = toast_error {
                        toast!(widget, message);
                    }
                }));
            });

            klass.install_property_action("room-history.markdown", "markdown-enabled");

            klass.install_action(
                "room-history.clear-related-event",
                None,
                move |widget, _, _| widget.clear_related_event(),
            );

            klass.install_action("room-history.reply", Some("s"), move |widget, _, v| {
                if let Some(event_id) = v
                    .and_then(String::from_variant)
                    .and_then(|s| EventId::parse(s).ok())
                {
                    if let Some(event) = widget
                        .room()
                        .and_then(|room| room.timeline().event_by_key(&EventKey::EventId(event_id)))
                        .and_then(|event| event.downcast().ok())
                    {
                        widget.set_reply_to(event);
                    }
                }
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
                    glib::ParamSpecBoolean::builder("compact")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<Room>("room")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("empty")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("markdown-enabled")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("sticky")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecEnum::builder::<RelatedEventType>("related-event-type")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<Event>("related-event")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "compact" => obj.set_compact(value.get().unwrap()),
                "room" => obj.set_room(value.get().unwrap()),
                "markdown-enabled" => obj.set_markdown_enabled(value.get().unwrap()),
                "sticky" => obj.set_sticky(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "compact" => obj.compact().to_value(),
                "room" => obj.room().to_value(),
                "empty" => obj.is_empty().to_value(),
                "markdown-enabled" => obj.markdown_enabled().to_value(),
                "sticky" => obj.sticky().to_value(),
                "related-event-type" => obj.related_event_type().to_value(),
                "related-event" => obj.related_event().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            let obj = self.obj();

            let factory = gtk::SignalListItemFactory::new();
            factory.connect_setup(clone!(@weak obj => move |_, item| {
                let item = match item.downcast_ref::<gtk::ListItem>() {
                    Some(item) => item,
                    None => {
                        error!("List item factory did not receive a list item: {item:?}");
                        return;
                    }
                };
                let row = ItemRow::new(&obj);
                item.set_child(Some(&row));
                item.bind_property("item", &row, "item").build();
                item.set_activatable(false);
                item.set_selectable(false);
            }));
            self.listview.set_factory(Some(&factory));

            // Needed to use the natural height of GtkPictures
            self.listview
                .set_vscroll_policy(gtk::ScrollablePolicy::Natural);

            self.listview.set_model(Some(obj.selection_model()));

            obj.set_sticky(true);
            let adj = self.listview.vadjustment().unwrap();

            adj.connect_value_changed(clone!(@weak obj => move |adj| {
                let imp = obj.imp();

                obj.trigger_read_receipts_update();

                let is_at_bottom = adj.value() + adj.page_size() == adj.upper();
                if imp.is_auto_scrolling.get() {
                    if is_at_bottom {
                        imp.is_auto_scrolling.set(false);
                        obj.set_sticky(true);
                    } else {
                        obj.scroll_down();
                    }
                } else {
                    obj.set_sticky(is_at_bottom);
                }

                // Remove the typing row if we scroll up.
                if !is_at_bottom {
                    if let Some(room) = obj.room() {
                        room.timeline().remove_empty_typing_row();
                    }
                }

                obj.start_loading();
            }));
            adj.connect_upper_notify(clone!(@weak obj => move |_| {
                if obj.sticky() {
                    obj.scroll_down();
                }
                obj.start_loading();
            }));
            adj.connect_page_size_notify(clone!(@weak obj => move |_| {
                if obj.sticky() {
                    obj.scroll_down();
                }
                obj.start_loading();
            }));

            let key_events = gtk::EventControllerKey::new();
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
            self.message_entry
                .connect_copy_clipboard(clone!(@weak obj => move |entry| {
                    entry.stop_signal_emission_by_name("copy-clipboard");
                    obj.copy_buffer_selection_to_clipboard();
                }));
            self.message_entry
                .connect_cut_clipboard(clone!(@weak obj => move |entry| {
                    entry.stop_signal_emission_by_name("cut-clipboard");
                    obj.copy_buffer_selection_to_clipboard();
                    entry.buffer().delete_selection(true, true);
                }));

            key_events
                .connect_key_pressed(clone!(@weak obj => @default-return Inhibit(false), move |_, key, _, modifier| {
                if modifier.is_empty() && (key == gdk::Key::Return || key == gdk::Key::KP_Enter) {
                    obj.activate_action("room-history.send-text-message", None).unwrap();
                    Inhibit(true)
                } else if modifier.is_empty() && key == gdk::Key::Escape && obj.related_event_type() != RelatedEventType::None {
                    obj.clear_related_event();
                    Inhibit(true)
                } else {
                    Inhibit(false)
                }
            }));
            self.message_entry.add_controller(key_events);

            let buffer = self
                .message_entry
                .buffer()
                .downcast::<sourceview::Buffer>()
                .unwrap();

            buffer.connect_text_notify(clone!(@weak obj => move |buffer| {
               let (start_iter, end_iter) = buffer.bounds();
               let is_empty = start_iter == end_iter;
               obj.action_set_enabled("room-history.send-text-message", !is_empty);
               obj.send_typing_notification(!is_empty);
            }));
            crate::utils::sourceview::setup_style_scheme(&buffer);

            let (start_iter, end_iter) = buffer.bounds();
            obj.action_set_enabled("room-history.send-text-message", start_iter != end_iter);

            let md_lang = sourceview::LanguageManager::default().language("markdown");
            buffer.set_language(md_lang.as_ref());
            obj.bind_property("markdown-enabled", &buffer, "highlight-syntax")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build();

            let settings = Application::default().settings();
            settings
                .bind("markdown-enabled", &*obj, "markdown-enabled")
                .build();

            self.completion.set_parent(&*self.message_entry);

            obj.setup_drop_target();

            self.parent_constructed();
        }

        fn dispose(&self) {
            self.completion.unparent();

            if let Some(invite_action) = self.invite_action_watch.take() {
                invite_action.unwatch();
            }
        }
    }

    impl WidgetImpl for RoomHistory {}
    impl BinImpl for RoomHistory {}
}

glib::wrapper! {
    pub struct RoomHistory(ObjectSubclass<imp::RoomHistory>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl RoomHistory {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Whether a compact view is used.
    pub fn compact(&self) -> bool {
        self.imp().compact.get()
    }

    /// Set whether a compact view is used.
    pub fn set_compact(&self, compact: bool) {
        self.imp().compact.set(compact);
        self.notify("compact");
    }

    /// Set the room currently displayed.
    pub fn set_room(&self, room: Option<Room>) {
        let imp = self.imp();

        if self.room() == room {
            return;
        }

        if let Some(room) = self.room() {
            if let Some(category_handler) = imp.category_handler.take() {
                room.disconnect(category_handler);
            }

            if let Some(empty_timeline_handler) = imp.empty_timeline_handler.take() {
                room.timeline().disconnect(empty_timeline_handler);
            }

            if let Some(state_timeline_handler) = imp.state_timeline_handler.take() {
                room.timeline().disconnect(state_timeline_handler);
            }

            if let Some(invite_action) = imp.invite_action_watch.take() {
                invite_action.unwatch();
            }

            imp.load_when_timeline_ready.set(false);

            self.clear_related_event();
        }

        if let Some(source_id) = imp.scroll_timeout.take() {
            source_id.remove();
        }
        if let Some(source_id) = imp.read_timeout.take() {
            source_id.remove();
        }

        if let Some(ref room) = room {
            let timeline = room.timeline();

            let handler_id = room.connect_notify_local(
                Some("category"),
                clone!(@weak self as obj => move |_, _| {
                    obj.update_room_state();
                }),
            );
            imp.category_handler.replace(Some(handler_id));

            let handler_id = timeline.connect_notify_local(
                Some("empty"),
                clone!(@weak self as obj => move |_, _| {
                    obj.update_view();
                }),
            );
            imp.empty_timeline_handler.replace(Some(handler_id));

            let handler_id = timeline.connect_notify_local(
                Some("state"),
                clone!(@weak self as obj => move |timeline, _| {
                    obj.update_view();

                    let load_when_timeline_ready = &obj.imp().load_when_timeline_ready;
                    if load_when_timeline_ready.get() && timeline.state() == TimelineState::Ready {
                        load_when_timeline_ready.set(false);
                        obj.start_loading();
                    }
                }),
            );
            imp.state_timeline_handler.replace(Some(handler_id));

            timeline.remove_empty_typing_row();
            self.trigger_read_receipts_update();

            room.load_members();
            self.init_invite_action(room);
            self.scroll_down();
        }

        let model = room.as_ref().map(|room| room.timeline());
        self.selection_model().set_model(model);

        imp.is_loading.set(false);
        imp.message_entry.grab_focus();
        imp.room.replace(room);
        self.update_view();
        self.start_loading();
        self.update_room_state();
        self.update_completion();
        self.notify("room");
        self.notify("empty");
    }

    /// The room currently displayed.
    pub fn room(&self) -> Option<Room> {
        self.imp().room.borrow().clone()
    }

    /// Whether this `RoomHistory` is empty, aka no room is currently displayed.
    pub fn is_empty(&self) -> bool {
        self.imp().room.borrow().is_none()
    }

    /// Whether outgoing messages should be interpreted as markdown.
    pub fn markdown_enabled(&self) -> bool {
        self.imp().md_enabled.get()
    }

    /// Set whether outgoing messages should be interpreted as markdown.
    pub fn set_markdown_enabled(&self, enabled: bool) {
        let imp = self.imp();

        imp.md_enabled.set(enabled);

        self.notify("markdown-enabled");
    }

    /// The type of related event of the composer.
    pub fn related_event_type(&self) -> RelatedEventType {
        self.imp().related_event_type.get()
    }

    /// Set the type of related event of the composer.
    fn set_related_event_type(&self, related_type: RelatedEventType) {
        if self.related_event_type() == related_type {
            return;
        }

        self.imp().related_event_type.set(related_type);
        self.notify("related-event-type");
    }

    /// The related event of the composer.
    pub fn related_event(&self) -> Option<Event> {
        self.imp().related_event.borrow().clone()
    }

    /// Set the related event of the composer.
    fn set_related_event(&self, event: Option<Event>) {
        // We shouldn't reply to events that are not sent yet.
        if let Some(event) = &event {
            if event.event_id().is_none() {
                return;
            }
        }

        let prev_event = self.related_event();

        if prev_event == event {
            return;
        }

        self.imp().related_event.replace(event);
        self.notify("related-event");
    }

    pub fn clear_related_event(&self) {
        self.set_related_event(None);
        self.set_related_event_type(RelatedEventType::default());
    }

    fn selection_model(&self) -> &gtk::NoSelection {
        self.imp()
            .selection_model
            .get_or_init(|| gtk::NoSelection::new(gio::ListModel::NONE.cloned()))
    }

    pub fn set_reply_to(&self, event: Event) {
        let imp = self.imp();
        imp.related_event_header
            .set_widgets(vec![Pill::for_user(event.sender().upcast_ref())]);
        imp.related_event_header
            // Translators: Do NOT translate the content between '{' and '}',
            // this is a variable name. In this string, 'Reply' is a noun.
            .set_label(Some(gettext_f("Reply to {user}", &[("user", "<widget>")])));

        imp.related_event_content.update_for_event(&event);

        self.set_related_event_type(RelatedEventType::Reply);
        self.set_related_event(Some(event));
        imp.message_entry.grab_focus();
    }

    /// Get an iterator over chunks of the message entry's text between the
    /// given start and end, split by mentions.
    fn split_buffer_mentions(&self, start: gtk::TextIter, end: gtk::TextIter) -> SplitMentions {
        SplitMentions { iter: start, end }
    }

    pub fn send_text_message(&self) {
        let imp = self.imp();
        let buffer = imp.message_entry.buffer();
        let (start_iter, end_iter) = buffer.bounds();
        let body_len = buffer.text(&start_iter, &end_iter, true).len();

        let is_markdown = imp.md_enabled.get();
        let mut has_mentions = false;
        let mut plain_body = String::with_capacity(body_len);
        // formatted_body is Markdown if is_markdown is true, and HTML if false.
        let mut formatted_body = String::with_capacity(body_len);

        for chunk in self.split_buffer_mentions(start_iter, end_iter) {
            match chunk {
                MentionChunk::Text(text) => {
                    plain_body.push_str(&text);
                    formatted_body.push_str(&text);
                }
                MentionChunk::Mention { name, uri } => {
                    has_mentions = true;
                    plain_body.push_str(&name);
                    formatted_body.push_str(&if is_markdown {
                        format!("[{name}]({uri})")
                    } else {
                        format!("<a href=\"{uri}\">{name}</a>")
                    });
                }
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

        let content = if is_emote {
            MessageType::Emote(if let Some(html_body) = html_body {
                EmoteMessageEventContent::html(plain_body, html_body)
            } else {
                EmoteMessageEventContent::plain(plain_body)
            })
            .into()
        } else {
            let mut content = if let Some(html_body) = html_body {
                RoomMessageEventContent::text_html(plain_body, html_body)
            } else {
                RoomMessageEventContent::text_plain(plain_body)
            };

            if self.related_event_type() == RelatedEventType::Reply {
                let related_event = self
                    .related_event()
                    .unwrap()
                    .raw()
                    .unwrap()
                    .deserialize()
                    .unwrap();
                if let AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
                    SyncMessageLikeEvent::Original(related_message_event),
                )) = related_event
                {
                    let full_related_message_event = related_message_event
                        .into_full_event(self.room().unwrap().room_id().to_owned());
                    content = content.make_reply_to(&full_related_message_event, ForwardThread::Yes)
                }
            }

            content
        };

        self.room().unwrap().send_room_message_event(content);
        buffer.set_text("");
        self.clear_related_event();
    }

    pub fn leave(&self) {
        if let Some(room) = &*self.imp().room.borrow() {
            room.set_category(RoomType::Left);
        }
    }

    pub async fn permalink(&self) {
        if let Some(room) = self.room() {
            let room = room.matrix_room();
            let handle = spawn_tokio!(async move { room.matrix_to_permalink().await });
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

    fn init_invite_action(&self, room: &Room) {
        let invite_possible = room.own_user_is_allowed_to_expr(PowerLevelAction::Invite);

        let watch = invite_possible.watch(
            glib::Object::NONE,
            clone!(@weak self as obj => move || {
                obj.update_invite_action();
            }),
        );

        self.imp().invite_action_watch.replace(Some(watch));
        self.update_invite_action();
    }

    fn update_invite_action(&self) {
        if let Some(invite_action) = &*self.imp().invite_action_watch.borrow() {
            let allow_invite = invite_action
                .evaluate_as::<bool>()
                .expect("Created expression needs to be valid and a boolean");
            self.action_set_enabled("room-history.invite-members", allow_invite);
        };
    }

    /// Opens the room details on the page with the given name.
    pub fn open_room_details(&self, page_name: room_details::PageName) {
        if let Some(room) = self.room() {
            let window = RoomDetails::new(&self.parent_window(), &room);
            window.set_visible_page(page_name);
            window.present();
        }
    }

    fn update_room_state(&self) {
        let imp = self.imp();

        if let Some(room) = &*imp.room.borrow() {
            let menu_visible = if room.category() == RoomType::Left {
                self.action_set_enabled("room-history.leave", false);
                false
            } else {
                self.action_set_enabled("room-history.leave", true);
                true
            };
            imp.room_menu.set_visible(menu_visible);
        }
    }

    fn update_view(&self) {
        let imp = self.imp();

        if let Some(room) = &*imp.room.borrow() {
            if room.timeline().is_empty() {
                if room.timeline().state() == TimelineState::Error {
                    imp.stack.set_visible_child(&*imp.error);
                } else {
                    imp.stack.set_visible_child(&*imp.loading);
                }
            } else {
                imp.stack.set_visible_child(&*imp.content);
            }
        }
    }

    /// Whether we need to load more messages.
    fn need_messages(&self) -> bool {
        let Some(room) = self.room() else {
            return false;
        };
        let timeline = room.timeline();
        let adj = self.imp().listview.vadjustment().unwrap();

        if adj.value() <= 0.0 && timeline.n_items() > 0 {
            // The room history is loading the timeline items, so wait until they are done.
            return false;
        }

        // Load more messages when the user gets close to the end of the known room
        // history. Use the page size twice to detect if the user gets close to
        // the end.
        adj.value() < adj.page_size() * 2.0 || adj.upper() <= adj.page_size() / 2.0
    }

    fn start_loading(&self) {
        let imp = self.imp();

        if imp.is_loading.get() {
            return;
        }

        let Some(room) = self.room() else {
            return;
        };
        let timeline = room.timeline();

        if timeline.state() == TimelineState::Initial {
            // Retry when the timeline is ready.
            imp.load_when_timeline_ready.set(true);
        }

        if !self.need_messages() && !room.timeline().is_empty() {
            return;
        }

        imp.is_loading.set(true);

        let obj_weak = self.downgrade();
        spawn!(glib::PRIORITY_DEFAULT_IDLE, async move {
            room.timeline().load().await;

            // Remove the task
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().is_loading.set(false);
            }
        });
    }

    /// Returns the parent GtkWindow containing this widget.
    fn parent_window(&self) -> Option<gtk::Window> {
        self.root()?.downcast().ok()
    }

    /// Whether the room history should stick to the newest message in the
    /// timeline.
    pub fn sticky(&self) -> bool {
        self.imp().sticky.get()
    }

    /// Set whether the room history should stick to the newest message in the
    /// timeline.
    pub fn set_sticky(&self, sticky: bool) {
        let imp = self.imp();

        if self.sticky() == sticky {
            return;
        }

        imp.scroll_btn_revealer.set_reveal_child(!sticky);

        imp.sticky.set(sticky);
        self.notify("sticky");
    }

    /// Scroll to the newest message in the timeline
    pub fn scroll_down(&self) {
        let imp = self.imp();

        imp.is_auto_scrolling.set(true);

        imp.scrolled_window
            .emit_scroll_child(gtk::ScrollType::End, false);
    }

    /// Set `RoomHistory` to stick to the bottom based on scrollbar position
    pub fn enable_sticky_mode(&self) {
        let imp = self.imp();
        let adj = imp.listview.vadjustment().unwrap();
        let is_at_bottom = adj.value() + adj.page_size() == adj.upper();
        self.set_sticky(is_at_bottom);
    }

    fn try_again(&self) {
        self.start_loading();
    }

    fn open_emoji(&self) {
        self.imp().message_entry.emit_insert_emoji();
    }

    async fn send_location(&self) -> ashpd::Result<()> {
        let Some(room) = self.room() else {
            return Ok(());
        };

        let handle = spawn_tokio!(async move {
            let proxy = LocationProxy::new().await?;
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

            ashpd::Result::Ok(location)
        });

        let location = handle.await.unwrap()?;
        let geo_uri = GeoUri::builder()
            .latitude(location.latitude())
            .longitude(location.longitude())
            .build()
            .expect("Got invalid coordinates from ashpd");

        let window = self.root().unwrap().downcast::<gtk::Window>().unwrap();
        let dialog = AttachmentDialog::for_location(&window, &gettext("Your Location"), &geo_uri);
        if dialog.run_future().await != gtk::ResponseType::Ok {
            return Ok(());
        }

        let geo_uri_string = geo_uri.to_string();
        let iso8601_datetime =
            glib::DateTime::from_unix_local(location.timestamp().as_secs() as i64)
                .expect("Valid location timestamp");
        let location_body = gettext_f(
            // Translators: Do NOT translate the content between '{' and '}', this is a variable
            // name.
            "User Location {geo_uri} at {iso8601_datetime}",
            &[
                ("geo_uri", &geo_uri_string),
                (
                    "iso8601_datetime",
                    iso8601_datetime.format_iso8601().unwrap().as_str(),
                ),
            ],
        );
        room.send_room_message_event(AnyMessageLikeEventContent::RoomMessage(
            RoomMessageEventContent::new(MessageType::Location(LocationMessageEventContent::new(
                location_body,
                geo_uri_string,
            ))),
        ));

        Ok(())
    }

    async fn send_image(&self, image: gdk::Texture) {
        let window = self.root().unwrap().downcast::<gtk::Window>().unwrap();
        let filename = filename_for_mime(Some(mime::IMAGE_PNG.as_ref()), None);
        let dialog = AttachmentDialog::for_image(&window, &filename, &image);

        if dialog.run_future().await != gtk::ResponseType::Ok {
            return;
        }

        let Some(room) = self.room() else {
            return;
        };

        let bytes = image.save_to_png_bytes();
        let info = AttachmentInfo::Image(BaseImageInfo {
            width: Some((image.width() as u32).into()),
            height: Some((image.height() as u32).into()),
            size: Some((bytes.len() as u32).into()),
            blurhash: None,
        });

        room.send_attachment(bytes.to_vec(), mime::IMAGE_PNG, &filename, info);
    }

    pub async fn select_file(&self) {
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Select File"))
            .modal(true)
            .accept_label(gettext("Select"))
            .build();

        match dialog
            .open_future(
                self.root()
                    .as_ref()
                    .and_then(|r| r.downcast_ref::<gtk::Window>()),
            )
            .await
        {
            Ok(file) => {
                self.send_file(file).await;
            }
            Err(error) => {
                if error.matches(gtk::DialogError::Dismissed) {
                    debug!("File dialog dismissed by user");
                } else {
                    error!("Could not open file: {error:?}");
                    toast!(self, gettext("Could not open file"));
                }
            }
        };
    }

    async fn send_file(&self, file: gio::File) {
        match load_file(&file).await {
            Ok((bytes, file_info)) => {
                let window = self.root().unwrap().downcast::<gtk::Window>().unwrap();
                let dialog = AttachmentDialog::for_file(&window, &file_info.filename, &file);

                if dialog.run_future().await != gtk::ResponseType::Ok {
                    return;
                }

                let Some(room) = self.room() else {
                    error!("Cannot send file without a room");
                    return;
                };

                let size = file_info.size.map(Into::into);
                let info = match file_info.mime.type_() {
                    mime::IMAGE => {
                        let mut info = get_image_info(&file).await;
                        info.size = size;
                        AttachmentInfo::Image(info)
                    }
                    mime::VIDEO => {
                        let mut info = get_video_info(&file).await;
                        info.size = size;
                        AttachmentInfo::Video(info)
                    }
                    mime::AUDIO => {
                        let mut info = get_audio_info(&file).await;
                        info.size = size;
                        AttachmentInfo::Audio(info)
                    }
                    _ => AttachmentInfo::File(BaseFileInfo { size }),
                };

                room.send_attachment(bytes, file_info.mime, &file_info.filename, info);
            }
            Err(error) => {
                warn!("Could not read file: {error}");
                toast!(self, gettext("Error reading file"));
            }
        }
    }

    fn setup_drop_target(&self) {
        let imp = self.imp();

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
                        toast!(
                            obj,
                            gettext("Error getting file from drop")
                        );

                        false
                    }
                }
            }),
        );

        imp.drag_overlay.set_drop_target(target);
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

            toast!(self, gettext("Error getting image from clipboard"));
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

            toast!(self, gettext("Error getting file from clipboard"));
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

    // Update the completion for the current room.
    fn update_completion(&self) {
        if let Some(room) = self.room() {
            let completion = &self.imp().completion;
            completion.set_user_id(Some(room.session().user().unwrap().user_id().to_string()));
            completion.set_members(Some(room.members()))
        }
    }

    // Copy the selection in the message entry to the clipboard while replacing
    // mentions.
    fn copy_buffer_selection_to_clipboard(&self) {
        if let Some((start, end)) = self.imp().message_entry.buffer().selection_bounds() {
            let content: String = self
                .split_buffer_mentions(start, end)
                .map(|chunk| match chunk {
                    MentionChunk::Text(str) => str,
                    MentionChunk::Mention { name, .. } => name,
                })
                .collect();
            self.clipboard().set_text(&content);
        }
    }

    #[template_callback]
    fn handle_related_event_click(&self, n_pressed: i32) {
        if n_pressed == 1 {
            if let Some(related_event) = &*self.imp().related_event.borrow() {
                self.scroll_to_event(&related_event.key());
            }
        }
    }

    fn scroll_to_event(&self, key: &EventKey) {
        let room = match self.room() {
            Some(room) => room,
            None => return,
        };

        if let Some(pos) = room.timeline().find_event_position(key) {
            let pos = pos as u32;
            let _ = self
                .imp()
                .listview
                .activate_action("list.scroll-to-item", Some(&pos.to_variant()));
        }
    }

    fn send_typing_notification(&self, typing: bool) {
        if let Some(room) = self.room() {
            room.send_typing_notification(typing);
        }
    }

    /// Trigger the process to update read receipts.
    fn trigger_read_receipts_update(&self) {
        let Some(room) = self.room() else {
            return;
        };

        let timeline = room.timeline();
        if !timeline.is_empty() {
            let imp = self.imp();

            if let Some(source_id) = imp.scroll_timeout.take() {
                source_id.remove();
            }
            if let Some(source_id) = imp.read_timeout.take() {
                source_id.remove();
            }

            // Only send read receipt when scrolling stopped.
            imp.scroll_timeout
                .replace(Some(glib::timeout_add_local_once(
                    SCROLL_TIMEOUT,
                    clone!(@weak self as obj => move || {
                        obj.update_read_receipts();
                    }),
                )));
        }
    }

    /// Update the read receipts.
    fn update_read_receipts(&self) {
        let imp = self.imp();
        imp.scroll_timeout.take();

        if let Some(source_id) = imp.read_timeout.take() {
            source_id.remove();
        }

        imp.read_timeout.replace(Some(glib::timeout_add_local_once(
            READ_TIMEOUT,
            clone!(@weak self as obj => move || {
                obj.update_read_marker();
            }),
        )));

        let last_event_id = self.last_visible_event_id();

        if let Some(event_id) = last_event_id {
            spawn!(clone!(@weak self as obj => async move {
                obj.send_receipt(ReceiptType::Read, event_id).await;
            }));
        }
    }

    /// Update the read marker.
    fn update_read_marker(&self) {
        let imp = self.imp();
        imp.read_timeout.take();

        let last_event_id = self.last_visible_event_id();

        if let Some(event_id) = last_event_id {
            spawn!(clone!(@weak self as obj => async move {
                obj.send_receipt(ReceiptType::FullyRead, event_id).await;
            }));
        }
    }

    /// Get the ID of the last visible event in the room history.
    fn last_visible_event_id(&self) -> Option<OwnedEventId> {
        let listview = &*self.imp().listview;
        let mut child = listview.last_child();
        // The visible part of the listview spans between 0 and max.
        let max = listview.height() as f64;

        while let Some(item) = child {
            // Vertical position of the top of the item.
            let (_, top_pos) = item.translate_coordinates(listview, 0.0, 0.0).unwrap();
            // Vertical position of the bottom of the item.
            let (_, bottom_pos) = item
                .translate_coordinates(listview, 0.0, item.height() as f64)
                .unwrap();

            let top_in_view = top_pos > 0.0 && top_pos <= max;
            let bottom_in_view = bottom_pos > 0.0 && bottom_pos <= max;
            // If a message is too big and takes more space than the current view.
            let content_in_view = top_pos <= max && bottom_pos > 0.0;
            if top_in_view || bottom_in_view || content_in_view {
                if let Some(event_id) = item
                    .first_child()
                    .and_then(|child| child.downcast::<ItemRow>().ok())
                    .and_then(|row| row.item())
                    .and_then(|item| item.downcast::<Event>().ok())
                    .and_then(|event| event.event_id())
                {
                    return Some(event_id);
                }
            }

            child = item.prev_sibling();
        }

        None
    }

    /// Send the given receipt.
    async fn send_receipt(&self, receipt_type: ReceiptType, event_id: OwnedEventId) {
        let Some(room) = self.room() else {
            return;
        };

        let matrix_timeline = room.timeline().matrix_timeline();
        let handle = spawn_tokio!(async move {
            matrix_timeline
                .send_single_receipt(receipt_type, ReceiptThread::Unthreaded, event_id)
                .await
        });

        if let Err(error) = handle.await.unwrap() {
            error!("Failed to send read receipt: {error}");
        }
    }
}

enum MentionChunk {
    Text(String),
    Mention { name: String, uri: String },
}

struct SplitMentions {
    iter: gtk::TextIter,
    end: gtk::TextIter,
}

impl Iterator for SplitMentions {
    type Item = MentionChunk;

    fn next(&mut self) -> Option<Self::Item> {
        if self.iter == self.end {
            // We reached the end.
            return None;
        }

        if let Some(pill) = self
            .iter
            .child_anchor()
            .map(|anchor| anchor.widgets())
            .as_ref()
            .and_then(|widgets| widgets.first())
            .and_then(|widget| widget.downcast_ref::<Pill>())
        {
            // This chunk is a mention.
            let (name, uri) = if let Some(user) = pill.user() {
                (
                    user.display_name(),
                    user.user_id().matrix_to_uri().to_string(),
                )
            } else if let Some(room) = pill.room() {
                (
                    room.display_name(),
                    room.room_id().matrix_to_uri().to_string(),
                )
            } else {
                unreachable!()
            };

            self.iter.forward_cursor_position();

            return Some(MentionChunk::Mention { name, uri });
        }

        // This chunk is not a mention. Go forward until the next mention or the
        // end and return the text in between.
        let start = self.iter;
        while self.iter.forward_cursor_position() && self.iter != self.end {
            if self
                .iter
                .child_anchor()
                .map(|anchor| anchor.widgets())
                .as_ref()
                .and_then(|widgets| widgets.first())
                .and_then(|widget| widget.downcast_ref::<Pill>())
                .is_some()
            {
                break;
            }
        }

        let text = self.iter.buffer().text(&start, &self.iter, false);
        // We might somehow have an empty string before the end, or at the end,
        // because of hidden `char`s in the buffer, so we must only return
        // `None` when we have an empty string at the end.
        if self.iter == self.end && text.is_empty() {
            None
        } else {
            Some(MentionChunk::Text(text.into()))
        }
    }
}

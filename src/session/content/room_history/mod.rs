mod divider_row;
mod item_row;
mod message_row;
mod state_row;
mod verification_info_bar;

use adw::subclass::prelude::*;
use gtk::{
    gdk, glib,
    glib::{clone, signal::Inhibit},
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
};
use matrix_sdk::ruma::events::room::message::{
    EmoteMessageEventContent, FormattedBody, MessageType, RoomMessageEventContent,
    TextMessageEventContent,
};
use sourceview::prelude::*;

use self::{
    divider_row::DividerRow, item_row::ItemRow, state_row::StateRow,
    verification_info_bar::VerificationInfoBar,
};
use crate::{
    components::{CustomEntry, Pill, RoomTitle},
    session::{
        content::{MarkdownPopover, RoomDetails},
        room::{Item, Room, RoomType, Timeline, TimelineState},
        user::UserExt,
    },
    spawn,
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{signal::SignalHandlerId, subclass::InitializingObject};

    use super::*;
    use crate::Application;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/content-room-history.ui")]
    pub struct RoomHistory {
        pub compact: Cell<bool>,
        pub room: RefCell<Option<Room>>,
        pub category_handler: RefCell<Option<SignalHandlerId>>,
        pub empty_timeline_handler: RefCell<Option<SignalHandlerId>>,
        pub state_timeline_handler: RefCell<Option<SignalHandlerId>>,
        pub md_enabled: Cell<bool>,
        pub is_auto_scrolling: Cell<bool>,
        pub sticky: Cell<bool>,
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
            // Needed to use the natural height of GtkPictures
            self.listview
                .set_vscroll_policy(gtk::ScrollablePolicy::Natural);

            self.listview
                .connect_activate(clone!(@weak obj => move |listview, pos| {
                    if let Some(item) = listview
                        .model()
                        .and_then(|model| model.item(pos))
                        .and_then(|o| o.downcast::<Item>().ok())
                    {
                        if let Some(event) = item.event() {
                            if let Some(room) = obj.room() {
                                room.session().show_media(event);
                            }
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
                            user.user_id().matrix_to_url().to_string(),
                            user.display_name(),
                        )
                    })
                    .or_else(|| {
                        pill.room().map(|room| {
                            (
                                // No server name needed. matrix.to URIs for mentions aren't
                                // routable
                                room.room_id().matrix_to_url([]).to_string(),
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

        self.room().unwrap().send_message(content);
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
}

impl Default for RoomHistory {
    fn default() -> Self {
        Self::new()
    }
}

mod explore;
mod invite;
mod markdown_popover;
mod room_details;
mod room_history;
pub mod verification;

use adw::subclass::prelude::*;
use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};

use self::{
    explore::Explore, invite::Invite, markdown_popover::MarkdownPopover, room_details::RoomDetails,
    room_history::RoomHistory, verification::IdentityVerificationWidget,
};
use crate::session::{
    room::{Room, RoomType},
    sidebar::{Entry, EntryType},
    verification::{IdentityVerification, VerificationMode},
    Session,
};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{object::WeakRef, signal::SignalHandlerId, subclass::InitializingObject};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content.ui")]
    pub struct Content {
        pub compact: Cell<bool>,
        pub session: RefCell<Option<WeakRef<Session>>>,
        pub item: RefCell<Option<glib::Object>>,
        pub error_list: RefCell<Option<gio::ListStore>>,
        pub signal_handler: RefCell<Option<SignalHandlerId>>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub room_history: TemplateChild<RoomHistory>,
        #[template_child]
        pub invite: TemplateChild<Invite>,
        #[template_child]
        pub explore: TemplateChild<Explore>,
        #[template_child]
        pub empty_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub verification_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub identity_verification_widget: TemplateChild<IdentityVerificationWidget>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Content {
        const NAME: &'static str = "Content";
        type Type = super::Content;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            RoomHistory::static_type();
            Invite::static_type();
            Explore::static_type();
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Group);

            klass.install_action("content.go-back", None, move |widget, _, _| {
                widget.set_item(None);
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Content {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::new(
                        "session",
                        "Session",
                        "The session",
                        Session::static_type(),
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "compact",
                        "Compact",
                        "Whether a compact view is used",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecObject::new(
                        "item",
                        "Item",
                        "The item currently shown",
                        glib::Object::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecObject::new(
                        "error-list",
                        "Error List",
                        "A list of errors shown as in-app-notification",
                        gio::ListStore::static_type(),
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
                "session" => obj.set_session(value.get().unwrap()),
                "item" => obj.set_item(value.get().unwrap()),
                "error-list" => {
                    self.error_list.replace(value.get().unwrap());
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "compact" => self.compact.get().to_value(),
                "session" => obj.session().to_value(),
                "item" => obj.item().to_value(),
                "error-list" => self.error_list.borrow().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);
            self.stack
                .connect_visible_child_notify(clone!(@weak obj => move |stack| {
                    let priv_ = obj.imp();
                    if stack.visible_child().as_ref() != Some(priv_.verification_page.upcast_ref::<gtk::Widget>()) {
                        priv_.identity_verification_widget.set_request(None);
                    }
                }));
        }
    }

    impl WidgetImpl for Content {}
    impl BinImpl for Content {}
}

glib::wrapper! {
    pub struct Content(ObjectSubclass<imp::Content>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Content {
    pub fn new(session: &Session) -> Self {
        glib::Object::new(&[("session", session)]).expect("Failed to create Content")
    }

    pub fn session(&self) -> Option<Session> {
        self.imp()
            .session
            .borrow()
            .as_ref()
            .and_then(|session| session.upgrade())
    }

    pub fn set_session(&self, session: Option<Session>) {
        if session == self.session() {
            return;
        }

        self.imp()
            .session
            .replace(session.map(|session| session.downgrade()));
        self.notify("session");
    }

    pub fn set_item(&self, item: Option<glib::Object>) {
        let priv_ = self.imp();

        if self.item() == item {
            return;
        }

        if let Some(signal_handler) = priv_.signal_handler.take() {
            if let Some(item) = self.item() {
                item.disconnect(signal_handler);
            }
        }

        if let Some(ref item) = item {
            if item.is::<Room>() {
                let handler_id = item.connect_notify_local(
                    Some("category"),
                    clone!(@weak self as obj => move |_, _| {
                            obj.set_visible_child();
                    }),
                );

                priv_.signal_handler.replace(Some(handler_id));
            }

            if item.is::<IdentityVerification>() {
                let handler_id = item.connect_notify_local(
                    Some("state"),
                    clone!(@weak self as obj => move |request, _| {
                        let request = request.downcast_ref::<IdentityVerification>().unwrap();
                        if request.is_finished() {
                            obj.set_item(None);
                        }
                    }),
                );
                priv_.signal_handler.replace(Some(handler_id));
            }
        }

        priv_.item.replace(item);
        self.set_visible_child();
        self.notify("item");
    }

    pub fn item(&self) -> Option<glib::Object> {
        self.imp().item.borrow().clone()
    }

    fn set_visible_child(&self) {
        let priv_ = self.imp();

        match self.item() {
            None => {
                priv_.stack.set_visible_child(&*priv_.empty_page);
            }
            Some(o) if o.is::<Room>() => {
                if let Some(room) = priv_
                    .item
                    .borrow()
                    .as_ref()
                    .and_then(|item| item.downcast_ref::<Room>())
                {
                    if room.category() == RoomType::Invited {
                        priv_.invite.set_room(Some(room.clone()));
                        priv_.stack.set_visible_child(&*priv_.invite);
                    } else {
                        priv_.room_history.set_room(Some(room.clone()));
                        priv_.stack.set_visible_child(&*priv_.room_history);
                    }
                }
            }
            Some(o)
                if o.is::<Entry>()
                    && o.downcast_ref::<Entry>().unwrap().type_() == EntryType::Explore =>
            {
                priv_.explore.init();
                priv_.stack.set_visible_child(&*priv_.explore);
            }
            Some(o) if o.is::<IdentityVerification>() => {
                if let Some(item) = priv_
                    .item
                    .borrow()
                    .as_ref()
                    .and_then(|item| item.downcast_ref::<IdentityVerification>())
                {
                    if item.mode() != VerificationMode::CurrentSession {
                        priv_
                            .identity_verification_widget
                            .set_request(Some(item.clone()));
                        priv_.stack.set_visible_child(&*priv_.verification_page);
                    }
                }
            }
            _ => {}
        }
    }
}

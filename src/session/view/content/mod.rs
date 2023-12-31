mod explore;
mod invite;
mod room_details;
mod room_history;
pub mod verification;

use adw::subclass::prelude::*;
use gtk::{glib, glib::clone, prelude::*, CompositeTemplate};

use self::{
    explore::Explore, invite::Invite, room_details::RoomDetails, room_history::RoomHistory,
    verification::IdentityVerificationWidget,
};
use crate::session::model::{
    Entry, EntryType, IdentityVerification, Room, RoomType, Session, VerificationMode,
};

mod imp {
    use std::cell::RefCell;

    use glib::{object::WeakRef, signal::SignalHandlerId, subclass::InitializingObject};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/session/view/content/mod.ui")]
    pub struct Content {
        pub session: WeakRef<Session>,
        pub item_binding: RefCell<Option<glib::Binding>>,
        pub item: RefCell<Option<glib::Object>>,
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
        pub empty_page: TemplateChild<adw::ToolbarView>,
        #[template_child]
        pub verification_page: TemplateChild<adw::ToolbarView>,
        #[template_child]
        pub identity_verification_widget: TemplateChild<IdentityVerificationWidget>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Content {
        const NAME: &'static str = "Content";
        type Type = super::Content;
        type ParentType = adw::NavigationPage;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Content {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Session>("session").build(),
                    glib::ParamSpecObject::builder::<glib::Object>("item")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.set_session(value.get().unwrap()),
                "item" => obj.set_item(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.session().to_value(),
                "item" => obj.item().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.stack
                .connect_visible_child_notify(clone!(@weak self as imp => move |stack| {
                    if stack.visible_child().as_ref() != Some(imp.verification_page.upcast_ref::<gtk::Widget>()) {
                        imp.identity_verification_widget.set_request(None);
                    }
                }));

            if let Some(binding) = self.item_binding.take() {
                binding.unbind()
            }
        }
    }

    impl WidgetImpl for Content {}

    impl NavigationPageImpl for Content {
        fn hidden(&self) {
            self.obj().set_item(None);
        }
    }
}

glib::wrapper! {
    pub struct Content(ObjectSubclass<imp::Content>)
        @extends gtk::Widget, adw::NavigationPage, @implements gtk::Accessible;
}

impl Content {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    pub fn handle_paste_action(&self) {
        let imp = self.imp();
        if imp
            .stack
            .visible_child()
            .as_ref()
            .map(|c| c == imp.room_history.upcast_ref::<gtk::Widget>())
            .unwrap_or_default()
        {
            imp.room_history.handle_paste_action();
        }
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Set the current session.
    pub fn set_session(&self, session: Option<Session>) {
        if session == self.session() {
            return;
        }

        let imp = self.imp();

        if let Some(binding) = imp.item_binding.take() {
            binding.unbind();
        }

        if let Some(session) = &session {
            let item_binding = session
                .sidebar_list_model()
                .selection_model()
                .bind_property("selected-item", self, "item")
                .sync_create()
                .bidirectional()
                .build();

            imp.item_binding.replace(Some(item_binding));
        }

        imp.session.set(session.as_ref());
        self.notify("session");
    }

    /// Set the item currently displayed.
    pub fn set_item(&self, item: Option<glib::Object>) {
        let imp = self.imp();

        if self.item() == item {
            return;
        }

        if let Some(signal_handler) = imp.signal_handler.take() {
            if let Some(item) = self.item() {
                item.disconnect(signal_handler);
            }
        }

        if let Some(ref item) = item {
            if item.is::<Room>() {
                let handler_id = item.connect_notify_local(
                    Some("category"),
                    clone!(@weak self as obj => move |_, _| {
                        obj.update_visible_child();
                    }),
                );

                imp.signal_handler.replace(Some(handler_id));
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
                imp.signal_handler.replace(Some(handler_id));
            }
        }

        imp.item.replace(item);
        self.update_visible_child();
        self.notify("item");
    }

    /// The item currently displayed.
    pub fn item(&self) -> Option<glib::Object> {
        self.imp().item.borrow().clone()
    }

    /// Update the visible child according to the current item.
    fn update_visible_child(&self) {
        let imp = self.imp();

        match self.item() {
            None => {
                imp.stack.set_visible_child(&*imp.empty_page);
            }
            Some(o) if o.is::<Room>() => {
                if let Some(room) = imp.item.borrow().and_downcast_ref::<Room>() {
                    if room.category() == RoomType::Invited {
                        imp.invite.set_room(Some(room.clone()));
                        imp.stack.set_visible_child(&*imp.invite);
                    } else {
                        imp.room_history.set_room(Some(room.clone()));
                        imp.stack.set_visible_child(&*imp.room_history);
                    }
                }
            }
            Some(o)
                if o.is::<Entry>()
                    && o.downcast_ref::<Entry>().unwrap().type_() == EntryType::Explore =>
            {
                imp.explore.init();
                imp.stack.set_visible_child(&*imp.explore);
            }
            Some(o) if o.is::<IdentityVerification>() => {
                if let Some(item) = imp.item.borrow().and_downcast_ref::<IdentityVerification>() {
                    if item.mode() != VerificationMode::CurrentSession {
                        imp.identity_verification_widget
                            .set_request(Some(item.clone()));
                        imp.stack.set_visible_child(&*imp.verification_page);
                    }
                }
            }
            _ => {}
        }
    }
}

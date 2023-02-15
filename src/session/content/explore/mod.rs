mod public_room;
mod public_room_list;
mod public_room_row;
mod server;
mod server_list;
mod server_row;
mod servers_popover;

use adw::subclass::prelude::*;
use gtk::{glib, glib::clone, prelude::*, CompositeTemplate};

pub use self::{
    public_room::PublicRoom, public_room_list::PublicRoomList, public_room_row::PublicRoomRow,
    servers_popover::ExploreServersPopover,
};
use crate::session::Session;

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{object::WeakRef, subclass::InitializingObject};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-explore.ui")]
    pub struct Explore {
        pub compact: Cell<bool>,
        pub session: WeakRef<Session>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub spinner: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub empty_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub servers_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub servers_popover: TemplateChild<ExploreServersPopover>,
        #[template_child]
        pub listview: TemplateChild<gtk::ListView>,
        #[template_child]
        pub scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        pub public_room_list: RefCell<Option<PublicRoomList>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Explore {
        const NAME: &'static str = "ContentExplore";
        type Type = super::Explore;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            PublicRoom::static_type();
            PublicRoomList::static_type();
            PublicRoomRow::static_type();
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Explore {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::builder("compact").build(),
                    glib::ParamSpecObject::builder::<Session>("session")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "compact" => obj.set_compact(value.get().unwrap()),
                "session" => obj.set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "compact" => obj.compact().to_value(),
                "session" => obj.session().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            let adj = self.scrolled_window.vadjustment();

            adj.connect_value_changed(clone!(@weak self as imp => move |adj| {
                if adj.upper() - adj.value() < adj.page_size() * 2.0 {
                    if let Some(public_room_list) = &*imp.public_room_list.borrow() {
                        public_room_list.load_public_rooms(false);
                    }
                }
            }));

            self.search_entry
                .connect_search_changed(clone!(@weak obj => move |_| {
                    obj.trigger_search();
                }));

            self.servers_popover.connect_selected_server_changed(
                clone!(@weak obj => move |_, server| {
                    if let Some(server) = server {
                        obj.imp().servers_button.set_label(server.name());
                        obj.trigger_search();
                    }
                }),
            );
        }
    }

    impl WidgetImpl for Explore {}
    impl BinImpl for Explore {}
}

glib::wrapper! {
    pub struct Explore(ObjectSubclass<imp::Explore>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Explore {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Whether a compact view is used.
    pub fn compact(&self) -> bool {
        self.imp().compact.get()
    }

    /// Set whether a compact view is used.
    pub fn set_compact(&self, compact: bool) {
        self.imp().compact.set(compact)
    }

    pub fn init(&self) {
        let imp = self.imp();

        imp.servers_popover.init();
        imp.servers_button
            .set_label(imp.servers_popover.selected_server().unwrap().name());

        if let Some(public_room_list) = &*imp.public_room_list.borrow() {
            public_room_list.load_public_rooms(true);
        }

        self.imp().search_entry.grab_focus();
    }

    /// Set the current session.
    pub fn set_session(&self, session: Option<Session>) {
        let imp = self.imp();

        if session == self.session() {
            return;
        }

        if let Some(ref session) = session {
            let public_room_list = PublicRoomList::new(session);
            imp.listview
                .set_model(Some(&gtk::NoSelection::new(Some(public_room_list.clone()))));

            public_room_list.connect_notify_local(
                Some("loading"),
                clone!(@weak self as obj => move |_, _| {
                    obj.set_visible_child();
                }),
            );

            public_room_list.connect_notify_local(
                Some("empty"),
                clone!(@weak self as obj => move |_, _| {
                    obj.set_visible_child();
                }),
            );

            imp.public_room_list.replace(Some(public_room_list));
        }

        imp.session.set(session.as_ref());
        self.notify("session");
    }

    fn set_visible_child(&self) {
        let imp = self.imp();
        if let Some(public_room_list) = &*imp.public_room_list.borrow() {
            if public_room_list.loading() {
                imp.stack.set_visible_child(&*imp.spinner);
            } else if public_room_list.empty() {
                imp.stack.set_visible_child(&*imp.empty_label);
            } else {
                imp.stack.set_visible_child(&*imp.scrolled_window);
            }
        }
    }

    fn trigger_search(&self) {
        let imp = self.imp();
        if let Some(public_room_list) = &*imp.public_room_list.borrow() {
            let text = imp.search_entry.text().as_str().to_string();
            let server = imp.servers_popover.selected_server().unwrap();
            public_room_list.search(Some(text), server);
        };
    }
}

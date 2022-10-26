use adw::subclass::prelude::*;
use gtk::{
    glib,
    glib::{clone, FromVariant},
    prelude::*,
    CompositeTemplate,
};
use ruma::ServerName;

use super::{server::Server, server_list::ServerList, server_row::ExploreServerRow};
use crate::session::Session;

mod imp {
    use std::cell::RefCell;

    use glib::{object::WeakRef, subclass::InitializingObject};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-explore-servers-popover.ui")]
    pub struct ExploreServersPopover {
        pub session: WeakRef<Session>,
        pub server_list: RefCell<Option<ServerList>>,
        #[template_child]
        pub listbox: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub server_entry: TemplateChild<gtk::Entry>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ExploreServersPopover {
        const NAME: &'static str = "ContentExploreServersPopover";
        type Type = super::ExploreServersPopover;
        type ParentType = gtk::Popover;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action(
                "explore-servers-popover.add-server",
                None,
                move |obj, _, _| {
                    obj.add_server();
                },
            );
            klass.install_action(
                "explore-servers-popover.remove-server",
                Some("s"),
                move |obj, _, variant| {
                    if let Some(variant) = variant.and_then(String::from_variant) {
                        obj.remove_server(&variant);
                    }
                },
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ExploreServersPopover {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<ServerList>("server-list")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<Session>("session")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "session" => self.obj().set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.session().to_value(),
                "server-list" => obj.server_list().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            self.server_entry
                .connect_changed(clone!(@weak obj => move |_| {
                    obj.update_add_server_state()
                }));
            self.server_entry
                .connect_activate(clone!(@weak obj => move |_| {
                    obj.add_server()
                }));

            obj.update_add_server_state();
        }
    }

    impl WidgetImpl for ExploreServersPopover {}
    impl PopoverImpl for ExploreServersPopover {}
}

glib::wrapper! {
    pub struct ExploreServersPopover(ObjectSubclass<imp::ExploreServersPopover>)
        @extends gtk::Widget, gtk::Popover, @implements gtk::Accessible;
}

impl ExploreServersPopover {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
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

        self.imp().session.set(session.as_ref());
        self.notify("session");
    }

    pub fn init(&self) {
        if let Some(session) = &self.session() {
            let priv_ = self.imp();
            let server_list = ServerList::new(session);

            priv_.listbox.bind_model(Some(&server_list), |obj| {
                ExploreServerRow::new(obj.downcast_ref::<Server>().unwrap()).upcast()
            });

            // Select the first server by default.
            priv_
                .listbox
                .select_row(priv_.listbox.row_at_index(0).as_ref());

            priv_.server_list.replace(Some(server_list));
            self.notify("server-list");
        }
    }

    /// The server list.
    pub fn server_list(&self) -> Option<ServerList> {
        self.imp().server_list.borrow().clone()
    }

    pub fn selected_server(&self) -> Option<Server> {
        self.imp()
            .listbox
            .selected_row()
            .and_then(|row| row.downcast::<ExploreServerRow>().ok())
            .and_then(|row| row.server().cloned())
    }

    pub fn connect_selected_server_changed<F: Fn(&Self, Option<Server>) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.imp()
            .listbox
            .connect_row_selected(clone!(@weak self as obj => move |_, row| {
                f(&obj, row.and_then(|row| row.downcast_ref::<ExploreServerRow>()).and_then(|row| row.server().cloned()));
            }))
    }

    fn can_add_server(&self) -> bool {
        let server = self.imp().server_entry.text();
        ServerName::parse(server.as_str()).is_ok()
            // Don't allow duplicates
            && self
                .server_list()
                .filter(|l| !l.contains_matrix_server(&server))
                .is_some()
    }

    fn update_add_server_state(&self) {
        self.action_set_enabled("explore-servers-popover.add-server", self.can_add_server())
    }

    fn add_server(&self) {
        if !self.can_add_server() {
            return;
        }

        if let Some(server_list) = self.server_list() {
            let priv_ = self.imp();

            let server = priv_.server_entry.text();
            priv_.server_entry.set_text("");

            server_list.add_custom_matrix_server(server.into());
            priv_.listbox.select_row(
                priv_
                    .listbox
                    .row_at_index(server_list.n_items() as i32 - 1)
                    .as_ref(),
            );
        }
    }

    fn remove_server(&self, server: &str) {
        if let Some(server_list) = self.server_list() {
            let priv_ = self.imp();

            // If the selected server is gonna be removed, select the first one.
            if self.selected_server().unwrap().server() == Some(server) {
                priv_
                    .listbox
                    .select_row(priv_.listbox.row_at_index(0).as_ref());
            }

            server_list.remove_custom_matrix_server(server);
        }
    }
}

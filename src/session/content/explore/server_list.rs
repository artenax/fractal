use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use log::error;
use ruma::api::client::thirdparty::get_protocols;

use super::server::Server;
use crate::{prelude::*, session::Session, spawn, spawn_tokio};

mod imp {
    use std::cell::RefCell;

    use glib::object::WeakRef;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ServerList {
        pub session: WeakRef<Session>,
        pub list: RefCell<Vec<Server>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ServerList {
        const NAME: &'static str = "ServerList";
        type Type = super::ServerList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for ServerList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Session>("session")
                    .construct_only()
                    .build()]
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
            match pspec.name() {
                "session" => self.obj().session().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for ServerList {
        fn item_type(&self) -> glib::Type {
            Server::static_type()
        }

        fn n_items(&self) -> u32 {
            self.list.borrow().len() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.list
                .borrow()
                .get(position as usize)
                .map(glib::object::Cast::upcast_ref::<glib::Object>)
                .cloned()
        }
    }
}

glib::wrapper! {
    /// The list of servers to explore.
    pub struct ServerList(ObjectSubclass<imp::ServerList>)
        @implements gio::ListModel;
}

impl ServerList {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// Set the current session.
    fn set_session(&self, session: Session) {
        let priv_ = self.imp();

        priv_.session.set(Some(&session));

        let user_id = session.user().unwrap().user_id();
        priv_.list.replace(vec![Server::with_default_server(
            user_id.server_name().as_str(),
        )]);
        self.items_changed(0, 0, 1);

        spawn!(clone!(@weak self as obj => async move {
            obj.load_servers().await;
        }));
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    async fn load_servers(&self) {
        self.load_protocols().await;

        let custom_servers = self.session().unwrap().settings().explore_custom_servers();
        self.imp().list.borrow_mut().extend(
            custom_servers
                .into_iter()
                .map(|server| Server::with_custom_matrix_server(&server)),
        );

        let added = self.imp().list.borrow().len();
        self.items_changed(1, 0, (added - 1) as u32);
    }

    async fn load_protocols(&self) {
        let client = self.session().unwrap().client();

        let handle =
            spawn_tokio!(async move { client.send(get_protocols::v3::Request::new(), None).await });

        match handle.await.unwrap() {
            Ok(response) => self.add_protocols(response),
            Err(error) => {
                error!("Error loading supported protocols: {}", error);
            }
        }
    }

    fn add_protocols(&self, protocols: get_protocols::v3::Response) {
        let protocols_servers =
            protocols
                .protocols
                .into_iter()
                .flat_map(|(protocol_id, protocol)| {
                    protocol.instances.into_iter().map(move |instance| {
                        Server::with_third_party_protocol(&protocol_id, &instance)
                    })
                });

        self.imp().list.borrow_mut().extend(protocols_servers)
    }

    pub fn contains_matrix_server(&self, server: &str) -> bool {
        let list = &self.imp().list.borrow();
        // The user's matrix server is a special case that doesn't have a "server", so
        // use its name.
        list[0].name() == server || list.iter().any(|s| s.server() == Some(server))
    }

    pub fn add_custom_matrix_server(&self, server_name: String) {
        let server = Server::with_custom_matrix_server(&server_name);
        let pos = {
            let mut list = self.imp().list.borrow_mut();
            let pos = list.len();

            list.push(server);
            pos
        };

        let session = self.session().unwrap();
        let settings = session.settings();
        let mut servers = settings.explore_custom_servers();
        servers.push(server_name);
        settings.set_explore_custom_servers(servers);

        self.items_changed(pos as u32, 0, 1);
    }

    pub fn remove_custom_matrix_server(&self, server_name: &str) {
        let pos = {
            let mut list = self.imp().list.borrow_mut();
            let pos = list
                .iter()
                .position(|s| s.deletable() && s.server() == Some(server_name));

            if let Some(pos) = pos {
                list.remove(pos);
            }
            pos
        };

        if let Some(pos) = pos {
            let session = self.session().unwrap();
            let settings = session.settings();
            let servers = settings
                .explore_custom_servers()
                .into_iter()
                .filter(|s| *s != server_name)
                .collect::<Vec<_>>();
            settings.set_explore_custom_servers(servers);

            self.items_changed(pos as u32, 1, 0);
        }
    }
}

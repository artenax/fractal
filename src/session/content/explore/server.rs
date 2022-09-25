use gtk::{glib, prelude::*, subclass::prelude::*};
use ruma::thirdparty::ProtocolInstance;

mod imp {
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct Server {
        /// The name of the server that is displayed in the list.
        pub name: OnceCell<String>,

        /// The ID of the network that is used during search.
        pub network: OnceCell<String>,

        /// The server name that is used during search.
        pub server: OnceCell<String>,

        /// Whether this server can be deleted from the list.
        pub deletable: OnceCell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Server {
        const NAME: &'static str = "Server";
        type Type = super::Server;
    }

    impl ObjectImpl for Server {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::new(
                        "name",
                        "Name",
                        "The name of the server",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecString::new(
                        "network",
                        "Network",
                        "The ID of the network that is used during search",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecString::new(
                        "server",
                        "Server",
                        "The server name that is used during search",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecBoolean::new(
                        "deletable",
                        "Deletable",
                        "Whether this server can be deleted from the list",
                        false,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "name" => self.name.set(value.get().unwrap()).unwrap(),
                "network" => self.network.set(value.get().unwrap()).unwrap(),
                "server" => {
                    if let Some(server) = value.get().unwrap() {
                        self.server.set(server).unwrap();
                    }
                }
                "deletable" => self.deletable.set(value.get().unwrap()).unwrap(),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "name" => obj.name().to_value(),
                "network" => obj.network().to_value(),
                "server" => obj.server().to_value(),
                "deletable" => obj.deletable().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct Server(ObjectSubclass<imp::Server>);
}

impl Server {
    pub fn with_default_server(name: &str) -> Self {
        glib::Object::new(&[
            ("name", &name),
            ("network", &"matrix"),
            ("deletable", &false),
        ])
        .expect("Failed to create Server")
    }

    pub fn with_third_party_protocol(protocol_id: &str, instance: &ProtocolInstance) -> Self {
        let name = format!("{} ({protocol_id})", instance.desc);
        glib::Object::new(&[
            ("name", &name),
            ("network", &instance.instance_id),
            ("deletable", &false),
        ])
        .expect("Failed to create Server")
    }

    pub fn with_custom_matrix_server(server: &str) -> Self {
        glib::Object::new(&[
            ("name", &server),
            ("network", &"matrix"),
            ("server", &server),
            ("deletable", &true),
        ])
        .expect("Failed to create Server")
    }

    pub fn name(&self) -> &str {
        self.imp().name.get().unwrap()
    }

    pub fn network(&self) -> &str {
        self.imp().network.get().unwrap()
    }

    pub fn server(&self) -> Option<&str> {
        self.imp().server.get().map(String::as_ref)
    }

    pub fn deletable(&self) -> bool {
        *self.imp().deletable.get().unwrap()
    }
}

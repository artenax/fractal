use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use super::server::Server;

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/session/view/content/explore/server_row.ui")]
    pub struct ExploreServerRow {
        /// The server displayed by this row.
        pub server: OnceCell<Server>,
        #[template_child]
        pub remove_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ExploreServerRow {
        const NAME: &'static str = "ExploreServerRow";
        type Type = super::ExploreServerRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ExploreServerRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Server>("server")
                    .construct_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "server" => self.server.set(value.get().unwrap()).unwrap(),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "server" => self.obj().server().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            if let Some(server) = self.obj().server().and_then(|s| s.server()) {
                self.remove_button.set_action_target(Some(&server));
                self.remove_button
                    .set_action_name(Some("explore-servers-popover.remove-server"));
            }
        }
    }

    impl WidgetImpl for ExploreServerRow {}
    impl ListBoxRowImpl for ExploreServerRow {}
}

glib::wrapper! {
    pub struct ExploreServerRow(ObjectSubclass<imp::ExploreServerRow>)
        @extends gtk::Widget, gtk::ListBoxRow, @implements gtk::Accessible;
}

impl ExploreServerRow {
    pub fn new(server: &Server) -> Self {
        glib::Object::builder().property("server", server).build()
    }

    /// The server displayed by this row.
    pub fn server(&self) -> Option<&Server> {
        self.imp().server.get()
    }
}

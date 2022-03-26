use std::cell::Cell;

use adw::subclass::prelude::*;
use gtk::{gdk, glib, prelude::*, subclass::prelude::*, CompositeTemplate};

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/login-advanced-dialog.ui")]
    pub struct LoginAdvancedDialog {
        pub autodiscovery: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LoginAdvancedDialog {
        const NAME: &'static str = "LoginAdvancedDialog";
        type Type = super::LoginAdvancedDialog;
        type ParentType = adw::PreferencesWindow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.add_binding_signal(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                "close-request",
                None,
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LoginAdvancedDialog {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecBoolean::new(
                    "autodiscovery",
                    "Auto-discovery",
                    "Whether auto-discovery is enabled",
                    true,
                    glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "autodiscovery" => obj.autodiscovery().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "autodiscovery" => obj.set_autodiscovery(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for LoginAdvancedDialog {}
    impl WindowImpl for LoginAdvancedDialog {}
    impl AdwWindowImpl for LoginAdvancedDialog {}
    impl PreferencesWindowImpl for LoginAdvancedDialog {}
}

glib::wrapper! {
    pub struct LoginAdvancedDialog(ObjectSubclass<imp::LoginAdvancedDialog>)
        @extends gtk::Widget, gtk::Window, adw::Window, adw::PreferencesWindow, @implements gtk::Accessible;
}

impl LoginAdvancedDialog {
    pub fn new(window: &gtk::Window) -> Self {
        glib::Object::new(&[("transient-for", window)])
            .expect("Failed to create LoginAdvancedDialog")
    }

    pub fn autodiscovery(&self) -> bool {
        self.imp().autodiscovery.get()
    }

    pub fn set_autodiscovery(&self, autodiscovery: bool) {
        let priv_ = self.imp();

        priv_.autodiscovery.set(autodiscovery);
        self.notify("autodiscovery");
    }

    pub async fn run_future(&self) {
        let (sender, receiver) = futures::channel::oneshot::channel();
        let sender = Cell::new(Some(sender));

        self.connect_close_request(move |_| {
            if let Some(sender) = sender.take() {
                sender.send(()).unwrap();
            }
            gtk::Inhibit(false)
        });

        self.show();
        receiver.await.unwrap();
    }
}

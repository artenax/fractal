use gtk::{glib, prelude::*, subclass::prelude::*};

mod imp {
    use std::cell::RefCell;

    use adw::subclass::prelude::*;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct DragOverlay {
        pub overlay: gtk::Overlay,
        pub revealer: gtk::Revealer,
        pub status: adw::StatusPage,
        pub drop_target: RefCell<Option<gtk::DropTarget>>,
        pub handler_id: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DragOverlay {
        const NAME: &'static str = "DragOverlay";
        type Type = super::DragOverlay;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("dragoverlay");
        }
    }

    impl ObjectImpl for DragOverlay {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::builder("title").build(),
                    glib::ParamSpecObject::builder::<gtk::Widget>("child").build(),
                    glib::ParamSpecObject::builder::<gtk::DropTarget>("drop-target")
                        .explicit_notify()
                        .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "title" => obj.title().to_value(),
                "child" => obj.child().to_value(),
                "drop-target" => obj.drop_target().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "title" => obj.set_title(value.get().unwrap()),
                "child" => obj.set_child(value.get().ok().as_ref()),
                "drop-target" => obj.set_drop_target(&value.get().unwrap()),
                _ => unimplemented!(),
            };
        }

        fn constructed(&self) {
            let obj = self.obj();

            self.overlay.set_parent(&*obj);
            self.overlay.add_overlay(&self.revealer);

            self.revealer.set_can_target(false);
            self.revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
            self.revealer.set_reveal_child(false);

            self.status.set_icon_name(Some("document-send-symbolic"));

            self.revealer.set_child(Some(&self.status));
        }

        fn dispose(&self) {
            self.overlay.unparent();
        }
    }
    impl WidgetImpl for DragOverlay {}
    impl BinImpl for DragOverlay {}
}

glib::wrapper! {
    pub struct DragOverlay(ObjectSubclass<imp::DragOverlay>)
        @extends gtk::Widget, adw::Bin;
}

impl DragOverlay {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// The title of this `DragOverlay`.
    pub fn title(&self) -> String {
        self.imp().status.title().into()
    }

    /// Set the title of this `DragOverlay`.
    pub fn set_title(&self, title: &str) {
        self.imp().status.set_title(title)
    }

    /// The child of this `DragOverlay`.
    pub fn child(&self) -> Option<gtk::Widget> {
        self.imp().overlay.child()
    }

    /// Set the child of this `DragOverlay`.
    pub fn set_child(&self, child: Option<&gtk::Widget>) {
        self.imp().overlay.set_child(child)
    }

    /// The [`gtk::DropTarget`] of this `DragOverlay`.
    pub fn drop_target(&self) -> Option<gtk::DropTarget> {
        self.imp().drop_target.borrow().clone()
    }

    /// Set the [`gtk::DropTarget`] of this `DragOverlay`.
    pub fn set_drop_target(&self, drop_target: &gtk::DropTarget) {
        let imp = self.imp();

        if let Some(target) = imp.drop_target.borrow_mut().take() {
            self.remove_controller(&target);

            if let Some(handler_id) = imp.handler_id.borrow_mut().take() {
                target.disconnect(handler_id);
            }
        }

        let handler_id = drop_target.connect_current_drop_notify(
            glib::clone!(@weak imp.revealer as revealer => move |target| {
                revealer.set_reveal_child(target.current_drop().is_some());
            }),
        );
        imp.handler_id.replace(Some(handler_id));

        self.add_controller(drop_target);
        imp.drop_target.replace(Some(drop_target.clone()));
        self.notify("drop-target");
    }
}

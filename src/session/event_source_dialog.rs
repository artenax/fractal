use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};
use sourceview::prelude::*;

use crate::session::room::Event;

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::unsync::OnceCell;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/event-source-dialog.ui")]
    pub struct EventSourceDialog {
        pub event: OnceCell<Event>,
        #[template_child]
        pub source_view: TemplateChild<sourceview::View>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EventSourceDialog {
        const NAME: &'static str = "EventSourceDialog";
        type Type = super::EventSourceDialog;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.install_action("event-source-dialog.copy", None, move |widget, _, _| {
                widget.copy_to_clipboard();
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for EventSourceDialog {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "event",
                    "Event",
                    "The event that is displayed in the Dialog",
                    Event::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                )]
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
                "event" => {
                    let event = value.get().unwrap();
                    let _ = self.event.set(event);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "event" => self.event.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            let buffer = self
                .source_view
                .buffer()
                .downcast::<sourceview::Buffer>()
                .unwrap();

            let json_lang = sourceview::LanguageManager::default().language("json");
            buffer.set_language(json_lang.as_ref());
            crate::utils::setup_style_scheme(&buffer);

            self.parent_constructed(obj);
        }
    }

    impl WidgetImpl for EventSourceDialog {}
    impl WindowImpl for EventSourceDialog {}
    impl AdwWindowImpl for EventSourceDialog {}
}

glib::wrapper! {
    pub struct EventSourceDialog(ObjectSubclass<imp::EventSourceDialog>)
        @extends gtk::Widget, gtk::Window, adw::Window, @implements gtk::Accessible;
}

impl EventSourceDialog {
    pub fn new(window: &gtk::Window, event: &Event) -> Self {
        glib::Object::new(&[("transient-for", window), ("event", event)])
            .expect("Failed to create EventSourceDialog")
    }

    pub fn copy_to_clipboard(&self) {
        let clipboard = self.clipboard();
        let buffer = self.imp().source_view.buffer();
        let (start_iter, end_iter) = buffer.bounds();
        clipboard.set_text(buffer.text(&start_iter, &end_iter, true).as_ref());
    }
}

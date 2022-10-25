use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};
use html2pango::markup;

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/room-title.ui")]
    pub struct RoomTitle {
        // The markup for the title
        pub title: RefCell<Option<String>>,
        // The markup for the subtitle
        pub subtitle: RefCell<Option<String>>,
        #[template_child]
        pub title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub subtitle_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoomTitle {
        const NAME: &'static str = "RoomTitle";
        type Type = super::RoomTitle;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for RoomTitle {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::new(
                        "title",
                        "Title",
                        "The title of the room",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecString::new(
                        "subtitle",
                        "Subtitle",
                        "The subtitle of the room",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "title" => obj.title().to_value(),
                "subtitle" => obj.subtitle().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "title" => obj.set_title(value.get().unwrap()),
                "subtitle" => obj.set_subtitle(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for RoomTitle {}
    impl BinImpl for RoomTitle {}
}

glib::wrapper! {
    pub struct RoomTitle(ObjectSubclass<imp::RoomTitle>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl RoomTitle {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    pub fn set_title(&self, title: Option<String>) {
        let priv_ = self.imp();
        // Parse and escape markup in title
        let title = title.map(|s| markup(&s));
        // If there's an existing title, check that current title and new title aren't
        // equal
        if priv_.title.borrow().as_deref() != title.as_deref() {
            priv_.title.replace(title);
            priv_
                .title_label
                .set_visible(priv_.title.borrow().is_some());
        }

        self.notify("title");
    }

    pub fn title(&self) -> Option<String> {
        self.imp().title.borrow().clone()
    }

    pub fn set_subtitle(&self, subtitle: Option<String>) {
        let priv_ = self.imp();
        // Parse and escape markup in subtitle
        let subtitle = subtitle.map(|s| markup(&s));
        // If there's an existing subtitle, check that current subtitle and new subtitle
        // aren't equal
        if priv_.subtitle.borrow().as_deref() != subtitle.as_deref() {
            priv_.subtitle.replace(subtitle);
            priv_
                .subtitle_label
                .set_visible(priv_.subtitle.borrow().is_some());
        }

        self.notify("subtitle");
    }

    pub fn subtitle(&self) -> Option<String> {
        self.imp().subtitle.borrow().clone()
    }
}

impl Default for RoomTitle {
    fn default() -> Self {
        Self::new()
    }
}

use adw::{prelude::*, subclass::prelude::*};
use gtk::{glib, glib::clone, CompositeTemplate};

use crate::{
    session::{
        content::room_details::history_viewer::{AudioRow, Timeline, TimelineFilter},
        Room,
    },
    spawn,
};

const MIN_N_ITEMS: u32 = 20;

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-audio-history-viewer.ui")]
    pub struct AudioHistoryViewer {
        pub room_timeline: OnceCell<Timeline>,
        #[template_child]
        pub list_view: TemplateChild<gtk::ListView>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AudioHistoryViewer {
        const NAME: &'static str = "ContentAudioHistoryViewer";
        type Type = super::AudioHistoryViewer;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            AudioRow::static_type();
            Self::bind_template(klass);

            klass.set_css_name("audiohistoryviewer");
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AudioHistoryViewer {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Room>("room")
                    .construct_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "room" => self.obj().set_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room" => self.obj().room().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for AudioHistoryViewer {}
    impl BinImpl for AudioHistoryViewer {}
}

glib::wrapper! {
    pub struct AudioHistoryViewer(ObjectSubclass<imp::AudioHistoryViewer>)
        @extends gtk::Widget, adw::Bin;
}

impl AudioHistoryViewer {
    pub fn new(room: &Room) -> Self {
        glib::Object::builder().property("room", room).build()
    }

    fn set_room(&self, room: &Room) {
        let imp = self.imp();

        let timeline = Timeline::new(room, TimelineFilter::Audio);
        let model = gtk::NoSelection::new(Some(timeline.clone()));
        imp.list_view.set_model(Some(&model));

        // Load an initial number of items
        spawn!(clone!(@weak self as obj, @weak timeline => async move {
            while timeline.n_items() < MIN_N_ITEMS {
                if !timeline.load().await {
                    break;
                }
            }

            let adj = obj.imp().list_view.vadjustment().unwrap();
            adj.connect_value_notify(clone!(@weak timeline => move |adj| {
                if adj.value() + adj.page_size() * 2.0 >= adj.upper() {
                    spawn!(async move { timeline.load().await; });
                }
            }));
        }));

        imp.room_timeline.set(timeline).unwrap();
    }

    pub fn room(&self) -> &Room {
        self.imp().room_timeline.get().unwrap().room()
    }
}

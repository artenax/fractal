use adw::{prelude::*, subclass::prelude::*};
use gtk::{glib, glib::clone, CompositeTemplate};

use crate::{
    session::{
        content::room_details::history_viewer::{MediaItem, Timeline, TimelineFilter},
        Room,
    },
    spawn,
};

const MIN_N_ITEMS: u32 = 50;

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-media-history-viewer.ui")]
    pub struct MediaHistoryViewer {
        pub room_timeline: OnceCell<Timeline>,
        #[template_child]
        pub grid_view: TemplateChild<gtk::GridView>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MediaHistoryViewer {
        const NAME: &'static str = "ContentMediaHistoryViewer";
        type Type = super::MediaHistoryViewer;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            MediaItem::static_type();
            Self::bind_template(klass);

            klass.set_css_name("mediahistoryviewer");
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MediaHistoryViewer {
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

    impl WidgetImpl for MediaHistoryViewer {}
    impl BinImpl for MediaHistoryViewer {}
}

glib::wrapper! {
    pub struct MediaHistoryViewer(ObjectSubclass<imp::MediaHistoryViewer>)
        @extends gtk::Widget, adw::Bin;
}

impl MediaHistoryViewer {
    pub fn new(room: &Room) -> Self {
        glib::Object::builder().property("room", room).build()
    }

    fn set_room(&self, room: &Room) {
        let imp = self.imp();

        let timeline = Timeline::new(room, TimelineFilter::Media);
        let model = gtk::NoSelection::new(Some(timeline.clone()));
        imp.grid_view.set_model(Some(&model));

        // Load an initial number of items
        spawn!(clone!(@weak timeline => async move {
            while timeline.n_items() < MIN_N_ITEMS {
                if !timeline.load().await {
                    break;
                }
            }
        }));

        imp.room_timeline.set(timeline).unwrap();
    }

    pub fn room(&self) -> &Room {
        self.imp().room_timeline.get().unwrap().room()
    }
}

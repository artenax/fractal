use adw::{prelude::*, subclass::prelude::*};
use gtk::{glib, glib::clone, CompositeTemplate};
use ruma::events::AnyMessageLikeEventContent;
use tracing::error;

use super::{MediaItem, Timeline, TimelineFilter};
use crate::{
    session::{model::Room, view::MediaViewer},
    spawn,
};

const MIN_N_ITEMS: u32 = 50;

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_details/history_viewer/media.ui"
    )]
    pub struct MediaHistoryViewer {
        pub room_timeline: OnceCell<Timeline>,
        #[template_child]
        pub media_viewer: TemplateChild<MediaViewer>,
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

    pub fn show_media(&self, item: &MediaItem) {
        let imp = self.imp();
        let event = item.event().unwrap();

        let Some(AnyMessageLikeEventContent::RoomMessage(message)) = event.original_content()
        else {
            error!("Trying to open the media viewer with an event that is not a message");
            return;
        };

        imp.media_viewer.set_message(
            &event.room().unwrap(),
            event.matrix_event().0.event_id().into(),
            message.msgtype,
        );
        imp.media_viewer.reveal(item);
    }

    fn set_room(&self, room: &Room) {
        let imp = self.imp();

        let timeline = Timeline::new(room, TimelineFilter::Media);
        let model = gtk::NoSelection::new(Some(timeline.clone()));
        imp.grid_view.set_model(Some(&model));

        // Load an initial number of items
        spawn!(clone!(@weak self as obj, @weak timeline => async move {
            while timeline.n_items() < MIN_N_ITEMS {
                if !timeline.load().await {
                    break;
                }
            }

            let adj = obj.imp().grid_view.vadjustment().unwrap();
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

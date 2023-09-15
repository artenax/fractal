use adw::subclass::prelude::*;
use gtk::{glib, glib::clone, prelude::*, CompositeTemplate};
use ruma::UserId;

use crate::{
    components::{Avatar, OverlappingBox},
    prelude::*,
    session::model::Room,
    utils::BoundObjectWeakRef,
};

// Keep in sync with the `max-children` property of the `overlapping_box` in the
// UI file.
const MAX_RECEIPTS_SHOWN: u32 = 10;

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_history/read_receipts_list.ui"
    )]
    pub struct ReadReceiptsList {
        #[template_child]
        pub label: TemplateChild<gtk::Label>,
        #[template_child]
        pub overlapping_box: TemplateChild<OverlappingBox>,

        /// The read receipts that are bound, if any.
        pub bound_receipts: BoundObjectWeakRef<gtk::StringList>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ReadReceiptsList {
        const NAME: &'static str = "ContentReadReceiptsList";
        type Type = super::ReadReceiptsList;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ReadReceiptsList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<gtk::StringList>("list")
                    .read_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "list" => obj.list().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            self.bound_receipts.disconnect_signals();
        }
    }

    impl WidgetImpl for ReadReceiptsList {}

    impl BinImpl for ReadReceiptsList {}
}

glib::wrapper! {
    /// A widget displaying the read receipts on a message.
    pub struct ReadReceiptsList(ObjectSubclass<imp::ReadReceiptsList>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl ReadReceiptsList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn list(&self) -> Option<gtk::StringList> {
        self.imp().bound_receipts.obj()
    }

    pub fn set_list(&self, room: &Room, read_receipts: &gtk::StringList) {
        let imp = self.imp();

        imp.overlapping_box.bind_model(
            Some(read_receipts),
            clone!(@weak room => @default-return { Avatar::new().upcast() }, move |item| {
                let user_id = UserId::parse(
                    item.downcast_ref::<gtk::StringObject>()
                        .unwrap()
                        .string()
                    )
                    .expect("Strings in read receipts list are valid UserIds");
                let member = room.members().get_or_create(user_id);

                let avatar_data = member.avatar_data();
                let avatar = Avatar::new();
                avatar.set_size(20);
                avatar.set_data(Some(avatar_data.clone()));
                avatar.upcast()
            }),
        );

        let items_changed_handler_id = read_receipts.connect_items_changed(
            clone!(@weak self as obj => move |read_receipts, _, _, _| {
                obj.update_label(read_receipts);
            }),
        );

        imp.bound_receipts
            .set(read_receipts, vec![items_changed_handler_id]);
        self.update_label(read_receipts);
        self.notify("list");
    }

    fn update_label(&self, read_receipts: &gtk::StringList) {
        let label = &self.imp().label;
        let n_items = read_receipts.n_items();
        if n_items > MAX_RECEIPTS_SHOWN {
            label.set_text(&format!("{} +", n_items - MAX_RECEIPTS_SHOWN));
        } else {
            label.set_text("");
        }
    }
}

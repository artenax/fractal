use adw::{
    prelude::*,
    subclass::{bin::BinImpl, prelude::*},
};
use gtk::{gio, glib, CompositeTemplate};

pub mod extra_lists;
mod item_row;
mod member_row;
mod membership_subpage_item;
mod membership_subpage_row;

use item_row::ItemRow;
use member_row::MemberRow;
pub use membership_subpage_item::MembershipSubpageItem;
use membership_subpage_row::MembershipSubpageRow;

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-member-page-list-view.ui")]
    pub struct MembersListView {
        #[template_child]
        pub members_list_view: TemplateChild<gtk::ListView>,
        pub model: glib::WeakRef<gio::ListModel>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MembersListView {
        const NAME: &'static str = "ContentMembersListView";
        type Type = super::MembersListView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            ItemRow::static_type();
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MembersListView {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "model",
                    "Model",
                    "The model used for this view",
                    gio::ListModel::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "model" => obj.set_model(value.get::<&gio::ListModel>().ok()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "model" => obj.model().to_value(),
                _ => unimplemented!(),
            }
        }
    }
    impl WidgetImpl for MembersListView {}
    impl BinImpl for MembersListView {}
}

glib::wrapper! {
    pub struct MembersListView(ObjectSubclass<imp::MembersListView>)
        @extends gtk::Widget, adw::Bin;
}

impl MembersListView {
    pub fn new(model: &impl IsA<gio::ListModel>) -> Self {
        glib::Object::new(&[("model", model)]).expect("Failed to create MembersListView")
    }

    pub fn model(&self) -> Option<gio::ListModel> {
        self.imp().model.upgrade()
    }

    pub fn set_model(&self, model: Option<&impl IsA<gio::ListModel>>) {
        let model: Option<&gio::ListModel> = model.map(|model| model.upcast_ref());
        if self.model().as_ref() == model {
            return;
        }

        self.imp()
            .members_list_view
            .set_model(Some(&gtk::NoSelection::new(model)));

        self.imp().model.set(model);
        self.notify("model");
    }
}

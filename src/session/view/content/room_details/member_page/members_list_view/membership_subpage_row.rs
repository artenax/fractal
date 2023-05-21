use adw::subclass::prelude::*;
use gtk::{gdk, glib, glib::clone, prelude::*, CompositeTemplate};

use super::MembershipSubpageItem;

mod imp {
    use std::cell::RefCell;

    use glib::{signal::SignalHandlerId, subclass::InitializingObject};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_details/member_page/members_list_view/membership_subpage_row.ui"
    )]
    pub struct MembershipSubpageRow {
        /// The item of this row.
        pub item: RefCell<Option<MembershipSubpageItem>>,
        pub gesture: gtk::GestureClick,
        #[template_child]
        pub members_count: TemplateChild<gtk::Label>,
        pub members_count_handler_id: RefCell<Option<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MembershipSubpageRow {
        const NAME: &'static str = "ContentMemberPageMembershipSubpageRow";
        type Type = super::MembershipSubpageRow;
        type ParentType = adw::ActionRow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MembershipSubpageRow {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<MembershipSubpageItem>("item")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecString::builder("label").read_only().build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "item" => self.obj().set_item(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "item" => obj.item().to_value(),
                "label" => obj.label().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            self.gesture.set_touch_only(false);
            self.gesture.set_button(gdk::BUTTON_PRIMARY);

            self.gesture
                .connect_released(clone!(@weak obj => move |_, _, _, _| {
                    if let Some(item) = obj.item() {
                        obj.activate_action(
                            "members.subpage",
                            Some(&item.state().to_variant()),
                        )
                        .unwrap();
                    }
                }));

            self.gesture
                .set_propagation_phase(gtk::PropagationPhase::Capture);
            obj.add_controller(self.gesture.clone());
        }
    }

    impl WidgetImpl for MembershipSubpageRow {}
    impl ListBoxRowImpl for MembershipSubpageRow {}
    impl PreferencesRowImpl for MembershipSubpageRow {}
    impl ActionRowImpl for MembershipSubpageRow {}
}

glib::wrapper! {
    pub struct MembershipSubpageRow(ObjectSubclass<imp::MembershipSubpageRow>)
        @extends gtk::Widget, adw::ActionRow, @implements gtk::Accessible;
}

impl MembershipSubpageRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The item of this row.
    pub fn item(&self) -> Option<MembershipSubpageItem> {
        self.imp().item.borrow().clone()
    }

    /// Set the item of this row.
    pub fn set_item(&self, item: Option<MembershipSubpageItem>) {
        let imp = self.imp();
        let prev_item = self.item();

        if prev_item == item {
            return;
        }

        if let Some(signal_id) = imp.members_count_handler_id.take() {
            if let Some(prev_item) = prev_item {
                prev_item.disconnect(signal_id);
            }
        }

        if let Some(item) = item.as_ref() {
            let model = item.model();
            let signal_id =
                model.connect_items_changed(clone!(@weak self as obj => move |model, _, _, _| {
                    obj.member_count_changed(model.n_items());
                }));

            self.member_count_changed(model.n_items());

            self.imp().members_count_handler_id.replace(Some(signal_id));
        }

        self.imp().item.replace(item);
        self.notify("item");
        self.notify("label");
    }

    /// The label of this row.
    pub fn label(&self) -> Option<String> {
        Some(self.item()?.state().to_string())
    }

    fn member_count_changed(&self, n: u32) {
        self.imp().members_count.set_text(&format!("{n}"));
    }
}

use adw::subclass::prelude::BinImpl;
use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::{
    components::{Avatar, Badge},
    session::{
        content::room_details::{member_page::MemberMenu, MemberPage},
        room::Member,
    },
};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-member-row.ui")]
    pub struct MemberRow {
        pub member: RefCell<Option<Member>>,
        #[template_child]
        pub menu_btn: TemplateChild<gtk::ToggleButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MemberRow {
        const NAME: &'static str = "ContentMemberRow";
        type Type = super::MemberRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Avatar::static_type();
            Badge::static_type();
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MemberRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Member>("member")
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "member" => {
                    self.obj().set_member(value.get().unwrap());
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "member" => self.obj().member().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            self.menu_btn
                .connect_toggled(clone!(@weak obj => move |btn| {
                    if btn.is_active() {
                        if let Some(menu) = obj.member_menu() {
                            menu.present_popover(btn, obj.member());
                        }
                    }
                }));
        }
    }
    impl WidgetImpl for MemberRow {}
    impl BinImpl for MemberRow {}
}

glib::wrapper! {
    pub struct MemberRow(ObjectSubclass<imp::MemberRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl MemberRow {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// The member displayed by this row.
    pub fn member(&self) -> Option<Member> {
        self.imp().member.borrow().clone()
    }

    /// Set the member displayed by this row.
    pub fn set_member(&self, member: Option<Member>) {
        let imp = self.imp();

        if self.member() == member {
            return;
        }

        // We need to update the member of the menu if it's shown for this row
        if imp.menu_btn.is_active() {
            if let Some(menu) = self.member_menu() {
                menu.set_member(member.clone());
            }
        }

        imp.member.replace(member);
        self.notify("member");
    }

    fn member_menu(&self) -> Option<MemberMenu> {
        let member_page = self
            .ancestor(MemberPage::static_type())?
            .downcast::<MemberPage>()
            .unwrap();
        Some(member_page.member_menu().clone())
    }
}

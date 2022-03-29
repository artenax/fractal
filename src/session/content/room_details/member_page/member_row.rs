use adw::subclass::prelude::BinImpl;
use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::session::{content::RoomDetails, room::Member};

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
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MemberRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "member",
                    "Member",
                    "The member this row is showing",
                    Member::static_type(),
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
                "member" => {
                    obj.set_member(value.get().unwrap());
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "member" => obj.member().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.menu_btn
                .connect_toggled(clone!(@weak obj => move |btn| {
                    if let Some(details) = obj.details() {
                        let page = details.member_page();
                        let menu = page.member_menu();
                        if btn.is_active() {
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
    pub fn new(member: &Member) -> Self {
        glib::Object::new(&[("member", member)]).expect("Failed to create MemberRow")
    }

    pub fn member(&self) -> Option<Member> {
        self.imp().member.borrow().clone()
    }

    pub fn set_member(&self, member: Option<Member>) {
        let priv_ = self.imp();

        if self.member() == member {
            return;
        }

        // We need to update the member of the menu if it's shown for this row
        if priv_.menu_btn.is_active() {
            if let Some(details) = self.details() {
                let page = details.member_page();
                let menu = page.member_menu();

                menu.set_member(member.clone());
            }
        }

        priv_.member.replace(member);
        self.notify("member");
    }

    fn details(&self) -> Option<RoomDetails> {
        Some(self.root()?.downcast::<RoomDetails>().unwrap())
    }
}

use adw::{prelude::BinExt, subclass::prelude::*};
use gtk::{glib, glib::prelude::*};

use super::{MemberRow, MembershipSubpageItem, MembershipSubpageRow};
use crate::session::room::Member;

mod imp {
    use std::cell::RefCell;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ItemRow {
        pub item: RefCell<Option<glib::Object>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ItemRow {
        const NAME: &'static str = "ContentMemberItemRow";
        type Type = super::ItemRow;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for ItemRow {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "item",
                    "Item",
                    "The membership subpage item represented by this row",
                    glib::Object::static_type(),
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
                "item" => obj.set_item(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "item" => obj.item().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for ItemRow {}
    impl BinImpl for ItemRow {}
}

glib::wrapper! {
    pub struct ItemRow(ObjectSubclass<imp::ItemRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl ItemRow {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create ItemRow")
    }

    pub fn item(&self) -> Option<glib::Object> {
        self.imp().item.borrow().clone()
    }

    fn set_item(&self, item: Option<glib::Object>) {
        if self.item() == item {
            return;
        }

        if let Some(item) = item.as_ref() {
            if let Some(member) = item.downcast_ref::<Member>() {
                let child = if let Some(Ok(child)) = self.child().map(|w| w.downcast::<MemberRow>())
                {
                    child
                } else {
                    let child = MemberRow::new();
                    self.set_child(Some(&child));
                    child
                };
                child.set_member(Some(member.clone()));
            } else if let Some(item) = item.downcast_ref::<MembershipSubpageItem>() {
                let child = if let Some(Ok(child)) =
                    self.child().map(|w| w.downcast::<MembershipSubpageRow>())
                {
                    child
                } else {
                    let child = MembershipSubpageRow::new();
                    self.set_child(Some(&child));
                    child
                };

                child.set_item(Some(item.clone()));
            } else {
                unimplemented!("The object {:?} doesn't have a widget implementation", item);
            }
        }

        self.imp().item.replace(item);
        self.notify("item");
    }
}

impl Default for ItemRow {
    fn default() -> Self {
        Self::new()
    }
}
use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};

use crate::session::content::room_details::member_page::MembershipSubpageItem;

mod imp {
    use std::cell::Cell;

    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct ExtraLists {
        pub invited: OnceCell<MembershipSubpageItem>,
        pub banned: OnceCell<MembershipSubpageItem>,
        pub invited_is_empty: Cell<bool>,
        pub banned_is_empty: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ExtraLists {
        const NAME: &'static str = "ContentMembersExtraLists";
        type Type = super::ExtraLists;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for ExtraLists {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<MembershipSubpageItem>("invited")
                        .construct_only()
                        .build(),
                    glib::ParamSpecObject::builder::<MembershipSubpageItem>("banned")
                        .construct_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "invited" => obj.set_invited(value.get().unwrap()),
                "banned" => obj.set_banned(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "invited" => obj.invited().to_value(),
                "banned" => obj.banned().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let invited_members = obj.invited().model();
            let banned_members = obj.banned().model();

            invited_members.connect_items_changed(clone!(@weak obj => move |_, _, _, _| {
                obj.update_invited();
            }));

            banned_members.connect_items_changed(clone!(@weak obj => move |_, _, _, _| {
                obj.update_banned();
            }));

            self.invited_is_empty.set(invited_members.n_items() == 0);
            self.banned_is_empty.set(banned_members.n_items() == 0);
        }
    }

    impl ListModelImpl for ExtraLists {
        fn item_type(&self) -> glib::Type {
            glib::Object::static_type()
        }

        fn n_items(&self) -> u32 {
            let mut len = 0;

            if !self.invited_is_empty.get() {
                len += 1;
            }
            if !self.banned_is_empty.get() {
                len += 1;
            }

            len
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            let has_invited = !self.invited_is_empty.get();
            let has_banned = !self.banned_is_empty.get();

            if position == 0 && has_invited {
                let invited = self.invited.get().unwrap();
                return Some(invited.clone().upcast());
            }

            if has_banned && ((position == 0 && !has_invited) || (position == 1 && has_invited)) {
                let banned = self.banned.get().unwrap();
                return Some(banned.clone().upcast());
            }

            None
        }
    }
}

glib::wrapper! {
    pub struct ExtraLists(ObjectSubclass<imp::ExtraLists>)
        @implements gio::ListModel;
}

impl ExtraLists {
    pub fn new(invited: &MembershipSubpageItem, banned: &MembershipSubpageItem) -> Self {
        glib::Object::builder()
            .property("invited", invited)
            .property("banned", banned)
            .build()
    }

    /// The subpage item for invited members.
    pub fn invited(&self) -> &MembershipSubpageItem {
        self.imp().invited.get().unwrap()
    }

    /// Set the subpage item for invited members.
    fn set_invited(&self, item: MembershipSubpageItem) {
        self.imp().invited.set(item).unwrap();
    }

    /// The subpage for banned members.
    pub fn banned(&self) -> &MembershipSubpageItem {
        self.imp().banned.get().unwrap()
    }

    /// Set the subpage for banned members.
    fn set_banned(&self, item: MembershipSubpageItem) {
        self.imp().banned.set(item).unwrap();
    }

    fn update_invited(&self) {
        let imp = self.imp();

        let was_empty = imp.invited_is_empty.get();
        let is_empty = self.invited().model().n_items() == 0;

        if was_empty == is_empty {
            // Nothing changed so don't do anything
            return;
        }

        imp.invited_is_empty.set(is_empty);

        let added = if was_empty { 1 } else { 0 };
        // If it is not added, it is removed.
        let removed = 1 - added;

        self.items_changed(0, removed, added);
    }

    fn update_banned(&self) {
        let imp = self.imp();

        let was_empty = imp.banned_is_empty.get();
        let is_empty = self.banned().model().n_items() == 0;

        if was_empty == is_empty {
            // Nothing changed so don't do anything
            return;
        }

        imp.banned_is_empty.set(is_empty);

        let position = if imp.invited_is_empty.get() { 0 } else { 1 };

        let added = if was_empty { 1 } else { 0 };
        // If it is not added, it is removed.
        let removed = 1 - added;

        self.items_changed(position, removed, added);
    }
}

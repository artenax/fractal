use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};

use crate::session::content::room_details::member_page::MembershipSubpageItem;

mod imp {
    use std::cell::Cell;

    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct ExtraLists {
        pub joined: OnceCell<gio::ListModel>,
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
                    glib::ParamSpecObject::new(
                        "joined",
                        "Joined",
                        "The item for the subpage of joined members",
                        gio::ListModel::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecObject::new(
                        "invited",
                        "Invited",
                        "The item for the subpage of invited members",
                        MembershipSubpageItem::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecObject::new(
                        "banned",
                        "Banned",
                        "The item for the subpage of banned members",
                        MembershipSubpageItem::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "joined" => obj.set_joined(value.get().unwrap()),
                "invited" => obj.set_invited(value.get().unwrap()),
                "banned" => obj.set_banned(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "joined" => obj.joined().to_value(),
                "invited" => obj.invited().to_value(),
                "banned" => obj.banned().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let joined_members = obj.joined();
            let invited_members = obj.invited().model();
            let banned_members = obj.banned().model();

            joined_members.connect_items_changed(
                clone!(@weak obj => move |_, position, removed, added| {
                    obj.items_changed(position + obj.n_visible_extras(), removed, added)
                }),
            );

            invited_members.connect_items_changed(clone!(@weak obj => move |_, _, _, _| {
                obj.update_items();
            }));

            banned_members.connect_items_changed(clone!(@weak obj => move |_, _, _, _| {
                obj.update_items();
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
            let obj = self.obj();
            obj.joined().n_items() + obj.n_visible_extras()
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            let obj = self.obj();

            if position == 0 && !self.invited_is_empty.get() {
                let invited = self.invited.get().unwrap();
                return Some(invited.clone().upcast());
            }

            if (position == 0 && self.invited_is_empty.get() && !self.banned_is_empty.get())
                || (position == 1 && !self.banned_is_empty.get())
            {
                let banned = self.banned.get().unwrap();
                return Some(banned.clone().upcast());
            }

            obj.joined().item(position - obj.n_visible_extras())
        }
    }
}

glib::wrapper! {
    pub struct ExtraLists(ObjectSubclass<imp::ExtraLists>)
        @implements gio::ListModel;
}

impl ExtraLists {
    pub fn new(
        joined: &impl IsA<gio::ListModel>,
        invited: &MembershipSubpageItem,
        banned: &MembershipSubpageItem,
    ) -> Self {
        glib::Object::builder()
            .property("joined", joined)
            .property("invited", invited)
            .property("banned", banned)
            .build()
    }

    pub fn joined(&self) -> &gio::ListModel {
        self.imp().joined.get().unwrap()
    }

    fn set_joined(&self, model: gio::ListModel) {
        self.imp().joined.set(model).unwrap();
    }

    pub fn invited(&self) -> &MembershipSubpageItem {
        self.imp().invited.get().unwrap()
    }

    fn set_invited(&self, item: MembershipSubpageItem) {
        self.imp().invited.set(item).unwrap();
    }

    pub fn banned(&self) -> &MembershipSubpageItem {
        self.imp().banned.get().unwrap()
    }

    fn set_banned(&self, item: MembershipSubpageItem) {
        self.imp().banned.set(item).unwrap();
    }

    fn update_items(&self) {
        let priv_ = self.imp();

        let invited_was_empty = priv_.invited_is_empty.get();
        let banned_was_empty = priv_.banned_is_empty.get();

        let invited_is_empty = self.invited().model().n_items() == 0;
        let banned_is_empty = self.banned().model().n_items() == 0;

        let invited_changed = invited_was_empty != invited_is_empty;
        let banned_changed = banned_was_empty != banned_is_empty;

        if !invited_changed && !banned_changed {
            // Nothing changed so don't do anything
            return;
        }

        let mut position = 0;
        let mut removed = 0;
        let mut added = 0;

        if invited_changed {
            if invited_is_empty {
                removed = 1;
            } else {
                added = 1;
            }
        } else if !invited_is_empty {
            position = 1;
        }

        if banned_changed {
            if banned_is_empty {
                removed += 1;
            } else {
                added += 1;
            }
        }

        priv_.invited_is_empty.set(invited_is_empty);
        priv_.banned_is_empty.set(banned_is_empty);

        self.items_changed(position, removed, added);
    }

    fn n_visible_extras(&self) -> u32 {
        let priv_ = self.imp();
        let mut len = 0;
        if !priv_.invited_is_empty.get() {
            len += 1;
        }
        if !priv_.banned_is_empty.get() {
            len += 1;
        }
        len
    }
}

use std::convert::TryFrom;

use gtk::{gio::ListStore, glib, prelude::*, subclass::prelude::*};

use super::{add_account::AddAccountRow, user_entry::UserEntryRow};
use crate::session::Session;

mod imp {
    use std::cell::Cell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct ExtraItemObj(pub Cell<super::ExtraItem>);

    #[glib::object_subclass]
    impl ObjectSubclass for ExtraItemObj {
        const NAME: &'static str = "ExtraItemObj";
        type Type = super::ExtraItemObj;
    }

    impl ObjectImpl for ExtraItemObj {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecEnum::new(
                    "inner",
                    "Inner",
                    "Inner value of ExtraItem",
                    super::ExtraItem::static_type(),
                    0,
                    glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "inner" => obj.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "inner" => self.0.set(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }
    }
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "ExtraItem")]
pub enum ExtraItem {
    Separator = 0,
    AddAccount = 1,
}

impl ExtraItem {
    const VALUES: [Self; 2] = [Self::Separator, Self::AddAccount];
}

impl Default for ExtraItem {
    fn default() -> Self {
        Self::Separator
    }
}

glib::wrapper! {
    pub struct ExtraItemObj(ObjectSubclass<imp::ExtraItemObj>);
}

impl From<&ExtraItem> for ExtraItemObj {
    fn from(item: &ExtraItem) -> Self {
        glib::Object::new(&[("inner", item)]).expect("Failed to create ExtraItem")
    }
}

impl ExtraItemObj {
    pub fn list_store() -> ListStore {
        ExtraItem::VALUES.iter().map(ExtraItemObj::from).fold(
            ListStore::new(ExtraItemObj::static_type()),
            |list_items, item| {
                list_items.append(&item);
                list_items
            },
        )
    }

    pub fn get(&self) -> ExtraItem {
        self.imp().0.get()
    }

    pub fn is_separator(&self) -> bool {
        self.get() == ExtraItem::Separator
    }

    pub fn is_add_account(&self) -> bool {
        self.get() == ExtraItem::AddAccount
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    User(gtk::StackPage, bool),
    Separator,
    AddAccount,
}

impl From<ExtraItem> for Item {
    fn from(extra_item: ExtraItem) -> Self {
        match extra_item {
            ExtraItem::Separator => Self::Separator,
            ExtraItem::AddAccount => Self::AddAccount,
        }
    }
}

impl TryFrom<glib::Object> for Item {
    type Error = glib::Object;

    fn try_from(object: glib::Object) -> Result<Self, Self::Error> {
        object
            .downcast::<gtk::StackPage>()
            .map(|sp| Self::User(sp, false))
            .or_else(|object| object.downcast::<ExtraItemObj>().map(|it| it.get().into()))
    }
}

impl Item {
    pub fn set_hint(self, session_root: Session) -> Self {
        match self {
            Self::User(session_page, _) => {
                let hinted = session_root == session_page.child();
                Self::User(session_page, hinted)
            }
            other => other,
        }
    }

    pub fn build_widget(&self) -> gtk::Widget {
        match self {
            Self::User(ref session_page, hinted) => {
                let user_entry = UserEntryRow::new(session_page);
                user_entry.set_hint(*hinted);
                user_entry.upcast()
            }
            Self::Separator => gtk::Separator::new(gtk::Orientation::Vertical).upcast(),
            Self::AddAccount => AddAccountRow::new().upcast(),
        }
    }
}

use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::{
    components::Avatar,
    session::{room::Member, UserExt},
};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-completion-row.ui")]
    pub struct CompletionRow {
        #[template_child]
        pub avatar: TemplateChild<Avatar>,
        #[template_child]
        pub display_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub id: TemplateChild<gtk::Label>,
        /// The room member presented by this row.
        pub member: RefCell<Option<Member>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CompletionRow {
        const NAME: &'static str = "ContentCompletionRow";
        type Type = super::CompletionRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CompletionRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "member",
                    "Member",
                    "The room member presented by this row",
                    Member::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "member" => self.obj().set_member(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "member" => self.obj().member().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for CompletionRow {}
    impl ListBoxRowImpl for CompletionRow {}
}

glib::wrapper! {
    /// A popover to allow completion for a given text buffer.
    pub struct CompletionRow(ObjectSubclass<imp::CompletionRow>)
        @extends gtk::Widget, gtk::ListBoxRow;
}

impl CompletionRow {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    pub fn member(&self) -> Option<Member> {
        self.imp().member.borrow().clone()
    }

    pub fn set_member(&self, member: Option<Member>) {
        let priv_ = self.imp();

        if priv_.member.borrow().as_ref() == member.as_ref() {
            return;
        }

        if let Some(member) = &member {
            priv_.avatar.set_item(Some(member.avatar().to_owned()));
            priv_.display_name.set_label(&member.display_name());
            priv_.id.set_label(member.user_id().as_str());
        } else {
            priv_.avatar.set_item(None);
            priv_.display_name.set_label("");
            priv_.id.set_label("");
        }

        priv_.member.replace(member);
        self.notify("member");
    }
}

impl Default for CompletionRow {
    fn default() -> Self {
        Self::new()
    }
}

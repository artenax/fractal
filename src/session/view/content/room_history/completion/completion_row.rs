use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::{components::Avatar, prelude::*, session::model::Member};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_history/completion/completion_row.ui"
    )]
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
                vec![glib::ParamSpecObject::builder::<Member>("member")
                    .explicit_notify()
                    .build()]
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
        glib::Object::new()
    }

    /// The room member displayed by this row.
    pub fn member(&self) -> Option<Member> {
        self.imp().member.borrow().clone()
    }

    /// Set the room member displayed by this row.
    pub fn set_member(&self, member: Option<Member>) {
        let imp = self.imp();

        if imp.member.borrow().as_ref() == member.as_ref() {
            return;
        }

        if let Some(member) = &member {
            imp.avatar.set_data(Some(member.avatar_data().to_owned()));
            imp.display_name.set_label(&member.display_name());
            imp.id.set_label(member.user_id().as_str());
        } else {
            imp.avatar.set_data(None);
            imp.display_name.set_label("");
            imp.id.set_label("");
        }

        imp.member.replace(member);
        self.notify("member");
    }
}

impl Default for CompletionRow {
    fn default() -> Self {
        Self::new()
    }
}

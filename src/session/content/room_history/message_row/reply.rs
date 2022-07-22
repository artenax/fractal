use adw::{prelude::*, subclass::prelude::*};
use gtk::{glib, CompositeTemplate};

use crate::{components::Pill, session::User};

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-message-reply.ui")]
    pub struct MessageReply {
        #[template_child]
        pub pill: TemplateChild<Pill>,
        #[template_child]
        pub related_content: TemplateChild<adw::Bin>,
        #[template_child]
        pub content: TemplateChild<adw::Bin>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MessageReply {
        const NAME: &'static str = "ContentMessageReply";
        type Type = super::MessageReply;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MessageReply {}

    impl WidgetImpl for MessageReply {}

    impl BoxImpl for MessageReply {}
}

glib::wrapper! {
    pub struct MessageReply(ObjectSubclass<imp::MessageReply>)
        @extends gtk::Widget, gtk::Box, @implements gtk::Accessible;
}

impl MessageReply {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create MessageReply")
    }

    pub fn set_related_content_sender(&self, user: User) {
        self.imp().pill.set_user(Some(user));
    }

    pub fn related_content(&self) -> &adw::Bin {
        self.imp().related_content.as_ref()
    }

    pub fn content(&self) -> &adw::Bin {
        self.imp().content.as_ref()
    }
}

impl Default for MessageReply {
    fn default() -> Self {
        Self::new()
    }
}

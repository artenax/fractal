use adw::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/verification-emoji.ui")]
    pub struct Emoji {
        #[template_child]
        pub emoji: TemplateChild<gtk::Label>,
        #[template_child]
        pub emoji_name: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Emoji {
        const NAME: &'static str = "VerificationEmoji";
        type Type = super::Emoji;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Emoji {}
    impl WidgetImpl for Emoji {}
    impl BinImpl for Emoji {}
}

glib::wrapper! {
    /// Preference Window to display and update room details.
    pub struct Emoji(ObjectSubclass<imp::Emoji>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Emoji {
    pub fn new(symbol: &str, name: &str) -> Self {
        let obj: Self = glib::Object::new();

        obj.set_emoji(symbol, name);
        obj
    }

    pub fn set_emoji(&self, symbol: &str, name: &str) {
        let imp = self.imp();

        imp.emoji.set_text(symbol);
        imp.emoji_name.set_text(name);
    }
}

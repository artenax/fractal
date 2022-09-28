use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};

use crate::{components::Avatar, session::Avatar as AvatarItem};

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/avatar-with-selection.ui")]
    pub struct AvatarWithSelection {
        #[template_child]
        pub child_avatar: TemplateChild<Avatar>,
        #[template_child]
        pub checkmark: TemplateChild<gtk::Image>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AvatarWithSelection {
        const NAME: &'static str = "AvatarWithSelection";
        type Type = super::AvatarWithSelection;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AvatarWithSelection {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::new(
                        "item",
                        "Item",
                        "The Avatar item displayed by this widget",
                        AvatarItem::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecInt::new(
                        "size",
                        "Size",
                        "The size of the Avatar",
                        -1,
                        i32::MAX,
                        -1,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "selected",
                        "Selected",
                        "Style helper for the inner Avatar",
                        false,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                ]
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
                "item" => self.child_avatar.set_item(value.get().unwrap()),
                "size" => self.child_avatar.set_size(value.get().unwrap()),
                "selected" => obj.set_selected(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "item" => self.child_avatar.item().to_value(),
                "size" => self.child_avatar.size().to_value(),
                "selected" => obj.is_selected().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for AvatarWithSelection {}
    impl BinImpl for AvatarWithSelection {}
}

glib::wrapper! {
    /// A widget displaying an `Avatar` for a `Room` or `User`.
    pub struct AvatarWithSelection(ObjectSubclass<imp::AvatarWithSelection>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl AvatarWithSelection {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create AvatarWithSelection")
    }

    pub fn set_selected(&self, selected: bool) {
        let priv_ = self.imp();

        if self.is_selected() == selected {
            return;
        }

        priv_.checkmark.set_visible(selected);

        if selected {
            priv_.child_avatar.add_css_class("selected-avatar");
        } else {
            priv_.child_avatar.remove_css_class("selected-avatar");
        }

        self.notify("selected");
    }

    pub fn is_selected(&self) -> bool {
        self.imp().checkmark.get_visible()
    }

    pub fn avatar(&self) -> &Avatar {
        &self.imp().child_avatar
    }
}

impl Default for AvatarWithSelection {
    fn default() -> Self {
        Self::new()
    }
}

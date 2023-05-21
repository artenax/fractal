use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};

use crate::{components::Avatar, session::model::AvatarData};

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
                    glib::ParamSpecObject::builder::<AvatarData>("data")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecInt::builder("size")
                        .minimum(-1)
                        .default_value(-1)
                        .build(),
                    glib::ParamSpecBoolean::builder("selected")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "data" => self.child_avatar.set_data(value.get().unwrap()),
                "size" => self.child_avatar.set_size(value.get().unwrap()),
                "selected" => self.obj().set_selected(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "data" => self.child_avatar.data().to_value(),
                "size" => self.child_avatar.size().to_value(),
                "selected" => self.obj().is_selected().to_value(),
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
        glib::Object::new()
    }

    /// Set whether this avatar is selected.
    pub fn set_selected(&self, selected: bool) {
        let imp = self.imp();

        if self.is_selected() == selected {
            return;
        }

        imp.checkmark.set_visible(selected);

        if selected {
            imp.child_avatar.add_css_class("selected-avatar");
        } else {
            imp.child_avatar.remove_css_class("selected-avatar");
        }

        self.notify("selected");
    }

    /// Whether this avatar is selected.
    pub fn is_selected(&self) -> bool {
        self.imp().checkmark.get_visible()
    }

    pub fn avatar(&self) -> &Avatar {
        &self.imp().child_avatar
    }

    /// The [`AvatarData`] displayed by this widget.
    pub fn data(&self) -> Option<AvatarData> {
        self.avatar().data()
    }

    /// The size of the Avatar.
    pub fn size(&self) -> i32 {
        self.avatar().size()
    }
}

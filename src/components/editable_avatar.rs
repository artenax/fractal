use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{
    gdk, gio, glib,
    glib::{clone, closure_local},
    prelude::*,
    CompositeTemplate,
};
use log::error;

use super::{ActionButton, ActionState, ImagePaintable};
use crate::{session::Avatar, spawn, toast};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::{InitializingObject, Signal};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-editable-avatar.ui")]
    pub struct EditableAvatar {
        /// The avatar to display.
        pub avatar: RefCell<Option<Avatar>>,
        /// Whether this avatar is changeable.
        pub editable: Cell<bool>,
        /// The state of the avatar edit.
        pub edit_state: Cell<ActionState>,
        /// Whether the edit button is sensitive.
        pub edit_sensitive: Cell<bool>,
        /// Whether this avatar is removable.
        pub removable: Cell<bool>,
        /// The state of the avatar removal.
        pub remove_state: Cell<ActionState>,
        /// Whether the remove button is sensitive.
        pub remove_sensitive: Cell<bool>,
        /// A temporary image to show instead of the avatar.
        pub temp_image: RefCell<Option<gdk::Paintable>>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub button_remove: TemplateChild<ActionButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EditableAvatar {
        const NAME: &'static str = "ComponentsEditableAvatar";
        type Type = super::EditableAvatar;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("editable-avatar.edit-avatar", None, |obj, _, _| {
                spawn!(clone!(@weak obj => async move {
                    obj.choose_avatar().await;
                }));
            });
            klass.install_action("editable-avatar.remove-avatar", None, |obj, _, _| {
                obj.emit_by_name::<()>("remove-avatar", &[]);
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for EditableAvatar {
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("edit-avatar")
                        .param_types([gio::File::static_type()])
                        .build(),
                    Signal::builder("remove-avatar").build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Avatar>("avatar")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("editable")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecEnum::builder::<ActionState>("edit-state")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("edit-sensitive")
                        .default_value(true)
                        .explicit_notify()
                        .construct()
                        .build(),
                    glib::ParamSpecBoolean::builder("removable")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecEnum::builder::<ActionState>("remove-state")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("remove-sensitive")
                        .default_value(true)
                        .explicit_notify()
                        .construct()
                        .build(),
                    glib::ParamSpecObject::builder::<gdk::Paintable>("temp-image")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "avatar" => obj.set_avatar(value.get().unwrap()),
                "editable" => obj.set_editable(value.get().unwrap()),
                "edit-state" => obj.set_edit_state(value.get().unwrap()),
                "edit-sensitive" => obj.set_edit_sensitive(value.get().unwrap()),
                "removable" => obj.set_removable(value.get().unwrap()),
                "remove-state" => obj.set_remove_state(value.get().unwrap()),
                "remove-sensitive" => obj.set_remove_sensitive(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "avatar" => obj.avatar().to_value(),
                "editable" => obj.editable().to_value(),
                "edit-state" => obj.edit_state().to_value(),
                "edit-sensitive" => obj.edit_sensitive().to_value(),
                "removable" => obj.removable().to_value(),
                "remove-state" => obj.remove_state().to_value(),
                "remove-sensitive" => obj.remove_sensitive().to_value(),
                "temp-image" => obj.temp_image().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.button_remove.set_extra_classes(&["error"]);
        }
    }

    impl WidgetImpl for EditableAvatar {}

    impl BinImpl for EditableAvatar {}
}

glib::wrapper! {
    /// An `Avatar` that can be edited.
    pub struct EditableAvatar(ObjectSubclass<imp::EditableAvatar>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl EditableAvatar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The Avatar to display.
    pub fn avatar(&self) -> Option<Avatar> {
        self.imp().avatar.borrow().to_owned()
    }

    /// Set the Avatar to display.
    pub fn set_avatar(&self, avatar: Option<Avatar>) {
        if self.avatar() == avatar {
            return;
        }

        self.imp().avatar.replace(avatar);
        self.notify("avatar");
    }

    /// Whether this avatar is editable.
    pub fn editable(&self) -> bool {
        self.imp().editable.get()
    }

    /// Set whether this avatar is editable.
    pub fn set_editable(&self, editable: bool) {
        if self.editable() == editable {
            return;
        }

        self.imp().editable.set(editable);
        self.notify("editable");
    }

    /// The state of the avatar edit.
    pub fn edit_state(&self) -> ActionState {
        self.imp().edit_state.get()
    }

    /// Set the state of the avatar edit.
    pub fn set_edit_state(&self, state: ActionState) {
        if self.edit_state() == state {
            return;
        }

        self.imp().edit_state.set(state);
        self.notify("edit-state");
    }

    /// Whether the edit button is sensitive.
    pub fn edit_sensitive(&self) -> bool {
        self.imp().edit_sensitive.get()
    }

    /// Set whether the edit button is sensitive.
    pub fn set_edit_sensitive(&self, sensitive: bool) {
        if self.edit_sensitive() == sensitive {
            return;
        }

        self.imp().edit_sensitive.set(sensitive);
        self.notify("edit-sensitive");
    }

    /// Whether this avatar is removable.
    pub fn removable(&self) -> bool {
        self.imp().removable.get()
    }

    /// Set whether this avatar is removable.
    pub fn set_removable(&self, removable: bool) {
        if self.removable() == removable {
            return;
        }

        self.imp().removable.set(removable);
        self.notify("removable");
    }

    /// The state of the avatar removal.
    pub fn remove_state(&self) -> ActionState {
        self.imp().remove_state.get()
    }

    /// Set the state of the avatar removal.
    pub fn set_remove_state(&self, state: ActionState) {
        if self.remove_state() == state {
            return;
        }

        self.imp().remove_state.set(state);
        self.notify("remove-state");
    }

    /// Whether the remove button is sensitive.
    pub fn remove_sensitive(&self) -> bool {
        self.imp().remove_sensitive.get()
    }

    /// Set whether the remove button is sensitive.
    pub fn set_remove_sensitive(&self, sensitive: bool) {
        if self.remove_sensitive() == sensitive {
            return;
        }

        self.imp().remove_sensitive.set(sensitive);
        self.notify("remove-sensitive");
    }

    /// The temporary image to show instead of the avatar.
    pub fn temp_image(&self) -> Option<gdk::Paintable> {
        self.imp().temp_image.borrow().clone()
    }

    pub fn set_temp_image_from_file(&self, file: Option<&gio::File>) {
        self.imp().temp_image.replace(
            file.and_then(|file| ImagePaintable::from_file(file).ok())
                .map(|texture| texture.upcast()),
        );
        self.notify("temp-image");
    }

    /// Show an avatar with `temp_image` instead of `avatar`.
    pub fn show_temp_image(&self, show_temp: bool) {
        let stack = &self.imp().stack;
        if show_temp {
            stack.set_visible_child_name("temp");
        } else {
            stack.set_visible_child_name("default");
        }
    }

    async fn choose_avatar(&self) {
        let image_filter = gtk::FileFilter::new();
        image_filter.add_mime_type("image/*");

        let dialog = gtk::FileChooserNative::builder()
            .title(gettext("Choose Avatar"))
            .modal(true)
            .transient_for(
                self.root()
                    .as_ref()
                    .and_then(|root| root.downcast_ref::<gtk::Window>())
                    .unwrap(),
            )
            .action(gtk::FileChooserAction::Open)
            .accept_label(gettext("Choose"))
            .cancel_label(gettext("Cancel"))
            .filter(&image_filter)
            .build();

        if dialog.run_future().await != gtk::ResponseType::Accept {
            return;
        }

        let Some(file) = dialog.file() else {
            error!("No file chosen");
            toast!(self, gettext("No file was chosen"));
            return;
        };

        if let Some(content_type) = file
            .query_info_future(
                gio::FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE,
                gio::FileQueryInfoFlags::NONE,
                glib::PRIORITY_LOW,
            )
            .await
            .ok()
            .and_then(|info| info.content_type())
        {
            if gio::content_type_is_a(&content_type, "image/*") {
                self.emit_by_name::<()>("edit-avatar", &[&file]);
            } else {
                error!("The chosen file is not an image");
                toast!(self, gettext("The chosen file is not an image"));
            }
        } else {
            error!("Could not get the content type of the file");
            toast!(
                self,
                gettext("Could not determine the type of the chosen file")
            );
        }
    }

    pub fn connect_edit_avatar<F: Fn(&Self, gio::File) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "edit-avatar",
            true,
            closure_local!(|obj: Self, file: gio::File| {
                f(&obj, file);
            }),
        )
    }

    pub fn connect_remove_avatar<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "remove-avatar",
            true,
            closure_local!(|obj: Self| {
                f(&obj);
            }),
        )
    }
}

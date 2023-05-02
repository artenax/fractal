use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};

use crate::{
    components::Avatar,
    prelude::*,
    session::{Room, User},
};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/pill.ui")]
    pub struct Pill {
        /// The user displayed by this widget
        pub user: RefCell<Option<User>>,
        /// The room displayed by this widget
        pub room: RefCell<Option<Room>>,
        #[template_child]
        pub display_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub avatar: TemplateChild<Avatar>,
        pub bindings: RefCell<Vec<glib::Binding>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Pill {
        const NAME: &'static str = "Pill";
        type Type = super::Pill;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Pill {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<User>("user")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<Room>("room")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "user" => obj.set_user(value.get().unwrap()),
                "room" => obj.set_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "user" => obj.user().to_value(),
                "room" => obj.room().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for Pill {}

    impl BinImpl for Pill {}
}

glib::wrapper! {
    /// Inline widget displaying an emphasized `User` or `Room`.
    pub struct Pill(ObjectSubclass<imp::Pill>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl Pill {
    pub fn for_user(user: &User) -> Self {
        glib::Object::builder().property("user", user).build()
    }

    pub fn for_room(room: &Room) -> Self {
        glib::Object::builder().property("room", room).build()
    }

    /// Set the user displayed by this widget.
    ///
    /// This removes the room, if one was set.
    pub fn set_user(&self, user: Option<User>) {
        let imp = self.imp();

        if *imp.user.borrow() == user {
            return;
        }

        while let Some(binding) = imp.bindings.borrow_mut().pop() {
            binding.unbind();
        }
        self.set_room(None);

        if let Some(ref user) = user {
            let display_name_binding = user
                .bind_property("display-name", &*imp.display_name, "label")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build();

            imp.bindings.borrow_mut().push(display_name_binding);
        }

        imp.avatar
            .set_data(user.as_ref().map(|user| user.avatar_data().clone()));
        imp.user.replace(user);

        self.notify("user");
    }

    /// The user displayed by this widget.
    pub fn user(&self) -> Option<User> {
        self.imp().user.borrow().clone()
    }

    /// Set the room displayed by this widget.
    ///
    /// This removes the user, if one was set.
    pub fn set_room(&self, room: Option<Room>) {
        let imp = self.imp();

        if *imp.room.borrow() == room {
            return;
        }

        while let Some(binding) = imp.bindings.borrow_mut().pop() {
            binding.unbind();
        }
        self.set_user(None);

        if let Some(ref room) = room {
            let display_name_binding = room
                .bind_property("display-name", &*imp.display_name, "label")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build();

            imp.bindings.borrow_mut().push(display_name_binding);
        }

        imp.avatar
            .set_data(room.as_ref().map(|room| room.avatar_data().clone()));
        imp.room.replace(room);

        self.notify("room");
    }

    /// The room displayed by this widget.
    pub fn room(&self) -> Option<Room> {
        self.imp().room.borrow().clone()
    }
}

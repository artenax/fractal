use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};
use log::debug;

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/context-menu-bin.ui")]
    pub struct ContextMenuBin {
        #[template_child]
        pub click_gesture: TemplateChild<gtk::GestureClick>,
        #[template_child]
        pub long_press_gesture: TemplateChild<gtk::GestureLongPress>,
        pub popover: gtk::PopoverMenu,
    }

    impl Default for ContextMenuBin {
        fn default() -> Self {
            Self {
                click_gesture: Default::default(),
                long_press_gesture: Default::default(),
                // WORKAROUND: there is some issue with creating the popover from the template
                popover: gtk::PopoverMenu::builder()
                    .position(gtk::PositionType::Bottom)
                    .has_arrow(false)
                    .halign(gtk::Align::Start)
                    .build(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ContextMenuBin {
        const NAME: &'static str = "ContextMenuBin";
        type Type = super::ContextMenuBin;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("context-menu.activate", None, move |widget, _, _| {
                widget.open_menu_at(0, 0)
            });
            klass.add_binding_action(
                gdk::Key::F10,
                gdk::ModifierType::SHIFT_MASK,
                "context-menu.activate",
                None,
            );
            klass.add_binding_action(
                gdk::Key::Menu,
                gdk::ModifierType::empty(),
                "context-menu.activate",
                None,
            );

            klass.install_action("context-menu.close", None, move |widget, _, _| {
                widget.imp().popover.popdown();
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ContextMenuBin {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "context-menu",
                    "Context Menu",
                    "The context menu",
                    gio::MenuModel::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                )]
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
                "context-menu" => {
                    obj.set_context_menu(value.get::<Option<gio::MenuModel>>().unwrap().as_ref())
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "context-menu" => obj.context_menu().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.popover.set_parent(obj);
            self.long_press_gesture
                .connect_pressed(clone!(@weak obj => move |gesture, x, y| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    gesture.reset();
                    obj.open_menu_at(x as i32, y as i32);
                }));

            self.click_gesture.connect_released(
                clone!(@weak obj => move |gesture, n_press, x, y| {
                    if n_press > 1 {
                        return;
                    }

                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    obj.open_menu_at(x as i32, y as i32);
                }),
            );
            self.parent_constructed(obj);
        }

        fn dispose(&self, _obj: &Self::Type) {
            self.popover.unparent();
        }
    }

    impl WidgetImpl for ContextMenuBin {}

    impl BinImpl for ContextMenuBin {}
}

glib::wrapper! {
    /// A Bin widget that adds a context menu.
    pub struct ContextMenuBin(ObjectSubclass<imp::ContextMenuBin>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl ContextMenuBin {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create ContextMenuBin")
    }

    fn open_menu_at(&self, x: i32, y: i32) {
        let popover = &self.imp().popover;

        debug!("Context menu was activated");

        if popover.menu_model().is_none() {
            return;
        }

        popover.set_pointing_to(Some(&gdk::Rectangle::new(x, y, 0, 0)));
        popover.popup();
    }
}

pub trait ContextMenuBinExt: 'static {
    /// Set the `MenuModel` used in the context menu.
    fn set_context_menu(&self, menu: Option<&gio::MenuModel>);

    /// Get the `MenuModel` used in the context menu.
    fn context_menu(&self) -> Option<gio::MenuModel>;

    /// Get the `PopoverMenu` used in the context menu.
    fn popover(&self) -> &gtk::PopoverMenu;
}

impl<O: IsA<ContextMenuBin>> ContextMenuBinExt for O {
    fn set_context_menu(&self, menu: Option<&gio::MenuModel>) {
        if self.context_menu().as_ref() == menu {
            return;
        }

        self.upcast_ref().imp().popover.set_menu_model(menu);
        self.notify("context-menu");
    }

    fn context_menu(&self) -> Option<gio::MenuModel> {
        self.upcast_ref().imp().popover.menu_model()
    }

    fn popover(&self) -> &gtk::PopoverMenu {
        &self.upcast_ref().imp().popover
    }
}

pub trait ContextMenuBinImpl: BinImpl {}

unsafe impl<T: ContextMenuBinImpl> IsSubclassable<T> for ContextMenuBin {
    fn class_init(class: &mut glib::Class<Self>) {
        <gtk::Widget as IsSubclassable<T>>::class_init(class);
    }
    fn instance_init(instance: &mut glib::subclass::InitializingObject<T>) {
        <gtk::Widget as IsSubclassable<T>>::instance_init(instance);
    }
}

impl Default for ContextMenuBin {
    fn default() -> Self {
        Self::new()
    }
}

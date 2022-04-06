use adw::subclass::prelude::*;
use gtk::{gdk, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};
use log::debug;

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;

    use super::*;
    type FactoryFn = RefCell<Option<Box<dyn Fn(&super::ContextMenuBin, &gtk::PopoverMenu)>>>;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/context-menu-bin.ui")]
    pub struct ContextMenuBin {
        #[template_child]
        pub click_gesture: TemplateChild<gtk::GestureClick>,
        #[template_child]
        pub long_press_gesture: TemplateChild<gtk::GestureLongPress>,
        pub factory: FactoryFn,
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
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ContextMenuBin {
        fn constructed(&self, obj: &Self::Type) {
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
        debug!("Context menu was activated");
        if let Some(factory) = &*self.imp().factory.borrow() {
            let popover = gtk::PopoverMenu::builder()
                .position(gtk::PositionType::Bottom)
                .has_arrow(false)
                .halign(gtk::Align::Start)
                .build();

            popover.set_parent(self);

            popover.connect_closed(|popover| {
                popover.unparent();
            });

            (factory)(self, &popover);

            popover.set_pointing_to(Some(&gdk::Rectangle::new(x, y, 0, 0)));
            popover.popup();
        }
    }
}

pub trait ContextMenuBinExt: 'static {
    /// Set the closure used to create the content of the `gtk::PopoverMenu`
    fn set_factory<F>(&self, factory: F)
    where
        F: Fn(&Self, &gtk::PopoverMenu) + 'static;

    fn remove_factory(&self);
}

impl<O: IsA<ContextMenuBin>> ContextMenuBinExt for O {
    fn set_factory<F>(&self, factory: F)
    where
        F: Fn(&O, &gtk::PopoverMenu) + 'static,
    {
        let f = move |obj: &ContextMenuBin, popover: &gtk::PopoverMenu| {
            factory(obj.downcast_ref::<O>().unwrap(), popover);
        };
        self.upcast_ref().imp().factory.replace(Some(Box::new(f)));
    }

    fn remove_factory(&self) {
        self.upcast_ref().imp().factory.take();
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

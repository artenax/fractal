use adw::subclass::prelude::*;
use gtk::{gdk, glib, glib::clone, prelude::*, CompositeTemplate};
use log::debug;

mod imp {
    use std::cell::RefCell;

    use glib::{subclass::InitializingObject, SignalHandlerId};

    use super::*;

    #[repr(C)]
    pub struct ContextMenuBinClass {
        pub parent_class: glib::object::Class<adw::Bin>,
        pub menu_opened: fn(&super::ContextMenuBin),
    }

    unsafe impl ClassStruct for ContextMenuBinClass {
        type Type = ContextMenuBin;
    }

    pub(super) fn context_menu_bin_menu_opened(this: &super::ContextMenuBin) {
        let klass = this.class();
        (klass.as_ref().menu_opened)(this)
    }

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/context-menu-bin.ui")]
    pub struct ContextMenuBin {
        #[template_child]
        pub click_gesture: TemplateChild<gtk::GestureClick>,
        #[template_child]
        pub long_press_gesture: TemplateChild<gtk::GestureLongPress>,
        pub popover: RefCell<Option<gtk::PopoverMenu>>,
        pub signal_handler: RefCell<Option<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ContextMenuBin {
        const NAME: &'static str = "ContextMenuBin";
        const ABSTRACT: bool = true;
        type Type = super::ContextMenuBin;
        type ParentType = adw::Bin;
        type Class = ContextMenuBinClass;

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
                if let Some(popover) = widget.popover() {
                    popover.popdown();
                }
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
                vec![
                    glib::ParamSpecObject::builder::<gtk::PopoverMenu>("popover")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "popover" => self.obj().set_popover(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "popover" => self.obj().popover().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            let obj = self.obj();

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
            self.parent_constructed();
        }

        fn dispose(&self) {
            if let Some(popover) = self.popover.take() {
                popover.unparent()
            }
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
    fn open_menu_at(&self, x: i32, y: i32) {
        debug!("Open menu at ({x}, {y})");
        self.menu_opened();

        if let Some(popover) = self.popover() {
            debug!("Context menu was activated");
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x, y, 0, 0)));
            popover.popup();
        }
    }
}

pub trait ContextMenuBinExt: 'static {
    /// Get the `PopoverMenu` used in the context menu.
    fn popover(&self) -> Option<gtk::PopoverMenu>;

    /// Set the `PopoverMenu` used in the context menu.
    fn set_popover(&self, popover: Option<gtk::PopoverMenu>);

    /// Called when the menu was requested to open but before the menu is shown.
    fn menu_opened(&self);
}

impl<O: IsA<ContextMenuBin>> ContextMenuBinExt for O {
    fn popover(&self) -> Option<gtk::PopoverMenu> {
        self.upcast_ref().imp().popover.borrow().clone()
    }

    fn set_popover(&self, popover: Option<gtk::PopoverMenu>) {
        let obj = self.upcast_ref();

        if obj.popover() == popover {
            return;
        }

        let priv_ = obj.imp();

        if let Some(popover) = &popover {
            popover.unparent();
            popover.set_parent(obj);
            priv_
                .signal_handler
                .replace(Some(popover.connect_parent_notify(
                    clone!(@weak obj => move |popover| {
                        if popover.parent().as_ref() != Some(obj.upcast_ref()) {
                            let priv_ = obj.imp();
                            if let Some(popover) = priv_.popover.take() {
                                if let Some(signal_handler) = priv_.signal_handler.take() {
                                    popover.disconnect(signal_handler)
                                }
                            }
                        }
                    }),
                )));
        }

        obj.imp().popover.replace(popover);
        obj.notify("popover");
    }

    fn menu_opened(&self) {
        imp::context_menu_bin_menu_opened(self.upcast_ref())
    }
}

/// Public trait that must be implemented for everything that derives from
/// `ContextMenuBin`.
///
/// Overriding a method from this Trait overrides also its behavior in
/// `ContextMenuBinExt`.
pub trait ContextMenuBinImpl: BinImpl {
    /// Called when the menu was requested to open but before the menu is shown.
    ///
    /// This method should be used to set the popover dynamically.
    fn menu_opened(&self) {}
}

unsafe impl<T> IsSubclassable<T> for ContextMenuBin
where
    T: ContextMenuBinImpl,
    T::Type: IsA<ContextMenuBin>,
{
    fn class_init(class: &mut glib::Class<Self>) {
        Self::parent_class_init::<T>(class.upcast_ref_mut());

        let klass = class.as_mut();

        klass.menu_opened = menu_opened_trampoline::<T>;
    }
}

// Virtual method implementation trampolines.
fn menu_opened_trampoline<T>(this: &ContextMenuBin)
where
    T: ObjectSubclass + ContextMenuBinImpl,
    T::Type: IsA<ContextMenuBin>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().menu_opened()
}

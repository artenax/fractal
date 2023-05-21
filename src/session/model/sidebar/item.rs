use gtk::{glib, prelude::*, subclass::prelude::*};

use super::CategoryType;

mod imp {
    use std::cell::Cell;

    use once_cell::sync::Lazy;

    use super::*;

    #[repr(C)]
    pub struct SidebarItemClass {
        pub parent_class: glib::object::ObjectClass,
        pub update_visibility: fn(&super::SidebarItem, for_category: CategoryType),
    }

    unsafe impl ClassStruct for SidebarItemClass {
        type Type = SidebarItem;
    }

    pub(super) fn sidebar_item_update_visibility(
        this: &super::SidebarItem,
        for_category: CategoryType,
    ) {
        let klass = this.class();
        (klass.as_ref().update_visibility)(this, for_category)
    }

    #[derive(Debug)]
    pub struct SidebarItem {
        /// Whether this item is visible.
        pub visible: Cell<bool>,
    }

    impl Default for SidebarItem {
        fn default() -> Self {
            Self {
                visible: Cell::new(true),
            }
        }
    }

    #[glib::object_subclass]
    unsafe impl ObjectSubclass for SidebarItem {
        const NAME: &'static str = "SidebarItem";
        const ABSTRACT: bool = true;
        type Type = super::SidebarItem;
        type Class = SidebarItemClass;
    }

    impl ObjectImpl for SidebarItem {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecBoolean::builder("visible")
                    .default_value(true)
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "visible" => self.obj().set_visible(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "visible" => self.obj().visible().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// Parent class of items inside the `Sidebar`.
    pub struct SidebarItem(ObjectSubclass<imp::SidebarItem>);
}

/// Public trait containing implemented methods for everything that derives from
/// `SidebarItem`.
///
/// To override the behavior of these methods, override the corresponding method
/// of `SidebarItemImpl`.
pub trait SidebarItemExt: 'static {
    /// Whether this `SidebarItem` is visible.
    ///
    /// Defaults to `true`.
    fn visible(&self) -> bool;

    /// Set the visibility of the `SidebarItem`.
    fn set_visible(&self, visible: bool);

    /// Update the visibility of the `SidebarItem` for the given `CategoryType`.
    fn update_visibility(&self, for_category: CategoryType);
}

impl<O: IsA<SidebarItem>> SidebarItemExt for O {
    fn visible(&self) -> bool {
        self.upcast_ref().imp().visible.get()
    }

    fn set_visible(&self, visible: bool) {
        if self.visible() == visible {
            return;
        }

        self.upcast_ref().imp().visible.set(visible);
        self.notify("visible");
    }

    fn update_visibility(&self, for_category: CategoryType) {
        imp::sidebar_item_update_visibility(self.upcast_ref(), for_category)
    }
}

/// Public trait that must be implemented for everything that derives from
/// `SidebarItem`.
///
/// Overriding a method from this Trait overrides also its behavior in
/// `SidebarItemExt`.
pub trait SidebarItemImpl: ObjectImpl {
    fn update_visibility(&self, _for_category: CategoryType) {}
}

// Make `SidebarItem` subclassable.
unsafe impl<T> IsSubclassable<T> for SidebarItem
where
    T: SidebarItemImpl,
    T::Type: IsA<SidebarItem>,
{
    fn class_init(class: &mut glib::Class<Self>) {
        Self::parent_class_init::<T>(class.upcast_ref_mut());

        let klass = class.as_mut();

        klass.update_visibility = update_visibility_trampoline::<T>;
    }
}

// Virtual method implementation trampolines.
fn update_visibility_trampoline<T>(this: &SidebarItem, for_category: CategoryType)
where
    T: ObjectSubclass + SidebarItemImpl,
    T::Type: IsA<SidebarItem>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().update_visibility(for_category)
}

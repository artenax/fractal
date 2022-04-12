use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::session::room::Member;

mod imp {
    use std::cell::Cell;

    use once_cell::sync::Lazy;

    use super::*;

    #[repr(C)]
    pub struct TimelineItemClass {
        pub parent_class: glib::object::ObjectClass,
        pub selectable: fn(&super::TimelineItem) -> bool,
        pub activatable: fn(&super::TimelineItem) -> bool,
        pub can_hide_header: fn(&super::TimelineItem) -> bool,
        pub sender: fn(&super::TimelineItem) -> Option<Member>,
    }

    unsafe impl ClassStruct for TimelineItemClass {
        type Type = TimelineItem;
    }

    pub(super) fn timeline_item_selectable(this: &super::TimelineItem) -> bool {
        let klass = this.class();
        (klass.as_ref().selectable)(this)
    }

    pub(super) fn timeline_item_activatable(this: &super::TimelineItem) -> bool {
        let klass = this.class();
        (klass.as_ref().activatable)(this)
    }

    pub(super) fn timeline_item_can_hide_header(this: &super::TimelineItem) -> bool {
        let klass = this.class();
        (klass.as_ref().can_hide_header)(this)
    }

    pub(super) fn timeline_item_sender(this: &super::TimelineItem) -> Option<Member> {
        let klass = this.class();
        (klass.as_ref().sender)(this)
    }

    #[derive(Debug, Default)]
    pub struct TimelineItem {
        pub show_header: Cell<bool>,
    }

    #[glib::object_subclass]
    unsafe impl ObjectSubclass for TimelineItem {
        const NAME: &'static str = "TimelineItem";
        const ABSTRACT: bool = true;
        type Type = super::TimelineItem;
        type Class = TimelineItemClass;
    }

    impl ObjectImpl for TimelineItem {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::new(
                        "selectable",
                        "Selectable",
                        "Whether this item is selectable.",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "activatable",
                        "Activatable",
                        "Whether this item is activatable.",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "show-header",
                        "Show Header",
                        "Whether this item should show its header.",
                        false,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecBoolean::new(
                        "can-hide-header",
                        "Can hide header",
                        "Whether this item is allowed to hide its header.",
                        false,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecObject::new(
                        "sender",
                        "Sender",
                        "If this item is a Matrix event, the sender of the event.",
                        Member::static_type(),
                        glib::ParamFlags::READABLE,
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
                "show-header" => obj.set_show_header(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "selectable" => obj.selectable().to_value(),
                "activatable" => obj.activatable().to_value(),
                "show-header" => obj.show_header().to_value(),
                "can-hide-header" => obj.can_hide_header().to_value(),
                "sender" => obj.sender().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// Interface implemented by items inside the `Timeline`.
    pub struct TimelineItem(ObjectSubclass<imp::TimelineItem>);
}

/// Public trait containing implemented methods for everything that derives from
/// `TimelineItem`.
///
/// To override the behavior of these methods, override the corresponding method
/// of `TimelineItemImpl`.
pub trait TimelineItemExt: 'static {
    /// Whether this `TimelineItem` is selectable.
    ///
    /// Defaults to `false`.
    fn selectable(&self) -> bool;

    /// Whether this `TimelineItem` is activatable.
    ///
    /// Defaults to `false`.
    fn activatable(&self) -> bool;

    /// Whether this `TimelineItem` should show its header.
    ///
    /// Defaults to `false`.
    fn show_header(&self) -> bool;

    /// Set whether this `TimelineItem` should show its header.
    fn set_show_header(&self, show: bool);

    /// Whether this `TimelineItem` is allowed to hide its header.
    ///
    /// Defaults to `false`.
    fn can_hide_header(&self) -> bool;

    /// If this is a Matrix event, the sender of the event.
    ///
    /// Defaults to `None`.
    fn sender(&self) -> Option<Member>;
}

impl<O: IsA<TimelineItem>> TimelineItemExt for O {
    fn selectable(&self) -> bool {
        imp::timeline_item_selectable(self.upcast_ref())
    }

    fn activatable(&self) -> bool {
        imp::timeline_item_activatable(self.upcast_ref())
    }

    fn show_header(&self) -> bool {
        self.upcast_ref().imp().show_header.get()
    }

    fn set_show_header(&self, show: bool) {
        let item = self.upcast_ref();

        if item.show_header() == show {
            return;
        }

        item.imp().show_header.set(show);
        item.notify("show-header");
    }

    fn can_hide_header(&self) -> bool {
        imp::timeline_item_can_hide_header(self.upcast_ref())
    }

    fn sender(&self) -> Option<Member> {
        imp::timeline_item_sender(self.upcast_ref())
    }
}

/// Public trait that must be implemented for everything that derives from
/// `TimelineItem`.
///
/// Overriding a method from this Trait overrides also its behavior in
/// `TimelineItemExt`.
pub trait TimelineItemImpl: ObjectImpl {
    fn selectable(&self, _obj: &Self::Type) -> bool {
        false
    }

    fn activatable(&self, _obj: &Self::Type) -> bool {
        false
    }

    fn can_hide_header(&self, _obj: &Self::Type) -> bool {
        false
    }

    fn sender(&self, _obj: &Self::Type) -> Option<Member> {
        None
    }
}

// Make `TimelineItem` subclassable.
unsafe impl<T> IsSubclassable<T> for TimelineItem
where
    T: TimelineItemImpl,
    T::Type: IsA<TimelineItem>,
{
    fn class_init(class: &mut glib::Class<Self>) {
        Self::parent_class_init::<T>(class.upcast_ref_mut());

        let klass = class.as_mut();

        klass.selectable = selectable_trampoline::<T>;
        klass.activatable = activatable_trampoline::<T>;
        klass.can_hide_header = can_hide_header_trampoline::<T>;
        klass.sender = sender_trampoline::<T>;
    }
}

// Virtual method implementation trampolines.
fn selectable_trampoline<T>(this: &TimelineItem) -> bool
where
    T: ObjectSubclass + TimelineItemImpl,
    T::Type: IsA<TimelineItem>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().selectable(this)
}

fn activatable_trampoline<T>(this: &TimelineItem) -> bool
where
    T: ObjectSubclass + TimelineItemImpl,
    T::Type: IsA<TimelineItem>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().activatable(this)
}

fn can_hide_header_trampoline<T>(this: &TimelineItem) -> bool
where
    T: ObjectSubclass + TimelineItemImpl,
    T::Type: IsA<TimelineItem>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().can_hide_header(this)
}

fn sender_trampoline<T>(this: &TimelineItem) -> Option<Member>
where
    T: ObjectSubclass + TimelineItemImpl,
    T::Type: IsA<TimelineItem>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().sender(this)
}

use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk::room::timeline::{TimelineItem as SdkTimelineItem, VirtualTimelineItem};

use super::{TimelineDayDivider, TimelineNewMessagesDivider, TimelinePlaceholder};
use crate::session::{
    room::{Event, Member},
    Room,
};

mod imp {
    use std::cell::Cell;

    use once_cell::sync::Lazy;

    use super::*;

    #[repr(C)]
    pub struct TimelineItemClass {
        pub parent_class: glib::object::ObjectClass,
        pub is_visible: fn(&super::TimelineItem) -> bool,
        pub selectable: fn(&super::TimelineItem) -> bool,
        pub activatable: fn(&super::TimelineItem) -> bool,
        pub can_hide_header: fn(&super::TimelineItem) -> bool,
        pub event_sender: fn(&super::TimelineItem) -> Option<Member>,
    }

    unsafe impl ClassStruct for TimelineItemClass {
        type Type = TimelineItem;
    }

    pub(super) fn timeline_item_is_visible(this: &super::TimelineItem) -> bool {
        let klass = this.class();
        (klass.as_ref().is_visible)(this)
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

    pub(super) fn timeline_item_event_sender(this: &super::TimelineItem) -> Option<Member> {
        let klass = this.class();
        (klass.as_ref().event_sender)(this)
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
                    glib::ParamSpecBoolean::builder("is-visible")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("selectable")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("activatable")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("show-header")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("can-hide-header")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<Member>("event-sender")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "show-header" => self.obj().set_show_header(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "is-visible" => obj.is_visible().to_value(),
                "selectable" => obj.selectable().to_value(),
                "activatable" => obj.activatable().to_value(),
                "show-header" => obj.show_header().to_value(),
                "can-hide-header" => obj.can_hide_header().to_value(),
                "event-sender" => obj.event_sender().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// Interface implemented by items inside the `Timeline`.
    pub struct TimelineItem(ObjectSubclass<imp::TimelineItem>);
}

impl TimelineItem {
    /// Create a new `TimelineItem` with the given SDK timeline item.
    ///
    /// Constructs the proper child type.
    pub fn new(item: &SdkTimelineItem, room: &Room) -> Self {
        match item {
            SdkTimelineItem::Event(event) => {
                let event = Event::new(event.clone(), room);
                event.upcast()
            }
            SdkTimelineItem::Virtual(item) => match item {
                VirtualTimelineItem::DayDivider(ts) => {
                    TimelineDayDivider::with_timestamp(*ts).upcast()
                }
                VirtualTimelineItem::ReadMarker => TimelineNewMessagesDivider::new().upcast(),
                VirtualTimelineItem::LoadingIndicator => TimelinePlaceholder::spinner().upcast(),
                VirtualTimelineItem::TimelineStart => {
                    TimelinePlaceholder::timeline_start().upcast()
                }
            },
        }
    }

    /// Try to update this `TimelineItem` with the given SDK timeline item.
    ///
    /// Returns `true` if the update succeeded.
    pub fn try_update_with(&self, item: &SdkTimelineItem) -> bool {
        match item {
            SdkTimelineItem::Event(new_event) => {
                if let Some(event) = self.downcast_ref::<Event>() {
                    return event.try_update_with(new_event);
                }
            }
            SdkTimelineItem::Virtual(_item) => {
                // Always invalidate. It shouldn't happen often and updating
                // those should be unexpensive.
            }
        }

        false
    }
}

/// Public trait containing implemented methods for everything that derives from
/// `TimelineItem`.
///
/// To override the behavior of these methods, override the corresponding method
/// of `TimelineItemImpl`.
pub trait TimelineItemExt: 'static {
    /// Whether this `TimelineItem` is visible.
    ///
    /// Defaults to `true`.
    fn is_visible(&self) -> bool;

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
    fn event_sender(&self) -> Option<Member>;
}

impl<O: IsA<TimelineItem>> TimelineItemExt for O {
    fn is_visible(&self) -> bool {
        imp::timeline_item_is_visible(self.upcast_ref())
    }

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

    fn event_sender(&self) -> Option<Member> {
        imp::timeline_item_event_sender(self.upcast_ref())
    }
}

/// Public trait that must be implemented for everything that derives from
/// `TimelineItem`.
///
/// Overriding a method from this Trait overrides also its behavior in
/// `TimelineItemExt`.
pub trait TimelineItemImpl: ObjectImpl {
    fn is_visible(&self) -> bool {
        true
    }

    fn selectable(&self) -> bool {
        false
    }

    fn activatable(&self) -> bool {
        false
    }

    fn can_hide_header(&self) -> bool {
        false
    }

    fn event_sender(&self) -> Option<Member> {
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

        klass.is_visible = is_visible_trampoline::<T>;
        klass.selectable = selectable_trampoline::<T>;
        klass.activatable = activatable_trampoline::<T>;
        klass.can_hide_header = can_hide_header_trampoline::<T>;
        klass.event_sender = event_sender_trampoline::<T>;
    }
}

// Virtual method implementation trampolines.
fn is_visible_trampoline<T>(this: &TimelineItem) -> bool
where
    T: ObjectSubclass + TimelineItemImpl,
    T::Type: IsA<TimelineItem>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().is_visible()
}

fn selectable_trampoline<T>(this: &TimelineItem) -> bool
where
    T: ObjectSubclass + TimelineItemImpl,
    T::Type: IsA<TimelineItem>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().selectable()
}

fn activatable_trampoline<T>(this: &TimelineItem) -> bool
where
    T: ObjectSubclass + TimelineItemImpl,
    T::Type: IsA<TimelineItem>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().activatable()
}

fn can_hide_header_trampoline<T>(this: &TimelineItem) -> bool
where
    T: ObjectSubclass + TimelineItemImpl,
    T::Type: IsA<TimelineItem>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().can_hide_header()
}

fn event_sender_trampoline<T>(this: &TimelineItem) -> Option<Member>
where
    T: ObjectSubclass + TimelineItemImpl,
    T::Type: IsA<TimelineItem>,
{
    let this = this.downcast_ref::<T::Type>().unwrap();
    this.imp().event_sender()
}

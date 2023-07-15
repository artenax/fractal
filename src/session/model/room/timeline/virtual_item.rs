use gtk::{glib, prelude::*, subclass::prelude::*};
use matrix_sdk_ui::timeline::VirtualTimelineItem;
use ruma::MilliSecondsSinceUnixEpoch;

use super::{TimelineItem, TimelineItemImpl};

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub enum VirtualItemKind {
    #[default]
    Spinner,
    Typing,
    TimelineStart,
    DayDivider(glib::DateTime),
    NewMessages,
}

impl VirtualItemKind {
    /// Convert this into a [`VirtualItemKindBoxed`].
    fn boxed(self) -> VirtualItemKindBoxed {
        VirtualItemKindBoxed(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, glib::Boxed)]
#[boxed_type(name = "VirtualItemKindBoxed")]
struct VirtualItemKindBoxed(VirtualItemKind);

mod imp {
    use std::cell::RefCell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct VirtualItem {
        /// The kind of virtual item.
        pub kind: RefCell<VirtualItemKind>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VirtualItem {
        const NAME: &'static str = "TimelineVirtualItem";
        type Type = super::VirtualItem;
        type ParentType = TimelineItem;
    }

    impl ObjectImpl for VirtualItem {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoxed::builder::<VirtualItemKindBoxed>("kind")
                        .construct()
                        .write_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "kind" => {
                    let boxed = value.get::<VirtualItemKindBoxed>().unwrap();
                    self.kind.replace(boxed.0);
                }
                _ => unimplemented!(),
            }
        }
    }

    impl TimelineItemImpl for VirtualItem {
        fn id(&self) -> String {
            match self.obj().kind() {
                VirtualItemKind::Spinner => "VirtualItem::Spinner".to_owned(),
                VirtualItemKind::Typing => "VirtualItem::Typing".to_owned(),
                VirtualItemKind::TimelineStart => "VirtualItem::TimelineStart".to_owned(),
                VirtualItemKind::DayDivider(date) => {
                    format!("VirtualItem::DayDivider({})", date.format("%F").unwrap())
                }
                VirtualItemKind::NewMessages => "VirtualItem::NewMessages".to_owned(),
            }
        }
    }
}

glib::wrapper! {
    /// A virtual item in the timeline.
    ///
    /// A virtual item is an item not based on a timeline event.
    pub struct VirtualItem(ObjectSubclass<imp::VirtualItem>) @extends TimelineItem;
}

impl VirtualItem {
    /// Create a new `VirtualItem` from a virtual timeline item.
    pub fn new(item: &VirtualTimelineItem) -> Self {
        match item {
            VirtualTimelineItem::DayDivider(ts) => Self::day_divider_with_timestamp(*ts),
            VirtualTimelineItem::ReadMarker => Self::new_messages(),
        }
    }

    /// Create a spinner virtual item.
    pub fn spinner() -> Self {
        glib::Object::builder()
            .property("kind", VirtualItemKind::Spinner.boxed())
            .build()
    }

    /// Create a typing virtual item.
    pub fn typing() -> Self {
        glib::Object::builder()
            .property("kind", VirtualItemKind::Typing.boxed())
            .build()
    }

    /// Create a timeline start virtual item.
    pub fn timeline_start() -> Self {
        glib::Object::builder()
            .property("kind", VirtualItemKind::TimelineStart.boxed())
            .build()
    }

    /// Create a new messages virtual item.
    pub fn new_messages() -> Self {
        glib::Object::builder()
            .property("kind", VirtualItemKind::NewMessages.boxed())
            .build()
    }

    /// Creates a new day divider virtual item for the given timestamp.
    ///
    /// If the timestamp is out of range for `glib::DateTime` (later than the
    /// end of year 9999), this fallbacks to creating a divider with the
    /// current local time.
    ///
    /// Panics if an error occurred when accessing the current local time.
    pub fn day_divider_with_timestamp(timestamp: MilliSecondsSinceUnixEpoch) -> Self {
        let date = glib::DateTime::from_unix_utc(timestamp.as_secs().into())
            .or_else(|_| glib::DateTime::now_local())
            .expect("We should be able to get the current time");

        glib::Object::builder()
            .property("kind", VirtualItemKind::DayDivider(date).boxed())
            .build()
    }

    /// The kind of virtual item.
    pub fn kind(&self) -> VirtualItemKind {
        self.imp().kind.borrow().clone()
    }
}

use gtk::{glib, subclass::prelude::*};

use super::{TimelineItem, TimelineItemImpl};

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct TimelineNewMessagesDivider;

    #[glib::object_subclass]
    impl ObjectSubclass for TimelineNewMessagesDivider {
        const NAME: &'static str = "TimelineNewMessagesDivider";
        type Type = super::TimelineNewMessagesDivider;
        type ParentType = TimelineItem;
    }

    impl ObjectImpl for TimelineNewMessagesDivider {}

    impl TimelineItemImpl for TimelineNewMessagesDivider {
        fn id(&self) -> String {
            "TimelineNewMessagesDivider".to_owned()
        }
    }
}

glib::wrapper! {
    /// A divider for the read marker in the timeline.
    pub struct TimelineNewMessagesDivider(ObjectSubclass<imp::TimelineNewMessagesDivider>) @extends TimelineItem;
}

impl TimelineNewMessagesDivider {
    pub fn new() -> Self {
        glib::Object::new()
    }
}

use gtk::{glib, subclass::prelude::*};

use super::{TimelineItem, TimelineItemImpl};

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct TimelineSpinner;

    #[glib::object_subclass]
    impl ObjectSubclass for TimelineSpinner {
        const NAME: &'static str = "TimelineSpinner";
        type Type = super::TimelineSpinner;
        type ParentType = TimelineItem;
    }

    impl ObjectImpl for TimelineSpinner {}
    impl TimelineItemImpl for TimelineSpinner {}
}

glib::wrapper! {
    /// A loading spinner in the timeline.
    pub struct TimelineSpinner(ObjectSubclass<imp::TimelineSpinner>) @extends TimelineItem;
}

impl TimelineSpinner {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create TimelineSpinner")
    }
}

impl Default for TimelineSpinner {
    fn default() -> Self {
        Self::new()
    }
}

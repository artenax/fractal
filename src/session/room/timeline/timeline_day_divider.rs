use gettextrs::gettext;
use gtk::{glib, prelude::*, subclass::prelude::*};
use ruma::MilliSecondsSinceUnixEpoch;

use super::{TimelineItem, TimelineItemImpl};

mod imp {
    use std::cell::RefCell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct TimelineDayDivider {
        /// The date of this divider.
        pub date: RefCell<Option<glib::DateTime>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TimelineDayDivider {
        const NAME: &'static str = "TimelineDayDivider";
        type Type = super::TimelineDayDivider;
        type ParentType = TimelineItem;
    }

    impl ObjectImpl for TimelineDayDivider {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoxed::builder::<glib::DateTime>("date")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecString::builder("formatted-date")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "date" => self.obj().set_date(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "date" => obj.date().to_value(),
                "formatted-date" => obj.formatted_date().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl TimelineItemImpl for TimelineDayDivider {
        fn id(&self) -> String {
            format!(
                "TimelineDayDivider::{}",
                self.obj()
                    .date()
                    .map(|d| d.format("%F").unwrap())
                    .unwrap_or_default()
            )
        }
    }
}

glib::wrapper! {
    /// A day divider in the timeline.
    pub struct TimelineDayDivider(ObjectSubclass<imp::TimelineDayDivider>) @extends TimelineItem;
}

impl TimelineDayDivider {
    pub fn new(date: glib::DateTime) -> Self {
        glib::Object::builder().property("date", &date).build()
    }

    /// Creates a new `TimelineDayDivider` for the given timestamp.
    ///
    /// If the timestamp is out of range for `glib::DateTime` (later than the
    /// end of year 9999), this fallbacks to creating a divider with the
    /// current local time.
    ///
    /// Panics if an error occurred when accessing the current local time.
    pub fn with_timestamp(timestamp: MilliSecondsSinceUnixEpoch) -> Self {
        let date = glib::DateTime::from_unix_utc(timestamp.as_secs().into())
            .expect("The day divider timestamp should be before year 10,000");
        Self::new(date)
    }

    /// The date of this divider.
    pub fn date(&self) -> Option<glib::DateTime> {
        self.imp().date.borrow().clone()
    }

    /// Set the date of this divider.
    pub fn set_date(&self, date: Option<glib::DateTime>) {
        let imp = self.imp();

        if imp.date.borrow().as_ref() == date.as_ref() {
            return;
        }

        imp.date.replace(date);
        self.notify("date");
        self.notify("formatted-date");
    }

    /// The localized representation of the date of this divider.
    pub fn formatted_date(&self) -> String {
        self.date()
            .map(|date| {
                let fmt = if date.year() == glib::DateTime::now_local().unwrap().year() {
                    // Translators: This is a date format in the day divider without the year
                    gettext("%A, %B %e")
                } else {
                    // Translators: This is a date format in the day divider with the year
                    gettext("%A, %B %e, %Y")
                };
                date.format(&fmt).unwrap().to_string()
            })
            .unwrap_or_default()
    }
}

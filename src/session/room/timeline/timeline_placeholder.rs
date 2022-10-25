use gtk::{glib, prelude::*, subclass::prelude::*};

use super::{TimelineItem, TimelineItemImpl};

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "PlaceholderKind")]
pub enum PlaceholderKind {
    Spinner = 0,
    Typing = 1,
}

impl Default for PlaceholderKind {
    fn default() -> Self {
        Self::Spinner
    }
}

mod imp {
    use std::cell::Cell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct TimelinePlaceholder {
        /// The kind of placeholder.
        pub kind: Cell<PlaceholderKind>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TimelinePlaceholder {
        const NAME: &'static str = "TimelinePlaceholder";
        type Type = super::TimelinePlaceholder;
        type ParentType = TimelineItem;
    }

    impl ObjectImpl for TimelinePlaceholder {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecEnum::new(
                    "kind",
                    "Kind",
                    "The kind of placeholder",
                    PlaceholderKind::static_type(),
                    PlaceholderKind::default() as i32,
                    glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "kind" => self.kind.set(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "kind" => self.kind.get().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl TimelineItemImpl for TimelinePlaceholder {}
}

glib::wrapper! {
    /// A loading spinner in the timeline.
    pub struct TimelinePlaceholder(ObjectSubclass<imp::TimelinePlaceholder>) @extends TimelineItem;
}

impl TimelinePlaceholder {
    pub fn spinner() -> Self {
        glib::Object::new(&[])
    }

    pub fn typing() -> Self {
        glib::Object::builder()
            .property("kind", &PlaceholderKind::Typing)
            .build()
    }

    pub fn kind(&self) -> PlaceholderKind {
        self.imp().kind.get()
    }
}

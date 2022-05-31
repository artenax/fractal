use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::components::LabelWithWidgets;

mod imp {
    use std::cell::RefCell;

    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Default)]
    pub struct Toast {
        pub title: RefCell<Option<String>>,
        pub widgets: RefCell<Vec<gtk::Widget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Toast {
        const NAME: &'static str = "ComponentsToast";
        type Type = super::Toast;
    }

    impl ObjectImpl for Toast {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecString::new(
                    "title",
                    "Title",
                    "The title of the toast",
                    None,
                    glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                )]
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
                "title" => obj.set_title(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "title" => obj.title().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// A `Toast` that can be shown in the UI.
    pub struct Toast(ObjectSubclass<imp::Toast>);
}

impl Toast {
    pub fn new(title: &str) -> Self {
        glib::Object::new(&[("title", &title)]).expect("Failed to create Toast")
    }

    pub fn builder() -> ToastBuilder {
        ToastBuilder::new()
    }

    pub fn title(&self) -> Option<String> {
        self.imp().title.borrow().clone()
    }

    pub fn set_title(&self, title: Option<&str>) {
        let priv_ = self.imp();
        if priv_.title.borrow().as_deref() == title {
            return;
        }

        priv_.title.replace(title.map(ToOwned::to_owned));
        self.notify("title");
    }

    pub fn widgets(&self) -> Vec<gtk::Widget> {
        self.imp().widgets.borrow().clone()
    }

    pub fn set_widgets(&self, widgets: &[&impl IsA<gtk::Widget>]) {
        self.imp()
            .widgets
            .replace(widgets.iter().map(|w| w.upcast_ref().clone()).collect());
    }

    pub fn widget(&self) -> gtk::Widget {
        if self.widgets().is_empty() {
            gtk::Label::builder()
                .wrap(true)
                .label(&self.title().unwrap_or_default())
                .build()
                .upcast()
        } else {
            LabelWithWidgets::new(&self.title().unwrap_or_default(), self.widgets()).upcast()
        }
    }
}

impl From<Toast> for adw::Toast {
    fn from(toast: Toast) -> Self {
        if toast.widgets().is_empty() {
            adw::Toast::new(&toast.title().unwrap_or_default())
        } else {
            // When AdwToast supports custom titles.
            todo!()
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ToastBuilder {
    title: Option<String>,
    widgets: Option<Vec<gtk::Widget>>,
}

impl ToastBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub fn widgets(mut self, widgets: &[impl IsA<gtk::Widget>]) -> Self {
        self.widgets = Some(widgets.iter().map(|w| w.upcast_ref().clone()).collect());
        self
    }

    pub fn build(&self) -> Toast {
        let toast = Toast::new(self.title.as_ref().unwrap());
        if let Some(widgets) = &self.widgets {
            toast.set_widgets(widgets.iter().collect::<Vec<_>>().as_slice());
        }
        toast
    }
}

use adw::subclass::prelude::*;
use gtk::{glib, prelude::*, CompositeTemplate};

use super::Spinner;

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/components/spinner_button.ui")]
    pub struct SpinnerButton {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub label: TemplateChild<gtk::Label>,
        #[template_child]
        pub spinner: TemplateChild<Spinner>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SpinnerButton {
        const NAME: &'static str = "SpinnerButton";
        type Type = super::SpinnerButton;
        type ParentType = gtk::Button;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SpinnerButton {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecOverride::for_class::<gtk::Button>("label"),
                    glib::ParamSpecBoolean::builder("loading")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "label" => obj.set_label(value.get().unwrap()),
                "loading" => obj.set_loading(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "label" => obj.label().to_value(),
                "loading" => obj.loading().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for SpinnerButton {}

    impl ButtonImpl for SpinnerButton {}
}

glib::wrapper! {
    /// Button showing a spinner, revealing its label once loaded.
    pub struct SpinnerButton(ObjectSubclass<imp::SpinnerButton>)
        @extends gtk::Widget, gtk::Button, @implements gtk::Accessible, gtk::Actionable;
}

impl SpinnerButton {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Set the text of the button.
    pub fn set_label(&self, label: &str) {
        let imp = self.imp();

        if imp.label.label().as_str() == label {
            return;
        }

        imp.label.set_label(label);

        self.notify("label");
    }

    /// The text of the button.
    pub fn label(&self) -> glib::GString {
        self.imp().label.label()
    }

    /// Set whether to display the loading spinner.
    pub fn set_loading(&self, loading: bool) {
        let imp = self.imp();

        if self.loading() == loading {
            return;
        }

        // The action should have been enabled or disabled so the sensitive
        // state should update itself.
        if self.action_name().is_none() {
            self.set_sensitive(!loading);
        }

        if loading {
            imp.stack.set_visible_child(&*imp.spinner);
        } else {
            imp.stack.set_visible_child(&*imp.label);
        }

        self.notify("loading");
    }

    /// Whether to display the loading spinner.
    ///
    /// If this is `false`, the text will be displayed.
    pub fn loading(&self) -> bool {
        let imp = self.imp();
        imp.stack.visible_child().as_ref() == Some(imp.spinner.upcast_ref())
    }
}

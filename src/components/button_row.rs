use adw::subclass::prelude::*;
use gtk::{
    glib,
    glib::{clone, closure_local},
    prelude::*,
    CompositeTemplate,
};

mod imp {
    use std::cell::Cell;

    use glib::subclass::{InitializingObject, Signal};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-button-row.ui")]
    pub struct ButtonRow {
        /// Whether activating this button opens a subpage.
        pub to_subpage: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ButtonRow {
        const NAME: &'static str = "ComponentsButtonRow";
        type Type = super::ButtonRow;
        type ParentType = adw::PreferencesRow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ButtonRow {
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> =
                Lazy::new(|| vec![Signal::builder("activated").build()]);
            SIGNALS.as_ref()
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecBoolean::builder("to-subpage")
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "to-subpage" => self.obj().set_to_subpage(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "to-subpage" => self.obj().to_subpage().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.obj().connect_parent_notify(|obj| {
                if let Some(listbox) = obj
                    .parent()
                    .as_ref()
                    .and_then(|parent| parent.downcast_ref::<gtk::ListBox>())
                {
                    listbox.connect_row_activated(clone!(@weak obj => move |_, row| {
                        if row == obj.upcast_ref::<gtk::ListBoxRow>() {
                            obj.emit_by_name::<()>("activated", &[]);
                        }
                    }));
                }
            });
        }
    }
    impl WidgetImpl for ButtonRow {}
    impl ListBoxRowImpl for ButtonRow {}
    impl PreferencesRowImpl for ButtonRow {}
}

glib::wrapper! {
    /// An `AdwPreferencesRow` usable as a button.
    pub struct ButtonRow(ObjectSubclass<imp::ButtonRow>)
        @extends gtk::Widget, gtk::ListBoxRow, adw::PreferencesRow, @implements gtk::Accessible;
}

impl ButtonRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Whether activating this button opens a subpage.
    pub fn to_subpage(&self) -> bool {
        self.imp().to_subpage.get()
    }

    /// Set whether activating this button opens a subpage.
    pub fn set_to_subpage(&self, to_subpage: bool) {
        if self.to_subpage() == to_subpage {
            return;
        }

        self.imp().to_subpage.replace(to_subpage);
        self.notify("to-subpage");
    }

    pub fn connect_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            true,
            closure_local!(move |obj: Self| {
                f(&obj);
            }),
        )
    }
}

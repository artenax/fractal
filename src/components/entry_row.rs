use adw::subclass::prelude::*;
use gtk::{
    gdk, glib,
    glib::{clone, closure_local},
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
};

use super::{ActionButton, ActionState};
use crate::utils::TemplateCallbacks;

mod imp {
    use std::cell::RefCell;

    use glib::subclass::{InitializingObject, Signal};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-entry-row.ui")]
    pub struct EntryRow {
        #[template_child]
        pub entry: TemplateChild<gtk::Text>,
        #[template_child]
        pub action_button: TemplateChild<ActionButton>,
        /// The hint of the entry.
        pub hint: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EntryRow {
        const NAME: &'static str = "ComponentsEntryRow";
        type Type = super::EntryRow;
        type ParentType = adw::PreferencesRow;

        fn class_init(klass: &mut Self::Class) {
            ActionButton::static_type();
            Self::bind_template(klass);
            TemplateCallbacks::bind_template_callbacks(klass);

            klass.install_action("entry-row.activate", None, move |widget, _, _| {
                let priv_ = widget.imp();
                if priv_.action_button.state() == ActionState::Default {
                    priv_.entry.grab_focus();
                } else {
                    widget.emit_by_name::<()>("activated", &[]);
                }
            });
            klass.install_action("entry-row.cancel", None, move |widget, _, _| {
                widget.emit_by_name::<()>("cancel", &[]);
            });
            klass.add_binding_action(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                "entry-row.cancel",
                None,
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for EntryRow {
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder(
                        "focused",
                        &[bool::static_type().into()],
                        <()>::static_type().into(),
                    )
                    .build(),
                    Signal::builder("activated", &[], <()>::static_type().into()).build(),
                    Signal::builder("cancel", &[], <()>::static_type().into()).build(),
                    Signal::builder("changed", &[], <()>::static_type().into()).build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::new(
                        "text",
                        "Text",
                        "The value of the entry",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecString::new(
                        "placeholder-text",
                        "Placeholder Text",
                        "The placeholder text for the entry",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecEnum::new(
                        "input-purpose",
                        "Input Purpose",
                        "Purpose of the entry",
                        gtk::InputPurpose::static_type(),
                        0,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecFlags::new(
                        "input-hints",
                        "Input Hints",
                        "Additional hints that allow input methods to fine-tune their behavior",
                        gtk::InputHints::static_type(),
                        0,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecString::new(
                        "hint",
                        "Hint",
                        "The hint of the entry",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecBoolean::new(
                        "entry-sensitive",
                        "Entry Sensitive",
                        "Whether the entry is sensitive",
                        true,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecEnum::new(
                        "action-state",
                        "Action State",
                        "The state of the entry action button",
                        ActionState::static_type(),
                        ActionState::default() as i32,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecBoolean::new(
                        "action-sensitive",
                        "Action Sensitive",
                        "Whether the action button is sensitive",
                        true,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
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
                "text" => obj.set_text(value.get().unwrap()),
                "placeholder-text" => obj.set_placeholder_text(value.get().unwrap()),
                "input-purpose" => obj.set_input_purpose(value.get().unwrap()),
                "input-hints" => obj.set_input_hints(value.get().unwrap()),
                "hint" => obj.set_hint(value.get().unwrap()),
                "entry-sensitive" => obj.set_entry_sensitive(value.get().unwrap()),
                "action-state" => obj.set_action_state(value.get().unwrap()),
                "action-sensitive" => obj.set_action_sensitive(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "text" => obj.text().to_value(),
                "placeholder-text" => obj.placeholder_text().to_value(),
                "input-purpose" => obj.input_purpose().to_value(),
                "input-hints" => obj.input_hints().to_value(),
                "hint" => obj.hint().to_value(),
                "entry-sensitive" => obj.entry_sensitive().to_value(),
                "action-state" => obj.action_state().to_value(),
                "action-sensitive" => obj.action_sensitive().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.entry
                .connect_has_focus_notify(clone!(@weak obj => move |entry| {
                    obj.emit_by_name::<()>("focused", &[&entry.has_focus()]);
                }));
            self.entry.connect_changed(clone!(@weak obj => move |_| {
                obj.emit_by_name::<()>("changed", &[]);
            }));
            self.entry.connect_activate(clone!(@weak obj => move |_| {
                obj.emit_by_name::<()>("activated", &[]);
            }));
            self.action_button.set_extra_classes(&["flat"]);
        }
    }

    impl WidgetImpl for EntryRow {
        fn grab_focus(&self, _obj: &Self::Type) -> bool {
            self.entry.grab_focus()
        }
    }

    impl ListBoxRowImpl for EntryRow {}
    impl PreferencesRowImpl for EntryRow {}
}

glib::wrapper! {
    /// An entry usable as an `AdwPreferencesRow`.
    pub struct EntryRow(ObjectSubclass<imp::EntryRow>)
        @extends gtk::Widget, gtk::ListBoxRow, adw::PreferencesRow, @implements gtk::Accessible;
}

impl EntryRow {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create EntryRow")
    }

    pub fn text(&self) -> glib::GString {
        self.imp().entry.text()
    }

    pub fn set_text(&self, text: &str) {
        if self.text() == text {
            return;
        }

        self.imp().entry.set_text(text);
        self.notify("text");
    }

    pub fn placeholder_text(&self) -> Option<glib::GString> {
        self.imp().entry.placeholder_text()
    }

    pub fn set_placeholder_text(&self, text: Option<&str>) {
        if self.placeholder_text().as_deref() == text {
            return;
        }

        self.imp().entry.set_placeholder_text(text);
        self.notify("placeholder-text");
    }

    pub fn input_purpose(&self) -> gtk::InputPurpose {
        self.imp().entry.input_purpose()
    }

    pub fn set_input_purpose(&self, purpose: gtk::InputPurpose) {
        if self.input_purpose() == purpose {
            return;
        }

        self.imp().entry.set_input_purpose(purpose);
        self.notify("input-purpose");
    }

    pub fn input_hints(&self) -> gtk::InputHints {
        self.imp().entry.input_hints()
    }

    pub fn set_input_hints(&self, hints: gtk::InputHints) {
        if self.input_hints() == hints {
            return;
        }

        self.imp().entry.set_input_hints(hints);
        self.notify("input-hints");
    }

    pub fn hint(&self) -> String {
        self.imp().hint.borrow().to_owned()
    }

    pub fn set_hint(&self, hint: &str) {
        if self.hint() == hint {
            return;
        }

        self.imp().hint.replace(hint.to_owned());
        self.notify("hint");
    }

    pub fn entry_sensitive(&self) -> bool {
        self.imp().entry.is_sensitive()
    }

    pub fn set_entry_sensitive(&self, sensitive: bool) {
        if self.entry_sensitive() == sensitive {
            return;
        }

        self.imp().entry.set_sensitive(sensitive);
        self.notify("entry-sensitive");
    }

    pub fn action_state(&self) -> ActionState {
        self.imp().action_button.state()
    }

    pub fn set_action_state(&self, state: ActionState) {
        if self.action_state() == state {
            return;
        }

        self.imp().action_button.set_state(state);
        self.notify("action-state");
    }

    pub fn action_sensitive(&self) -> bool {
        self.imp().action_button.is_sensitive()
    }

    pub fn set_action_sensitive(&self, sensitive: bool) {
        if self.action_sensitive() == sensitive {
            return;
        }

        self.imp().action_button.set_sensitive(sensitive);
        self.notify("action-sensitive");
    }

    pub fn connect_focused<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "focused",
            true,
            closure_local!(move |obj: Self, focused: bool| {
                f(&obj, focused);
            }),
        )
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

    pub fn connect_cancel<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "cancel",
            true,
            closure_local!(move |obj: Self| {
                f(&obj);
            }),
        )
    }

    pub fn connect_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "changed",
            true,
            closure_local!(move |obj: Self| {
                f(&obj);
            }),
        )
    }
}

impl Default for EntryRow {
    fn default() -> Self {
        Self::new()
    }
}

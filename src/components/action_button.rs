use adw::subclass::prelude::*;
use gtk::{glib, glib::closure_local, prelude::*, CompositeTemplate};

use super::Spinner;

#[derive(Debug, Default, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "ActionState")]
pub enum ActionState {
    #[default]
    Default = 0,
    Confirm = 1,
    Retry = 2,
    Loading = 3,
    Success = 4,
    Warning = 5,
    Error = 6,
}

impl AsRef<str> for ActionState {
    fn as_ref(&self) -> &str {
        match self {
            ActionState::Default => "default",
            ActionState::Confirm => "confirm",
            ActionState::Retry => "retry",
            ActionState::Loading => "loading",
            ActionState::Success => "success",
            ActionState::Warning => "warning",
            ActionState::Error => "error",
        }
    }
}

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::{InitializingObject, Signal};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-action-button.ui")]
    pub struct ActionButton {
        /// The icon used in the default state.
        pub icon_name: RefCell<String>,
        /// The extra classes applied to the button in the default state.
        pub extra_classes: RefCell<Vec<String>>,
        /// The action emitted by the button.
        pub action_name: RefCell<Option<glib::GString>>,
        /// The target value of the action of the button.
        pub action_target_value: RefCell<Option<glib::Variant>>,
        /// The state of the button.
        pub state: Cell<ActionState>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub button_default: TemplateChild<gtk::Button>,
        #[template_child]
        pub spinner: TemplateChild<Spinner>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ActionButton {
        const NAME: &'static str = "ComponentsActionButton";
        type Type = super::ActionButton;
        type ParentType = adw::Bin;
        type Interfaces = (gtk::Actionable,);

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
            klass.set_css_name("action-button");
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ActionButton {
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> =
                Lazy::new(|| vec![Signal::builder("clicked").build()]);
            SIGNALS.as_ref()
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::builder("icon-name")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecEnum::builder::<ActionState>("state")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecOverride::for_interface::<gtk::Actionable>("action-name"),
                    glib::ParamSpecOverride::for_interface::<gtk::Actionable>("action-target"),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "icon-name" => obj.set_icon_name(value.get().unwrap()),
                "state" => obj.set_state(value.get().unwrap()),
                "action-name" => obj.set_action_name(value.get().unwrap()),
                "action-target" => obj.set_action_target_value(value.get().ok().as_ref()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "icon-name" => obj.icon_name().to_value(),
                "state" => obj.state().to_value(),
                "action-name" => obj.action_name().to_value(),
                "action-target" => obj.action_target_value().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for ActionButton {}
    impl BinImpl for ActionButton {}

    impl ActionableImpl for ActionButton {
        fn action_name(&self) -> Option<glib::GString> {
            self.action_name.borrow().clone()
        }

        fn action_target_value(&self) -> Option<glib::Variant> {
            self.action_target_value.borrow().clone()
        }

        fn set_action_name(&self, name: Option<&str>) {
            self.action_name.replace(name.map(Into::into));
        }

        fn set_action_target_value(&self, value: Option<&glib::Variant>) {
            self.action_target_value
                .replace(value.map(ToOwned::to_owned));
        }
    }
}

glib::wrapper! {
    /// A button to emit an action and handle its different states.
    pub struct ActionButton(ObjectSubclass<imp::ActionButton>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Actionable, gtk::Accessible;
}

#[gtk::template_callbacks]
impl ActionButton {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The icon used in the default state.
    pub fn icon_name(&self) -> String {
        self.imp().icon_name.borrow().clone()
    }

    /// Set the icon used in the default state.
    pub fn set_icon_name(&self, icon_name: &str) {
        if self.icon_name() == icon_name {
            return;
        }

        self.imp().icon_name.replace(icon_name.to_owned());
        self.notify("icon-name");
    }

    pub fn extra_classes(&self) -> Vec<String> {
        self.imp().extra_classes.borrow().clone()
    }

    pub fn set_extra_classes(&self, classes: &[&str]) {
        let imp = self.imp();
        for class in imp.extra_classes.borrow_mut().drain(..) {
            imp.button_default.remove_css_class(&class);
        }

        for class in classes.iter() {
            imp.button_default.add_css_class(class);
        }

        self.imp()
            .extra_classes
            .replace(classes.iter().map(ToString::to_string).collect());
    }

    /// The state of the button.
    pub fn state(&self) -> ActionState {
        self.imp().state.get()
    }

    /// Set the state of the button.
    pub fn set_state(&self, state: ActionState) {
        if self.state() == state {
            return;
        }

        let imp = self.imp();
        imp.stack.set_visible_child_name(state.as_ref());
        imp.state.replace(state);
        self.notify("state");
    }

    pub fn connect_clicked<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "clicked",
            true,
            closure_local!(move |obj: Self| {
                f(&obj);
            }),
        )
    }

    #[template_callback]
    fn button_clicked(&self) {
        self.emit_by_name::<()>("clicked", &[]);
    }
}

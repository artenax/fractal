use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{glib, glib::clone, prelude::*, CompositeTemplate};

use crate::{
    gettext_f,
    session::{
        user::UserExt,
        verification::{IdentityVerification, VerificationState},
    },
};

mod imp {
    use std::cell::RefCell;

    use glib::{subclass::InitializingObject, SignalHandlerId};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-verification-info-bar.ui")]
    pub struct VerificationInfoBar {
        #[template_child]
        pub revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub label: TemplateChild<gtk::Label>,
        #[template_child]
        pub accept_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub cancel_btn: TemplateChild<gtk::Button>,
        pub request: RefCell<Option<IdentityVerification>>,
        pub state_handler: RefCell<Option<SignalHandlerId>>,
        pub user_handler: RefCell<Option<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VerificationInfoBar {
        const NAME: &'static str = "ContentVerificationInfoBar";
        type Type = super::VerificationInfoBar;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("infobar");
            Self::bind_template(klass);

            klass.set_accessible_role(gtk::AccessibleRole::Group);

            klass.install_action("verification.accept", None, move |widget, _, _| {
                let request = widget.request().unwrap();
                request.accept();
                request.session().select_item(Some(request.upcast()));
            });

            klass.install_action("verification.decline", None, move |widget, _, _| {
                widget.request().unwrap().cancel(true);
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VerificationInfoBar {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "request",
                    "Request",
                    "The verification request this InfoBar is showing",
                    IdentityVerification::static_type(),
                    glib::ParamFlags::READWRITE,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "request" => self.obj().set_request(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "request" => self.obj().request().to_value(),
                _ => unimplemented!(),
            }
        }
    }
    impl WidgetImpl for VerificationInfoBar {}
    impl BinImpl for VerificationInfoBar {}
}

glib::wrapper! {
    pub struct VerificationInfoBar(ObjectSubclass<imp::VerificationInfoBar>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl VerificationInfoBar {
    pub fn new(label: String) -> Self {
        glib::Object::builder().property("label", &label).build()
    }

    pub fn request(&self) -> Option<IdentityVerification> {
        self.imp().request.borrow().clone()
    }

    pub fn set_request(&self, request: Option<IdentityVerification>) {
        let priv_ = self.imp();

        if let Some(old_request) = &*priv_.request.borrow() {
            if Some(old_request) == request.as_ref() {
                return;
            }

            if let Some(handler) = priv_.state_handler.take() {
                old_request.disconnect(handler);
            }

            if let Some(handler) = priv_.user_handler.take() {
                old_request.user().disconnect(handler);
            }
        }

        if let Some(ref request) = request {
            let handler = request.connect_notify_local(
                Some("state"),
                clone!(@weak self as obj => move |_, _| {
                    obj.update_view();
                }),
            );

            priv_.state_handler.replace(Some(handler));

            let handler = request.user().connect_notify_local(
                Some("display-name"),
                clone!(@weak self as obj => move |_, _| {
                    obj.update_view();
                }),
            );

            priv_.user_handler.replace(Some(handler));
        }

        priv_.request.replace(request);

        self.update_view();
        self.notify("request");
    }

    pub fn update_view(&self) {
        let priv_ = self.imp();
        let visible = if let Some(request) = self.request() {
            if request.is_finished() {
                false
            } else if matches!(request.state(), VerificationState::Requested) {
                priv_.label.set_markup(&gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    "{user_name} wants to be verified",
                    &[(
                        "user_name",
                        &format!("<b>{}</b>", request.user().display_name()),
                    )],
                ));
                priv_.accept_btn.set_label(&gettext("Verify"));
                priv_.cancel_btn.set_label(&gettext("Decline"));
                true
            } else {
                priv_.label.set_label(&gettext("Verification in progress"));
                priv_.accept_btn.set_label(&gettext("Continue"));
                priv_.cancel_btn.set_label(&gettext("Cancel"));
                true
            }
        } else {
            false
        };

        priv_.revealer.set_reveal_child(visible);
    }
}

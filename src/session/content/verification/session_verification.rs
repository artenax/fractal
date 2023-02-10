use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{glib, glib::clone, prelude::*, CompositeTemplate};
use log::{debug, error};

use super::IdentityVerificationWidget;
use crate::{
    components::{AuthDialog, AuthError, SpinnerButton},
    session::{
        verification::{IdentityVerification, VerificationState},
        UserExt,
    },
    spawn, toast, Session, Window,
};

mod imp {
    use std::cell::RefCell;

    use glib::{subclass::InitializingObject, SignalHandlerId, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/session-verification.ui")]
    pub struct SessionVerification {
        pub request: RefCell<Option<IdentityVerification>>,
        pub session: WeakRef<Session>,
        #[template_child]
        pub header_title: TemplateChild<gtk::Label>,
        #[template_child]
        pub bootstrap_button: TemplateChild<SpinnerButton>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub bootstrap_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub verification_widget: TemplateChild<IdentityVerificationWidget>,
        pub state_handler: RefCell<Option<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SessionVerification {
        const NAME: &'static str = "SessionVerification";
        type Type = super::SessionVerification;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("session-verification.previous", None, move |obj, _, _| {
                obj.previous();
            });

            klass.install_action(
                "session-verification.show-recovery",
                None,
                move |obj, _, _| {
                    obj.show_recovery();
                },
            );

            klass.install_action(
                "session-verification.show-bootstrap",
                None,
                move |obj, _, _| {
                    obj.show_bootstrap();
                },
            );

            klass.install_action(
                "session-verification.start-request",
                None,
                move |obj, _, _| {
                    obj.start_request();
                },
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SessionVerification {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Session>("session")
                    .construct_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "session" => self.obj().set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session" => self.obj().session().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.action_set_enabled("session-verification.show-recovery", false);

            self.bootstrap_button
                .connect_clicked(clone!(@weak obj => move |button| {
                    button.set_loading(true);
                    obj.bootstrap_cross_signing();
                }));

            obj.start_request();
        }

        fn dispose(&self) {
            if let Some(request) = self.obj().request() {
                request.cancel(true);
            }
        }
    }

    impl WidgetImpl for SessionVerification {}
    impl BinImpl for SessionVerification {}
}

glib::wrapper! {
    pub struct SessionVerification(ObjectSubclass<imp::SessionVerification>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl SessionVerification {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// The current session.
    pub fn session(&self) -> Session {
        self.imp().session.upgrade().unwrap()
    }

    /// Set the current session.
    fn set_session(&self, session: Option<Session>) {
        let imp = self.imp();

        if let Some(user) = session.as_ref().and_then(|s| s.user()) {
            imp.header_title.set_text(user.user_id().as_str())
        } else {
            imp.header_title.set_text("");
        }

        imp.session.set(session.as_ref())
    }

    fn request(&self) -> Option<IdentityVerification> {
        self.imp().request.borrow().clone()
    }

    fn set_request(&self, request: Option<IdentityVerification>) {
        let imp = self.imp();
        let previous_request = self.request();

        if previous_request == request {
            return;
        }

        self.reset();

        if let Some(previous_request) = previous_request {
            if let Some(handler) = imp.state_handler.take() {
                previous_request.disconnect(handler);
            }

            previous_request.cancel(true);
        }

        if let Some(ref request) = request {
            let handler = request.connect_notify_local(
                Some("state"),
                clone!(@weak self as obj => move |request, _| {
                    obj.update_view(request);
                }),
            );

            imp.state_handler.replace(Some(handler));
            self.update_view(request);
        }

        imp.verification_widget.set_request(request.clone());
        imp.request.replace(request);
    }

    /// Returns the parent GtkWindow containing this widget.
    fn parent_window(&self) -> Option<Window> {
        self.root()?.downcast().ok()
    }

    fn reset(&self) {
        let bootstrap_button = &self.imp().bootstrap_button;

        bootstrap_button.set_sensitive(true);
        bootstrap_button.set_loading(false);
    }

    fn update_view(&self, request: &IdentityVerification) {
        let imp = self.imp();

        if request.is_finished() && request.state() != VerificationState::Completed {
            self.start_request();
            return;
        }

        match request.state() {
            // FIXME: we bootstrap on all errors
            VerificationState::Error => {
                imp.main_stack.set_visible_child_name("bootstrap");
            }
            VerificationState::RequestSend => {
                imp.main_stack.set_visible_child_name("wait-for-device");
            }
            _ => {
                imp.main_stack.set_visible_child(&*imp.verification_widget);
            }
        }
    }

    fn show_recovery(&self) {
        // TODO: stop the request

        self.imp().main_stack.set_visible_child_name("recovery");
    }

    fn show_bootstrap(&self) {
        let imp = self.imp();

        self.set_request(None);
        imp.bootstrap_label.set_label(&gettext("If you lost access to all other sessions you can create a new crypto identity. Be careful because this will reset all verified users and make previously encrypted conversations unreadable."));
        imp.bootstrap_button.remove_css_class("suggested-action");
        imp.bootstrap_button.add_css_class("destructive-action");
        imp.bootstrap_button.set_label(&gettext("Reset"));
        imp.main_stack.set_visible_child_name("bootstrap");
    }

    fn start_request(&self) {
        self.imp()
            .main_stack
            .set_visible_child_name("wait-for-device");

        spawn!(clone!(@weak self as obj => async move {
            let session = obj.session();
            let verification_list = session.verification_list();
            let request = if let Some(request) = verification_list.get_session() {
                debug!("Use session verification started by another session");
                request
            } else {
                let request = IdentityVerification::create(&session, None).await;
                debug!("Start a new session verification");
                verification_list.add(request.clone());
                request
            };

            request.set_force_current_session(true);
            obj.set_request(Some(request));
        }));
    }

    fn previous(&self) {
        let main_stack = &self.imp().main_stack;

        if let Some(child_name) = main_stack.visible_child_name() {
            match child_name.as_str() {
                "recovery" => {
                    self.start_request();
                    return;
                }
                "recovery-passphrase" | "recovery-key" => {
                    main_stack.set_visible_child_name("recovery");
                    return;
                }
                "bootstrap" => {
                    self.start_request();
                    return;
                }
                _ => {}
            }
        }

        if let Some(request) = self.request() {
            if request.state() == VerificationState::RequestSend {
                self.set_request(None);
                self.activate_action("session.logout", None).unwrap();
            } else {
                self.start_request();
            }
        } else {
            self.activate_action("session.logout", None).unwrap();
        }
    }

    fn bootstrap_cross_signing(&self) {
        spawn!(clone!(@weak self as obj => async move {
            let dialog = AuthDialog::new(obj.parent_window().as_ref(), &obj.session());

            let result = dialog
            .authenticate(move |client, auth| async move {
                client.encryption().bootstrap_cross_signing(auth).await
            })
            .await;


            let error_message = match result {
                Ok(_) => None,
                Err(AuthError::UserCancelled) => {
                    error!("Failed to bootstrap cross-signing: User cancelled the authentication");
                    Some(gettext("You cancelled the authentication needed to create the encryption keys."))
                },
                Err(error) => {
                    error!("Failed to bootstrap cross-signing: {:?}", error);
                    Some(gettext("An error occurred during the creation of the encryption keys."))
                },
            };

            if let Some(error_message) = error_message {
                toast!(obj, error_message);
            } else {
                // TODO tell user that the a crypto identity was created
                obj.activate_action("session.mark-ready", None).unwrap();
            }
        }));
    }
}

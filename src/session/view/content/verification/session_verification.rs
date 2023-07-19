use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{glib, glib::clone, prelude::*, CompositeTemplate};
use log::{debug, error};

use super::IdentityVerificationWidget;
use crate::{
    components::{AuthDialog, AuthError, SpinnerButton},
    login::Login,
    session::model::{IdentityVerification, Session, VerificationState},
    spawn, spawn_tokio, toast, Window,
};

/// The mode of the bootstrap page.
#[derive(Debug, Clone, Copy)]
enum BootstrapMode {
    /// Create a new identity when no encryption identity exists.
    CreateIdentity,
    /// Reset the encryption identity because no device is available for
    /// verification.
    NoDevices,
    /// The user selected to reset the encryption identity.
    Reset,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{subclass::InitializingObject, SignalHandlerId, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/verification/session_verification.ui"
    )]
    pub struct SessionVerification {
        pub request: RefCell<Option<IdentityVerification>>,
        /// The ancestor login view.
        pub login: WeakRef<Login>,
        /// The current session.
        pub session: WeakRef<Session>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub bootstrap_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub bootstrap_setup_button: TemplateChild<SpinnerButton>,
        #[template_child]
        pub bootstrap_restart_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub verification_widget: TemplateChild<IdentityVerificationWidget>,
        pub state_handler: RefCell<Option<SignalHandlerId>>,
        pub bootstrap_can_restart: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SessionVerification {
        const NAME: &'static str = "SessionVerification";
        type Type = super::SessionVerification;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action(
                "session-verification.show-recovery",
                None,
                move |obj, _, _| {
                    obj.show_recovery();
                },
            );

            klass.install_action(
                "session-verification.reset-identity",
                None,
                move |obj, _, _| {
                    obj.reset_identity();
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
                vec![
                    glib::ParamSpecObject::builder::<Login>("login")
                        .construct_only()
                        .build(),
                    glib::ParamSpecObject::builder::<Session>("session")
                        .construct_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "login" => obj.set_login(value.get().unwrap()),
                "session" => obj.set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "login" => obj.login().to_value(),
                "session" => obj.session().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.action_set_enabled("session-verification.show-recovery", false);

            self.bootstrap_setup_button
                .connect_clicked(clone!(@weak obj => move |button| {
                    button.set_loading(true);

                    spawn!(clone!(@weak obj => async move {
                        obj.bootstrap_cross_signing().await;
                    }));
                }));

            obj.start();
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
    pub fn new(login: &Login, session: &Session) -> Self {
        glib::Object::builder()
            .property("login", login)
            .property("session", session)
            .build()
    }

    /// The ancestor login view.
    pub fn login(&self) -> Option<Login> {
        self.imp().login.upgrade()
    }

    /// Set the ancestor login view.
    fn set_login(&self, login: Option<Login>) {
        self.imp().login.set(login.as_ref())
    }

    /// The current session.
    pub fn session(&self) -> Session {
        self.imp().session.upgrade().unwrap()
    }

    /// Set the current session.
    fn set_session(&self, session: Option<Session>) {
        self.imp().session.set(session.as_ref())
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
        self.root().and_downcast()
    }

    fn reset(&self) {
        self.imp().bootstrap_setup_button.set_loading(false);
    }

    fn update_view(&self, request: &IdentityVerification) {
        let imp = self.imp();

        if request.is_finished() && request.state() != VerificationState::Completed {
            self.start();
            return;
        }

        match request.state() {
            // FIXME: we bootstrap on all errors
            VerificationState::Error => {
                self.show_bootstrap(BootstrapMode::Reset);
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

    fn show_bootstrap(&self, mode: BootstrapMode) {
        let imp = self.imp();
        let label = &imp.bootstrap_label;
        let setup_btn = &imp.bootstrap_setup_button;
        let restart_btn = &imp.bootstrap_restart_button;
        let bootstrap_can_restart = &imp.bootstrap_can_restart;

        match mode {
            BootstrapMode::CreateIdentity => {
                label.set_label(&gettext("You need to set up an encryption identity, since this is the first time you logged into your account."));
                setup_btn.add_css_class("suggested-action");
                setup_btn.remove_css_class("destructive-action");
                setup_btn.set_label(&gettext("Set Up"));
                restart_btn.set_visible(false);
                bootstrap_can_restart.set(false);
            }
            BootstrapMode::NoDevices => {
                label.set_label(&gettext("No other devices are available to verify this session. You can either restore cross-signing from another device and restart this process or reset the encryption identity."));
                setup_btn.remove_css_class("suggested-action");
                setup_btn.add_css_class("destructive-action");
                setup_btn.set_label(&gettext("Reset"));
                restart_btn.set_visible(true);
                bootstrap_can_restart.set(false);
            }
            BootstrapMode::Reset => {
                label.set_label(&gettext("If you lost access to all other sessions, you can create a new encryption identity. Be careful because this will cancel the verifications of all users and sessions."));
                setup_btn.remove_css_class("suggested-action");
                setup_btn.add_css_class("destructive-action");
                setup_btn.set_label(&gettext("Reset"));
                restart_btn.set_visible(false);
                bootstrap_can_restart.set(true);
            }
        }

        imp.main_stack.set_visible_child_name("bootstrap");
    }

    /// Show screen to reset the encryption user identity.
    fn reset_identity(&self) {
        self.set_request(None);
        self.show_bootstrap(BootstrapMode::Reset);
    }

    fn start(&self) {
        spawn!(clone!(@weak self as obj => async move {
            obj.start_inner().await;
        }));
    }

    async fn start_inner(&self) {
        let session = self.session();
        let client = session.client();

        let client_clone = client.clone();
        let user_identity_handle = spawn_tokio!(async move {
            let user_id = client_clone.user_id().unwrap();
            client_clone.encryption().get_user_identity(user_id).await
        });

        let needs_new_identity = match user_identity_handle.await.unwrap() {
            Ok(Some(_)) => false,
            Ok(None) => {
                debug!("No encryption user identity found");
                true
            }
            Err(error) => {
                error!("Failed to get encryption user identity: {error}");
                true
            }
        };

        if needs_new_identity {
            debug!("Creating new encryption user identity…");
            self.show_bootstrap(BootstrapMode::CreateIdentity);
            return;
        }

        let devices_handle = spawn_tokio!(async move {
            let user_id = client.user_id().unwrap();
            client.encryption().get_user_devices(user_id).await
        });

        let can_verify_with_devices = match devices_handle.await.unwrap() {
            Ok(devices) => devices.devices().any(|d| d.is_cross_signed_by_owner()),
            Err(error) => {
                error!("Failed to get user devices: {error}");
                // If there are actually no other devices, the user can still
                // reset the cross-signing identity.
                true
            }
        };

        if !can_verify_with_devices {
            debug!("No other device is cross-signed, don’t request verification");
            self.show_bootstrap(BootstrapMode::NoDevices);
            return;
        }

        debug!("Starting session verification with other device…");

        self.imp()
            .main_stack
            .set_visible_child_name("wait-for-device");

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
        self.set_request(Some(request));
    }

    /// Go to the previous step.
    ///
    /// Return `true` if the action was handled, `false` if the stack cannot go
    /// back.
    pub fn go_previous(&self) -> bool {
        let imp = self.imp();
        let main_stack = &imp.main_stack;

        if let Some(child_name) = main_stack.visible_child_name() {
            match child_name.as_str() {
                "recovery" => {
                    self.start();
                    return true;
                }
                "recovery-passphrase" | "recovery-key" => {
                    main_stack.set_visible_child_name("recovery");
                    return true;
                }
                "bootstrap" if imp.bootstrap_can_restart.get() => {
                    self.start();
                    return true;
                }
                _ => {}
            }
        }

        if let Some(request) = self.request() {
            if request.state() == VerificationState::RequestSend {
                self.set_request(None);
                false
            } else {
                self.start();
                true
            }
        } else {
            false
        }
    }

    /// Create a new encryption user identity.
    async fn bootstrap_cross_signing(&self) {
        let dialog = AuthDialog::new(self.parent_window().as_ref(), &self.session());

        let result = dialog
            .authenticate(move |client, auth| async move {
                client.encryption().bootstrap_cross_signing(auth).await
            })
            .await;

        let error_message = match result {
            Ok(_) => None,
            Err(AuthError::UserCancelled) => {
                error!("Failed to bootstrap cross-signing: User cancelled the authentication");
                Some(gettext(
                    "You cancelled the authentication needed to create the encryption identity.",
                ))
            }
            Err(error) => {
                error!("Failed to bootstrap cross-signing: {error:?}");
                Some(gettext(
                    "An error occurred during the creation of the encryption identity.",
                ))
            }
        };

        if let Some(error_message) = error_message {
            toast!(self, error_message);
            self.imp().bootstrap_setup_button.set_loading(false);
        } else {
            // TODO tell user that the a crypto identity was created
            self.login().unwrap().show_completed()
        }
    }
}

use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};
use log::{debug, error, warn};

use crate::components::{AuthDialog, SpinnerButton};
use crate::contrib::screenshot;
use crate::contrib::QRCode;
use crate::contrib::QRCodeExt;
use crate::contrib::QrCodeScanner;
use crate::session::verification::{Emoji, IdentityVerification, SasData, VerificationMode};
use crate::session::Session;
use crate::spawn;
use crate::Error;
use crate::Window;
use matrix_sdk::encryption::verification::QrVerificationData;

mod imp {
    use super::*;
    use glib::object::WeakRef;
    use glib::subclass::InitializingObject;
    use glib::SignalHandlerId;
    use once_cell::unsync::OnceCell;
    use std::cell::RefCell;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/FractalNext/session-verification.ui")]
    pub struct SessionVerification {
        pub request: OnceCell<IdentityVerification>,
        pub session: OnceCell<WeakRef<Session>>,
        #[template_child]
        pub bootstrap_button: TemplateChild<SpinnerButton>,
        #[template_child]
        pub qrcode: TemplateChild<QRCode>,
        #[template_child]
        pub emoji_row_1: TemplateChild<gtk::Box>,
        #[template_child]
        pub emoji_row_2: TemplateChild<gtk::Box>,
        #[template_child]
        pub emoji_match_btn: TemplateChild<SpinnerButton>,
        #[template_child]
        pub emoji_not_match_btn: TemplateChild<SpinnerButton>,
        #[template_child]
        pub start_emoji_btn: TemplateChild<SpinnerButton>,
        #[template_child]
        pub start_emoji_btn2: TemplateChild<SpinnerButton>,
        #[template_child]
        pub start_emoji_btn3: TemplateChild<SpinnerButton>,
        #[template_child]
        pub take_screenshot_btn2: TemplateChild<SpinnerButton>,
        #[template_child]
        pub take_screenshot_btn3: TemplateChild<SpinnerButton>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub qr_code_scanner: TemplateChild<QrCodeScanner>,
        pub mode_handler: RefCell<Option<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SessionVerification {
        const NAME: &'static str = "SessionVerification";
        type Type = super::SessionVerification;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            SpinnerButton::static_type();
            QRCode::static_type();
            Emoji::static_type();
            Self::bind_template(klass);

            klass.install_action("verification.show-recovery", None, move |obj, _, _| {
                obj.show_recovery();
            });

            klass.install_action("verification.previous", None, move |obj, _, _| {
                obj.previous();
            });
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
                    glib::ParamSpec::new_object(
                        "request",
                        "Request",
                        "The Object holding the data for the verification",
                        IdentityVerification::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpec::new_object(
                        "session",
                        "Session",
                        "The current Session",
                        Session::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
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
                "request" => obj.set_request(value.get().unwrap()),
                "session" => obj.set_session(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "request" => obj.request().to_value(),
                "session" => obj.session().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            obj.action_set_enabled("verification.show-recovery", false);

            self.emoji_match_btn
                .connect_clicked(clone!(@weak obj => move |button| {
                    let priv_ = imp::SessionVerification::from_instance(&obj);
                    button.set_loading(true);
                    priv_.emoji_not_match_btn.set_sensitive(false);
                    obj.request().emoji_match();
                }));

            self.emoji_not_match_btn
                .connect_clicked(clone!(@weak obj => move |button| {
                    let priv_ = imp::SessionVerification::from_instance(&obj);
                    button.set_loading(true);
                    priv_.emoji_match_btn.set_sensitive(false);
                    obj.request().emoji_not_match();
                }));

            self.start_emoji_btn
                .connect_clicked(clone!(@weak obj => move |button| {
                    button.set_loading(true);
                    obj.request().start_sas();
                }));

            self.start_emoji_btn2
                .connect_clicked(clone!(@weak obj => move |button| {
                    let priv_ = imp::SessionVerification::from_instance(&obj);
                    button.set_loading(true);
                    priv_.take_screenshot_btn2.set_sensitive(false);
                    obj.request().start_sas();
                }));
            self.start_emoji_btn3
                .connect_clicked(clone!(@weak obj => move |button| {
                    let priv_ = imp::SessionVerification::from_instance(&obj);
                    button.set_loading(true);
                    priv_.take_screenshot_btn3.set_sensitive(false);
                    obj.request().start_sas();
                }));

            self.bootstrap_button
                .connect_clicked(clone!(@weak obj => move |button| {
                button.set_loading(true);
                obj.bootstrap_cross_signing();
                }));

            self.take_screenshot_btn2
                .connect_clicked(clone!(@weak obj => move |button| {
                    let priv_ = imp::SessionVerification::from_instance(&obj);
                    button.set_loading(true);
                    priv_.start_emoji_btn2.set_sensitive(false);
                    obj.take_screenshot();
                }));

            self.take_screenshot_btn3
                .connect_clicked(clone!(@weak obj => move |button| {
                    let priv_ = imp::SessionVerification::from_instance(&obj);
                    button.set_loading(true);
                    priv_.start_emoji_btn3.set_sensitive(false);
                    obj.take_screenshot();
                }));
        }

        fn dispose(&self, obj: &Self::Type) {
            obj.silent_cancel();
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
    pub fn new(request: &IdentityVerification, session: &Session) -> Self {
        glib::Object::new(&[("request", request), ("session", session)])
            .expect("Failed to create SessionVerification")
    }

    pub fn request(&self) -> &IdentityVerification {
        let priv_ = imp::SessionVerification::from_instance(self);
        priv_.request.get().unwrap()
    }

    fn set_request(&self, request: IdentityVerification) {
        let priv_ = imp::SessionVerification::from_instance(self);

        priv_.request.set(request).unwrap();
    }

    pub fn session(&self) -> Session {
        let priv_ = imp::SessionVerification::from_instance(self);
        priv_.session.get().unwrap().upgrade().unwrap()
    }

    fn set_session(&self, session: Session) {
        let priv_ = imp::SessionVerification::from_instance(self);

        priv_.session.set(session.downgrade()).unwrap()
    }

    /// Returns the parent GtkWindow containing this widget.
    fn parent_window(&self) -> Option<Window> {
        self.root()?.downcast().ok()
    }

    fn reset(&self) {
        let priv_ = imp::SessionVerification::from_instance(self);
        priv_.emoji_not_match_btn.set_loading(false);
        priv_.emoji_not_match_btn.set_sensitive(true);
        priv_.emoji_match_btn.set_loading(false);
        priv_.emoji_match_btn.set_sensitive(true);
        priv_.start_emoji_btn.set_loading(false);
        priv_.start_emoji_btn.set_sensitive(true);
        priv_.bootstrap_button.set_loading(false);
        priv_.start_emoji_btn2.set_loading(false);
        priv_.start_emoji_btn2.set_sensitive(true);
        priv_.take_screenshot_btn2.set_loading(false);
        priv_.take_screenshot_btn2.set_sensitive(true);
        priv_.take_screenshot_btn3.set_loading(false);
        priv_.take_screenshot_btn3.set_sensitive(true);

        while let Some(child) = priv_.emoji_row_1.first_child() {
            priv_.emoji_row_1.remove(&child);
        }

        while let Some(child) = priv_.emoji_row_2.first_child() {
            priv_.emoji_row_2.remove(&child);
        }
    }

    pub fn start_verification(&self) {
        let priv_ = imp::SessionVerification::from_instance(self);
        let request = self.request();

        self.reset();

        let handler = request.connect_notify_local(
            Some("mode"),
            clone!(@weak self as obj => move |_, _| {
                obj.update_view();
            }),
        );

        priv_.mode_handler.replace(Some(handler));
    }

    /// Cancel the verification request without telling the user about it
    fn silent_cancel(&self) {
        let priv_ = imp::SessionVerification::from_instance(self);

        if let Some(handler) = priv_.mode_handler.take() {
            self.request().disconnect(handler);
        }

        debug!("Verification request was silently canceled");

        self.request().cancel();
    }

    fn update_view(&self) {
        let priv_ = imp::SessionVerification::from_instance(self);
        let request = self.request();
        match request.mode() {
            // FIXME: we bootstrap on all errors
            VerificationMode::Error => {
                priv_.main_stack.set_visible_child_name("bootstrap");
            }
            VerificationMode::Requested => {
                priv_.main_stack.set_visible_child_name("wait-for-device");
            }
            VerificationMode::QrV1Show => {
                if let Some(qrcode) = request.qr_code() {
                    priv_.qrcode.set_qrcode(qrcode.clone());
                    priv_.main_stack.set_visible_child_name("qrcode");
                } else {
                    warn!("Failed to get qrcode for QrVerification");
                    request.start_sas();
                }
            }
            VerificationMode::QrV1Scan => {
                self.start_scanning();
            }
            VerificationMode::SasV1 => {
                // TODO: implement sas fallback when emojis arn't supported
                if let Some(SasData::Emoji(emoji)) = request.sas_data() {
                    for (index, emoji) in emoji.iter().enumerate() {
                        if index < 4 {
                            priv_.emoji_row_1.append(&Emoji::new(emoji));
                        } else {
                            priv_.emoji_row_2.append(&Emoji::new(emoji));
                        }
                    }
                    priv_.main_stack.set_visible_child_name("emoji");
                }
            }
            VerificationMode::Completed => {
                priv_.main_stack.set_visible_child_name("completed");
            }
            _ => {
                warn!("Try to show a dismissed verification");
            }
        }
    }

    fn start_scanning(&self) {
        spawn!(clone!(@weak self as obj => async move {
            let priv_ = imp::SessionVerification::from_instance(&obj);
            if priv_.qr_code_scanner.start().await {
                priv_.main_stack.set_visible_child_name("scan-qr-code");
            } else {
                priv_.main_stack.set_visible_child_name("no-camera");
            }
        }));
    }

    fn take_screenshot(&self) {
        spawn!(clone!(@weak self as obj => async move {
            let root = obj.root().unwrap();
            if let Some(code) = screenshot::capture(&root).await {
                obj.finish_scanning(code);
            } else {
                obj.reset();
            }
        }));
    }

    fn finish_scanning(&self, data: QrVerificationData) {
        let priv_ = imp::SessionVerification::from_instance(self);
        priv_.qr_code_scanner.stop();
        self.request().scanned_qr_code(data);
        priv_.main_stack.set_visible_child_name("qr-code-scanned");
    }

    fn show_recovery(&self) {
        let priv_ = imp::SessionVerification::from_instance(self);

        self.silent_cancel();

        priv_.main_stack.set_visible_child_name("recovery");
    }

    fn previous(&self) {
        let priv_ = imp::SessionVerification::from_instance(self);

        match priv_.main_stack.visible_child_name().unwrap().as_str() {
            "recovery" => {
                self.silent_cancel();
                self.start_verification();
            }
            "recovery-passphrase" | "recovery-key" => {
                priv_.main_stack.set_visible_child_name("recovery");
            }
            "wait-for-device" | "complete" => {
                self.silent_cancel();
                self.activate_action("session.logout", None);
            }
            "emoji" | "qrcode" | "scan-qr-code" | "no-camera" => {
                self.silent_cancel();
                self.start_verification();
            }
            _ => {}
        }
    }

    fn bootstrap_cross_signing(&self) {
        spawn!(clone!(@weak self as obj => async move {
            let priv_ = imp::SessionVerification::from_instance(&obj);
            let dialog = AuthDialog::new(obj.parent_window().as_ref(), &obj.session());

            let result = dialog
            .authenticate(move |client, auth_data| async move {
                if let Some(auth) = auth_data {
                    let auth = Some(auth.as_matrix_auth_data());
                    client.bootstrap_cross_signing(auth).await
                } else {
                    client.bootstrap_cross_signing(None).await
                }
            })
            .await;


            let error_message = match result {
                Some(Ok(_)) => None,
                Some(Err(error)) => {
                    error!("Failed to bootstap cross singing: {}", error);
                    Some(gettext("An error occured during the creation of the encryption keys."))
                },
                None => {
                    error!("Failed to bootstap cross singing: User cancelled the authentication");
                    Some(gettext("You cancelled the authentication needed to create the encryption keys."))
                },
            };

            if let Some(error_message) = error_message {
                let error = Error::new(move |_| {
                    let error_label = gtk::LabelBuilder::new()
                        .label(&error_message)
                        .wrap(true)
                        .build();
                    Some(error_label.upcast())
                });

                if let Some(window) = obj.session().parent_window() {
                    window.append_error(&error);
                }

                obj.silent_cancel();
                obj.start_verification();
            } else {
                priv_
                .main_stack
                .set_visible_child_name("completed");
            }
        }));
    }
}

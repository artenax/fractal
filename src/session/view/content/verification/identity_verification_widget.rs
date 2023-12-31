use std::collections::HashMap;

use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib, glib::clone, prelude::*, CompositeTemplate};
use matrix_sdk::encryption::verification::QrVerificationData;
use ruma::events::key::verification::cancel::CancelCode;
use tracing::{error, warn};

use super::Emoji;
use crate::{
    components::SpinnerButton,
    contrib::{QRCode, QRCodeExt, QrCodeScanner},
    gettext_f,
    login::Login,
    prelude::*,
    session::model::{
        IdentityVerification, SasData, VerificationMode, VerificationState,
        VerificationSupportedMethods,
    },
    spawn, toast,
};

mod imp {
    use std::cell::RefCell;

    use glib::{subclass::InitializingObject, SignalHandlerId};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/verification/identity_verification_widget.ui"
    )]
    pub struct IdentityVerificationWidget {
        pub request: RefCell<Option<IdentityVerification>>,
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
        pub scan_qr_code_btn: TemplateChild<SpinnerButton>,
        #[template_child]
        pub accept_btn: TemplateChild<SpinnerButton>,
        #[template_child]
        pub decline_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub qr_code_scanner: TemplateChild<QrCodeScanner>,
        pub state_handler: RefCell<Option<SignalHandlerId>>,
        pub name_handler: RefCell<Option<SignalHandlerId>>,
        pub supported_methods_handler: RefCell<Option<SignalHandlerId>>,
        #[template_child]
        pub confirm_scanning_btn: TemplateChild<SpinnerButton>,
        #[template_child]
        pub cancel_scanning_btn: TemplateChild<SpinnerButton>,
        #[template_child]
        pub accept_request_title: TemplateChild<gtk::Label>,
        #[template_child]
        pub accept_request_instructions: TemplateChild<gtk::Label>,
        #[template_child]
        pub scan_qrcode_title: TemplateChild<gtk::Label>,
        #[template_child]
        pub scan_qrcode_instructions: TemplateChild<gtk::Label>,
        #[template_child]
        pub qrcode_scanned_message: TemplateChild<gtk::Label>,
        #[template_child]
        pub qrcode_title: TemplateChild<gtk::Label>,
        #[template_child]
        pub qrcode_instructions: TemplateChild<gtk::Label>,
        #[template_child]
        pub emoji_title: TemplateChild<gtk::Label>,
        #[template_child]
        pub emoji_instructions: TemplateChild<gtk::Label>,
        #[template_child]
        pub completed_title: TemplateChild<gtk::Label>,
        #[template_child]
        pub completed_message: TemplateChild<gtk::Label>,
        #[template_child]
        pub wait_for_other_party_title: TemplateChild<gtk::Label>,
        #[template_child]
        pub wait_for_other_party_instructions: TemplateChild<gtk::Label>,
        #[template_child]
        pub confirm_scanned_qr_code_question: TemplateChild<gtk::Label>,
        /// The ancestor login view, if this verification happens during login.
        pub login: glib::WeakRef<Login>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for IdentityVerificationWidget {
        const NAME: &'static str = "IdentityVerificationWidget";
        type Type = super::IdentityVerificationWidget;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.install_action("verification.decline", None, move |obj, _, _| {
                obj.decline();
            });

            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for IdentityVerificationWidget {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<IdentityVerification>("request")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecObject::builder::<Login>("login")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "request" => self.obj().set_request(value.get().unwrap()),
                "login" => self.obj().set_login(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "request" => self.obj().request().to_value(),
                "login" => self.obj().login().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            self.accept_btn
                .connect_clicked(clone!(@weak obj => move |button| {
                    button.set_loading(true);
                    obj.imp().decline_btn.set_sensitive(false);
                    obj.accept();
                }));

            self.emoji_match_btn
                .connect_clicked(clone!(@weak obj => move |button| {
                    button.set_loading(true);
                    obj.imp().emoji_not_match_btn.set_sensitive(false);
                    if let Some(request) = obj.request() {
                        request.emoji_match();
                    }
                }));

            self.emoji_not_match_btn
                .connect_clicked(clone!(@weak obj => move |button| {
                    button.set_loading(true);
                    obj.imp().emoji_match_btn.set_sensitive(false);
                    if let Some(request) = obj.request() {
                        request.emoji_not_match();
                    }
                }));

            self.start_emoji_btn
                .connect_clicked(clone!(@weak obj => move |button| {
                    button.set_loading(true);
                    obj.imp().scan_qr_code_btn.set_sensitive(false);
                    if let Some(request) = obj.request() {
                        request.start_sas();
                    }
                }));
            self.start_emoji_btn2
                .connect_clicked(clone!(@weak obj => move |button| {
                    button.set_loading(true);
                    if let Some(request) = obj.request() {
                        request.start_sas();
                    }
                }));

            self.scan_qr_code_btn
                .connect_clicked(clone!(@weak obj => move |button| {
                    let imp = obj.imp();
                    button.set_loading(true);
                    imp.start_emoji_btn.set_sensitive(false);
                    obj.start_scanning();
                }));

            self.confirm_scanning_btn
                .connect_clicked(clone!(@weak obj => move |button| {
                    button.set_loading(true);
                    obj.imp().cancel_scanning_btn.set_sensitive(false);
                    if let Some(request) = obj.request() {
                        request.confirm_scanning();
                    }
                }));

            self.cancel_scanning_btn
                .connect_clicked(clone!(@weak obj => move |button| {
                    button.set_loading(true);
                    obj.imp().confirm_scanning_btn.set_sensitive(false);
                    if let Some(request) = obj.request() {
                        request.cancel(true);
                    }
                }));

            self.qr_code_scanner
                .connect_code_detected(clone!(@weak obj => move |_, data| {
                    obj.finish_scanning(data);
                }));
        }

        fn dispose(&self) {
            if let Some(request) = self.obj().request() {
                if let Some(handler) = self.state_handler.take() {
                    request.disconnect(handler);
                }

                if let Some(handler) = self.name_handler.take() {
                    request.user().disconnect(handler);
                }

                if let Some(handler) = self.supported_methods_handler.take() {
                    request.disconnect(handler);
                }
            }
        }
    }

    impl WidgetImpl for IdentityVerificationWidget {
        fn map(&self) {
            self.parent_map();
            self.obj().update_view();
        }
    }
    impl BinImpl for IdentityVerificationWidget {}
}

glib::wrapper! {
    pub struct IdentityVerificationWidget(ObjectSubclass<imp::IdentityVerificationWidget>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl IdentityVerificationWidget {
    pub fn new(request: &IdentityVerification) -> Self {
        glib::Object::builder().property("request", request).build()
    }

    /// The ancestor login view, if this verification happens during login.
    pub fn login(&self) -> Option<Login> {
        self.imp().login.upgrade()
    }

    /// Set the ancestor login view.
    pub fn set_login(&self, login: Option<Login>) {
        if self.login() == login {
            return;
        }

        self.imp().login.set(login.as_ref());
        self.notify("login");
    }

    /// The object holding the data for the verification.
    pub fn request(&self) -> Option<IdentityVerification> {
        self.imp().request.borrow().clone()
    }

    /// Set the object holding the data for the verification.
    pub fn set_request(&self, request: Option<IdentityVerification>) {
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

            if let Some(handler) = imp.name_handler.take() {
                previous_request.user().disconnect(handler);
            }

            if let Some(handler) = imp.supported_methods_handler.take() {
                previous_request.disconnect(handler);
            }
        }

        if let Some(ref request) = request {
            let handler = request.connect_notify_local(
                Some("state"),
                clone!(@weak self as obj => move |_, _| {
                    obj.update_view();
                }),
            );

            imp.state_handler.replace(Some(handler));

            let handler = request.user().connect_notify_local(
                Some("display-name"),
                clone!(@weak self as obj => move |_, _| {
                    obj.init_mode();
                }),
            );

            imp.name_handler.replace(Some(handler));

            let handler = request.connect_notify_local(
                Some("supported-methods"),
                clone!(@weak self as obj => move |_, _| {
                    obj.update_supported_methods();
                }),
            );

            imp.supported_methods_handler.replace(Some(handler));
        }

        imp.request.replace(request);
        self.init_mode();
        self.update_view();
        self.update_supported_methods();
        self.notify("request");
    }

    fn reset(&self) {
        let imp = self.imp();
        imp.accept_btn.set_loading(false);
        imp.accept_btn.set_sensitive(true);
        imp.decline_btn.set_sensitive(true);
        imp.scan_qr_code_btn.set_loading(false);
        imp.scan_qr_code_btn.set_sensitive(true);
        imp.emoji_not_match_btn.set_loading(false);
        imp.emoji_not_match_btn.set_sensitive(true);
        imp.emoji_match_btn.set_loading(false);
        imp.emoji_match_btn.set_sensitive(true);
        imp.start_emoji_btn.set_loading(false);
        imp.start_emoji_btn.set_sensitive(true);
        imp.start_emoji_btn2.set_loading(false);
        imp.start_emoji_btn2.set_sensitive(true);
        imp.confirm_scanning_btn.set_loading(false);
        imp.confirm_scanning_btn.set_sensitive(true);
        imp.cancel_scanning_btn.set_loading(false);
        imp.cancel_scanning_btn.set_sensitive(true);

        self.clean_emoji();
    }

    fn clean_emoji(&self) {
        let imp = self.imp();

        while let Some(child) = imp.emoji_row_1.first_child() {
            imp.emoji_row_1.remove(&child);
        }

        while let Some(child) = imp.emoji_row_2.first_child() {
            imp.emoji_row_2.remove(&child);
        }
    }

    pub fn accept(&self) {
        if let Some(request) = self.request() {
            request.accept();
        }
    }

    pub fn decline(&self) {
        if let Some(request) = self.request() {
            request.cancel(true);
        }
    }

    fn update_view(&self) {
        let Some(request) = self.request() else {
            return;
        };
        let imp = self.imp();

        match request.state() {
            VerificationState::Requested => {
                imp.main_stack.set_visible_child_name("accept-request");
            }
            VerificationState::RequestSend => {
                imp.main_stack
                    .set_visible_child_name("wait-for-other-party");
            }
            VerificationState::QrV1Show => {
                if let Some(qrcode) = request.qr_code() {
                    imp.qrcode.set_qrcode(qrcode.clone());
                    imp.main_stack.set_visible_child_name("qrcode");
                } else {
                    warn!("Failed to get qrcode for QrVerification");
                    request.start_sas();
                }
            }
            VerificationState::QrV1Scan => {
                self.start_scanning();
            }
            VerificationState::QrV1Scanned => {
                imp.main_stack
                    .set_visible_child_name("confirm-scanned-qr-code");
            }
            VerificationState::SasV1 => {
                self.clean_emoji();
                match request.sas_data().unwrap() {
                    SasData::Emoji(emoji) => {
                        let emoji_i18n = sas_emoji_i18n();
                        for (index, emoji) in emoji.iter().enumerate() {
                            let emoji_name = emoji_i18n
                                .get(emoji.description)
                                .map(String::as_str)
                                .unwrap_or(emoji.description);
                            if index < 4 {
                                imp.emoji_row_1
                                    .append(&Emoji::new(emoji.symbol, emoji_name));
                            } else {
                                imp.emoji_row_2
                                    .append(&Emoji::new(emoji.symbol, emoji_name));
                            }
                        }
                    }
                    SasData::Decimal((a, b, c)) => {
                        let container = gtk::Box::builder()
                            .spacing(24)
                            .css_classes(vec!["emoji".to_string()])
                            .build();
                        container.append(&gtk::Label::builder().label(a.to_string()).build());
                        container.append(&gtk::Label::builder().label(b.to_string()).build());
                        container.append(&gtk::Label::builder().label(c.to_string()).build());
                        imp.emoji_row_1.append(&container);
                    }
                }
                imp.main_stack.set_visible_child_name("emoji");
            }
            VerificationState::Completed => {
                spawn!(clone!(@weak self as obj => async move {
                    obj.handle_completed().await;
                }));
            }
            VerificationState::Cancelled | VerificationState::Error => self.show_error(),
            VerificationState::Dismissed | VerificationState::Passive => {}
        }
    }

    fn start_scanning(&self) {
        spawn!(clone!(@weak self as obj => async move {
            let imp = obj.imp();
            imp.qr_code_scanner.start().await;
            imp.main_stack.set_visible_child_name("scan-qr-code");
        }));
    }

    fn finish_scanning(&self, data: QrVerificationData) {
        let imp = self.imp();
        imp.qr_code_scanner.stop();
        if let Some(request) = self.request() {
            request.scanned_qr_code(data);
        }
        imp.main_stack.set_visible_child_name("qr-code-scanned");
    }

    fn update_supported_methods(&self) {
        let imp = self.imp();
        if let Some(request) = self.request() {
            imp.scan_qr_code_btn.set_visible(
                request
                    .supported_methods()
                    .contains(VerificationSupportedMethods::QR_SCAN),
            );
        }
    }

    fn init_mode(&self) {
        let imp = self.imp();
        let request = if let Some(request) = self.request() {
            request
        } else {
            return;
        };

        match request.mode() {
            VerificationMode::CurrentSession => {
                // accept_request_title and accept_request_instructions won't be shown
                imp.accept_request_instructions
                    .set_label(&gettext("Verify the new session from the current session."));
                imp.scan_qrcode_title.set_label(&gettext("Verify Session"));
                imp.scan_qrcode_instructions.set_label(&gettext(
                    "Scan the QR code from another session logged into this account.",
                ));
                imp.qrcode_scanned_message.set_label(&gettext("You scanned the QR code successfully. You may need to confirm the verification from the other session."));
                imp.qrcode_title.set_label(&gettext("Verify Session"));
                imp.qrcode_instructions
                    .set_label(&gettext("Scan this QR code from the other session."));
                imp.emoji_title.set_label(&gettext("Verify Session"));
                imp.emoji_instructions.set_label(&gettext(
                    "Check if the same emoji appear in the same order on the other device.",
                ));
                imp.completed_title.set_label(&gettext("Request Complete"));
                imp.completed_message.set_label(&gettext(
                    "This session is ready to send and receive secure messages.",
                ));
                imp.confirm_scanned_qr_code_question
                    .set_label(&gettext("Does the other session show a confirmation?"));
            }
            VerificationMode::OtherSession => {
                imp.accept_request_title
                    .set_label(&gettext("Login Request From Another Session"));
                imp.accept_request_instructions
                    .set_label(&gettext("Verify the new session from the current session."));
                imp.scan_qrcode_title.set_label(&gettext("Verify Session"));
                imp.scan_qrcode_instructions
                    .set_label(&gettext("Scan the QR code displayed by the other session."));
                imp.qrcode_scanned_message.set_label(&gettext("You scanned the QR code successfully. You may need to confirm the verification from the other session."));
                imp.qrcode_title.set_label(&gettext("Verify Session"));
                imp.qrcode_instructions.set_label(&gettext(
                    "Scan this QR code from the newly logged in session.",
                ));
                imp.emoji_title.set_label(&gettext("Verify Session"));
                imp.emoji_instructions.set_label(&gettext(
                    "Check if the same emoji appear in the same order on the other device.",
                ));
                imp.completed_title.set_label(&gettext("Request Complete"));
                imp.completed_message.set_label(&gettext(
                    "The new session is now ready to send and receive secure messages.",
                ));
                imp.wait_for_other_party_title
                    .set_label(&gettext("Get Another Device"));
                imp.wait_for_other_party_instructions.set_label(&gettext(
                    "Accept the verification request from another session or device.",
                ));
                imp.confirm_scanned_qr_code_question
                    .set_label(&gettext("Does the other session show a confirmation?"));
            }
            VerificationMode::User => {
                let name = request.user().display_name();
                imp.accept_request_title
                    .set_markup(&gettext("Verification Request"));
                imp
                    .accept_request_instructions
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    .set_markup(&gettext_f("{user} asked to be verified. Verifying a user increases the security of the conversation.", &[("user", &format!("<b>{name}</b>"))]));
                imp.scan_qrcode_title
                    .set_markup(&gettext("Verification Request"));
                imp.scan_qrcode_instructions.set_markup(&gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    "Scan the QR code shown on the device of {user}.",
                    &[("user", &format!("<b>{name}</b>"))],
                ));
                // Translators: Do NOT translate the content between '{' and '}', this is a
                // variable name.
                imp.qrcode_scanned_message.set_markup(&gettext_f("You scanned the QR code successfully. {user} may need to confirm the verification.", &[("user", &format!("<b>{name}</b>"))]));
                imp.qrcode_title
                    .set_markup(&gettext("Verification Request"));
                imp.qrcode_instructions.set_markup(&gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    "Ask {user} to scan this QR code from their session.",
                    &[("user", &format!("<b>{name}</b>"))],
                ));
                imp.emoji_title.set_markup(&gettext("Verification Request"));
                imp.emoji_instructions.set_markup(&gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    "Ask {user} if they see the following emoji appear in the same order on their screen.",
                    &[("user", &format!("<b>{name}</b>"))]
                ));
                imp.completed_title
                    .set_markup(&gettext("Verification Complete"));
                // Translators: Do NOT translate the content between '{' and '}', this is a
                // variable name.
                imp.completed_message.set_markup(&gettext_f("{user} is verified and you can now be sure that your communication will be private.", &[("user", &format!("<b>{name}</b>"))]));
                imp.wait_for_other_party_title.set_markup(&gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    "Waiting for {user}",
                    &[("user", &format!("<b>{name}</b>"))],
                ));
                imp.wait_for_other_party_instructions.set_markup(&gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    "Ask {user} to accept the verification request.",
                    &[("user", &format!("<b>{name}</b>"))],
                ));
                imp.confirm_scanned_qr_code_question.set_markup(&gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    "Does {user} see a confirmation on their session?",
                    &[("user", &format!("<b>{name}</b>"))],
                ));
            }
        }
    }

    async fn handle_completed(&self) {
        let request = match self.request() {
            Some(request) => request,
            None => return,
        };
        let imp = self.imp();

        if request.mode() == VerificationMode::CurrentSession {
            // Check that the session is marked as verified.
            let session = request.session();
            if !session.is_verified().await {
                // This should not be possible if verification passed.
                error!("Session is not verified at the end of verification");
            }
        }

        if let Some(login) = self.login() {
            login.show_completed();
        } else {
            imp.main_stack.set_visible_child_name("completed");
        }
    }

    fn show_error(&self) {
        let Some(request) = self.request() else {
            return;
        };

        if request.hide_error() {
            return;
        }

        let error_message = if let Some(info) = request.cancel_info() {
            match info.cancel_code() {
                CancelCode::User => Some(gettext("You cancelled the verification process.")),
                CancelCode::Timeout => Some(gettext(
                    "The verification process failed because it reached a timeout.",
                )),
                CancelCode::Accepted => {
                    Some(gettext("You accepted the request from an other session."))
                }
                _ => match info.cancel_code().as_str() {
                    "m.mismatched_sas" => Some(gettext("The emoji did not match.")),
                    _ => None,
                },
            }
        } else {
            None
        };

        let error_message = error_message.unwrap_or_else(|| {
            gettext("An unknown error occurred during the verification process.")
        });

        toast!(self, error_message);
    }
}

/// Get the SAS emoji translations for the current locale.
///
/// Returns a map of emoji name to its translation.
fn sas_emoji_i18n() -> HashMap<String, String> {
    for lang in glib::language_names()
        .into_iter()
        .flat_map(|locale| glib::locale_variants(&locale))
    {
        if let Some(emoji_i18n) = gio::resources_lookup_data(
            &format!("/org/gnome/Fractal/sas-emoji/{lang}.json"),
            gio::ResourceLookupFlags::NONE,
        )
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        {
            return emoji_i18n;
        }
    }

    HashMap::new()
}

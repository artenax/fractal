use std::{cell::Cell, fmt::Debug, future::Future};

use adw::subclass::prelude::*;
use gtk::{
    gdk,
    gio::prelude::*,
    glib::{self, clone},
    prelude::*,
    CompositeTemplate,
};
use matrix_sdk::{
    ruma::{
        api::{
            client::{
                error::ErrorBody,
                uiaa::{
                    AuthData as MatrixAuthData, AuthType,
                    FallbackAcknowledgement as MatrixFallbackAcknowledgement,
                    Password as MatrixPassword, UiaaInfo, UiaaResponse, UserIdentifier,
                },
            },
            error::{FromHttpResponseError, ServerError},
        },
        assign,
    },
    Error,
    HttpError::UiaaError,
};

use crate::{
    session::{Session, UserExt},
    spawn_tokio,
};

pub struct Password {
    pub user_id: String,
    pub password: String,
    pub session: Option<String>,
}

pub struct FallbackAcknowledgement {
    pub session: String,
}

// FIXME: we can't move the ruma AuthData between threads
// because it's not owned data and doesn't live long enough.
// Therefore we have our own AuthData.
pub enum AuthData {
    Password(Password),
    FallbackAcknowledgement(FallbackAcknowledgement),
}

impl AuthData {
    pub fn as_matrix_auth_data(&self) -> MatrixAuthData {
        match self {
            AuthData::Password(Password {
                user_id,
                password,
                session,
            }) => MatrixAuthData::Password(assign!(MatrixPassword::new(
                                UserIdentifier::UserIdOrLocalpart(user_id),
                                password,
                            ), { session: session.as_deref() })),
            AuthData::FallbackAcknowledgement(FallbackAcknowledgement { session }) => {
                MatrixAuthData::FallbackAcknowledgement(MatrixFallbackAcknowledgement::new(session))
            }
        }
    }
}

#[derive(Debug)]
pub enum AuthError {
    ServerResponse(Box<Error>),
    MalformedResponse,
    StageFailed,
    NoStageToChoose,
    UserCancelled,
}

mod imp {
    use std::cell::RefCell;

    use glib::{
        object::WeakRef,
        subclass::{InitializingObject, Signal},
        SignalHandlerId,
    };
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/components-auth-dialog.ui")]
    pub struct AuthDialog {
        pub session: WeakRef<Session>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub password: TemplateChild<gtk::PasswordEntry>,
        #[template_child]
        pub error: TemplateChild<gtk::Label>,

        #[template_child]
        pub button_cancel: TemplateChild<gtk::Button>,
        #[template_child]
        pub button_ok: TemplateChild<gtk::Button>,

        #[template_child]
        pub open_browser_btn: TemplateChild<gtk::Button>,
        pub open_browser_btn_handler: RefCell<Option<SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AuthDialog {
        const NAME: &'static str = "ComponentsAuthDialog";
        type Type = super::AuthDialog;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            let response = [false].to_variant();
            klass.add_binding_signal(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                "response",
                Some(&response),
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AuthDialog {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Session>("session")
                    .construct_only()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "session" => self.session.set(value.get().ok().as_ref()),
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

            self.button_cancel
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("response", &[&false]);
                }));

            self.button_ok
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("response", &[&true]);
                }));

            obj.connect_close_request(
                clone!(@weak obj => @default-return gtk::Inhibit(false), move |_| {
                    obj.emit_by_name::<()>("response", &[&false]);
                    gtk::Inhibit(false)
                }),
            );
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder("response")
                    .param_types([bool::static_type()])
                    .action()
                    .build()]
            });
            SIGNALS.as_ref()
        }
    }
    impl WidgetImpl for AuthDialog {}
    impl WindowImpl for AuthDialog {}
    impl AdwWindowImpl for AuthDialog {}
}

glib::wrapper! {
    /// Dialog to guide the user through an authentication flow.
    pub struct AuthDialog(ObjectSubclass<imp::AuthDialog>)
        @extends gtk::Widget, adw::Window, gtk::Dialog, gtk::Window, @implements gtk::Accessible;
}

impl AuthDialog {
    pub fn new(transient_for: Option<&impl IsA<gtk::Window>>, session: &Session) -> Self {
        glib::Object::builder()
            .property("transient-for", &transient_for)
            .property("session", session)
            .build()
    }

    /// The current session.
    pub fn session(&self) -> Session {
        self.imp().session.upgrade().unwrap()
    }

    /// Authenticates the user to the server via an authentication flow.
    ///
    /// The type of flow and the required stages are negotiated at time of
    /// authentication. Returns the last server response on success.
    pub async fn authenticate<
        Response: Send + 'static,
        F1: Future<Output = Result<Response, Error>> + Send + 'static,
        FN: Fn(matrix_sdk::Client, Option<AuthData>) -> F1 + Send + 'static + Sync + Clone,
    >(
        &self,
        callback: FN,
    ) -> Result<Response, AuthError> {
        let client = self.session().client();
        let mut auth_data = None;

        loop {
            let callback_clone = callback.clone();
            let client_clone = client.clone();
            let handle = spawn_tokio!(async move { callback_clone(client_clone, auth_data).await });
            let response = handle.await.unwrap();

            let uiaa_info: UiaaInfo = match response {
                Ok(result) => return Ok(result),
                Err(Error::Http(UiaaError(FromHttpResponseError::Server(ServerError::Known(
                    UiaaResponse::AuthResponse(uiaa_info),
                ))))) => uiaa_info,
                Err(error) => return Err(AuthError::ServerResponse(Box::new(error))),
            };

            self.show_auth_error(&uiaa_info.auth_error);

            let stage_nr = uiaa_info.completed.len();
            let possible_stages: Vec<&AuthType> = uiaa_info
                .flows
                .iter()
                .filter(|flow| flow.stages.starts_with(&uiaa_info.completed))
                .flat_map(|flow| flow.stages.get(stage_nr))
                .collect();

            let session = uiaa_info.session;
            auth_data = Some(self.perform_next_stage(&session, &possible_stages).await?);
        }
    }

    /// Performs the most preferred one of the given stages.
    ///
    /// Stages that Fractal actually implements are preferred.
    async fn perform_next_stage(
        &self,
        session: &Option<String>,
        stages: &[&AuthType],
    ) -> Result<AuthData, AuthError> {
        // Default to first stage if non is supported.
        let a_stage = stages.first().ok_or(AuthError::NoStageToChoose)?;
        for stage in stages {
            if let Some(auth_result) = self.try_perform_stage(session, stage).await {
                return auth_result;
            }
        }
        let session = session.clone().ok_or(AuthError::MalformedResponse)?;
        self.perform_fallback(session, a_stage).await
    }

    /// Tries to perform the given stage.
    ///
    /// Returns None if the stage is not implemented by Fractal.
    async fn try_perform_stage(
        &self,
        session: &Option<String>,
        stage: &AuthType,
    ) -> Option<Result<AuthData, AuthError>> {
        match stage {
            AuthType::Password => Some(self.perform_password_stage(session.clone()).await),
            // TODO implement other authentication types
            // See: https://gitlab.gnome.org/GNOME/fractal/-/issues/835
            _ => None,
        }
    }

    /// Performs the password stage.
    async fn perform_password_stage(&self, session: Option<String>) -> Result<AuthData, AuthError> {
        let stack = &self.imp().stack;
        stack.set_visible_child_name(AuthType::Password.as_ref());
        self.show_and_wait_for_response().await?;

        let user_id = self.session().user().unwrap().user_id().to_string();
        let password = self.imp().password.text().to_string();

        Ok(AuthData::Password(Password {
            user_id,
            password,
            session,
        }))
    }

    /// Performs a web-based fallback for the given stage.
    async fn perform_fallback(
        &self,
        session: String,
        stage: &AuthType,
    ) -> Result<AuthData, AuthError> {
        let client = self.session().client();
        let homeserver = spawn_tokio!(async move { client.homeserver().await })
            .await
            .unwrap();
        self.imp().stack.set_visible_child_name("fallback");
        self.setup_fallback_page(homeserver.as_str(), stage.as_ref(), &session);
        self.show_and_wait_for_response().await?;

        Ok(AuthData::FallbackAcknowledgement(FallbackAcknowledgement {
            session,
        }))
    }

    /// Lets the user complete the current stage.
    async fn show_and_wait_for_response(&self) -> Result<(), AuthError> {
        let (sender, receiver) = futures::channel::oneshot::channel();
        let sender = Cell::new(Some(sender));

        let handler_id = self.connect_response(move |_, response| {
            if let Some(sender) = sender.take() {
                sender.send(response).unwrap();
            }
        });

        self.show();

        let result = receiver.await.unwrap();
        self.disconnect(handler_id);
        self.close();

        result.then_some(()).ok_or(AuthError::UserCancelled)
    }

    fn show_auth_error(&self, auth_error: &Option<ErrorBody>) {
        let priv_ = self.imp();

        if let Some(auth_error) = auth_error {
            priv_.error.set_label(&auth_error.message);
            priv_.error.show();
        } else {
            priv_.error.hide();
        }
    }

    fn setup_fallback_page(&self, homeserver: &str, auth_type: &str, session: &str) {
        let priv_ = self.imp();

        if let Some(handler) = priv_.open_browser_btn_handler.take() {
            priv_.open_browser_btn.disconnect(handler);
        }

        let uri = format!(
            "{}_matrix/client/r0/auth/{}/fallback/web?session={}",
            homeserver, auth_type, session
        );

        let handler =
            priv_
                .open_browser_btn
                .connect_clicked(clone!(@weak self as obj => move |_| {
                    gtk::show_uri(obj.transient_for().as_ref(), &uri, gdk::CURRENT_TIME);
                }));

        priv_.open_browser_btn_handler.replace(Some(handler));
    }

    pub fn connect_response<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("response", true, move |values| {
            let obj: Self = values[0].get().unwrap();
            let response = values[1].get::<bool>().unwrap();

            f(&obj, response);

            None
        })
    }
}

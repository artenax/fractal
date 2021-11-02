use crate::session::user::UserExt;
use crate::session::User;
use crate::spawn_tokio;
use gtk::{glib, glib::clone, prelude::*, subclass::prelude::*};
use log::error;
use matrix_sdk::{
    encryption::{
        identities::RequestVerificationError,
        verification::{
            CancelInfo, QrVerification, SasVerification, Verification as MatrixVerification,
            VerificationRequest,
        },
    },
    ruma::{
        api::client::r0::sync::sync_events::ToDevice, events::AnyToDeviceEvent, identifiers::UserId,
    },
    Client, Error as MatrixError,
};
use qrcode::QrCode;
use tokio::sync::mpsc;

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum State {
    Request,
    Ready,
    Start,
    Cancel,
    Accept,
    Key,
    Mac,
    Done,
}

impl Default for State {
    fn default() -> Self {
        Self::Request
    }
}

#[derive(Debug, Clone)]
pub enum Verification {
    SasV1(SasVerification),
    QrV1(QrVerification),
    Request(VerificationRequest),
}

impl Verification {
    fn cancel_info(&self) -> Option<CancelInfo> {
        match self {
            Verification::QrV1(verification) => verification.cancel_info(),
            Verification::SasV1(verification) => verification.cancel_info(),
            Verification::Request(verification) => verification.cancel_info(),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, glib::GEnum)]
#[repr(u32)]
#[genum(type_name = "VerificationMode")]
pub enum Mode {
    IdentityNotFound,
    Unavailable,
    Requested,
    SasV1,
    QrV1,
    Completed,
    Cancelled,
}

impl Default for Mode {
    fn default() -> Self {
        Self::Unavailable
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum UserAction {
    Match,
    NotMatch,
    Cancel,
    StartSas,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Message {
    UserAction(UserAction),
    Sync((String, State)),
}

mod imp {
    use super::*;
    use glib::object::WeakRef;
    use glib::source::SourceId;
    use once_cell::{sync::Lazy, unsync::OnceCell};
    use std::cell::{Cell, RefCell};

    #[derive(Debug, Default)]
    pub struct IdentityVerification {
        pub user: OnceCell<WeakRef<User>>,
        pub mode: Cell<Mode>,
        pub sync_sender: RefCell<Option<mpsc::Sender<Message>>>,
        pub main_sender: OnceCell<glib::SyncSender<Verification>>,
        pub request: RefCell<Option<Verification>>,
        pub source_id: RefCell<Option<SourceId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for IdentityVerification {
        const NAME: &'static str = "IdentityVerification";
        type Type = super::IdentityVerification;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for IdentityVerification {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpec::new_object(
                        "user",
                        "User",
                        "The user to be verified",
                        User::static_type(),
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpec::new_enum(
                        "mode",
                        "Mode",
                        "The verification mode used",
                        Mode::static_type(),
                        Mode::default() as i32,
                        glib::ParamFlags::READABLE | glib::ParamFlags::EXPLICIT_NOTIFY,
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
                "user" => obj.set_user(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "user" => obj.user().to_value(),
                "mode" => obj.mode().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            let (main_sender, main_receiver) =
                glib::MainContext::sync_channel::<Verification>(Default::default(), 100);

            let source_id = main_receiver.attach(
                None,
                clone!(@weak obj => @default-return glib::Continue(false), move |verification| {
                    let mode = match verification {
                        Verification::QrV1(_) => Mode::QrV1,
                        Verification::SasV1(_) => Mode::SasV1,
                        Verification::Request(_) => Mode::Requested,
                    };
                    obj.set_request(Some(verification));
                    obj.set_mode(mode);

                    glib::Continue(true)
                }),
            );

            self.main_sender.set(main_sender).unwrap();
            self.source_id.replace(Some(source_id));
        }

        fn dispose(&self, obj: &Self::Type) {
            obj.cancel();
            if let Some(source_id) = self.source_id.take() {
                let _ = glib::Source::remove(source_id);
            }
        }
    }
}

glib::wrapper! {
    pub struct IdentityVerification(ObjectSubclass<imp::IdentityVerification>);
}

impl IdentityVerification {
    pub fn new(user: &User) -> Self {
        glib::Object::new(&[("user", user)]).expect("Failed to create IdentityVerification")
    }

    pub fn user(&self) -> User {
        let priv_ = imp::IdentityVerification::from_instance(self);
        priv_.user.get().unwrap().upgrade().unwrap()
    }

    fn set_user(&self, user: User) {
        let priv_ = imp::IdentityVerification::from_instance(self);
        priv_.user.set(user.downgrade()).unwrap()
    }

    /// Start an interactive identity verification
    /// Already in progress verifications are cancelled before starting a new one
    pub async fn start(&self) -> Result<(), RequestVerificationError> {
        let priv_ = imp::IdentityVerification::from_instance(self);
        let user = self.user();
        let client = user.session().client();
        let user_id = user.user_id().clone();
        let main_sender = priv_.main_sender.get().unwrap().clone();

        self.set_request(None);
        // TODO cancel any other request in progress

        let (sync_sender, sync_receiver) = mpsc::channel(100);
        priv_.sync_sender.replace(Some(sync_sender));

        // TODO add timeout

        let result =
            spawn_tokio!(async move { start(client, user_id, main_sender, sync_receiver).await })
                .await
                .unwrap()?;

        priv_.sync_sender.take();

        self.set_mode(result);
        Ok(())
    }

    pub fn emoji_match(&self) {
        let priv_ = imp::IdentityVerification::from_instance(self);

        if let Some(sync_sender) = &*priv_.sync_sender.borrow() {
            let result = sync_sender.try_send(Message::UserAction(UserAction::Match));

            if let Err(error) = result {
                error!("Failed to send message to tokio runtime: {}", error);
            }
        }
    }

    pub fn emoji_not_match(&self) {
        let priv_ = imp::IdentityVerification::from_instance(self);

        if let Some(sync_sender) = &*priv_.sync_sender.borrow() {
            let result = sync_sender.try_send(Message::UserAction(UserAction::NotMatch));
            if let Err(error) = result {
                error!("Failed to send message to tokio runtime: {}", error);
            }
        }
    }

    pub fn mode(&self) -> Mode {
        let priv_ = imp::IdentityVerification::from_instance(self);
        priv_.mode.get()
    }

    fn set_mode(&self, mode: Mode) {
        let priv_ = imp::IdentityVerification::from_instance(self);

        if self.mode() == mode {
            return;
        }

        priv_.mode.set(mode);
        self.notify("mode");
    }

    fn set_request(&self, request: Option<Verification>) {
        let priv_ = imp::IdentityVerification::from_instance(self);

        priv_.request.replace(request);
    }

    /// Get the QrCode for this verification request
    ///
    /// This is only set once the request reached the `State::Ready`
    /// and if QrCode verification is possible
    pub fn qr_code(&self) -> Option<QrCode> {
        let priv_ = imp::IdentityVerification::from_instance(self);

        match &*priv_.request.borrow() {
            Some(Verification::QrV1(qr_verification)) => qr_verification.to_qr_code().ok(),
            _ => None,
        }
    }

    /// Get the Emojis for this verification request
    ///
    /// This is only set once the request reached the `State::Ready`
    /// and if a Sas verification was started
    pub fn emoji(&self) -> Option<[(&'static str, &'static str); 7]> {
        let priv_ = imp::IdentityVerification::from_instance(self);

        match &*priv_.request.borrow() {
            Some(Verification::SasV1(qr_verification)) => qr_verification.emoji(),
            _ => None,
        }
    }

    pub fn start_sas(&self) {
        let priv_ = imp::IdentityVerification::from_instance(self);

        if let Some(sync_sender) = &*priv_.sync_sender.borrow() {
            let result = sync_sender.try_send(Message::UserAction(UserAction::StartSas));

            if let Err(error) = result {
                error!("Failed to send message to tokio runtime: {}", error);
            }
        }
    }

    pub fn cancel(&self) {
        let priv_ = imp::IdentityVerification::from_instance(self);
        if let Some(sync_sender) = &*priv_.sync_sender.borrow() {
            let result = sync_sender.try_send(Message::UserAction(UserAction::Cancel));
            if let Err(error) = result {
                error!("Failed to send message to tokio runtime: {}", error);
            }
        }
    }

    /// Get information about why the request was cancelled
    pub fn cancel_info(&self) -> Option<CancelInfo> {
        let priv_ = imp::IdentityVerification::from_instance(self);

        if let Some(verification) = &*priv_.request.borrow() {
            verification.cancel_info()
        } else {
            None
        }
    }

    pub fn handle_response_to_device(&self, to_device: ToDevice) {
        let priv_ = imp::IdentityVerification::from_instance(self);

        for event in to_device.events.iter().filter_map(|e| e.deserialize().ok()) {
            let (flow_id, state) = match event {
                AnyToDeviceEvent::KeyVerificationRequest(e) => {
                    (e.content.transaction_id, State::Request)
                }
                AnyToDeviceEvent::KeyVerificationReady(e) => {
                    (e.content.transaction_id, State::Ready)
                }
                AnyToDeviceEvent::KeyVerificationStart(e) => {
                    (e.content.transaction_id, State::Start)
                }
                AnyToDeviceEvent::KeyVerificationCancel(e) => {
                    (e.content.transaction_id, State::Cancel)
                }
                AnyToDeviceEvent::KeyVerificationAccept(e) => {
                    (e.content.transaction_id, State::Accept)
                }
                AnyToDeviceEvent::KeyVerificationMac(e) => (e.content.transaction_id, State::Mac),
                AnyToDeviceEvent::KeyVerificationKey(e) => (e.content.transaction_id, State::Key),
                AnyToDeviceEvent::KeyVerificationDone(e) => (e.content.transaction_id, State::Done),
                _ => continue,
            };

            if let Some(sync_sender) = &*priv_.sync_sender.borrow() {
                let result = sync_sender.try_send(Message::Sync((flow_id, state)));
                if let Err(error) = result {
                    error!("Failed to send message to tokio runtime: {}", error);
                }
            }
        }
    }
}

async fn start(
    client: Client,
    user_id: UserId,
    main_sender: glib::SyncSender<Verification>,
    mut sync_receiver: mpsc::Receiver<Message>,
) -> Result<Mode, RequestVerificationError> {
    let identity = if let Some(identity) = client
        .get_user_identity(&user_id)
        .await
        .map_err(|error| RequestVerificationError::Sdk(MatrixError::CryptoStoreError(error)))?
    {
        identity
    } else {
        return Ok(Mode::IdentityNotFound);
    };

    let request = identity.request_verification().await?;
    let flow_id = request.flow_id();

    let result = main_sender.send(Verification::Request(request.clone()));

    if let Err(error) = result {
        error!("Failed to send message to the main context: {}", error);
    }

    if wait_for_state(flow_id, State::Ready, &mut sync_receiver).await {
        request.cancel().await?;
        return Ok(Mode::Cancelled);
    }

    let qr_verification = request
        .generate_qr_code()
        .await
        .map_err(|error| RequestVerificationError::Sdk(error))?;

    let start_sas = if let Some(qr_verification) = qr_verification {
        let result = main_sender.send(Verification::QrV1(qr_verification));

        if let Err(error) = result {
            error!("Failed to send message to the main context: {}", error);
        }

        let (start_sas, cancel) = loop {
            match sync_receiver.recv().await.unwrap() {
                Message::Sync((id, State::Start)) if flow_id == &id => break (false, false),
                Message::Sync((id, State::Cancel)) if flow_id == &id => break (false, true),
                Message::UserAction(UserAction::Cancel) => break (false, true),
                Message::UserAction(UserAction::StartSas) => break (true, false),
                _ => {}
            }
        };

        if cancel {
            request.cancel().await?;
            return Ok(Mode::Cancelled);
        }
        start_sas
    } else {
        true
    };

    if start_sas {
        if request
            .start_sas()
            .await
            .map_err(|error| RequestVerificationError::Sdk(error))?
            .is_some()
        {
            let cancel = loop {
                match sync_receiver.recv().await {
                    Some(Message::Sync((id, State::Start))) if flow_id == &id => break false,
                    Some(Message::Sync((id, State::Accept))) if flow_id == &id => break false,
                    Some(Message::Sync((id, State::Cancel))) if flow_id == &id => break true,
                    Some(Message::UserAction(UserAction::Cancel)) => break true,
                    None => break true,
                    _ => {}
                }
            };

            if cancel {
                request.cancel().await?;
                return Ok(Mode::Cancelled);
            }
        } else {
            return Ok(Mode::Unavailable);
        }
    }

    // Get the verification struct from the sdk, this way we are sure we get the correct type
    let verification = if let Some(verification) = client.get_verification(&user_id, &flow_id).await
    {
        verification
    } else {
        return Ok(Mode::Unavailable);
    };

    match verification {
        MatrixVerification::QrV1(qr_verification) => {
            qr_verification.confirm().await?;

            if wait_for_state(flow_id, State::Done, &mut sync_receiver).await {
                request.cancel().await?;
                return Ok(Mode::Cancelled);
            }
        }
        MatrixVerification::SasV1(sas_verification) => {
            sas_verification.accept().await?;

            if wait_for_state(flow_id, State::Key, &mut sync_receiver).await {
                request.cancel().await?;
                return Ok(Mode::Cancelled);
            }

            let result = main_sender.send(Verification::SasV1(sas_verification.clone()));

            if let Err(error) = result {
                error!("Failed to send message to the main context: {}", error);
            }

            if wait_for_match_action(flow_id, &mut sync_receiver).await {
                request.cancel().await?;
                return Ok(Mode::Cancelled);
            }

            sas_verification.confirm().await?;

            if wait_for_state(flow_id, State::Done, &mut sync_receiver).await {
                request.cancel().await?;
                return Ok(Mode::Cancelled);
            }
        }
    }

    Ok(Mode::Completed)
}

async fn wait_for_state(
    flow_id: &str,
    expected_state: State,
    sync_receiver: &mut mpsc::Receiver<Message>,
) -> bool {
    loop {
        match sync_receiver.recv().await {
            Some(Message::Sync((id, State::Cancel))) if flow_id == &id => return true,
            Some(Message::Sync((id, state))) if flow_id == &id && expected_state == state => break,
            Some(Message::UserAction(UserAction::Cancel)) => return true,
            None => return true,
            _ => {}
        }
    }

    false
}

async fn wait_for_match_action(flow_id: &str, sync_receiver: &mut mpsc::Receiver<Message>) -> bool {
    loop {
        match sync_receiver.recv().await {
            Some(Message::Sync((id, State::Cancel))) if flow_id == &id => return true,
            Some(Message::UserAction(UserAction::Match)) => break,
            Some(Message::UserAction(UserAction::NotMatch)) => return true,
            Some(Message::UserAction(UserAction::Cancel)) => return true,
            None => return true,
            _ => {}
        }
    }

    false
}
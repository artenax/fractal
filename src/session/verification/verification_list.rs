use std::sync::Arc;

use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use log::{debug, warn};
use matrix_sdk::ruma::{
    api::client::sync::sync_events::v3::ToDevice, events::AnyToDeviceEvent, UserId,
};

use crate::session::{
    user::UserExt,
    verification::{IdentityVerification, VERIFICATION_CREATION_TIMEOUT},
    Session,
};

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct FlowId {
    user_id: Arc<UserId>,
    flow_id: String,
}

impl FlowId {
    pub fn new(user_id: Arc<UserId>, flow_id: String) -> Self {
        Self { user_id, flow_id }
    }
}

#[derive(Hash, PartialEq, Eq, Debug)]
struct FlowIdUnowned<'a> {
    user_id: &'a UserId,
    flow_id: &'a str,
}

impl<'a> FlowIdUnowned<'a> {
    pub fn new(user_id: &'a UserId, flow_id: &'a str) -> Self {
        Self { user_id, flow_id }
    }
}

impl indexmap::Equivalent<FlowId> for FlowIdUnowned<'_> {
    fn equivalent(&self, key: &FlowId) -> bool {
        self.user_id == &*key.user_id && self.flow_id == &*key.flow_id
    }
}

mod imp {
    use std::cell::RefCell;

    use glib::object::WeakRef;
    use indexmap::IndexMap;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct VerificationList {
        pub list: RefCell<IndexMap<FlowId, IdentityVerification>>,
        pub session: OnceCell<WeakRef<Session>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VerificationList {
        const NAME: &'static str = "VerificationList";
        type Type = super::VerificationList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for VerificationList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "session",
                    "Session",
                    "The session",
                    Session::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                )]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "session" => self
                    .session
                    .set(value.get::<Session>().unwrap().downgrade())
                    .unwrap(),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session" => obj.session().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for VerificationList {
        fn item_type(&self, _list_model: &Self::Type) -> glib::Type {
            IdentityVerification::static_type()
        }
        fn n_items(&self, _list_model: &Self::Type) -> u32 {
            self.list.borrow().len() as u32
        }
        fn item(&self, _list_model: &Self::Type, position: u32) -> Option<glib::Object> {
            self.list
                .borrow()
                .get_index(position as usize)
                .map(|(_, item)| item.clone().upcast::<glib::Object>())
        }
    }
}

glib::wrapper! {
    pub struct VerificationList(ObjectSubclass<imp::VerificationList>)
        @implements gio::ListModel;
}

impl VerificationList {
    pub fn new(session: &Session) -> Self {
        glib::Object::new(&[("session", session)]).expect("Failed to create VerificationList")
    }

    pub fn session(&self) -> Session {
        self.imp().session.get().unwrap().upgrade().unwrap()
    }

    pub fn handle_response_to_device(&self, to_device: ToDevice) {
        for event in to_device.events.iter().filter_map(|e| e.deserialize().ok()) {
            debug!("Received verification event: {:?}", event);
            let request = match event {
                AnyToDeviceEvent::KeyVerificationRequest(e) => {
                    if let Some(request) = self.get_by_id(&e.sender, &e.content.transaction_id) {
                        Some(request)
                    } else {
                        let session = self.session();
                        let user = session.user().unwrap();
                        // ToDevice verifications can only be send by us
                        if *e.sender != *user.user_id() {
                            warn!("Received a device verification event from a different user, which isn't allowed");
                            continue;
                        }

                        // Ignore request that are too old
                        let start_time = if let Some(time) = e.content.timestamp.to_system_time() {
                            if let Ok(duration) = time.elapsed() {
                                if duration > VERIFICATION_CREATION_TIMEOUT {
                                    debug!("Received verification event that already timedout");
                                    continue;
                                }

                                if let Ok(time) = glib::DateTime::from_unix_utc(
                                    e.content.timestamp.as_secs().into(),
                                )
                                .and_then(|t| t.to_local())
                                {
                                    time
                                } else {
                                    warn!("Ignore verification request because getting a correct timestamp failed");
                                    continue;
                                }
                            } else {
                                warn!("Ignore verification request because it was sent in the future. The system time of the server or the local machine is probably wrong.");
                                continue;
                            }
                        } else {
                            warn!("Ignore verification request because getting a correct timestamp failed");
                            continue;
                        };

                        let request = IdentityVerification::for_flow_id(
                            e.content.transaction_id.as_str(),
                            &session,
                            user,
                            &start_time,
                        );
                        self.add(request.clone());
                        Some(request)
                    }
                }
                AnyToDeviceEvent::KeyVerificationReady(e) => {
                    self.get_by_id(&e.sender, &e.content.transaction_id)
                }
                AnyToDeviceEvent::KeyVerificationStart(e) => {
                    self.get_by_id(&e.sender, &e.content.transaction_id)
                }
                AnyToDeviceEvent::KeyVerificationCancel(e) => {
                    self.get_by_id(&e.sender, &e.content.transaction_id)
                }
                AnyToDeviceEvent::KeyVerificationAccept(e) => {
                    self.get_by_id(&e.sender, &e.content.transaction_id)
                }
                AnyToDeviceEvent::KeyVerificationMac(e) => {
                    self.get_by_id(&e.sender, &e.content.transaction_id)
                }
                AnyToDeviceEvent::KeyVerificationKey(e) => {
                    self.get_by_id(&e.sender, &e.content.transaction_id)
                }
                AnyToDeviceEvent::KeyVerificationDone(e) => {
                    self.get_by_id(&e.sender, &e.content.transaction_id)
                }
                _ => continue,
            };
            if let Some(request) = request {
                request.notify_state();
            } else {
                warn!("Received verification event, but we don't have the initial event.");
            }
        }
    }

    /// Add a new `IdentityVerification` request
    pub fn add(&self, request: IdentityVerification) {
        // Don't add requests that are already finished
        if request.is_finished() {
            return;
        }

        let length = {
            let mut list = self.imp().list.borrow_mut();
            let length = list.len();
            request.connect_notify_local(
                Some("state"),
                clone!(@weak self as obj => move |request, _| {
                    if request.is_finished() {
                        obj.remove(request);
                    }
                }),
            );

            list.insert(
                FlowId::new(request.user().user_id(), request.flow_id().to_owned()),
                request,
            );
            length as u32
        };

        self.items_changed(length, 0, 1)
    }

    pub fn remove(&self, request: &IdentityVerification) {
        let position = if let Some((position, ..)) =
            self.imp()
                .list
                .borrow_mut()
                .shift_remove_full(&FlowIdUnowned::new(
                    request.user().user_id().as_ref(),
                    request.flow_id(),
                )) {
            position
        } else {
            return;
        };

        self.items_changed(position as u32, 1, 0);
    }

    pub fn get_by_id(
        &self,
        user_id: &UserId,
        flow_id: &impl AsRef<str>,
    ) -> Option<IdentityVerification> {
        let flow_id = FlowIdUnowned::new(user_id, flow_id.as_ref());
        self.imp().list.borrow().get(&flow_id).cloned()
    }

    // Returns the first valid session verification if any
    pub fn get_session(&self) -> Option<IdentityVerification> {
        let list = self.imp().list.borrow();
        let session = self.session();
        let user_id = session.user().unwrap().user_id();

        for (_, item) in list.iter() {
            if !item.is_finished() && item.user().user_id() == user_id {
                return Some(item.to_owned());
            }
        }

        None
    }
}

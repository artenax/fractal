use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{glib, glib::clone, CompositeTemplate};
use matrix_sdk::event_handler::EventHandlerDropGuard;
use ruma::{
    api::client::push::{set_pushrule_enabled, RuleKind, RuleScope},
    events::push_rules::{PushRulesEvent, PushRulesEventContent},
    push::{PredefinedOverrideRuleId, Ruleset},
};
use tracing::{error, warn};

use crate::{prelude::*, session::model::Session, spawn, spawn_tokio, toast};

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{subclass::InitializingObject, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/account_settings/notifications_page.ui"
    )]
    pub struct NotificationsPage {
        /// The current session.
        pub session: WeakRef<Session>,
        /// Binding to the session settings `notifications-enabled` property.
        pub settings_binding: RefCell<Option<glib::Binding>>,
        /// The guard of the event handler for push rules changes.
        pub event_handler_guard: RefCell<Option<EventHandlerDropGuard>>,
        /// Whether notifications are enabled for this account.
        pub account_enabled: Cell<bool>,
        /// Whether an account notifications change is being processed.
        pub account_loading: Cell<bool>,
        /// Whether notifications are enabled for this session.
        pub session_enabled: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NotificationsPage {
        const NAME: &'static str = "NotificationsPage";
        type Type = super::NotificationsPage;
        type ParentType = adw::PreferencesPage;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for NotificationsPage {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Session>("session")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("account-enabled")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("account-loading")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("session-enabled")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.set_session(value.get().unwrap()),
                "account-enabled" => obj.sync_account_enabled(value.get().unwrap()),
                "session-enabled" => obj.set_session_enabled(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.session().to_value(),
                "account-enabled" => obj.account_enabled().to_value(),
                "account-loading" => obj.account_loading().to_value(),
                "session-enabled" => obj.session_enabled().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for NotificationsPage {}
    impl PreferencesPageImpl for NotificationsPage {}
}

glib::wrapper! {
    /// Preferences page to edit notification settings.
    pub struct NotificationsPage(ObjectSubclass<imp::NotificationsPage>)
        @extends gtk::Widget, adw::PreferencesPage, @implements gtk::Accessible;
}

impl NotificationsPage {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Set the current session.
    pub fn set_session(&self, session: Option<Session>) {
        let prev_session = self.session();
        if prev_session == session {
            return;
        }

        let imp = self.imp();
        if let Some(binding) = imp.settings_binding.take() {
            binding.unbind();
        }
        imp.event_handler_guard.take();

        if let Some(session) = &session {
            let binding = session
                .settings()
                .bind_property("notifications-enabled", self, "session-enabled")
                .sync_create()
                .bidirectional()
                .build();
            imp.settings_binding.replace(Some(binding));
        }

        imp.session.set(session.as_ref());
        self.notify("session");

        spawn!(
            glib::PRIORITY_DEFAULT_IDLE,
            clone!(@weak self as obj => async move {
                obj.init_page().await;
            })
        );
    }

    /// Initialize the page.
    async fn init_page(&self) {
        let session = match self.session() {
            Some(session) => session,
            None => return,
        };

        let client = session.client();
        let account = client.account();
        let handle =
            spawn_tokio!(async move { account.account_data::<PushRulesEventContent>().await });

        match handle.await.unwrap() {
            Ok(Some(pushrules)) => match pushrules.deserialize() {
                Ok(pushrules) => {
                    self.update_page(pushrules.global);
                }
                Err(error) => {
                    error!("Could not deserialize push rules: {error}");
                    toast!(
                        self,
                        gettext("Could not load notifications settings. Try again later")
                    );
                }
            },
            Ok(None) => {
                warn!("Could not find push rules, using the default ruleset instead.");
                let user_id = session.user().unwrap().user_id();
                self.update_page(Ruleset::server_default(&user_id));
            }
            Err(error) => {
                error!("Could not get push rules: {error}");
                toast!(
                    self,
                    gettext("Could not load notifications settings. Try again later")
                );
            }
        }

        let obj_weak = glib::SendWeakRef::from(self.downgrade());
        let handler = client.add_event_handler(move |event: PushRulesEvent| {
            let obj_weak = obj_weak.clone();
            async move {
                let ctx = glib::MainContext::default();
                ctx.spawn(async move {
                    if let Some(obj) = obj_weak.upgrade() {
                        obj.update_page(event.content.global)
                    }
                });
            }
        });
        self.imp()
            .event_handler_guard
            .replace(Some(client.event_handler_drop_guard(handler)));
    }

    /// Update the page for the given ruleset.
    fn update_page(&self, rules: Ruleset) {
        let account_enabled = if let Some(rule) = rules
            .override_
            .iter()
            .find(|r| r.rule_id == PredefinedOverrideRuleId::Master.as_str())
        {
            !rule.enabled
        } else {
            warn!("Could not find `.m.rule.master` push rule, using the default rule instead.");
            true
        };
        self.set_account_enabled(account_enabled);
    }

    /// Whether notifications are enabled for this account.
    pub fn account_enabled(&self) -> bool {
        self.imp().account_enabled.get()
    }

    /// Set whether notifications are enabled for this account.
    ///
    /// This only sets the property locally.
    fn set_account_enabled(&self, enabled: bool) {
        if self.account_enabled() == enabled {
            return;
        }

        if !enabled {
            if let Some(session) = self.session() {
                session.notifications().clear();
            }
        }

        self.imp().account_enabled.set(enabled);
        self.notify("account-enabled");
    }

    /// Sync whether notifications are enabled for this account.
    ///
    /// This sets the property locally and synchronizes the change with the
    /// homeserver.
    pub fn sync_account_enabled(&self, enabled: bool) {
        self.set_account_enabled(enabled);

        self.set_account_loading(true);

        spawn!(clone!(@weak self as obj => async move {
            obj.send_account_enabled(enabled).await;
        }));
    }

    /// Send whether notifications are enabled for this account.
    ///
    /// This only changes the setting on the homeserver.
    async fn send_account_enabled(&self, enabled: bool) {
        let client = match self.session() {
            Some(session) => session.client(),
            None => return,
        };

        let request = set_pushrule_enabled::v3::Request::new(
            RuleScope::Global,
            RuleKind::Override,
            PredefinedOverrideRuleId::Master.to_string(),
            !enabled,
        );

        let handle = spawn_tokio!(async move { client.send(request, None).await });

        match handle.await.unwrap() {
            Ok(_) => {}
            Err(error) => {
                error!(
                    "Could not update `{}` push rule: {error}",
                    PredefinedOverrideRuleId::Master
                );

                let msg = if enabled {
                    gettext("Could not enable account notifications")
                } else {
                    gettext("Could not disable account notifications")
                };
                toast!(self, msg);

                // Revert the local change.
                self.set_account_enabled(!enabled);
            }
        }

        self.set_account_loading(false);
    }

    /// Whether an account notifications change is being processed.
    pub fn account_loading(&self) -> bool {
        self.imp().account_loading.get()
    }

    /// Set whether an account notifications change is being processed.
    fn set_account_loading(&self, loading: bool) {
        if self.account_loading() == loading {
            return;
        }

        self.imp().account_loading.set(loading);
        self.notify("account-loading");
    }

    /// Whether notifications are enabled for this session.
    pub fn session_enabled(&self) -> bool {
        self.imp().session_enabled.get()
    }

    /// Set whether notifications are enabled for this session.
    pub fn set_session_enabled(&self, enabled: bool) {
        if self.session_enabled() == enabled {
            return;
        }

        if !enabled {
            if let Some(session) = self.session() {
                session.notifications().clear();
            }
        }

        self.imp().session_enabled.set(enabled);
        self.notify("session-enabled");
    }
}

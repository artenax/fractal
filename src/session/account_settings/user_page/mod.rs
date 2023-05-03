use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    CompositeTemplate,
};
use log::error;
use matrix_sdk::ruma::{api::client::discovery::get_capabilities, OwnedMxcUri};

mod change_password_subpage;
mod deactivate_account_subpage;
mod log_out_subpage;

use change_password_subpage::ChangePasswordSubpage;
use deactivate_account_subpage::DeactivateAccountSubpage;
use log_out_subpage::LogOutSubpage;

use crate::{
    components::{ActionButton, ActionState, ButtonRow, EditableAvatar},
    session::{Session, User, UserExt},
    spawn, spawn_tokio, toast,
    utils::{media::load_file, template_callbacks::TemplateCallbacks, OngoingAsyncAction},
};

mod imp {
    use std::cell::RefCell;

    use glib::{subclass::InitializingObject, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/account-settings-user-page.ui")]
    pub struct UserPage {
        pub session: WeakRef<Session>,
        #[template_child]
        pub avatar: TemplateChild<EditableAvatar>,
        #[template_child]
        pub display_name: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub display_name_button: TemplateChild<ActionButton>,
        #[template_child]
        pub change_password_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub change_password_subpage: TemplateChild<ChangePasswordSubpage>,
        #[template_child]
        pub homeserver: TemplateChild<gtk::Label>,
        #[template_child]
        pub user_id: TemplateChild<gtk::Label>,
        #[template_child]
        pub session_id: TemplateChild<gtk::Label>,
        #[template_child]
        pub deactivate_account_subpage: TemplateChild<DeactivateAccountSubpage>,
        #[template_child]
        pub log_out_subpage: TemplateChild<LogOutSubpage>,
        pub changing_avatar: RefCell<Option<OngoingAsyncAction<OwnedMxcUri>>>,
        pub changing_display_name: RefCell<Option<OngoingAsyncAction<String>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for UserPage {
        const NAME: &'static str = "UserPage";
        type Type = super::UserPage;
        type ParentType = adw::PreferencesPage;

        fn class_init(klass: &mut Self::Class) {
            ButtonRow::static_type();
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);
            TemplateCallbacks::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for UserPage {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Session>("session")
                    .explicit_notify()
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

            obj.init_avatar();
            obj.init_display_name();
            obj.init_change_password();
        }
    }

    impl WidgetImpl for UserPage {}
    impl PreferencesPageImpl for UserPage {}
}

glib::wrapper! {
    /// Account settings page about the user and the session.
    pub struct UserPage(ObjectSubclass<imp::UserPage>)
        @extends gtk::Widget, adw::PreferencesPage, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl UserPage {
    pub fn new(parent_window: &Option<gtk::Window>, session: &Session) -> Self {
        glib::Object::builder()
            .property("transient-for", parent_window)
            .property("session", session)
            .build()
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Set the current session.
    pub fn set_session(&self, session: Option<Session>) {
        if self.session() == session {
            return;
        }
        self.imp().session.set(session.as_ref());
        self.notify("session");

        self.user().avatar_data().image().connect_notify_local(
            Some("uri"),
            clone!(@weak self as obj => move |avatar_image, _| {
                obj.avatar_changed(avatar_image.uri());
            }),
        );
        self.user().connect_notify_local(
            Some("display-name"),
            clone!(@weak self as obj => move |user, _| {
                obj.display_name_changed(user.display_name());
            }),
        );

        spawn!(
            glib::PRIORITY_LOW,
            clone!(@weak self as obj => async move {
                let imp = obj.imp();
                let client = obj.session().unwrap().client();

                let homeserver = client.homeserver().await;
                imp.homeserver.set_label(homeserver.as_ref());

                let user_id = client.user_id().unwrap();
                imp.user_id.set_label(user_id.as_ref());

                let session_id = client.device_id().unwrap();
                imp.session_id.set_label(session_id.as_ref());
            })
        );
    }

    fn user(&self) -> User {
        self.session()
            .as_ref()
            .and_then(|session| session.user())
            .unwrap()
            .to_owned()
    }

    fn init_avatar(&self) {
        let avatar = &self.imp().avatar;
        avatar.connect_edit_avatar(clone!(@weak self as obj => move |_, file| {
            spawn!(
                clone!(@weak obj => async move {
                    obj.change_avatar(file).await;
                })
            );
        }));
        avatar.connect_remove_avatar(clone!(@weak self as obj => move |_| {
            spawn!(
                clone!(@weak obj => async move {
                    obj.remove_avatar().await;
                })
            );
        }));
    }

    fn avatar_changed(&self, uri: Option<OwnedMxcUri>) {
        let imp = self.imp();

        if let Some(action) = imp.changing_avatar.borrow().as_ref() {
            if uri.as_ref() != action.as_value() {
                // This is not the change we expected, maybe another device did a change too.
                // Let's wait for another change.
                return;
            }
        } else {
            // No action is ongoing, we don't need to do anything.
            return;
        };

        // Reset the state.
        imp.changing_avatar.take();
        imp.avatar.success();
        if uri.is_none() {
            toast!(self, gettext("Avatar removed successfully"));
        } else {
            toast!(self, gettext("Avatar changed successfully"));
        }
    }

    async fn change_avatar(&self, file: gio::File) {
        let imp = self.imp();
        let avatar = &imp.avatar;
        avatar.edit_in_progress();

        let (data, info) = match load_file(&file).await {
            Ok(res) => res,
            Err(error) => {
                error!("Could not load user avatar file: {error}");
                toast!(self, gettext("Could not load file"));
                avatar.reset();
                return;
            }
        };

        let client = self.session().unwrap().client();
        let client_clone = client.clone();
        let handle =
            spawn_tokio!(async move { client_clone.media().upload(&info.mime, data).await });

        let uri = match handle.await.unwrap() {
            Ok(res) => res.content_uri,
            Err(error) => {
                error!("Could not upload user avatar: {error}");
                toast!(self, gettext("Could not upload avatar"));
                avatar.reset();
                return;
            }
        };

        let (action, weak_action) = OngoingAsyncAction::set(uri.clone());
        imp.changing_avatar.replace(Some(action));

        let uri_clone = uri.clone();
        let handle =
            spawn_tokio!(async move { client.account().set_avatar_url(Some(&uri_clone)).await });

        match handle.await.unwrap() {
            Ok(_) => {
                // If the user is in no rooms, we won't receive the update via sync, so change
                // the avatar manually if this request succeeds before the avatar is updated.
                // Because this action can finish in avatar_changed, we must only act if this is
                // still the current action.
                if weak_action.is_ongoing() {
                    self.user().set_avatar_url(Some(uri))
                }
            }
            Err(error) => {
                // Because this action can finish in avatar_changed, we must only act if this is
                // still the current action.
                if weak_action.is_ongoing() {
                    imp.changing_avatar.take();
                    error!("Could not change user avatar: {error}");
                    toast!(self, gettext("Could not change avatar"));
                    avatar.reset();
                }
            }
        }
    }

    async fn remove_avatar(&self) {
        let imp = self.imp();
        let avatar = &*imp.avatar;
        avatar.removal_in_progress();

        let (action, weak_action) = OngoingAsyncAction::remove();
        imp.changing_avatar.replace(Some(action));

        let client = self.session().unwrap().client();
        let handle = spawn_tokio!(async move { client.account().set_avatar_url(None).await });

        match handle.await.unwrap() {
            Ok(_) => {
                // If the user is in no rooms, we won't receive the update via sync, so change
                // the avatar manually if this request succeeds before the avatar is updated.
                // Because this action can finish in avatar_changed, we must only act if this is
                // still the current action.
                if weak_action.is_ongoing() {
                    self.user().set_avatar_url(None)
                }
            }
            Err(error) => {
                // Because this action can finish in avatar_changed, we must only act if this is
                // still the current action.
                if weak_action.is_ongoing() {
                    imp.changing_avatar.take();
                    error!("Couldn’t remove user avatar: {error}");
                    toast!(self, gettext("Could not remove avatar"));
                    avatar.reset();
                }
            }
        }
    }

    fn init_display_name(&self) {
        let imp = self.imp();
        let entry = &imp.display_name;
        entry.connect_changed(clone!(@weak self as obj => move|entry| {
            obj.imp().display_name_button.set_visible(entry.text() != obj.user().display_name());
        }));
    }

    fn display_name_changed(&self, name: String) {
        let imp = self.imp();

        if let Some(action) = imp.changing_display_name.borrow().as_ref() {
            if action.as_value() == Some(&name) {
                // This is not the change we expected, maybe another device did a change too.
                // Let's wait for another change.
                return;
            }
        } else {
            // No action is ongoing, we don't need to do anything.
            return;
        }

        // Reset state.
        imp.changing_display_name.take();

        let entry = &imp.display_name;
        let button = &imp.display_name_button;

        entry.remove_css_class("error");
        entry.set_sensitive(true);
        button.set_visible(false);
        button.set_state(ActionState::Confirm);
        toast!(self, gettext("Name changed successfully"));
    }

    async fn change_display_name(&self) {
        let imp = self.imp();
        let entry = &imp.display_name;
        let button = &imp.display_name_button;

        entry.set_sensitive(false);
        button.set_state(ActionState::Loading);

        let display_name = entry.text().trim().to_string();

        let (action, weak_action) = OngoingAsyncAction::set(display_name.clone());
        imp.changing_display_name.replace(Some(action));

        let client = self.session().unwrap().client();
        let display_name_clone = display_name.clone();
        let handle = spawn_tokio!(async move {
            client
                .account()
                .set_display_name(Some(&display_name_clone))
                .await
        });

        match handle.await.unwrap() {
            Ok(_) => {
                // If the user is in no rooms, we won't receive the update via sync, so change
                // the avatar manually if this request succeeds before the avatar is updated.
                // Because this action can finish in display_name_changed, we must only act if
                // this is still the current action.
                if weak_action.is_ongoing() {
                    self.user().set_display_name(Some(display_name));
                }
            }
            Err(error) => {
                // Because this action can finish in display_name_changed, we must only act if
                // this is still the current action.
                if weak_action.is_ongoing() {
                    imp.changing_display_name.take();
                    error!("Could not change user display name: {error}");
                    toast!(self, gettext("Could not change display name"));
                    button.set_state(ActionState::Retry);
                    entry.add_css_class("error");
                    entry.set_sensitive(true);
                }
            }
        }
    }

    fn init_change_password(&self) {
        spawn!(
            glib::PRIORITY_LOW,
            clone!(@weak self as obj => async move {
                let client = obj.session().unwrap().client();

                // Check whether the user can change their password.
                let handle = spawn_tokio!(async move {
                    client.send(get_capabilities::v3::Request::new(), None).await
                });
                match handle.await.unwrap() {
                    Ok(res) => {
                        obj.imp().change_password_group.set_visible(res.capabilities.change_password.enabled);
                    }
                    Err(error) => error!("Could not get server capabilities: {error}"),
                }
            })
        );
    }

    #[template_callback]
    fn handle_change_display_name(&self) {
        spawn!(clone!(@weak self as obj => async move {
            obj.change_display_name().await;
        }));
    }

    #[template_callback]
    fn show_change_password(&self) {
        self.root()
            .as_ref()
            .and_then(|root| root.downcast_ref::<adw::PreferencesWindow>())
            .unwrap()
            .present_subpage(&*self.imp().change_password_subpage);
    }

    #[template_callback]
    fn show_deactivate_account(&self) {
        self.root()
            .as_ref()
            .and_then(|root| root.downcast_ref::<adw::PreferencesWindow>())
            .unwrap()
            .present_subpage(&*self.imp().deactivate_account_subpage);
    }

    #[template_callback]
    pub fn show_log_out_page(&self) {
        self.root()
            .as_ref()
            .and_then(|root| root.downcast_ref::<adw::PreferencesWindow>())
            .unwrap()
            .present_subpage(&*self.imp().log_out_subpage);
    }
}

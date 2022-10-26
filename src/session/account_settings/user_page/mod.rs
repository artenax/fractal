use std::time::Duration;

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    CompositeTemplate,
};
use log::error;
use matrix_sdk::ruma::{api::client::discovery::get_capabilities, MxcUri, OwnedMxcUri};

mod change_password_subpage;
mod deactivate_account_subpage;

use change_password_subpage::ChangePasswordSubpage;
use deactivate_account_subpage::DeactivateAccountSubpage;

use crate::{
    components::{ActionButton, ActionState, ButtonRow, EditableAvatar},
    session::{Session, User, UserExt},
    spawn, spawn_tokio, toast,
    utils::template_callbacks::TemplateCallbacks,
};

mod imp {
    use std::cell::{Cell, RefCell};

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
        pub changing_avatar_to: RefCell<Option<OwnedMxcUri>>,
        pub removing_avatar: Cell<bool>,
        pub changing_display_name_to: RefCell<Option<String>>,
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

        self.user().avatar().connect_notify_local(
            Some("url"),
            clone!(@weak self as obj => move |avatar, _| {
                obj.avatar_changed(avatar.url().as_deref());
            }),
        );
        self.user().connect_notify_local(
            Some("display-name"),
            clone!(@weak self as obj => move |user, _| {
                obj.display_name_changed(&user.display_name());
            }),
        );

        spawn!(
            glib::PRIORITY_LOW,
            clone!(@weak self as obj => async move {
                let priv_ = obj.imp();
                let client = obj.session().unwrap().client();

                let homeserver = client.homeserver().await;
                priv_.homeserver.set_label(homeserver.as_ref());

                let user_id = client.user_id().unwrap();
                priv_.user_id.set_label(user_id.as_ref());

                let session_id = client.device_id().unwrap();
                priv_.session_id.set_label(session_id.as_ref());
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

    fn avatar_changed(&self, uri: Option<&MxcUri>) {
        let priv_ = self.imp();
        let avatar = &*priv_.avatar;
        if uri.is_none() && priv_.removing_avatar.get() {
            priv_.removing_avatar.set(false);
            avatar.show_temp_image(false);
            avatar.set_remove_state(ActionState::Success);
            avatar.set_edit_sensitive(true);
            toast!(self, gettext("Avatar removed successfully"));
            glib::timeout_add_local_once(
                Duration::from_secs(2),
                clone!(@weak avatar => move || {
                    avatar.set_remove_state(ActionState::Default);
                }),
            );
        } else if uri.is_some() {
            let to_uri = priv_.changing_avatar_to.borrow().clone();
            if to_uri.as_deref() == uri {
                priv_.changing_avatar_to.take();
                avatar.set_edit_state(ActionState::Success);
                avatar.show_temp_image(false);
                avatar.set_temp_image_from_file(None);
                avatar.set_remove_sensitive(true);
                toast!(self, gettext("Avatar changed successfully"));
                glib::timeout_add_local_once(
                    Duration::from_secs(2),
                    clone!(@weak avatar => move || {
                        avatar.set_edit_state(ActionState::Default);
                    }),
                );
            }
        }
    }

    async fn change_avatar(&self, file: gio::File) {
        let priv_ = self.imp();
        let avatar = &priv_.avatar;
        avatar.set_temp_image_from_file(Some(&file));
        avatar.show_temp_image(true);
        avatar.set_edit_state(ActionState::Loading);
        avatar.set_remove_sensitive(false);

        let client = self.session().unwrap().client();
        let mime = file
            .query_info_future(
                &gio::FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE,
                gio::FileQueryInfoFlags::NONE,
                glib::PRIORITY_LOW,
            )
            .await
            .ok()
            .and_then(|info| info.content_type())
            .and_then(|content_type| gio::content_type_get_mime_type(&content_type))
            .unwrap();
        let (data, _) = file.load_contents_future().await.unwrap();

        let client_clone = client.clone();
        let handle = spawn_tokio!(async move {
            client_clone
                .media()
                .upload(&mime.parse().unwrap(), &data)
                .await
        });

        let uri = match handle.await.unwrap() {
            Ok(res) => res.content_uri,
            Err(error) => {
                error!("Could not upload user avatar: {}", error);
                toast!(self, gettext("Could not upload avatar"));
                avatar.show_temp_image(false);
                avatar.set_temp_image_from_file(None);
                avatar.set_edit_state(ActionState::Default);
                avatar.set_remove_sensitive(true);
                return;
            }
        };

        priv_.changing_avatar_to.replace(Some(uri.clone()));
        let handle = spawn_tokio!(async move { client.account().set_avatar_url(Some(&uri)).await });

        match handle.await.unwrap() {
            Ok(_) => {
                let to_uri = priv_.changing_avatar_to.borrow().clone();
                if let Some(avatar) = to_uri {
                    self.user().set_avatar_url(Some(avatar))
                }
            }
            Err(error) => {
                if priv_.changing_avatar_to.take().is_some() {
                    error!("Could not change user avatar: {}", error);
                    toast!(self, gettext("Could not change avatar"));
                    avatar.show_temp_image(false);
                    avatar.set_temp_image_from_file(None);
                    avatar.set_edit_state(ActionState::Default);
                    avatar.set_remove_sensitive(true);
                }
            }
        }
    }

    async fn remove_avatar(&self) {
        let priv_ = self.imp();
        let avatar = &*priv_.avatar;
        avatar.show_temp_image(true);
        avatar.set_remove_state(ActionState::Loading);
        avatar.set_edit_sensitive(false);

        let client = self.session().unwrap().client();
        let handle = spawn_tokio!(async move { client.account().set_avatar_url(None).await });
        priv_.removing_avatar.set(true);

        match handle.await.unwrap() {
            Ok(_) => {
                self.user().set_avatar_url(None);
            }
            Err(error) => {
                if priv_.removing_avatar.get() {
                    priv_.removing_avatar.set(false);
                    error!("Couldn’t remove user avatar: {}", error);
                    toast!(self, gettext("Could not remove avatar"));
                    avatar.show_temp_image(false);
                    avatar.set_remove_state(ActionState::Default);
                    avatar.set_edit_sensitive(true);
                }
            }
        }
    }

    fn init_display_name(&self) {
        let priv_ = self.imp();
        let entry = &priv_.display_name;
        entry.connect_changed(clone!(@weak self as obj => move|entry| {
            obj.imp().display_name_button.set_visible(entry.text() != obj.user().display_name());
        }));
    }

    fn display_name_changed(&self, name: &str) {
        let priv_ = self.imp();
        let entry = &priv_.display_name;
        let button = &priv_.display_name_button;

        let to_display_name = priv_
            .changing_display_name_to
            .borrow()
            .clone()
            .unwrap_or_default();
        if to_display_name == name {
            priv_.changing_display_name_to.take();
            entry.remove_css_class("error");
            entry.set_sensitive(true);
            button.hide();
            button.set_state(ActionState::Confirm);
            toast!(self, gettext("Name changed successfully"));
        }
    }

    async fn change_display_name(&self) {
        let priv_ = self.imp();
        let entry = &priv_.display_name;
        let button = &priv_.display_name_button;

        entry.set_sensitive(false);
        button.set_state(ActionState::Loading);

        let display_name = entry.text();
        priv_
            .changing_display_name_to
            .replace(Some(display_name.to_string()));

        let client = self.session().unwrap().client();
        let handle =
            spawn_tokio!(
                async move { client.account().set_display_name(Some(&display_name)).await }
            );

        match handle.await.unwrap() {
            Ok(_) => {
                let to_display_name = priv_.changing_display_name_to.borrow().clone();
                if let Some(display_name) = to_display_name {
                    self.user().set_display_name(Some(display_name));
                }
            }
            Err(err) => {
                error!("Couldn’t change user display name: {}", err);
                toast!(self, gettext("Could not change display name"));
                button.set_state(ActionState::Retry);
                entry.add_css_class("error");
                entry.set_sensitive(true);
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
                    Err(error) => error!("Could not get server capabilities: {}", error),
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
}

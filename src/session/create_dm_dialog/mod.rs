use adw::subclass::prelude::*;
use gtk::{gdk, glib, glib::clone, prelude::*, CompositeTemplate};

mod dm_user;
use self::dm_user::DmUser;
mod dm_user_list;
mod dm_user_row;
use self::{
    dm_user_list::{DmUserList, DmUserListState},
    dm_user_row::DmUserRow,
};
use crate::{
    gettext,
    session::{user::UserExt, Session},
    spawn,
};

mod imp {
    use glib::{object::WeakRef, subclass::InitializingObject};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/create-dm-dialog.ui")]
    pub struct CreateDmDialog {
        pub session: WeakRef<Session>,
        #[template_child]
        pub list_box: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub error_page: TemplateChild<adw::StatusPage>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CreateDmDialog {
        const NAME: &'static str = "CreateDmDialog";
        type Type = super::CreateDmDialog;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            DmUserRow::static_type();
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);

            klass.add_binding(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                |obj, _| {
                    obj.close();
                    true
                },
                None,
            );
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CreateDmDialog {
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
    }

    impl WidgetImpl for CreateDmDialog {}
    impl WindowImpl for CreateDmDialog {}
    impl AdwWindowImpl for CreateDmDialog {}
}

glib::wrapper! {
    /// Preference Window to display and update room details.
    pub struct CreateDmDialog(ObjectSubclass<imp::CreateDmDialog>)
        @extends gtk::Widget, gtk::Window, adw::Window, adw::Bin, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl CreateDmDialog {
    pub fn new(parent_window: Option<&impl IsA<gtk::Window>>, session: &Session) -> Self {
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
        let imp = self.imp();

        if self.session() == session {
            return;
        }

        if let Some(ref session) = session {
            let user_list = DmUserList::new(session);

            // We don't need to disconnect this signal since the `DmUserList` will be
            // disposed once unbound from the `gtk::ListBox`
            user_list.connect_notify_local(
                Some("state"),
                clone!(@weak self as obj => move |model, _| {
                    obj.update_view(model);
                }),
            );

            imp.search_entry
                .bind_property("text", &user_list, "search-term")
                .flags(glib::BindingFlags::SYNC_CREATE)
                .build();

            imp.list_box.bind_model(Some(&user_list), |user| {
                DmUserRow::new(
                    user.downcast_ref::<DmUser>()
                        .expect("DmUserList must contain only `DmUser`"),
                )
                .upcast()
            });

            self.update_view(&user_list);
        } else {
            imp.list_box.unbind_model();
        }

        imp.session.set(session.as_ref());
        self.notify("session");
    }

    fn update_view(&self, model: &DmUserList) {
        let visible_child_name = match model.state() {
            DmUserListState::Initial => "no-search-page",
            DmUserListState::Loading => "loading-page",
            DmUserListState::NoMatching => "no-matching-page",
            DmUserListState::Matching => "matching-page",
            DmUserListState::Error => {
                self.show_error(&gettext("An error occurred while searching for users"));
                return;
            }
        };

        self.imp().stack.set_visible_child_name(visible_child_name);
    }

    fn show_error(&self, message: &str) {
        self.imp().error_page.set_description(Some(message));
        self.imp().stack.set_visible_child_name("error-page");
    }

    #[template_callback]
    fn row_activated_cb(&self, row: gtk::ListBoxRow) {
        let Some(user): Option<DmUser> = row.downcast::<DmUserRow>().ok().and_then(|r| r.user()) else { return; };

        // TODO: For now we show the loading page while we create the room,
        // ideally we would like to have the same behavior as Element:
        // Create the room only once the user sends a message
        self.imp().stack.set_visible_child_name("loading-page");
        self.imp().search_entry.set_sensitive(false);
        spawn!(clone!(@weak self as obj, @weak user => async move {
            match user.start_chat().await {
                Ok(room) => {
                    user.session().select_room(Some(room));
                    obj.close();
                }
                Err(_) => {
                    obj.show_error(&gettext("Failed to create a new Direct Chat"));
                }
            }
        }));
    }
}

use std::cmp::Ordering;

use adw::subclass::prelude::*;
use gtk::{glib, glib::clone, prelude::*, CompositeTemplate};

use crate::{
    components::{Avatar, OverlappingBox},
    i18n::{gettext_f, ngettext_f},
    prelude::*,
    session::room::TypingList,
};

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-typing-row.ui")]
    pub struct TypingRow {
        #[template_child]
        pub avatar_box: TemplateChild<OverlappingBox>,
        #[template_child]
        pub label: TemplateChild<gtk::Label>,
        /// The list of members that are currently typing.
        pub list: RefCell<Option<(TypingList, glib::SignalHandlerId)>>,
        /// The current avatars that are displayed.
        pub avatars: RefCell<Vec<Avatar>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TypingRow {
        const NAME: &'static str = "ContentTypingRow";
        type Type = super::TypingRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_css_name("typing-bar");
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TypingRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<TypingList>("list")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoolean::builder("is-empty")
                        .default_value(true)
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "list" => self.obj().set_list(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "list" => obj.list().to_value(),
                "is-empty" => obj.is_empty().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            if let Some((list, handler_id)) = self.list.take() {
                list.disconnect(handler_id);
            }
        }
    }

    impl WidgetImpl for TypingRow {}
    impl BinImpl for TypingRow {}
}

glib::wrapper! {
    /// A widget row used to display typing notification.
    pub struct TypingRow(ObjectSubclass<imp::TypingRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl TypingRow {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// The list of members that are currently typing.
    pub fn list(&self) -> Option<TypingList> {
        self.imp()
            .list
            .borrow()
            .as_ref()
            .map(|(list, _)| list.clone())
    }

    /// Set the list of members that are currently typing.
    pub fn set_list(&self, list: Option<TypingList>) {
        if self.list() == list {
            return;
        }

        let priv_ = self.imp();
        let prev_is_empty = self.is_empty();

        if let Some((list, handler_id)) = priv_.list.take() {
            list.disconnect(handler_id);
        }

        if let Some(list) = list {
            let handler_id = list.connect_items_changed(
                clone!(@weak self as obj => move |list, _pos, removed, added| {
                    obj.update(list, removed, added);
                }),
            );

            priv_.list.replace(Some((list.clone(), handler_id)));
            self.update(&list, 1, 1);
        }

        if prev_is_empty != self.is_empty() {
            self.notify("is-empty");
        }

        self.notify("list");
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.imp()
            .list
            .borrow()
            .as_ref()
            .filter(|(list, _)| !list.is_empty())
            .is_none()
    }

    pub fn update(&self, list: &TypingList, removed: u32, added: u32) {
        if removed == 0 && added == 0 {
            // Nothing changed;
            return;
        }

        let len = list.n_items();

        if len == 0 {
            self.notify("is-empty");
            return;
        }

        // Update label and avatars
        let priv_ = self.imp();
        let members = list.members();

        {
            // Show 10 avatars max.
            let len = len.min(10) as usize;

            let mut avatars = priv_.avatars.borrow_mut();
            let avatars_len = avatars.len();

            match len.cmp(&avatars_len) {
                Ordering::Less => {
                    priv_.avatar_box.truncate_children(len);
                }
                Ordering::Equal => {}
                Ordering::Greater => {
                    avatars.reserve_exact(10 - avatars_len);
                }
            }

            for (i, member) in members.iter().enumerate().take(len) {
                let item = member.avatar().clone();

                if let Some(avatar) = avatars.get(i) {
                    avatar.set_item(Some(item));
                } else {
                    let avatar = Avatar::new();
                    avatar.set_item(Some(item));
                    avatar.set_size(30);
                    priv_.avatar_box.append(&avatar);
                    avatars.push(avatar);
                }
            }
        }

        let label = if len == 1 {
            let user = members[0].display_name();
            // Translators: Do NOT translate the content between '{' and '}', this is a
            // variable name.
            gettext_f("<b>{user}</b> is typing…", &[("user", &user)])
        } else {
            let user1 = members[0].display_name();
            let user2 = members[1].display_name();
            let n = len - 2;

            if n == 0 {
                gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', these are
                    // variable names.
                    "<b>{user1}</b> and <b>{user2}</b> are typing…",
                    &[("user1", &user1), ("user2", &user2)],
                )
            } else {
                ngettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', these are
                    // variable names.
                    "<b>{user1}</b>, <b>{user2}</b> and 1 other are typing…",
                    "<b>{user1}</b>, <b>{user2}</b> and {n} others are typing…",
                    n,
                    &[("user1", &user1), ("user2", &user2), ("n", &n.to_string())],
                )
            }
        };
        priv_.label.set_label(&label);

        if removed == 0 && added == len {
            self.notify("is-empty");
        }
    }
}

impl Default for TypingRow {
    fn default() -> Self {
        Self::new()
    }
}

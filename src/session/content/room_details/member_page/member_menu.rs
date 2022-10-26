use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};

use crate::session::{room::Member, UserActions, UserExt};

mod imp {
    use std::cell::RefCell;

    use once_cell::{sync::Lazy, unsync::OnceCell};

    use super::*;

    #[derive(Debug, Default)]
    pub struct MemberMenu {
        pub member: RefCell<Option<Member>>,
        pub popover: OnceCell<gtk::PopoverMenu>,
        pub destroy_handler: RefCell<Option<glib::signal::SignalHandlerId>>,
        pub actions_handler: RefCell<Option<glib::signal::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MemberMenu {
        const NAME: &'static str = "ContentMemberMenu";
        type Type = super::MemberMenu;
    }

    impl ObjectImpl for MemberMenu {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Member>("member")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecFlags::builder::<UserActions>("allowed-actions")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "member" => self.obj().set_member(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "member" => obj.member().to_value(),
                "allowed-actions" => obj.allowed_actions().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.popover_menu()
                .connect_closed(clone!(@weak obj => move |_| {
                    obj.close_popover();
                }));
        }
    }
}

glib::wrapper! {
    pub struct MemberMenu(ObjectSubclass<imp::MemberMenu>);
}

impl MemberMenu {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// The member to apply actions to.
    pub fn member(&self) -> Option<Member> {
        self.imp().member.borrow().clone()
    }

    /// Set the member to apply actions to.
    pub fn set_member(&self, member: Option<Member>) {
        let imp = self.imp();
        let prev_member = self.member();

        if prev_member == member {
            return;
        }

        if let Some(member) = prev_member {
            if let Some(handler) = imp.actions_handler.take() {
                member.disconnect(handler);
            }
        }

        if let Some(ref member) = member {
            let handler = member.connect_notify_local(
                Some("allowed-actions"),
                clone!(@weak self as obj => move |_, _| {
                    obj.notify("allowed-actions");
                }),
            );

            imp.actions_handler.replace(Some(handler));
        }

        imp.member.replace(member);
        self.notify("member");
        self.notify("allowed-actions");
    }

    /// The actions the logged-in user is allowed to perform on the member.
    pub fn allowed_actions(&self) -> UserActions {
        self.member()
            .map_or(UserActions::NONE, |member| member.allowed_actions())
    }

    fn popover_menu(&self) -> &gtk::PopoverMenu {
        self.imp().popover.get_or_init(|| {
            gtk::PopoverMenu::from_model(Some(
                &gtk::Builder::from_resource("/org/gnome/Fractal/member-menu.ui")
                    .object::<gio::MenuModel>("menu_model")
                    .unwrap(),
            ))
        })
    }

    /// Show the menu on the specific button
    ///
    /// For convenience it allows to set the member for which the popover is
    /// shown
    pub fn present_popover(&self, button: &gtk::ToggleButton, member: Option<Member>) {
        let popover = self.popover_menu();
        let _guard = popover.freeze_notify();

        self.close_popover();
        self.unparent_popover();

        self.set_member(member);

        let handler = button.connect_destroy(clone!(@weak self as obj => move |_| {
            obj.unparent_popover();
        }));

        self.imp().destroy_handler.replace(Some(handler));

        popover.set_parent(button);
        popover.show();
    }

    fn unparent_popover(&self) {
        let popover = self.popover_menu();

        if let Some(parent) = popover.parent() {
            if let Some(handler) = self.imp().destroy_handler.take() {
                parent.disconnect(handler);
            }

            popover.unparent();
        }
    }

    /// Closes the popover
    pub fn close_popover(&self) {
        let popover = self.popover_menu();
        let _guard = popover.freeze_notify();

        if let Some(button) = popover.parent() {
            if popover.is_visible() {
                popover.hide();
            }
            button
                .downcast::<gtk::ToggleButton>()
                .expect("The parent of a MemberMenu needs to be a gtk::ToggleButton")
                .set_active(false);
        }
    }
}

impl Default for MemberMenu {
    fn default() -> Self {
        Self::new()
    }
}

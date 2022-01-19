use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};

use crate::session::{room::Member, UserActions, UserExt};

mod imp {
    use super::*;
    use once_cell::sync::Lazy;
    use once_cell::unsync::OnceCell;
    use std::cell::RefCell;

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
        type ParentType = glib::Object;
    }

    impl ObjectImpl for MemberMenu {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "member",
                    "Member",
                    "The member this row is showing",
                    Member::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                ),
                glib::ParamSpecFlags::new(
                        "allowed-actions",
                        "Allowed Actions",
                        "The actions the currently logged-in user is allowed to perform on the selected member.",
                        UserActions::static_type(),
                        UserActions::default().bits(),
                        glib::ParamFlags::READABLE,
                    )
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
                "member" => obj.set_member(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "member" => obj.member().to_value(),
                "allowed-actions" => obj.allowed_actions().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

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
        glib::Object::new(&[]).expect("Failed to create MemberMenu")
    }

    pub fn member(&self) -> Option<Member> {
        let priv_ = imp::MemberMenu::from_instance(self);
        priv_.member.borrow().clone()
    }

    pub fn set_member(&self, member: Option<Member>) {
        let priv_ = imp::MemberMenu::from_instance(self);
        let prev_member = self.member();

        if prev_member == member {
            return;
        }

        if let Some(member) = prev_member {
            if let Some(handler) = priv_.actions_handler.take() {
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

            priv_.actions_handler.replace(Some(handler));
        }

        priv_.member.replace(member);
        self.notify("member");
        self.notify("allowed-actions");
    }

    pub fn allowed_actions(&self) -> UserActions {
        self.member()
            .map_or(UserActions::NONE, |member| member.allowed_actions())
    }

    fn popover_menu(&self) -> &gtk::PopoverMenu {
        let priv_ = imp::MemberMenu::from_instance(self);
        priv_.popover.get_or_init(|| {
            gtk::PopoverMenu::from_model(Some(
                &gtk::Builder::from_resource("/org/gnome/FractalNext/member-menu.ui")
                    .object::<gio::MenuModel>("menu_model")
                    .unwrap(),
            ))
        })
    }

    /// Show the menu on the specific button
    ///
    /// For convenience it allows to set the member for which the popover is shown
    pub fn present_popover(&self, button: &gtk::ToggleButton, member: Option<Member>) {
        let priv_ = imp::MemberMenu::from_instance(self);
        let popover = self.popover_menu();
        let _guard = popover.freeze_notify();

        self.close_popover();
        self.unparent_popover();

        self.set_member(member);

        let handler = button.connect_destroy(clone!(@weak self as obj => move |_| {
            obj.unparent_popover();
        }));

        priv_.destroy_handler.replace(Some(handler));

        popover.set_parent(button);
        popover.show();
    }

    fn unparent_popover(&self) {
        let priv_ = imp::MemberMenu::from_instance(self);
        let popover = self.popover_menu();

        if let Some(parent) = popover.parent() {
            if let Some(handler) = priv_.destroy_handler.take() {
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
                .expect("The parent of a MemberMenu needs to ba a gtk::ToggleButton")
                .set_active(false);
        }
    }
}

impl Default for MemberMenu {
    fn default() -> Self {
        Self::new()
    }
}
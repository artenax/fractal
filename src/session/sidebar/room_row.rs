use adw::subclass::prelude::BinImpl;
use gtk::{gdk, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};

use super::Row;
use crate::{
    components::{ContextMenuBin, ContextMenuBinExt, ContextMenuBinImpl},
    session::room::{HighlightFlags, Room, RoomType},
};

mod imp {
    use std::cell::RefCell;

    use glib::{subclass::InitializingObject, SignalHandlerId};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/sidebar-room-row.ui")]
    pub struct RoomRow {
        pub room: RefCell<Option<Room>>,
        pub binding: RefCell<Option<glib::Binding>>,
        pub signal_handler: RefCell<Option<SignalHandlerId>>,
        #[template_child]
        pub display_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub notification_count: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RoomRow {
        const NAME: &'static str = "SidebarRoomRow";
        type Type = super::RoomRow;
        type ParentType = ContextMenuBin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.set_accessible_role(gtk::AccessibleRole::Group);

            klass.install_action("room-row.accept-invite", None, move |widget, _, _| {
                widget.set_room_as_normal_or_direct();
            });
            klass.install_action("room-row.reject-invite", None, move |widget, _, _| {
                widget.room().unwrap().set_category(RoomType::Left)
            });

            klass.install_action("room-row.set-favorite", None, move |widget, _, _| {
                widget.room().unwrap().set_category(RoomType::Favorite);
            });
            klass.install_action("room-row.set-normal", None, move |widget, _, _| {
                widget.room().unwrap().set_category(RoomType::Normal);
            });
            klass.install_action("room-row.set-lowpriority", None, move |widget, _, _| {
                widget.room().unwrap().set_category(RoomType::LowPriority);
            });
            klass.install_action("room-row.set-direct", None, move |widget, _, _| {
                widget.room().unwrap().set_category(RoomType::Direct);
            });

            klass.install_action("room-row.leave", None, move |widget, _, _| {
                widget.room().unwrap().set_category(RoomType::Left);
            });
            klass.install_action("room-row.join", None, move |widget, _, _| {
                widget.set_room_as_normal_or_direct();
            });
            klass.install_action("room-row.forget", None, move |widget, _, _| {
                widget.room().unwrap().forget();
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for RoomRow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Room>("room")
                    .explicit_notify()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "room" => self.obj().set_room(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "room" => self.obj().room().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Allow to drag rooms
            let drag = gtk::DragSource::builder()
                .actions(gdk::DragAction::MOVE)
                .build();
            drag.connect_prepare(
                clone!(@weak obj => @default-return None, move |drag, x, y| {
                    obj.drag_prepare(drag, x, y)
                }),
            );
            drag.connect_drag_begin(clone!(@weak obj => move |_, _| {
                obj.drag_begin();
            }));
            drag.connect_drag_end(clone!(@weak obj => move |_, _, _| {
                obj.drag_end();
            }));
            obj.add_controller(drag);
        }

        fn dispose(&self) {
            if let Some(room) = self.room.take() {
                if let Some(id) = self.signal_handler.take() {
                    room.disconnect(id);
                }
            }
        }
    }

    impl WidgetImpl for RoomRow {}
    impl BinImpl for RoomRow {}

    impl ContextMenuBinImpl for RoomRow {
        fn menu_opened(&self) {
            let obj = self.obj();

            if let Some(sidebar) = obj
                .parent()
                .as_ref()
                .and_then(|obj| obj.downcast_ref::<Row>())
                .map(|row| row.sidebar())
            {
                let popover = sidebar.room_row_popover();
                obj.set_popover(Some(popover.to_owned()));
            }
        }
    }
}

glib::wrapper! {
    pub struct RoomRow(ObjectSubclass<imp::RoomRow>)
        @extends gtk::Widget, adw::Bin, ContextMenuBin, @implements gtk::Accessible;
}

impl RoomRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The room represented by this row.
    pub fn room(&self) -> Option<Room> {
        self.imp().room.borrow().clone()
    }

    /// Set the room represented by this row.
    pub fn set_room(&self, room: Option<Room>) {
        let imp = self.imp();

        if self.room() == room {
            return;
        }

        if let Some(room) = imp.room.take() {
            if let Some(id) = imp.signal_handler.take() {
                room.disconnect(id);
            }
            if let Some(binding) = imp.binding.take() {
                binding.unbind();
            }
            imp.display_name.remove_css_class("dim-label");
        }

        if let Some(ref room) = room {
            imp.binding.replace(Some(
                room.bind_property(
                    "notification-count",
                    &imp.notification_count.get(),
                    "visible",
                )
                .flags(glib::BindingFlags::SYNC_CREATE)
                .transform_from(|_, count: u64| Some(count > 0))
                .build(),
            ));

            imp.signal_handler.replace(Some(room.connect_notify_local(
                Some("highlight"),
                clone!(@weak self as obj => move |_, _| {
                        obj.update_highlight();
                }),
            )));

            if room.category() == RoomType::Left {
                imp.display_name.add_css_class("dim-label");
            }
        }
        imp.room.replace(room);

        self.update_highlight();
        self.update_actions();
        self.notify("room");
    }

    fn update_highlight(&self) {
        let imp = self.imp();
        if let Some(room) = &*imp.room.borrow() {
            let flags = room.highlight();

            if flags.contains(HighlightFlags::HIGHLIGHT) {
                imp.notification_count.add_css_class("highlight");
            } else {
                imp.notification_count.remove_css_class("highlight");
            }

            if flags.contains(HighlightFlags::BOLD) {
                imp.display_name.add_css_class("bold");
            } else {
                imp.display_name.remove_css_class("bold");
            }
        } else {
            imp.notification_count.remove_css_class("highlight");
            imp.display_name.remove_css_class("bold");
        }
    }

    /// Enable or disable actions according to the category of the room.
    fn update_actions(&self) {
        if let Some(room) = self.room() {
            match room.category() {
                RoomType::Invited => {
                    self.action_set_enabled("room-row.accept-invite", true);
                    self.action_set_enabled("room-row.reject-invite", true);
                    self.action_set_enabled("room-row.set-favorite", false);
                    self.action_set_enabled("room-row.set-normal", false);
                    self.action_set_enabled("room-row.set-lowpriority", false);
                    self.action_set_enabled("room-row.leave", false);
                    self.action_set_enabled("room-row.join", false);
                    self.action_set_enabled("room-row.forget", false);
                    self.action_set_enabled("room-row.set-direct", false);
                    return;
                }
                RoomType::Favorite => {
                    self.action_set_enabled("room-row.accept-invite", false);
                    self.action_set_enabled("room-row.reject-invite", false);
                    self.action_set_enabled("room-row.set-favorite", false);
                    self.action_set_enabled("room-row.set-normal", true);
                    self.action_set_enabled("room-row.set-lowpriority", true);
                    self.action_set_enabled("room-row.leave", true);
                    self.action_set_enabled("room-row.join", false);
                    self.action_set_enabled("room-row.forget", false);
                    self.action_set_enabled("room-row.set-direct", true);
                    return;
                }
                RoomType::Normal => {
                    self.action_set_enabled("room-row.accept-invite", false);
                    self.action_set_enabled("room-row.reject-invite", false);
                    self.action_set_enabled("room-row.set-favorite", true);
                    self.action_set_enabled("room-row.set-normal", false);
                    self.action_set_enabled("room-row.set-lowpriority", true);
                    self.action_set_enabled("room-row.leave", true);
                    self.action_set_enabled("room-row.join", false);
                    self.action_set_enabled("room-row.forget", false);
                    self.action_set_enabled("room-row.set-direct", true);
                    return;
                }
                RoomType::LowPriority => {
                    self.action_set_enabled("room-row.accept-invite", false);
                    self.action_set_enabled("room-row.reject-invite", false);
                    self.action_set_enabled("room-row.set-favorite", true);
                    self.action_set_enabled("room-row.set-normal", true);
                    self.action_set_enabled("room-row.set-lowpriority", false);
                    self.action_set_enabled("room-row.leave", true);
                    self.action_set_enabled("room-row.join", false);
                    self.action_set_enabled("room-row.forget", false);
                    self.action_set_enabled("room-row.set-direct", true);
                    return;
                }
                RoomType::Left => {
                    self.action_set_enabled("room-row.accept-invite", false);
                    self.action_set_enabled("room-row.reject-invite", false);
                    self.action_set_enabled("room-row.set-favorite", false);
                    self.action_set_enabled("room-row.set-normal", false);
                    self.action_set_enabled("room-row.set-lowpriority", false);
                    self.action_set_enabled("room-row.leave", false);
                    self.action_set_enabled("room-row.join", true);
                    self.action_set_enabled("room-row.forget", true);
                    self.action_set_enabled("room-row.set-direct", false);
                    return;
                }
                RoomType::Outdated => {}
                RoomType::Space => {}
                RoomType::Direct => {
                    self.action_set_enabled("room-row.accept-invite", false);
                    self.action_set_enabled("room-row.reject-invite", false);
                    self.action_set_enabled("room-row.set-favorite", true);
                    self.action_set_enabled("room-row.set-normal", true);
                    self.action_set_enabled("room-row.set-lowpriority", true);
                    self.action_set_enabled("room-row.leave", true);
                    self.action_set_enabled("room-row.join", false);
                    self.action_set_enabled("room-row.forget", false);
                    self.action_set_enabled("room-row.set-direct", false);
                    return;
                }
            }
        }

        self.action_set_enabled("room-row.accept-invite", false);
        self.action_set_enabled("room-row.reject-invite", false);
        self.action_set_enabled("room-row.set-favorite", false);
        self.action_set_enabled("room-row.set-normal", false);
        self.action_set_enabled("room-row.set-lowpriority", false);
        self.action_set_enabled("room-row.leave", false);
        self.action_set_enabled("room-row.join", false);
        self.action_set_enabled("room-row.forget", false);
        self.action_set_enabled("room-row.set-direct", false);
    }

    fn drag_prepare(&self, drag: &gtk::DragSource, x: f64, y: f64) -> Option<gdk::ContentProvider> {
        let paintable = gtk::WidgetPaintable::new(Some(&self.parent().unwrap()));
        // FIXME: The hotspot coordinates don't work.
        // See https://gitlab.gnome.org/GNOME/gtk/-/issues/2341
        drag.set_icon(Some(&paintable), x as i32, y as i32);
        self.room()
            .map(|room| gdk::ContentProvider::for_value(&room.to_value()))
    }

    fn drag_begin(&self) {
        self.parent().unwrap().add_css_class("drag");
        let category = Some(u32::from(self.room().unwrap().category()));
        self.activate_action("sidebar.set-drop-source-type", Some(&category.to_variant()))
            .unwrap();
    }

    fn drag_end(&self) {
        self.activate_action("sidebar.set-drop-source-type", None)
            .unwrap();
        self.parent().unwrap().remove_css_class("drag");
    }

    fn set_room_as_normal_or_direct(&self) {
        let room = self.room().unwrap();
        if room.is_direct() {
            room.set_category(RoomType::Direct);
        } else {
            room.set_category(RoomType::Normal);
        }
    }
}

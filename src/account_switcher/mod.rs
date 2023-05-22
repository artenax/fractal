use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
};

mod avatar_with_selection;
mod session_item;

use self::session_item::SessionItemRow;
use crate::utils::BoundObjectWeakRef;

mod imp {
    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/ui/account_switcher/mod.ui")]
    pub struct AccountSwitcher {
        #[template_child]
        pub sessions: TemplateChild<gtk::ListBox>,
        /// The model containing the logged-in sessions selection.
        pub session_selection: BoundObjectWeakRef<gtk::SingleSelection>,
        /// The selected row.
        pub selected_row: glib::WeakRef<SessionItemRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AccountSwitcher {
        const NAME: &'static str = "AccountSwitcher";
        type Type = super::AccountSwitcher;
        type ParentType = gtk::Popover;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            Self::Type::bind_template_callbacks(klass);

            klass.install_action("account-switcher.close", None, move |item, _, _| {
                item.popdown();
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AccountSwitcher {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<gtk::SingleSelection>("session-selection")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "session-selection" => self.obj().set_session_selection(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "session-selection" => self.obj().session_selection().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self) {
            self.session_selection.disconnect_signals();
        }
    }

    impl WidgetImpl for AccountSwitcher {}
    impl PopoverImpl for AccountSwitcher {}
}

glib::wrapper! {
    pub struct AccountSwitcher(ObjectSubclass<imp::AccountSwitcher>)
        @extends gtk::Widget, gtk::Popover, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl AccountSwitcher {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The model containing the logged-in sessions selection.
    pub fn session_selection(&self) -> Option<gtk::SingleSelection> {
        self.imp().session_selection.obj()
    }

    /// Set the model containing the logged-in sessions selection.
    pub fn set_session_selection(&self, selection: Option<gtk::SingleSelection>) {
        let imp = self.imp();
        let prev_selection = self.session_selection();

        if selection == prev_selection {
            return;
        }

        imp.session_selection.disconnect_signals();

        imp.sessions.bind_model(selection.as_ref(), |session| {
            let row = SessionItemRow::new(session.downcast_ref().unwrap());
            row.upcast()
        });

        if let Some(selection) = &selection {
            let selected_handler = selection.connect_selected_item_notify(
                clone!(@weak self as obj => move |selection| {
                    obj.update_selected_item(selection.selected());
                }),
            );
            self.update_selected_item(selection.selected());

            imp.session_selection.set(selection, vec![selected_handler]);
        }

        self.notify("session-selection");
    }

    fn selected_row(&self) -> Option<SessionItemRow> {
        self.imp().selected_row.upgrade()
    }

    /// Select the given row in the session list.
    #[template_callback]
    fn select_row(&self, row: gtk::ListBoxRow) {
        self.popdown();

        let Some(selection) = self.session_selection() else {
            return;
        };

        // The index is -1 when it is not in a GtkListBox, but we just got it from the
        // GtkListBox so we can safely assume it's a valid u32.
        selection.set_selected(row.index() as u32);
    }

    /// Update the selected item in the session list.
    fn update_selected_item(&self, selected: u32) {
        let imp = self.imp();

        let old_selected = self.selected_row();
        let new_selected = if selected == gtk::INVALID_LIST_POSITION {
            None
        } else {
            imp.sessions
                .row_at_index(selected as i32)
                .and_downcast::<SessionItemRow>()
        };

        if old_selected == new_selected {
            return;
        }

        if let Some(row) = &old_selected {
            row.set_selected(false);
        }
        if let Some(row) = &new_selected {
            row.set_selected(true);
        }

        imp.selected_row.set(new_selected.as_ref());
    }
}

impl Default for AccountSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

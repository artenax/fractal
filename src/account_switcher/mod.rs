use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate, SelectionModel,
};

use crate::session::Session;

mod avatar_with_selection;
mod user_entry;

use user_entry::UserEntryRow;

mod imp {
    use std::cell::RefCell;

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/sidebar-account-switcher.ui")]
    pub struct AccountSwitcher {
        #[template_child]
        pub entries: TemplateChild<gtk::ListBox>,
        pub pages: RefCell<Option<gtk::SelectionModel>>,
        pub pages_handler: RefCell<Option<glib::SignalHandlerId>>,
        pub selection_handler: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AccountSwitcher {
        const NAME: &'static str = "AccountSwitcher";
        type Type = super::AccountSwitcher;
        type ParentType = gtk::Popover;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

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
                    glib::ParamSpecObject::builder::<gtk::SelectionModel>("pages")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "pages" => self.obj().set_pages(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "pages" => self.obj().pages().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.entries.connect_row_activated(move |_, row| {
                row.activate_action("account-switcher.close", None).unwrap();

                if let Some(session) = row
                    .downcast_ref::<UserEntryRow>()
                    .and_then(|row| row.session())
                {
                    session
                        .parent()
                        .unwrap()
                        .downcast::<gtk::Stack>()
                        .unwrap()
                        .set_visible_child(&session);
                }
            });
        }
    }

    impl WidgetImpl for AccountSwitcher {}
    impl PopoverImpl for AccountSwitcher {}
}

glib::wrapper! {
    pub struct AccountSwitcher(ObjectSubclass<imp::AccountSwitcher>)
        @extends gtk::Widget, gtk::Popover, @implements gtk::Accessible;
}

impl AccountSwitcher {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// Set the model containing the stack pages for each logged in account.
    pub fn set_pages(&self, pages: Option<gtk::SelectionModel>) {
        let imp = self.imp();
        let prev_pages = self.pages();

        if pages == prev_pages {
            return;
        }
        if let Some(prev_pages) = prev_pages {
            if let Some(handler) = imp.pages_handler.take() {
                prev_pages.disconnect(handler);
            }

            if let Some(handler) = imp.selection_handler.take() {
                prev_pages.disconnect(handler);
            }
        }

        if let Some(ref pages) = pages {
            let handler = pages.connect_items_changed(
                clone!(@weak self as obj => move |model, position, removed, added| {
                    obj.update_rows(model, position, removed, added);
                }),
            );

            imp.pages_handler.replace(Some(handler));

            let handler = pages.connect_selection_changed(
                clone!(@weak self as obj => move |_, position, n_items| {
                    obj.update_selection(position, n_items);
                }),
            );

            imp.selection_handler.replace(Some(handler));

            self.update_rows(pages, 0, 0, pages.n_items());
        }

        self.imp().pages.replace(pages);
        self.notify("pages");
    }

    /// The model containing the stack pages for each logged in account.
    pub fn pages(&self) -> Option<gtk::SelectionModel> {
        self.imp().pages.borrow().clone()
    }

    fn update_rows(&self, model: &SelectionModel, position: u32, removed: u32, added: u32) {
        let listbox = self.imp().entries.get();
        for _ in 0..removed {
            if let Some(row) = listbox.row_at_index(position as i32) {
                listbox.remove(&row);
            }
        }
        for i in position..(position + added) {
            let row = UserEntryRow::new(
                &model
                    .item(i)
                    .unwrap()
                    .downcast::<gtk::StackPage>()
                    .unwrap()
                    .child()
                    .downcast::<Session>()
                    .unwrap(),
            );
            row.set_selected(model.is_selected(i));
            listbox.insert(&row, i as i32);
        }
    }

    fn update_selection(&self, position: u32, n_items: u32) {
        let imp = self.imp();
        let pages = imp.pages.borrow();
        let pages = if let Some(pages) = &*pages {
            pages
        } else {
            return;
        };

        for i in position..(position + n_items) {
            if let Some(row) = imp
                .entries
                .row_at_index(i as i32)
                .and_then(|row| row.downcast::<UserEntryRow>().ok())
            {
                row.set_selected(pages.is_selected(i));
            }
        }
    }
}

impl Default for AccountSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

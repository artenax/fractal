use std::convert::TryFrom;

use gtk::{
    gio::{self, ListModel, ListStore},
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate, SelectionModel,
};

use super::account_switcher::item::{ExtraItemObj, Item as AccountSwitcherItem};
use crate::session::Session;

pub mod add_account;
pub mod avatar_with_selection;
pub mod item;
pub mod user_entry;

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/sidebar-account-switcher.ui")]
    pub struct AccountSwitcher {
        #[template_child]
        pub entries: TemplateChild<gtk::ListView>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AccountSwitcher {
        const NAME: &'static str = "AccountSwitcher";
        type Type = super::AccountSwitcher;
        type ParentType = gtk::Popover;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Dialog);

            klass.install_action("account-switcher.close", None, move |item, _, _| {
                item.popdown();
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AccountSwitcher {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.entries.connect_activate(|list_view, index| {
                if let Some(Ok(item)) = list_view
                    .model()
                    .and_then(|model| model.item(index))
                    .map(AccountSwitcherItem::try_from)
                {
                    match item {
                        AccountSwitcherItem::User(session_page, _) => {
                            let session_widget = session_page.child();
                            session_widget
                                .parent()
                                .unwrap()
                                .downcast::<gtk::Stack>()
                                .unwrap()
                                .set_visible_child(&session_widget);
                        }
                        AccountSwitcherItem::AddAccount => {
                            list_view.activate_action("app.new-session", None).unwrap();
                        }
                        _ => {}
                    }
                }
            });
        }
    }

    impl WidgetImpl for AccountSwitcher {}
    impl PopoverImpl for AccountSwitcher {}
}

glib::wrapper! {
    pub struct AccountSwitcher(ObjectSubclass<imp::AccountSwitcher>)
        @extends gtk::Widget, gtk::Popover, @implements gtk::Accessible, gio::ListModel;
}

impl AccountSwitcher {
    pub fn set_logged_in_users(
        &self,
        sessions_stack_pages: &SelectionModel,
        session_root: &Session,
    ) {
        let entries = self.imp().entries.get();

        // There is no permanent stuff to take care of,
        // so only bind and unbind are connected.
        let factory = &gtk::SignalListItemFactory::new();
        factory.connect_bind(clone!(@weak session_root => move |_, list_item| {
            list_item.set_selectable(false);
            let child = list_item
                .item()
                .map(AccountSwitcherItem::try_from)
                .and_then(Result::ok)
                .map(|item| {
                    // Given that all the account switchers are built per-session widget
                    // there is no need for callbacks or data bindings; just set the hint
                    // when building the entries and they will show correctly marked in
                    // each session widget.
                    let item = item.set_hint(session_root);

                    if item == AccountSwitcherItem::Separator {
                        list_item.set_activatable(false);
                    }

                    item
                })
                .as_ref()
                .map(AccountSwitcherItem::build_widget);

            list_item.set_child(child.as_ref());
        }));

        factory.connect_unbind(|_, list_item| {
            list_item.set_child(gtk::Widget::NONE);
        });

        entries.set_factory(Some(factory));

        let end_items = &ExtraItemObj::list_store();
        let items_split = &ListStore::new(ListModel::static_type());
        items_split.append(sessions_stack_pages);
        items_split.append(end_items);
        let items = &gtk::FlattenListModel::new(Some(items_split));
        let selectable_items = &gtk::NoSelection::new(Some(items));

        entries.set_model(Some(selectable_items));
    }
}

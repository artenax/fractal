use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    CompositeTemplate,
};
use matrix_sdk::encryption::{KeyExportError, RoomKeyImportError};
use tracing::{debug, error};

use crate::{
    components::SpinnerButton, ngettext_f, session::model::Session, spawn, spawn_tokio, toast,
};

#[derive(Debug, Default, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "KeysSubpageMode")]
pub enum KeysSubpageMode {
    #[default]
    Export = 0,
    Import = 1,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{subclass::InitializingObject, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/account_settings/security_page/import_export_keys_subpage.ui"
    )]
    pub struct ImportExportKeysSubpage {
        pub session: WeakRef<Session>,
        #[template_child]
        pub description: TemplateChild<gtk::Label>,
        #[template_child]
        pub instructions: TemplateChild<gtk::Label>,
        #[template_child]
        pub passphrase: TemplateChild<adw::PasswordEntryRow>,
        #[template_child]
        pub confirm_passphrase_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub confirm_passphrase: TemplateChild<adw::PasswordEntryRow>,
        #[template_child]
        pub confirm_passphrase_error_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub confirm_passphrase_error: TemplateChild<gtk::Label>,
        #[template_child]
        pub file_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub file_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub proceed_button: TemplateChild<SpinnerButton>,
        pub file_path: RefCell<Option<gio::File>>,
        pub mode: Cell<KeysSubpageMode>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ImportExportKeysSubpage {
        const NAME: &'static str = "ImportExportKeysSubpage";
        type Type = super::ImportExportKeysSubpage;
        type ParentType = adw::NavigationPage;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            Self::Type::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ImportExportKeysSubpage {
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Session>("session").build(),
                    glib::ParamSpecString::builder("file-path")
                        .read_only()
                        .build(),
                    glib::ParamSpecEnum::builder::<KeysSubpageMode>("mode")
                        .explicit_notify()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.set_session(value.get().unwrap()),
                "mode" => obj.set_mode(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "session" => obj.session().to_value(),
                "file-path" => obj
                    .file_path()
                    .and_then(|file| file.path())
                    .map(|path| path.to_string_lossy().to_string())
                    .to_value(),
                "mode" => obj.mode().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            self.passphrase
                .connect_changed(clone!(@weak obj => move|_| {
                    obj.validate_passphrase_confirmation();
                }));

            self.confirm_passphrase
                .connect_changed(clone!(@weak obj => move|_| {
                    obj.validate_passphrase_confirmation();
                }));

            obj.update_for_mode();
        }
    }

    impl WidgetImpl for ImportExportKeysSubpage {}
    impl NavigationPageImpl for ImportExportKeysSubpage {}
}

glib::wrapper! {
    /// Subpage to export room encryption keys for backup.
    pub struct ImportExportKeysSubpage(ObjectSubclass<imp::ImportExportKeysSubpage>)
        @extends gtk::Widget, adw::NavigationPage, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl ImportExportKeysSubpage {
    pub fn new(session: &Session) -> Self {
        glib::Object::builder().property("session", session).build()
    }

    /// The current session.
    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    /// Set the current session.
    pub fn set_session(&self, session: Option<Session>) {
        self.imp().session.set(session.as_ref());
    }

    /// The path to export the keys to.
    pub fn file_path(&self) -> Option<gio::File> {
        self.imp().file_path.borrow().clone()
    }

    /// Set the path to export the keys to.
    fn set_file_path(&self, path: Option<gio::File>) {
        let imp = self.imp();
        if imp.file_path.borrow().as_ref() == path.as_ref() {
            return;
        }

        imp.file_path.replace(path);
        self.update_button();
        self.notify("file-path");
    }

    /// The export/import mode of the subpage.
    pub fn mode(&self) -> KeysSubpageMode {
        self.imp().mode.get()
    }

    /// Set the export/import mode of the subpage.
    pub fn set_mode(&self, mode: KeysSubpageMode) {
        if self.mode() == mode {
            return;
        }

        self.imp().mode.set(mode);
        self.update_for_mode();
        self.clear();
        self.notify("mode");
    }

    fn clear(&self) {
        let imp = self.imp();

        self.set_file_path(None);
        imp.passphrase.set_text("");
        imp.confirm_passphrase.set_text("");
    }

    fn update_for_mode(&self) {
        let imp = self.imp();

        if self.mode() == KeysSubpageMode::Export {
            self.set_title(&gettext("Export Room Encryption Keys"));
            imp.description.set_label(&gettext(
                "Exporting your room encryption keys allows you to make a backup to be able to decrypt your messages in end-to-end encrypted rooms on another device or with another Matrix client.",
            ));
            imp.instructions.set_label(&gettext(
                "The backup must be stored in a safe place and must be protected with a strong passphrase that will be used to encrypt the data.",
            ));
            imp.confirm_passphrase_box.set_visible(true);
            imp.proceed_button.set_label(&gettext("Export Keys"));
        } else {
            self.set_title(&gettext("Import Room Encryption Keys"));
            imp.description.set_label(&gettext(
                "Importing your room encryption keys allows you to decrypt your messages in end-to-end encrypted rooms with a previous backup from a Matrix client.",
            ));
            imp.instructions.set_label(&gettext(
                "Enter the passphrase provided when the backup file was created.",
            ));
            imp.confirm_passphrase_box.set_visible(false);
            imp.proceed_button.set_label(&gettext("Import Keys"));
        }

        self.update_button();
    }

    #[template_callback]
    fn handle_choose_file(&self) {
        spawn!(clone!(@weak self as obj => async move {
            obj.choose_file().await;
        }));
    }

    async fn choose_file(&self) {
        let is_export = self.mode() == KeysSubpageMode::Export;

        let dialog = gtk::FileDialog::builder()
            .modal(true)
            .accept_label(gettext("Choose"))
            .build();

        if let Some(file) = self.file_path() {
            dialog.set_initial_file(Some(&file));
        } else if is_export {
            // Translators: Do no translate "fractal" as it is the application
            // name.
            dialog.set_initial_name(Some(&format!("{}.txt", gettext("fractal-encryption-keys"))));
        }

        let parent_window = self.root().and_downcast::<gtk::Window>();
        let res = if is_export {
            dialog.set_title(&gettext("Save Encryption Keys To…"));
            dialog.save_future(parent_window.as_ref()).await
        } else {
            dialog.set_title(&gettext("Import Encryption Keys From…"));
            dialog.open_future(parent_window.as_ref()).await
        };

        match res {
            Ok(file) => {
                self.set_file_path(Some(file));
            }
            Err(error) => {
                if error.matches(gtk::DialogError::Dismissed) {
                    debug!("File dialog dismissed by user");
                } else {
                    error!("Could not access file: {error:?}");
                    toast!(self, gettext("Could not access file"));
                }
            }
        };
    }

    fn validate_passphrase_confirmation(&self) {
        let imp = self.imp();
        let entry = &imp.confirm_passphrase;
        let revealer = &imp.confirm_passphrase_error_revealer;
        let label = &imp.confirm_passphrase_error;
        let passphrase = imp.passphrase.text();
        let confirmation = entry.text();

        if confirmation.is_empty() {
            revealer.set_reveal_child(false);
            entry.remove_css_class("success");
            entry.remove_css_class("warning");
            return;
        }

        if passphrase == confirmation {
            revealer.set_reveal_child(false);
            entry.add_css_class("success");
            entry.remove_css_class("warning");
        } else {
            label.set_label(&gettext("Passphrases do not match"));
            revealer.set_reveal_child(true);
            entry.remove_css_class("success");
            entry.add_css_class("warning");
        }
        self.update_button();
    }

    fn update_button(&self) {
        self.imp().proceed_button.set_sensitive(self.can_proceed());
    }

    fn can_proceed(&self) -> bool {
        let imp = self.imp();
        let file_path = imp.file_path.borrow();
        let passphrase = imp.passphrase.text();

        let mut res = file_path
            .as_ref()
            .filter(|file| file.path().is_some())
            .is_some()
            && !passphrase.is_empty();

        if self.mode() == KeysSubpageMode::Export {
            let confirmation = imp.confirm_passphrase.text();
            res = res && passphrase == confirmation;
        }

        res
    }

    #[template_callback]
    fn handle_proceed(&self) {
        spawn!(clone!(@weak self as obj => async move {
            obj.proceed().await;
        }));
    }

    async fn proceed(&self) {
        if !self.can_proceed() {
            return;
        }

        let imp = self.imp();
        let file_path = self.file_path().and_then(|file| file.path()).unwrap();
        let passphrase = imp.passphrase.text();
        let is_export = self.mode() == KeysSubpageMode::Export;

        imp.proceed_button.set_loading(true);
        imp.file_button.set_sensitive(false);
        imp.passphrase.set_sensitive(false);
        imp.confirm_passphrase.set_sensitive(false);

        let encryption = self.session().unwrap().client().encryption();

        let handle = spawn_tokio!(async move {
            if is_export {
                encryption
                    .export_room_keys(file_path, passphrase.as_str(), |_| true)
                    .await
                    .map(|_| 0usize)
                    .map_err::<Box<dyn std::error::Error + Send>, _>(|error| Box::new(error))
            } else {
                encryption
                    .import_room_keys(file_path, passphrase.as_str())
                    .await
                    .map(|res| res.imported_count)
                    .map_err::<Box<dyn std::error::Error + Send>, _>(|error| Box::new(error))
            }
        });

        match handle.await.unwrap() {
            Ok(nb) => {
                if is_export {
                    toast!(self, gettext("Room encryption keys exported successfully"));
                } else {
                    toast!(
                        self,
                        ngettext_f(
                            "Imported 1 room encryption key",
                            "Imported {n} room encryption keys",
                            nb as u32,
                            &[("n", &nb.to_string())]
                        )
                    );
                }
                self.clear();
                self.activate_action("win.close-subpage", None).unwrap();
            }
            Err(err) => {
                if is_export {
                    error!("Failed to export the keys: {err:?}");
                    toast!(self, gettext("Could not export the keys"));
                } else if err
                    .downcast_ref::<RoomKeyImportError>()
                    .filter(|err| {
                        matches!(err, RoomKeyImportError::Export(KeyExportError::InvalidMac))
                    })
                    .is_some()
                {
                    toast!(
                        self,
                        gettext("The passphrase doesn't match the one used to export the keys.")
                    );
                } else {
                    error!("Failed to import the keys: {err:?}");
                    toast!(self, gettext("Could not import the keys"));
                }
            }
        }
        imp.proceed_button.set_loading(false);
        imp.file_button.set_sensitive(true);
        imp.passphrase.set_sensitive(true);
        imp.confirm_passphrase.set_sensitive(true);
    }
}

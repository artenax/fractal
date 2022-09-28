use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    CompositeTemplate,
};
use log::error;
use matrix_sdk::encryption::{KeyExportError, RoomKeyImportError};

use crate::{
    components::{PasswordEntryRow, SpinnerButton},
    i18n::ngettext_f,
    session::Session,
    spawn, spawn_tokio, toast,
};

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "KeysSubpageMode")]
pub enum KeysSubpageMode {
    Export = 0,
    Import = 1,
}

impl Default for KeysSubpageMode {
    fn default() -> Self {
        Self::Export
    }
}

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{subclass::InitializingObject, WeakRef};

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/account-settings-import-export-keys-subpage.ui")]
    pub struct ImportExportKeysSubpage {
        pub session: WeakRef<Session>,
        #[template_child]
        pub title: TemplateChild<gtk::Label>,
        #[template_child]
        pub description: TemplateChild<gtk::Label>,
        #[template_child]
        pub instructions: TemplateChild<gtk::Label>,
        #[template_child]
        pub passphrase: TemplateChild<PasswordEntryRow>,
        #[template_child]
        pub confirm_passphrase: TemplateChild<PasswordEntryRow>,
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
        type ParentType = gtk::Box;

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
                    glib::ParamSpecObject::new(
                        "session",
                        "Session",
                        "The session",
                        Session::static_type(),
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecString::new(
                        "file-path",
                        "File Path",
                        "The path to export the keys to",
                        None,
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecEnum::new(
                        "mode",
                        "Mode",
                        "The export/import mode of the subpage",
                        KeysSubpageMode::static_type(),
                        KeysSubpageMode::default() as i32,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
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
                "session" => obj.set_session(value.get().unwrap()),
                "mode" => obj.set_mode(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
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

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.passphrase
                .connect_changed(clone!(@weak obj => move|_| {
                    obj.update_button();
                }));

            self.confirm_passphrase
                .connect_focused(clone!(@weak obj => move |entry, focused| {
                    if focused {
                        obj.validate_passphrase_confirmation();
                    } else {
                        entry.remove_css_class("warning");
                        entry.remove_css_class("success");
                    }
                }));
            self.confirm_passphrase
                .connect_changed(clone!(@weak obj => move|_| {
                    obj.validate_passphrase_confirmation();
                }));

            obj.update_for_mode();
        }
    }

    impl WidgetImpl for ImportExportKeysSubpage {}
    impl BoxImpl for ImportExportKeysSubpage {}
}

glib::wrapper! {
    /// Subpage to export room encryption keys for backup.
    pub struct ImportExportKeysSubpage(ObjectSubclass<imp::ImportExportKeysSubpage>)
        @extends gtk::Widget, gtk::Box, @implements gtk::Accessible;
}

#[gtk::template_callbacks]
impl ImportExportKeysSubpage {
    pub fn new(session: &Session) -> Self {
        glib::Object::new(&[("session", session)])
            .expect("Failed to create ImportExportKeysSubpage")
    }

    pub fn session(&self) -> Option<Session> {
        self.imp().session.upgrade()
    }

    pub fn set_session(&self, session: Option<Session>) {
        self.imp().session.set(session.as_ref());
    }

    pub fn file_path(&self) -> Option<gio::File> {
        self.imp().file_path.borrow().clone()
    }

    pub fn set_file_path(&self, path: Option<gio::File>) {
        let priv_ = self.imp();
        if priv_.file_path.borrow().as_ref() == path.as_ref() {
            return;
        }

        priv_.file_path.replace(path);
        self.update_button();
        self.notify("file-path");
    }

    pub fn mode(&self) -> KeysSubpageMode {
        self.imp().mode.get()
    }

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
        let priv_ = self.imp();

        self.set_file_path(None);
        priv_.passphrase.set_text("");
        priv_.confirm_passphrase.set_text("");
    }

    fn update_for_mode(&self) {
        let priv_ = self.imp();

        if self.mode() == KeysSubpageMode::Export {
            priv_
                .title
                .set_label(&gettext("Export Room Encryption Keys"));
            priv_.description.set_label(&gettext(
                "Exporting your room encryption keys allows you to make a backup to be able to decrypt your messages in end-to-end encrypted rooms on another device or with another Matrix client.",
            ));
            priv_.instructions.set_label(&gettext(
                "The backup must be stored in a safe place and must be protected with a strong passphrase that will be used to encrypt the data.",
            ));
            priv_.confirm_passphrase.show();
            priv_.proceed_button.set_label(&gettext("Export Keys"));
        } else {
            priv_
                .title
                .set_label(&gettext("Import Room Encryption Keys"));
            priv_.description.set_label(&gettext(
                "Importing your room encryption keys allows you to decrypt your messages in end-to-end encrypted rooms with a previous backup from a Matrix client.",
            ));
            priv_.instructions.set_label(&gettext(
                "Enter the passphrase provided when the backup file was created.",
            ));
            priv_.confirm_passphrase.hide();
            priv_.proceed_button.set_label(&gettext("Import Keys"));
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
        let (title, action) = if is_export {
            (
                gettext("Save Encryption Keys To…"),
                gtk::FileChooserAction::Save,
            )
        } else {
            (
                gettext("Import Encryption Keys From…"),
                gtk::FileChooserAction::Open,
            )
        };

        let dialog = gtk::FileChooserNative::builder()
            .title(&title)
            .modal(true)
            .transient_for(
                self.root()
                    .as_ref()
                    .and_then(|root| root.downcast_ref::<gtk::Window>())
                    .unwrap(),
            )
            .action(action)
            .accept_label(&gettext("Select"))
            .cancel_label(&gettext("Cancel"))
            .build();

        if let Some(file) = self.file_path() {
            let _ = dialog.set_file(&file);
        } else if is_export {
            // Translators: Do no translate "fractal" as it is the application
            // name.
            dialog.set_current_name(&format!("{}.txt", gettext("fractal-encryption-keys")));
        }

        if dialog.run_future().await == gtk::ResponseType::Accept {
            if let Some(file) = dialog.file() {
                self.set_file_path(Some(file));
            } else {
                error!("No file chosen");
                toast!(self, gettext("No file was chosen"));
            }
        }
    }

    fn validate_passphrase_confirmation(&self) {
        let priv_ = self.imp();
        let entry = &priv_.confirm_passphrase;
        let passphrase = priv_.passphrase.text();
        let confirmation = entry.text();

        if confirmation.is_empty() {
            entry.set_hint("");
            entry.remove_css_class("success");
            entry.remove_css_class("warning");
            return;
        }

        if passphrase == confirmation {
            entry.set_hint("");
            entry.add_css_class("success");
            entry.remove_css_class("warning");
        } else {
            entry.remove_css_class("success");
            entry.add_css_class("warning");
            entry.set_hint(&gettext("Passphrases do not match"));
        }
        self.update_button();
    }

    fn update_button(&self) {
        self.imp().proceed_button.set_sensitive(self.can_proceed());
    }

    fn can_proceed(&self) -> bool {
        let priv_ = self.imp();
        let file_path = priv_.file_path.borrow();
        let passphrase = priv_.passphrase.text();

        let mut res = file_path
            .as_ref()
            .filter(|file| file.path().is_some())
            .is_some()
            && !passphrase.is_empty();

        if self.mode() == KeysSubpageMode::Export {
            let confirmation = priv_.confirm_passphrase.text();
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

        let priv_ = self.imp();
        let file_path = self.file_path().and_then(|file| file.path()).unwrap();
        let passphrase = priv_.passphrase.text();
        let is_export = self.mode() == KeysSubpageMode::Export;

        priv_.proceed_button.set_loading(true);
        priv_.file_button.set_sensitive(false);
        priv_.passphrase.set_entry_sensitive(false);
        priv_.confirm_passphrase.set_entry_sensitive(false);

        let encryption = self.session().unwrap().client().encryption();

        let handle = spawn_tokio!(async move {
            if is_export {
                encryption
                    .export_keys(file_path, passphrase.as_str(), |_| true)
                    .await
                    .map(|_| 0usize)
                    .map_err::<Box<dyn std::error::Error + Send>, _>(|error| Box::new(error))
            } else {
                encryption
                    .import_keys(file_path, passphrase.as_str())
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
        priv_.proceed_button.set_loading(false);
        priv_.file_button.set_sensitive(true);
        priv_.passphrase.set_entry_sensitive(true);
        priv_.confirm_passphrase.set_entry_sensitive(true);
    }
}

use gtk::{self, glib, glib::clone, prelude::*, subclass::prelude::*, CompositeTemplate};
use matrix_sdk::ruma::api::client::session::get_login_types::v3::{
    IdentityProvider, IdentityProviderBrand,
};

#[derive(Hash, Debug, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(i32)]
#[enum_type(name = "IdpBrand")]
pub enum IdpBrand {
    Apple = 0,
    Facebook = 1,
    GitHub = 2,
    GitLab = 3,
    Google = 4,
    Twitter = 5,
}

impl IdpBrand {
    /// Get the icon name of this brand, according to the current theme.
    pub fn icon(&self) -> &'static str {
        let dark = adw::StyleManager::default().is_dark();
        match self {
            IdpBrand::Apple => {
                if dark {
                    "idp-apple-dark"
                } else {
                    "idp-apple"
                }
            }
            IdpBrand::Facebook => "idp-facebook",
            IdpBrand::GitHub => {
                if dark {
                    "idp-github-dark"
                } else {
                    "idp-github"
                }
            }
            IdpBrand::GitLab => "idp-gitlab",
            IdpBrand::Google => "idp-google",
            IdpBrand::Twitter => "idp-twitter",
        }
    }
}

impl Default for IdpBrand {
    fn default() -> Self {
        IdpBrand::Apple
    }
}

impl TryFrom<&IdentityProviderBrand> for IdpBrand {
    type Error = ();

    fn try_from(item: &IdentityProviderBrand) -> Result<Self, Self::Error> {
        match item {
            IdentityProviderBrand::Apple => Ok(IdpBrand::Apple),
            IdentityProviderBrand::Facebook => Ok(IdpBrand::Facebook),
            IdentityProviderBrand::GitHub => Ok(IdpBrand::GitHub),
            IdentityProviderBrand::GitLab => Ok(IdpBrand::GitLab),
            IdentityProviderBrand::Google => Ok(IdpBrand::Google),
            IdentityProviderBrand::Twitter => Ok(IdpBrand::Twitter),
            _ => Err(()),
        }
    }
}

impl From<IdpBrand> for &str {
    fn from(val: IdpBrand) -> Self {
        let dark = adw::StyleManager::default().is_dark();
        match val {
            IdpBrand::Apple => {
                if dark {
                    "idp-apple-dark"
                } else {
                    "idp-apple"
                }
            }
            IdpBrand::Facebook => "idp-facebook",
            IdpBrand::GitHub => {
                if dark {
                    "idp-github-dark"
                } else {
                    "idp-github"
                }
            }
            IdpBrand::GitLab => "idp-gitlab",
            IdpBrand::Google => "idp-google",
            IdpBrand::Twitter => "idp-twitter",
        }
    }
}

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/login-idp-button.ui")]
    pub struct IdpButton {
        pub brand: Cell<IdpBrand>,
        pub id: RefCell<Option<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for IdpButton {
        const NAME: &'static str = "IdpButton";
        type Type = super::IdpButton;
        type ParentType = gtk::Button;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            klass.set_accessible_role(gtk::AccessibleRole::Button);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for IdpButton {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecEnum::new(
                        "brand",
                        "Brand",
                        "The brand of this button",
                        IdpBrand::static_type(),
                        IdpBrand::default() as i32,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                    glib::ParamSpecString::new(
                        "id",
                        "Id",
                        "The id of the selected identity-provider",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "id" => obj.id().unwrap().to_value(),
                "brand" => obj.brand().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "brand" => {
                    obj.set_brand(value.get().unwrap());
                }
                "id" => {
                    obj.set_id(value.get().unwrap());
                }
                _ => unimplemented!(),
            };
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);
            adw::StyleManager::default()
                .connect_dark_notify(clone!(@weak obj => move |_| obj.update_icon()));
            obj.update_icon();
        }
    }

    impl WidgetImpl for IdpButton {}
    impl ButtonImpl for IdpButton {}
}

glib::wrapper! {
    pub struct IdpButton(ObjectSubclass<imp::IdpButton>)
        @extends gtk::Widget, gtk::Button,
        @implements gtk::Accessible, gtk::Actionable;
}

impl IdpButton {
    pub fn update_icon(&self) {
        self.set_icon_name(self.brand().icon());
    }

    pub fn set_id(&self, id: String) {
        self.set_action_target_value(Some(&Some(&id).to_variant()));
        self.imp().id.replace(Some(id));
    }

    pub fn set_brand(&self, brand: IdpBrand) {
        self.imp().brand.replace(brand);
    }

    pub fn id(&self) -> Option<String> {
        self.imp().id.borrow().clone()
    }

    pub fn brand(&self) -> IdpBrand {
        self.imp().brand.get()
    }

    pub fn new_from_identity_provider(idp: &IdentityProvider) -> Option<Self> {
        let gidp: IdpBrand = idp.brand.as_ref()?.try_into().ok()?;

        Some(
            glib::Object::new(&[("brand", &gidp), ("id", &idp.id)])
                .expect("Failed to create IdpButton"),
        )
    }
}

use std::fmt;

use gettextrs::gettext;
use gtk::glib;

use super::PowerLevel;

/// Role of a room member, like admin or moderator.
#[glib::flags(name = "MemberRole")]
pub enum MemberRole {
    #[flags_value(name = "ADMIN")]
    ADMIN = 1,
    #[flags_value(name = "MOD")]
    MOD = 2,
    #[flags_value(name = "PEASANT")]
    PEASANT = 0,
}

impl MemberRole {
    pub fn is_admin(&self) -> bool {
        matches!(*self, Self::ADMIN)
    }

    pub fn is_mod(&self) -> bool {
        matches!(*self, Self::MOD)
    }

    pub fn is_peasant(&self) -> bool {
        matches!(*self, Self::PEASANT)
    }
}

impl From<PowerLevel> for MemberRole {
    fn from(power_level: PowerLevel) -> Self {
        if (100..).contains(&power_level) {
            Self::ADMIN
        } else if (50..100).contains(&power_level) {
            Self::MOD
        } else {
            Self::PEASANT
        }
    }
}

impl fmt::Display for MemberRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::ADMIN => write!(f, "{}", gettext("Admin")),
            Self::MOD => write!(f, "{}", gettext("Moderator")),
            _ => write!(f, "{}", gettext("Normal user")),
        }
    }
}

use std::fmt;

use gettextrs::gettext;
use gtk::glib;

use super::PowerLevel;

/// Role of a room member, like admin or moderator.
#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "MemberRole")]
pub enum MemberRole {
    /// An administrator.
    Admin = 1,
    /// A moderator.
    Mod = 2,
    /// A regular room member.
    Peasant = 0,
}

impl MemberRole {
    pub fn is_admin(&self) -> bool {
        matches!(*self, Self::Admin)
    }

    pub fn is_mod(&self) -> bool {
        matches!(*self, Self::Mod)
    }

    pub fn is_peasant(&self) -> bool {
        matches!(*self, Self::Peasant)
    }
}

impl From<PowerLevel> for MemberRole {
    fn from(power_level: PowerLevel) -> Self {
        if (100..).contains(&power_level) {
            Self::Admin
        } else if (50..100).contains(&power_level) {
            Self::Mod
        } else {
            Self::Peasant
        }
    }
}

impl fmt::Display for MemberRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Admin => write!(f, "{}", gettext("Admin")),
            Self::Mod => write!(f, "{}", gettext("Moderator")),
            _ => write!(f, "{}", gettext("Normal user")),
        }
    }
}

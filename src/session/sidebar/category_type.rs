use std::fmt;

use gettextrs::gettext;
use gtk::glib;

use crate::session::room::RoomType;

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(i32)]
#[enum_type(name = "CategoryType")]
pub enum CategoryType {
    None = -1,
    VerificationRequest = 0,
    Invited = 1,
    Favorite = 2,
    Normal = 3,
    LowPriority = 4,
    Left = 5,
    Outdated = 6,
    Space = 7,
    Direct = 8,
}

impl Default for CategoryType {
    fn default() -> Self {
        CategoryType::Normal
    }
}

impl fmt::Display for CategoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            CategoryType::None => unimplemented!(),
            CategoryType::VerificationRequest => gettext("Verifications"),
            CategoryType::Invited => gettext("Invited"),
            CategoryType::Favorite => gettext("Favorites"),
            CategoryType::Normal => gettext("Rooms"),
            CategoryType::LowPriority => gettext("Low Priority"),
            CategoryType::Left => gettext("Historical"),
            // Translators: This shouldn't ever be visible to the user,
            CategoryType::Outdated => gettext("Outdated"),
            CategoryType::Space => gettext("Spaces"),
            CategoryType::Direct => gettext("People"),
        };
        f.write_str(&label)
    }
}

impl From<RoomType> for CategoryType {
    fn from(room_type: RoomType) -> Self {
        Self::from(&room_type)
    }
}

impl From<&RoomType> for CategoryType {
    fn from(room_type: &RoomType) -> Self {
        match room_type {
            RoomType::Invited => Self::Invited,
            RoomType::Favorite => Self::Favorite,
            RoomType::Normal => Self::Normal,
            RoomType::LowPriority => Self::LowPriority,
            RoomType::Left => Self::Left,
            RoomType::Outdated => Self::Outdated,
            RoomType::Space => Self::Space,
            RoomType::Direct => Self::Direct,
        }
    }
}

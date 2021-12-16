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

impl ToString for CategoryType {
    fn to_string(&self) -> String {
        match self {
            CategoryType::None => unimplemented!(),
            CategoryType::VerificationRequest => gettext("Verifications"),
            CategoryType::Invited => gettext("Invited"),
            CategoryType::Favorite => gettext("Favorite"),
            CategoryType::Normal => gettext("Rooms"),
            CategoryType::LowPriority => gettext("Low Priority"),
            CategoryType::Left => gettext("Historical"),
            // Translators: This shouldn't ever be visible to the user,
            CategoryType::Outdated => gettext("Outdated"),
            CategoryType::Space => gettext("Spaces"),
            CategoryType::Direct => gettext("People"),
        }
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

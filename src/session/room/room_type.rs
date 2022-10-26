use std::fmt;

use gtk::glib;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::session::sidebar::CategoryType;

// TODO: do we also want custom tags support?
// See https://spec.matrix.org/v1.2/client-server-api/#room-tagging
#[derive(
    Debug, Default, Hash, Eq, PartialEq, Clone, Copy, glib::Enum, IntoPrimitive, TryFromPrimitive,
)]
#[repr(u32)]
#[enum_type(name = "RoomType")]
pub enum RoomType {
    Invited = 0,
    Favorite = 1,
    #[default]
    Normal = 2,
    LowPriority = 3,
    Left = 4,
    Outdated = 5,
    Space = 6,
    Direct = 7,
}

impl RoomType {
    /// Check whether this `RoomType` can be changed to `category`.
    pub fn can_change_to(&self, category: &RoomType) -> bool {
        match self {
            Self::Invited => {
                matches!(
                    category,
                    Self::Favorite | Self::Normal | Self::Direct | Self::LowPriority | Self::Left
                )
            }
            Self::Favorite => {
                matches!(
                    category,
                    Self::Normal | Self::Direct | Self::LowPriority | Self::Left
                )
            }
            Self::Normal => {
                matches!(
                    category,
                    Self::Favorite | Self::Direct | Self::LowPriority | Self::Left
                )
            }
            Self::LowPriority => {
                matches!(
                    category,
                    Self::Favorite | Self::Direct | Self::Normal | Self::Left
                )
            }
            Self::Left => {
                matches!(
                    category,
                    Self::Favorite | Self::Direct | Self::Normal | Self::LowPriority
                )
            }
            Self::Outdated => false,
            Self::Space => false,
            Self::Direct => {
                matches!(
                    category,
                    Self::Favorite | Self::Normal | Self::LowPriority | Self::Left
                )
            }
        }
    }
}

impl fmt::Display for RoomType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        CategoryType::from(self).fmt(f)
    }
}

impl TryFrom<CategoryType> for RoomType {
    type Error = &'static str;

    fn try_from(category_type: CategoryType) -> Result<Self, Self::Error> {
        Self::try_from(&category_type)
    }
}

impl TryFrom<&CategoryType> for RoomType {
    type Error = &'static str;

    fn try_from(category_type: &CategoryType) -> Result<Self, Self::Error> {
        match category_type {
            CategoryType::None => Err("CategoryType::None cannot be a RoomType"),
            CategoryType::Invited => Ok(Self::Invited),
            CategoryType::Favorite => Ok(Self::Favorite),
            CategoryType::Normal => Ok(Self::Normal),
            CategoryType::LowPriority => Ok(Self::LowPriority),
            CategoryType::Left => Ok(Self::Left),
            CategoryType::Outdated => Ok(Self::Outdated),
            CategoryType::VerificationRequest => {
                Err("CategoryType::VerificationRequest cannot be a RoomType")
            }
            CategoryType::Space => Ok(Self::Space),
            CategoryType::Direct => Ok(Self::Direct),
        }
    }
}

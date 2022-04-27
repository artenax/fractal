use std::fmt;

use gettextrs::gettext;
use gtk::glib;

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "EntryType")]
pub enum EntryType {
    Explore = 0,
    Forget = 1,
}

impl Default for EntryType {
    fn default() -> Self {
        EntryType::Explore
    }
}

impl fmt::Display for EntryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            EntryType::Explore => gettext("Explore"),
            EntryType::Forget => gettext("Forget Room"),
        };

        f.write_str(&label)
    }
}

use gtk::glib;

#[glib::flags(name = "HighlightFlags")]
pub enum HighlightFlags {
    HIGHLIGHT = 0b00000001,
    BOLD = 0b00000010,
}

impl Default for HighlightFlags {
    fn default() -> Self {
        HighlightFlags::empty()
    }
}

use adw::{prelude::BinExt, subclass::prelude::*};
use gtk::{glib, prelude::*, subclass::prelude::*};
use html2pango::{
    block::{markup_html, HtmlBlock},
    html_escape, markup_links,
};
use log::warn;
use matrix_sdk::ruma::{
    events::room::message::{FormattedBody, MessageFormat},
    matrix_uri::MatrixId,
    MatrixToUri, MatrixUri,
};
use once_cell::sync::Lazy;
use regex::Regex;
use sourceview::prelude::*;

use crate::{
    components::{LabelWithWidgets, Pill, DEFAULT_PLACEHOLDER},
    session::{room::Member, Room, UserExt},
};

static EMOJI_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)
        ^
        [\p{White_Space}\p{Emoji_Component}]*
        [\p{Emoji}--\p{Decimal_Number}]+
        [\p{White_Space}\p{Emoji}\p{Emoji_Component}--\p{Decimal_Number}]*
        $
        # That string is made of at least one emoji, except digits, possibly more,
        # possibly with modifiers, possibly with spaces, but nothing else
        ",
    )
    .unwrap()
});

enum WithMentions<'a> {
    Yes(&'a Room),
    No,
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct MessageText {}

    #[glib::object_subclass]
    impl ObjectSubclass for MessageText {
        const NAME: &'static str = "ContentMessageText";
        type Type = super::MessageText;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for MessageText {}

    impl WidgetImpl for MessageText {}

    impl BinImpl for MessageText {}
}

glib::wrapper! {
    /// A widget displaying the content of a text message.
    // FIXME: We have to be able to allow text selection and override popover
    // menu. See https://gitlab.gnome.org/GNOME/gtk/-/issues/4606
    pub struct MessageText(ObjectSubclass<imp::MessageText>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl MessageText {
    /// Creates a text widget.
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create MessageText")
    }

    /// Display the given plain text.
    pub fn text(&self, body: String) {
        self.build_text(body, WithMentions::No);
    }

    /// Display the given text with markup.
    ///
    /// It will detect if it should display the body or the formatted body.
    pub fn markup(&self, formatted: Option<FormattedBody>, body: String, room: &Room) {
        if let Some(html_blocks) = formatted
            .filter(is_valid_formatted_body)
            .and_then(|formatted| parse_formatted_body(strip_reply(&formatted.body)))
        {
            self.build_html(html_blocks, room);
        } else {
            let body = linkify(strip_reply(&body));
            self.build_text(body, WithMentions::Yes(room));
        }
    }

    /// Display the given emote for `sender`.
    ///
    /// It will detect if it should display the body or the formatted body.
    pub fn emote(
        &self,
        formatted: Option<FormattedBody>,
        body: String,
        sender: Member,
        room: &Room,
    ) {
        if let Some(body) = formatted
            .filter(is_valid_formatted_body)
            .and_then(|formatted| {
                let body = format!("<b>{}</b> {}", sender.display_name(), formatted.body);

                parse_formatted_body(&body).map(|_| formatted.body)
            })
        {
            let formatted = FormattedBody {
                body: format!("<b>{}</b> {}", sender.display_name(), strip_reply(&body)),
                format: MessageFormat::Html,
            };

            let html = parse_formatted_body(&formatted.body).unwrap();
            self.build_html(html, room);
        } else {
            self.build_text(
                format!("<b>{}</b> {}", sender.display_name(), linkify(&body)),
                WithMentions::Yes(room),
            );
        }
    }

    fn build_text(&self, text: String, with_mentions: WithMentions) {
        let child = if let Some(Ok(child)) = self.child().map(|w| w.downcast::<LabelWithWidgets>())
        {
            child
        } else {
            let child = LabelWithWidgets::new();
            self.set_child(Some(&child));
            child
        };

        if EMOJI_REGEX.is_match(&text) {
            child.add_css_class("emoji");
        } else {
            child.remove_css_class("emoji");
        }

        if let WithMentions::Yes(room) = with_mentions {
            let (label, widgets) = extract_mentions(&text, room);
            child.set_use_markup(true);
            child.set_label(Some(label));
            child.set_widgets(widgets);
        } else {
            child.set_use_markup(false);
            child.set_widgets(Vec::<gtk::Widget>::new());
            child.set_label(Some(text));
        }
    }

    fn build_html(&self, blocks: Vec<HtmlBlock>, room: &Room) {
        let child = gtk::Box::new(gtk::Orientation::Vertical, 6);
        self.set_child(Some(&child));

        for block in blocks {
            let widget = create_widget_for_html_block(&block, room);
            child.append(&widget);
        }
    }
}

fn linkify(text: &str) -> String {
    markup_links(&html_escape(text))
}

fn is_valid_formatted_body(formatted: &FormattedBody) -> bool {
    formatted.format == MessageFormat::Html && !formatted.body.contains("<!-- raw HTML omitted -->")
}

fn parse_formatted_body(formatted: &str) -> Option<Vec<HtmlBlock>> {
    markup_html(formatted).ok()
}

fn create_widget_for_html_block(block: &HtmlBlock, room: &Room) -> gtk::Widget {
    match block {
        HtmlBlock::Heading(n, s) => {
            let (label, widgets) = extract_mentions(s, room);
            let w = LabelWithWidgets::with_label_and_widgets(&label, widgets);
            w.set_use_markup(true);
            w.add_css_class(&format!("h{}", n));
            w.upcast::<gtk::Widget>()
        }
        HtmlBlock::UList(elements) => {
            let bx = gtk::Box::new(gtk::Orientation::Vertical, 6);
            bx.set_margin_end(6);
            bx.set_margin_start(6);

            for li in elements.iter() {
                let h_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let bullet = gtk::Label::new(Some("•"));
                bullet.set_valign(gtk::Align::Start);
                let (label, widgets) = extract_mentions(li, room);
                let w = LabelWithWidgets::with_label_and_widgets(&label, widgets);
                w.set_use_markup(true);
                h_box.append(&bullet);
                h_box.append(&w);
                bx.append(&h_box);
            }

            bx.upcast::<gtk::Widget>()
        }
        HtmlBlock::OList(elements) => {
            let bx = gtk::Box::new(gtk::Orientation::Vertical, 6);
            bx.set_margin_end(6);
            bx.set_margin_start(6);

            for (i, ol) in elements.iter().enumerate() {
                let h_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let bullet = gtk::Label::new(Some(&format!("{}.", i + 1)));
                bullet.set_valign(gtk::Align::Start);
                let (label, widgets) = extract_mentions(ol, room);
                let w = LabelWithWidgets::with_label_and_widgets(&label, widgets);
                w.set_use_markup(true);
                h_box.append(&bullet);
                h_box.append(&w);
                bx.append(&h_box);
            }

            bx.upcast::<gtk::Widget>()
        }
        HtmlBlock::Code(s) => {
            let scrolled = gtk::ScrolledWindow::new();
            scrolled.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
            let buffer = sourceview::Buffer::new(None);
            buffer.set_highlight_matching_brackets(false);
            buffer.set_text(s);
            crate::utils::setup_style_scheme(&buffer);
            let view = sourceview::View::with_buffer(&buffer);
            view.set_editable(false);
            view.add_css_class("codeview");
            scrolled.set_child(Some(&view));
            scrolled.upcast::<gtk::Widget>()
        }
        HtmlBlock::Quote(blocks) => {
            let bx = gtk::Box::new(gtk::Orientation::Vertical, 6);
            bx.add_css_class("quote");
            for block in blocks.iter() {
                let w = create_widget_for_html_block(block, room);
                bx.append(&w);
            }
            bx.upcast::<gtk::Widget>()
        }
        HtmlBlock::Text(s) => {
            let (label, widgets) = extract_mentions(s, room);
            let w = LabelWithWidgets::with_label_and_widgets(&label, widgets);
            w.set_use_markup(true);
            w.upcast::<gtk::Widget>()
        }
    }
}

/// Remove the content between `mx-reply` tags.
///
/// Returns the unchanged string if none was found to be able to chain calls.
fn strip_reply(text: &str) -> &str {
    if let Some(end) = text.find("</mx-reply>") {
        if !text.starts_with("<mx-reply>") {
            warn!("Received a rich reply that doesn't start with '<mx-reply>'");
        }

        &text[end + 11..]
    } else {
        text
    }
}

/// Extract mentions from the given string.
///
/// Returns a new string with placeholders and the corresponding widgets.
fn extract_mentions(s: &str, room: &Room) -> (String, Vec<Pill>) {
    let session = room.session();
    let mut label = s.to_owned();
    let mut widgets = vec![];

    // The markup has been normalized by html2pango so we are sure of the format of
    // links.
    for (start, _) in s.rmatch_indices("<a href=") {
        let uri_start = start + 9;
        let link = &s[uri_start..];

        let uri_end = if let Some(end) = link.find('"') {
            end
        } else {
            continue;
        };

        let uri = &link[..uri_end];

        let id = if let Ok(mx_uri) = MatrixUri::parse(uri) {
            mx_uri.id().to_owned()
        } else if let Ok(mx_to_uri) = MatrixToUri::parse(uri) {
            mx_to_uri.id().to_owned()
        } else {
            continue;
        };

        let pill = match id {
            MatrixId::Room(room_id) => {
                if let Some(room) = session.room_list().get(&room_id) {
                    Pill::for_room(&room)
                } else {
                    continue;
                }
            }
            MatrixId::RoomAlias(room_alias) => {
                // TODO: Handle non-canonical aliases.
                if let Some(room) = session.client().rooms().iter().find_map(|matrix_room| {
                    matrix_room
                        .canonical_alias()
                        .filter(|alias| alias == &room_alias)
                        .and_then(|_| session.room_list().get(matrix_room.room_id()))
                }) {
                    Pill::for_room(&room)
                } else {
                    continue;
                }
            }
            MatrixId::User(user_id) => {
                let user = room.members().member_by_id(user_id).upcast();
                Pill::for_user(&user)
            }
            _ => continue,
        };

        let end = if let Some(end) = link.find("</a>") {
            uri_start + end + 4
        } else {
            continue;
        };

        label.replace_range(start..end, DEFAULT_PLACEHOLDER);
        widgets.insert(0, pill);
    }

    (label, widgets)
}

impl Default for MessageText {
    fn default() -> Self {
        Self::new()
    }
}

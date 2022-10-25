use gtk::{
    gdk, glib,
    glib::{clone, closure},
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
};
use pulldown_cmark::{Event, Parser, Tag};
use ruma::OwnedUserId;
use secular::lower_lay_string;

use super::CompletionRow;
use crate::{
    components::Pill,
    prelude::*,
    session::room::{Member, MemberList, Membership},
};

const MAX_MEMBERS: usize = 32;

#[derive(Debug, Default)]
pub struct MemberWatch {
    handlers: Vec<glib::SignalHandlerId>,
    // The position of the member in the list model.
    position: u32,
}

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };

    use glib::subclass::InitializingObject;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Fractal/content-completion-popover.ui")]
    pub struct CompletionPopover {
        #[template_child]
        pub list: TemplateChild<gtk::ListBox>,
        /// The user ID of the current session.
        pub user_id: RefCell<Option<String>>,
        /// The sorted and filtered room members.
        pub filtered_members: gtk::FilterListModel,
        /// The rows in the popover.
        pub rows: [CompletionRow; MAX_MEMBERS],
        /// The selected row in the popover.
        pub selected: Cell<Option<usize>>,
        /// The current autocompleted word.
        pub current_word: RefCell<Option<(gtk::TextIter, gtk::TextIter, String)>>,
        /// Whether the popover is inhibited for the current word.
        pub inhibit: Cell<bool>,
        /// The buffer to complete with its cursor position signal handler ID.
        pub buffer_handler: RefCell<Option<(gtk::TextBuffer, glib::SignalHandlerId)>>,
        /// The signal handler ID for when them members change.
        pub members_changed_handler: RefCell<Option<glib::SignalHandlerId>>,
        /// List of signal handler IDs for properties of members of the current
        /// list model with their position in it.
        pub members_watch: RefCell<HashMap<OwnedUserId, MemberWatch>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CompletionPopover {
        const NAME: &'static str = "ContentCompletionPopover";
        type Type = super::CompletionPopover;
        type ParentType = gtk::Popover;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CompletionPopover {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::new(
                        "view",
                        "View",
                        "The parent GtkTextView to autocomplete",
                        gtk::TextView::static_type(),
                        glib::ParamFlags::READABLE,
                    ),
                    glib::ParamSpecString::new(
                        "user-id",
                        "User ID",
                        "The user ID of the current session",
                        None,
                        glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY,
                    ),
                    glib::ParamSpecObject::new(
                        "members",
                        "Members",
                        "The room members used for completion",
                        MemberList::static_type(),
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecObject::new(
                        "filtered-members",
                        "Filtered Members",
                        "The sorted and filtered room members",
                        gtk::FilterListModel::static_type(),
                        glib::ParamFlags::READWRITE,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "user-id" => obj.set_user_id(value.get().unwrap()),
                "members" => obj.set_members(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "view" => obj.view().to_value(),
                "user-id" => obj.user_id().to_value(),
                "members" => obj.members().to_value(),
                "filtered-members" => obj.filtered_members().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Filter the members that are joined and that are not our user.
            let joined =
                gtk::BoolFilter::builder()
                    .expression(Member::this_expression("membership").chain_closure::<bool>(
                        closure!(|_obj: Option<glib::Object>, membership: Membership| {
                            membership == Membership::Join
                        }),
                    ))
                    .build();
            let not_user = gtk::BoolFilter::builder()
                .expression(gtk::ClosureExpression::new::<bool>(
                    &[
                        Member::this_expression("user-id"),
                        obj.property_expression("user-id"),
                    ],
                    closure!(
                        |_obj: Option<glib::Object>, user_id: &str, my_user_id: &str| {
                            user_id != my_user_id
                        }
                    ),
                ))
                .build();
            let filter = gtk::EveryFilter::new();
            filter.append(&joined);
            filter.append(&not_user);
            let first_model = gtk::FilterListModel::builder().filter(&filter).build();

            // Sort the members list by activity, then display name.
            let activity = gtk::NumericSorter::builder()
                .sort_order(gtk::SortType::Descending)
                .expression(Member::this_expression("latest-activity"))
                .build();
            let display_name = gtk::StringSorter::builder()
                .ignore_case(true)
                .expression(Member::this_expression("display-name"))
                .build();
            let sorter = gtk::MultiSorter::new();
            sorter.append(&activity);
            sorter.append(&display_name);
            let second_model = gtk::SortListModel::builder()
                .sorter(&sorter)
                .model(&first_model)
                .build();

            // Setup the search filter.
            let search = gtk::StringFilter::builder()
                .ignore_case(true)
                .match_mode(gtk::StringFilterMatchMode::Substring)
                .expression(gtk::ClosureExpression::new::<String>(
                    &[
                        Member::this_expression("user-id"),
                        Member::this_expression("display-name"),
                    ],
                    closure!(
                        |_: Option<glib::Object>, user_id: &str, display_name: &str| {
                            lower_lay_string(&format!("{display_name} {user_id}"))
                        }
                    ),
                ))
                .build();
            self.filtered_members.set_filter(Some(&search));
            self.filtered_members.set_model(Some(&second_model));

            for row in &self.rows {
                self.list.append(row);
            }

            obj.connect_parent_notify(|obj| {
                let priv_ = obj.imp();

                if let Some((buffer, handler_id)) = priv_.buffer_handler.take() {
                    buffer.disconnect(handler_id);
                }

                if obj.parent().is_some() {
                    let view = obj.view();
                    let buffer = view.buffer();
                    let handler_id =
                        buffer.connect_cursor_position_notify(clone!(@weak obj => move |_| {
                            obj.update_completion(false);
                        }));
                    priv_.buffer_handler.replace(Some((buffer, handler_id)));

                    let key_events = gtk::EventControllerKey::new();
                    view.add_controller(&key_events);
                    key_events.connect_key_pressed(clone!(@weak obj => @default-return glib::signal::Inhibit(false), move |_, key, _, modifier| {
                        if modifier.is_empty() {
                            if obj.is_visible() {
                                let priv_ = obj.imp();
                                if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::Tab) {
                                    // Activate completion.
                                    obj.activate_selected_row();
                                    return glib::signal::Inhibit(true);
                                } else if matches!(key, gdk::Key::Up | gdk::Key::KP_Up) {
                                    // Move up, if possible.
                                    let idx = obj.selected_row_index().unwrap_or_default();
                                    if idx > 0 {
                                        obj.select_row_at_index(Some(idx - 1));
                                    }
                                    return glib::signal::Inhibit(true);
                                } else if matches!(key, gdk::Key::Down | gdk::Key::KP_Down) {
                                    // Move down, if possible.
                                    let new_idx = if let Some(idx) = obj.selected_row_index() {
                                        idx + 1
                                    } else {
                                        0
                                    };
                                    let n_members = priv_.filtered_members.n_items() as usize;
                                    let max = MAX_MEMBERS.min(n_members);
                                    if new_idx < max {
                                        obj.select_row_at_index(Some(new_idx));
                                    }
                                    return glib::signal::Inhibit(true);
                                } else if matches!(key, gdk::Key::Escape) {
                                    // Close.
                                    obj.inhibit();
                                    return glib::signal::Inhibit(true);
                                }
                            } else if matches!(key, gdk::Key::Tab) {
                                obj.update_completion(true);
                                return glib::signal::Inhibit(true);
                            }
                        }
                        glib::signal::Inhibit(false)
                    }));

                    // Close popup when the entry is not focused.
                    view.connect_has_focus_notify(clone!(@weak obj => move |view| {
                        if !view.has_focus() && obj.get_visible() {
                            obj.hide();
                        }
                    }));
                }
            });

            self.list
                .connect_row_activated(clone!(@weak obj => move |_, row| {
                    if let Some(row) = row.downcast_ref::<CompletionRow>() {
                        obj.row_activated(row);
                    }
                }));
        }
    }

    impl WidgetImpl for CompletionPopover {}
    impl PopoverImpl for CompletionPopover {}
}

glib::wrapper! {
    /// A popover to autocomplete Matrix IDs for its parent `gtk::TextView`.
    pub struct CompletionPopover(ObjectSubclass<imp::CompletionPopover>)
        @extends gtk::Widget, gtk::Popover;
}

impl CompletionPopover {
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    pub fn view(&self) -> gtk::TextView {
        self.parent()
            .and_then(|parent| parent.downcast::<gtk::TextView>().ok())
            .unwrap()
    }

    pub fn user_id(&self) -> Option<String> {
        self.imp().user_id.borrow().clone()
    }

    pub fn set_user_id(&self, user_id: Option<String>) {
        let priv_ = self.imp();

        if priv_.user_id.borrow().as_ref() == user_id.as_ref() {
            return;
        }

        priv_.user_id.replace(user_id);
        self.notify("user-id");
    }

    fn first_model(&self) -> Option<gtk::FilterListModel> {
        self.imp()
            .filtered_members
            .model()
            .and_then(|model| model.downcast::<gtk::SortListModel>().ok())
            .and_then(|second_model| second_model.model())
            .and_then(|model| model.downcast::<gtk::FilterListModel>().ok())
    }

    pub fn members(&self) -> Option<MemberList> {
        self.first_model()
            .and_then(|first_model| first_model.model())
            .and_then(|model| model.downcast::<MemberList>().ok())
    }

    pub fn set_members(&self, members: Option<&MemberList>) {
        if let Some(first_model) = self.first_model() {
            let priv_ = self.imp();

            if let Some(old_members) = first_model.model() {
                // Remove the old handlers.
                if let Some(handler_id) = priv_.members_changed_handler.take() {
                    old_members.disconnect(handler_id);
                }

                let mut members_watch = priv_.members_watch.take();
                for member in old_members
                    .snapshot()
                    .into_iter()
                    .filter_map(|obj| obj.downcast::<Member>().ok())
                {
                    if let Some(watch) = members_watch.remove(&member.user_id()) {
                        for handler_id in watch.handlers {
                            member.disconnect(handler_id);
                        }
                    }
                }
            }

            first_model.set_model(members);

            if let Some(members) = members {
                self.members_changed(members, 0, 0, members.n_items());

                priv_
                    .members_changed_handler
                    .replace(Some(members.connect_items_changed(
                        clone!(@weak self as obj => move |members, pos, removed, added| {
                            obj.members_changed(members, pos, removed, added);
                        }),
                    )));
            }
        }
    }

    fn members_changed(&self, members: &MemberList, pos: u32, removed: u32, added: u32) {
        let mut members_watch = self.imp().members_watch.borrow_mut();

        // Remove the old members. We don't care about disconnecting them since
        // they are gone.
        for idx in pos..(pos + removed) {
            let user_id = members_watch
                .iter()
                .find_map(|(user_id, watch)| (watch.position == idx).then(|| user_id.to_owned()));

            if let Some(user_id) = user_id {
                members_watch.remove(&user_id);
            }
        }

        let upper_bound = if removed != added {
            members.n_items()
        } else {
            // If there are as many removed as added, we don't need to update
            // the position of the following members.
            pos + added
        };
        for idx in pos..upper_bound {
            if let Some(member) = members
                .item(idx)
                .and_then(|obj| obj.downcast::<Member>().ok())
            {
                let watch = members_watch.entry(member.user_id()).or_default();

                // Update the position of all members after pos.
                watch.position = idx;

                // Listen to property changes for added members because the list
                // models don't reevaluate the expressions when the membership
                // or latest-activity change.
                if idx < (pos + added) {
                    watch.handlers.push(member.connect_notify_local(
                        Some("membership"),
                        clone!(@weak self as obj => move |member, _| {
                            obj.reevaluate_member(member);
                        }),
                    ));
                    watch.handlers.push(member.connect_notify_local(
                        Some("latest-activity"),
                        clone!(@weak self as obj => move |member, _| {
                            obj.reevaluate_member(member);
                        }),
                    ));
                }
            }
        }
    }

    /// Force the given member to be reevaluated.
    fn reevaluate_member(&self, member: &Member) {
        if let Some(members) = self.members() {
            let pos = self
                .imp()
                .members_watch
                .borrow()
                .get(&member.user_id())
                .map(|watch| watch.position);

            if let Some(pos) = pos {
                // We pretend the item changed to get it reevaluated.
                members.items_changed(pos, 1, 1)
            }
        }
    }

    pub fn filtered_members(&self) -> &gtk::FilterListModel {
        &self.imp().filtered_members
    }

    fn current_word(&self) -> Option<(gtk::TextIter, gtk::TextIter, String)> {
        self.imp().current_word.borrow().clone()
    }

    fn set_current_word(&self, word: Option<(gtk::TextIter, gtk::TextIter, String)>) {
        if self.current_word() == word {
            return;
        }

        self.imp().current_word.replace(word);
    }

    /// Update completion.
    ///
    /// If trigger is `true`, the search term will not look for `@` at the start
    /// of the word.
    fn update_completion(&self, trigger: bool) {
        let search = self.find_search_term(trigger);

        if self.is_inhibited() && search.is_none() {
            self.imp().inhibit.set(false);
        } else if !self.is_inhibited() {
            if let Some((start, end, term)) = search {
                self.set_current_word(Some((start, end, term)));
                self.search_members();
            } else {
                self.hide();
                self.select_row_at_index(None);
                self.set_current_word(None);
            }
        }
    }

    /// Find the current search term in the underlying buffer.
    ///
    /// Returns the start and end of the search word and the term to search for.
    ///
    /// If trigger is `true`, the search term will not look for `@` at the start
    /// of the word.
    fn find_search_term(&self, trigger: bool) -> Option<(gtk::TextIter, gtk::TextIter, String)> {
        // Vocabular used in this method:
        // - `word`: sequence of characters that form a valid ID or display name. This
        //   includes characters that are usually not considered to be in words because
        //   of the grammar of Matrix IDs.
        // - `trigger`: character used to trigger the popover, usually the first
        //   character of the corresponding ID.

        #[derive(Default)]
        struct SearchContext {
            localpart: String,
            is_outside_ascii: bool,
            has_id_separator: bool,
            server_name: ServerNameContext,
            has_port_separator: bool,
            port: String,
        }

        enum ServerNameContext {
            Ipv6(String),
            // According to the Matrix spec definition, the IPv4 grammar is a
            // subset of the domain name grammar.
            Ipv4OrDomain(String),
            Unknown,
        }
        impl Default for ServerNameContext {
            fn default() -> Self {
                Self::Unknown
            }
        }

        fn is_possible_word_char(c: char) -> bool {
            c.is_alphanumeric() || matches!(c, '.' | '_' | '=' | '-' | '/' | ':' | '[' | ']' | '@')
        }

        let buffer = self.view().buffer();
        let cursor = buffer.iter_at_mark(&buffer.get_insert());

        let mut word_start = cursor;
        // Search for the beginning of the word.
        while word_start.backward_cursor_position() {
            let c = word_start.char();
            if !is_possible_word_char(c) {
                word_start.forward_cursor_position();
                break;
            }
        }

        if word_start.char() != '@'
            && !trigger
            && (cursor == word_start || self.current_word().is_none())
        {
            // No trigger or not updating the word.
            return None;
        }

        let mut ctx = SearchContext::default();
        let mut word_end = word_start;
        while word_end.forward_cursor_position() {
            let c = word_end.char();
            if !ctx.has_id_separator {
                // Localpart or display name.
                if !ctx.is_outside_ascii
                    && (c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '=' | '-' | '/'))
                {
                    ctx.localpart.push(c);
                } else if c.is_alphanumeric() {
                    ctx.is_outside_ascii = true;
                } else if !ctx.is_outside_ascii && c == ':' {
                    ctx.has_id_separator = true;
                } else {
                    break;
                }
            } else {
                // The server name of an ID.
                if !ctx.has_port_separator {
                    // An IPv6 address, IPv4 address, or a domain name.
                    if matches!(ctx.server_name, ServerNameContext::Unknown) {
                        if c == '[' {
                            ctx.server_name = ServerNameContext::Ipv6(c.into())
                        } else if c.is_alphanumeric() {
                            ctx.server_name = ServerNameContext::Ipv4OrDomain(c.into())
                        } else {
                            break;
                        }
                    } else if let ServerNameContext::Ipv6(address) = &mut ctx.server_name {
                        if address.ends_with(']') {
                            if c == ':' {
                                ctx.has_port_separator = true;
                            } else {
                                break;
                            }
                        } else if address.len() > 46 {
                            break;
                        } else if c.is_ascii_hexdigit() || matches!(c, ':' | '.' | ']') {
                            address.push(c);
                        } else {
                            break;
                        }
                    } else if let ServerNameContext::Ipv4OrDomain(address) = &mut ctx.server_name {
                        if c == ':' {
                            ctx.has_port_separator = true;
                        } else if c.is_ascii_alphanumeric() || matches!(c, '-' | '.') {
                            address.push(c);
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                } else {
                    // The port number
                    if ctx.port.len() <= 5 && c.is_ascii_digit() {
                        ctx.port.push(c);
                    } else {
                        break;
                    }
                }
            }
        }

        if cursor != word_end && !cursor.in_range(&word_start, &word_end) {
            return None;
        }

        if self.in_escaped_markdown(&word_start, &word_end) {
            return None;
        }

        // Remove the starting `@` for searching.
        let mut term_start = word_start;
        if term_start.char() == '@' {
            term_start.forward_cursor_position();
        }

        let term = buffer.text(&term_start, &word_end, true);

        // If the cursor jumped to another word, abort the completion.
        if let Some((_, _, prev_term)) = self.current_word() {
            if !term.contains(&prev_term) && !prev_term.contains(term.as_str()) {
                return None;
            }
        }

        Some((word_start, word_end, term.into()))
    }

    /// Check if the text is in markdown that would be escaped.
    ///
    /// This includes:
    /// - Inline code
    /// - Block code
    /// - Links (because nested links are not allowed in HTML)
    /// - Images
    fn in_escaped_markdown(&self, word_start: &gtk::TextIter, word_end: &gtk::TextIter) -> bool {
        let buffer = self.view().buffer();
        let (buf_start, buf_end) = buffer.bounds();

        // If the word is at the start or the end of the buffer, it cannot be escaped.
        if *word_start == buf_start || *word_end == buf_end {
            return false;
        }

        let text = buffer.slice(&buf_start, &buf_end, true);

        // Find the word string slice indexes, because GtkTextIter only gives us
        // the char offset but the parser gives us indexes.
        let word_start_offset = word_start.offset() as usize;
        let word_end_offset = word_end.offset() as usize;
        let mut word_start_index = 0;
        let mut word_end_index = 0;
        if word_start_offset != 0 && word_end_offset != 0 {
            for (offset, (index, _char)) in text.char_indices().enumerate() {
                if word_start_offset == offset {
                    word_start_index = index;
                }
                if word_end_offset == offset {
                    word_end_index = index;
                }

                if word_start_index != 0 && word_end_index != 0 {
                    break;
                }
            }
        }

        // Look if word is in escaped markdown.
        let mut in_escaped_tag = false;
        for (event, range) in Parser::new(&text).into_offset_iter() {
            match event {
                Event::Start(tag) => {
                    in_escaped_tag =
                        matches!(tag, Tag::CodeBlock(_) | Tag::Link(..) | Tag::Image(..));
                }
                Event::End(_) => {
                    // A link or a code block only contains text so an end tag
                    // always means the end of an escaped part.
                    in_escaped_tag = false;
                }
                Event::Code(_) if range.contains(&word_start_index) => {
                    return true;
                }
                Event::Text(_) if in_escaped_tag && range.contains(&word_start_index) => {
                    return true;
                }
                _ => {}
            }

            if range.end <= word_end_index {
                break;
            }
        }

        false
    }

    fn search_members(&self) {
        let priv_ = self.imp();
        let filtered_members = self.filtered_members();
        let filter = filtered_members
            .filter()
            .and_then(|filter| filter.downcast::<gtk::StringFilter>().ok())
            .unwrap();
        let term = self
            .current_word()
            .and_then(|(_, _, term)| (!term.is_empty()).then(|| lower_lay_string(&term)));
        filter.set_search(term.as_deref());

        let new_len = filtered_members.n_items();
        if new_len == 0 {
            self.hide();
            self.select_row_at_index(None);
        } else {
            for (idx, row) in priv_.rows.iter().enumerate() {
                if let Some(member) = filtered_members
                    .item(idx as u32)
                    .and_then(|obj| obj.downcast::<Member>().ok())
                {
                    row.set_member(Some(member));
                    row.show();
                } else if row.get_visible() {
                    row.hide();
                } else {
                    // All remaining rows should be hidden too.
                    break;
                }
            }

            self.update_pointing_to();
            self.popup();
        }
    }

    fn count_visible_rows(&self) -> usize {
        self.imp()
            .rows
            .iter()
            .filter(|row| row.get_visible())
            .fuse()
            .count()
    }

    fn popup(&self) {
        if self
            .selected_row_index()
            .filter(|index| *index < self.count_visible_rows())
            .is_none()
        {
            self.select_row_at_index(Some(0));
        }
        self.show()
    }

    fn update_pointing_to(&self) {
        let view = self.view();
        let (start, ..) = self.current_word().unwrap();
        let location = view.iter_location(&start);
        let (x, y) =
            view.buffer_to_window_coords(gtk::TextWindowType::Widget, location.x(), location.y());
        self.set_pointing_to(Some(&gdk::Rectangle::new(x - 6, y - 2, 0, 0)));
    }

    fn selected_row_index(&self) -> Option<usize> {
        self.imp().selected.get()
    }

    fn select_row_at_index(&self, idx: Option<usize>) {
        if self.selected_row_index() == idx || idx >= Some(self.count_visible_rows()) {
            return;
        }

        let priv_ = self.imp();

        if let Some(row) = idx.map(|idx| &priv_.rows[idx]) {
            // Make sure the row is visible.
            let row_bounds = row.compute_bounds(&*priv_.list).unwrap();
            let lower = row_bounds.top_left().y() as f64;
            let upper = row_bounds.bottom_left().y() as f64;
            priv_.list.adjustment().unwrap().clamp_page(lower, upper);

            priv_.list.select_row(Some(row));
        } else {
            priv_.list.select_row(gtk::ListBoxRow::NONE);
        }
        priv_.selected.set(idx);
    }

    fn activate_selected_row(&self) {
        if let Some(idx) = self.selected_row_index() {
            self.imp().rows[idx].activate();
        } else {
            self.inhibit();
        }
    }

    fn row_activated(&self, row: &CompletionRow) {
        if let Some(member) = row.member() {
            let priv_ = self.imp();

            if let Some((mut start, mut end, _)) = priv_.current_word.take() {
                let view = self.view();
                let buffer = view.buffer();

                buffer.delete(&mut start, &mut end);

                let anchor = match start.child_anchor() {
                    Some(anchor) => anchor,
                    None => buffer.create_child_anchor(&mut start),
                };
                let pill = Pill::for_user(member.upcast_ref());
                view.add_child_at_anchor(&pill, &anchor);

                self.hide();
                self.select_row_at_index(None);
                view.grab_focus();
            }
        }
    }

    fn is_inhibited(&self) -> bool {
        self.imp().inhibit.get()
    }

    fn inhibit(&self) {
        if !self.is_inhibited() {
            self.imp().inhibit.set(true);
            self.hide();
            self.select_row_at_index(None);
        }
    }
}

impl Default for CompletionPopover {
    fn default() -> Self {
        Self::new()
    }
}

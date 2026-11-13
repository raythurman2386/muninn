//! Kanban slice 13: board find, filters and checklist actions.
//!
//! - **Board Find** (`⌘F` / `Ctrl+F` on the Kanban page) searches the live
//!   board's cards, case-insensitively, and outlines the current match's
//!   card. `Enter` / `↓` step forward, `Shift+Enter` / `↑` step back, and
//!   `Esc` closes the bar and puts the selection back where it was. It reads
//!   the parsed board, not the Write buffer, so unsaved card edits are found
//!   at once. It never writes.
//! - **Filter** (the header's Filter button) hides cards that do not match a
//!   text query, a tag, a date (`today`, `overdue`, or a `YYYY-MM-DD` range)
//!   and/or a done state. Filters compose with AND. They are display state
//!   only: the board, its file and its undo history are untouched, and they
//!   are not saved in `settings.json`. The header shows what is active and a
//!   Clear control.
//! - A tag chip's click (`kanban_tags.rs`) sets the tag filter, so the chip
//!   and the filter menu agree.
//! - A nested checklist item is toggled through the core `toggle_card_task`,
//!   which changes exactly that one character through the card edit path, so
//!   it is one undo step and the card's own checkbox is unchanged.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use muninn_core::{Card, CivilDate, DateSettings, format_date, scan_card_text};
use rune::{El, Key, KeyEvent, TextEdit, Tx, col, row, text, text_input};
use rune_kit::Kit;
use rune_theme_opencode::icons as ic;

use crate::{Mode, State};

/// One card-text match in the shown board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KanbanMatch {
    pub lane: usize,
    pub card: usize,
    /// Byte range in the card's text.
    pub start: usize,
    pub end: usize,
}

/// Board Find on the Kanban page.
#[derive(Clone, Debug, Default)]
pub struct KanbanFindState {
    /// The bar's single-line field; focused while the bar is open.
    pub field: TextEdit,
    /// The query the matches were computed for (mirrors `field.text`).
    pub query: String,
    /// Every match, in lane/card order.
    pub matches: Vec<KanbanMatch>,
    /// Index of the current match in `matches`.
    pub index: usize,
    /// The board text the matches were computed against.
    pub(crate) hay: String,
    /// The board the matches belong to.
    pub(crate) rel: String,
    /// The filter the matches were computed under.
    pub(crate) filter: u64,
    /// The selection to put back when the bar closes.
    pub(crate) saved_selection: Option<(usize, usize)>,
}

/// The date shortcut in the filter menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KanbanDatePreset {
    Today,
    Overdue,
}

/// An inclusive `YYYY-MM-DD` date range; either end may be absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KanbanDateRange {
    pub from: Option<CivilDate>,
    pub to: Option<CivilDate>,
}

/// The resolved date part of a filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KanbanDateFilter {
    Today,
    Overdue,
    Range(KanbanDateRange),
}

/// The board's ephemeral filter state.
///
/// The four text fields are also the filter's values, so typing in the
/// Filter panel and the cards on screen cannot disagree. Nothing here is
/// saved; a board edit, save or undo does not read or write it.
#[derive(Clone, Debug, Default)]
pub struct KanbanFilter {
    /// The Filter panel is open.
    pub open: bool,
    /// Case-insensitive card-text query.
    pub text: TextEdit,
    /// A tag, without its `#`.
    pub tag: TextEdit,
    /// Inclusive range start, `YYYY-MM-DD`.
    pub from: TextEdit,
    /// Inclusive range end, `YYYY-MM-DD`.
    pub to: TextEdit,
    /// `today` / `overdue`; takes precedence over the range fields.
    pub preset: Option<KanbanDatePreset>,
    /// `Some(false)` open only, `Some(true)` done only.
    pub done: Option<bool>,
}

impl KanbanFilter {
    /// Whether any filter would hide a card.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.text.text.trim().is_empty()
            || self.tag_text().is_some()
            || self.date_filter().is_some()
            || self.done.is_some()
    }

    /// The tag's text without `#`, or `None` for an empty field.
    #[must_use]
    pub fn tag_text(&self) -> Option<&str> {
        let tag = self.tag.text.trim().trim_start_matches('#');
        (!tag.is_empty()).then_some(tag)
    }

    /// The date condition, or `None` for every date.
    #[must_use]
    pub fn date_filter(&self) -> Option<KanbanDateFilter> {
        if let Some(preset) = self.preset {
            return Some(match preset {
                KanbanDatePreset::Today => KanbanDateFilter::Today,
                KanbanDatePreset::Overdue => KanbanDateFilter::Overdue,
            });
        }
        let range = self.date_range()?;
        Some(KanbanDateFilter::Range(range))
    }

    /// The typed range, when at least one end parses. A range with a
    /// malformed end is ignored rather than guessed at.
    #[must_use]
    pub fn date_range(&self) -> Option<KanbanDateRange> {
        let from_raw = self.from.text.trim();
        let to_raw = self.to.text.trim();
        if from_raw.is_empty() && to_raw.is_empty() {
            return None;
        }
        let from = if from_raw.is_empty() {
            None
        } else {
            Some(filter_date(from_raw)?)
        };
        let to = if to_raw.is_empty() {
            None
        } else {
            Some(filter_date(to_raw)?)
        };
        Some(KanbanDateRange { from, to })
    }

    /// A typed date end that is not `YYYY-MM-DD`; the panel says so and the
    /// range is not applied.
    #[must_use]
    pub fn date_error(&self) -> bool {
        [self.from.text.trim(), self.to.text.trim()]
            .into_iter()
            .any(|value| !value.is_empty() && filter_date(value).is_none())
    }

    /// What the header's summary says, or `None` when nothing is filtered.
    #[must_use]
    pub fn summary(&self) -> Option<String> {
        let mut parts = Vec::new();
        let query = self.text.text.trim();
        if !query.is_empty() {
            parts.push(format!("“{query}”"));
        }
        if let Some(tag) = self.tag_text() {
            parts.push(format!("#{tag}"));
        }
        match self.date_filter() {
            Some(KanbanDateFilter::Today) => parts.push("today".to_owned()),
            Some(KanbanDateFilter::Overdue) => parts.push("overdue".to_owned()),
            Some(KanbanDateFilter::Range(range)) => parts.push(match (range.from, range.to) {
                (Some(from), Some(to)) => format!("{} – {}", day(from), day(to)),
                (Some(from), None) => format!("from {}", day(from)),
                (None, Some(to)) => format!("to {}", day(to)),
                (None, None) => String::new(),
            }),
            None => {}
        }
        match self.done {
            Some(false) => parts.push("open".to_owned()),
            Some(true) => parts.push("done".to_owned()),
            None => {}
        }
        parts.retain(|part| !part.is_empty());
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    /// Whether a card's text passes every active condition.
    #[must_use]
    pub fn matches(&self, text: &str, done: bool, dates: &DateSettings, today: CivilDate) -> bool {
        let query = self.text.text.trim();
        if !query.is_empty() && !text.to_lowercase().contains(&query.to_lowercase()) {
            return false;
        }
        let found = scan_card_text(text, dates);
        if let Some(tag) = self.tag_text()
            && !found
                .tags
                .iter()
                .any(|token| token.tag.eq_ignore_ascii_case(tag))
        {
            return false;
        }
        if let Some(want) = self.done
            && want != done
        {
            return false;
        }
        match self.date_filter() {
            Some(filter) => {
                let Some(token) = found.date else {
                    return false;
                };
                let Some(date) = dates.parse_date(&token.value) else {
                    return false;
                };
                match filter {
                    KanbanDateFilter::Today => date == today,
                    KanbanDateFilter::Overdue => date < today,
                    KanbanDateFilter::Range(range) => {
                        range.from.is_none_or(|from| date >= from)
                            && range.to.is_none_or(|to| date <= to)
                    }
                }
            }
            None => true,
        }
    }

    /// Clear every condition, leaving the panel as open as it was.
    pub fn clear(&mut self) {
        for field in [&mut self.text, &mut self.tag, &mut self.from, &mut self.to] {
            field.replace_text("");
        }
        self.preset = None;
        self.done = None;
    }

    /// Set the tag condition from a chip or a caller; keeps any other
    /// conditions so filters compose.
    pub fn set_tag(&mut self, tag: &str) {
        let tag = tag.trim().trim_start_matches('#');
        self.tag.replace_text(tag);
        let end = self.tag.text.len();
        self.tag.set_caret(end);
    }
}

/// Parse a filter's `YYYY-MM-DD` date.
#[must_use]
pub fn filter_date(text: &str) -> Option<CivilDate> {
    let text = text.trim();
    let mut parts = text.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    CivilDate::new(year, month, day)
}

fn day(date: CivilDate) -> String {
    format_date(date, "YYYY-MM-DD")
}

impl State {
    /// The board's filter. It exists for the whole session and is never
    /// written to a file or to `settings.json`.
    #[must_use]
    pub fn kanban_filter(&self) -> &KanbanFilter {
        &self.kanban_filter
    }

    /// Whether any filter condition would hide a card.
    #[must_use]
    pub fn kanban_filter_active(&self) -> bool {
        self.kanban_filter.is_active()
    }

    /// Whether a card passes the filter with the frame's resolved settings.
    #[must_use]
    pub fn kanban_filter_matches(
        &self,
        text: &str,
        done: bool,
        dates: &DateSettings,
        today: CivilDate,
    ) -> bool {
        self.kanban_filter.matches(text, done, dates, today)
    }

    /// Whether `card` passes the filter with the frame's settings.
    #[must_use]
    pub fn kanban_filter_matches_card(
        &self,
        card: &Card,
        dates: &DateSettings,
        today: CivilDate,
    ) -> bool {
        self.kanban_filter_matches(card.text(), card.is_done(), dates, today)
    }

    /// The number of cards the filter shows and the board's total, for the
    /// header's summary.
    #[must_use]
    pub fn kanban_filter_counts(&self) -> (usize, usize) {
        let Some(board) = self.kanban_board() else {
            return (0, 0);
        };
        let dates = self.kanban_date_settings();
        let today = crate::kanban_dates::local_today();
        let mut shown = 0;
        let mut total = 0;
        for lane in board.lanes() {
            for card in lane.cards() {
                total += 1;
                if self
                    .kanban_filter
                    .matches(card.text(), card.is_done(), &dates, today)
                {
                    shown += 1;
                }
            }
        }
        (shown, total)
    }

    /// A hash of the filter's values, so a lane's memo rebuilds when it
    /// changes.
    #[must_use]
    pub fn kanban_filter_hash(&self) -> u64 {
        let filter = &self.kanban_filter;
        let mut h = DefaultHasher::new();
        filter.text.text.hash(&mut h);
        filter.tag.text.hash(&mut h);
        filter.from.text.hash(&mut h);
        filter.to.text.hash(&mut h);
        filter.preset.hash(&mut h);
        filter.done.hash(&mut h);
        h.finish()
    }

    /// A hash of Board Find's visible state, so a lane's memo rebuilds when
    /// the current match changes its outline.
    #[must_use]
    pub fn kanban_find_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        if let Some(find) = &self.kanban_find {
            true.hash(&mut h);
            find.query.hash(&mut h);
            find.index.hash(&mut h);
            find.matches.hash(&mut h);
        }
        h.finish()
    }

    /// Whether any filter field owns the keyboard.
    #[must_use]
    pub fn kanban_filter_field_focused(&self) -> bool {
        self.kanban_filter.text.focused
            || self.kanban_filter.tag.focused
            || self.kanban_filter.from.focused
            || self.kanban_filter.to.focused
    }

    /// Drop focus from the filter fields (a choice button was pressed).
    pub fn blur_kanban_filter_fields(&mut self) {
        self.kanban_filter.text.focused = false;
        self.kanban_filter.tag.focused = false;
        self.kanban_filter.from.focused = false;
        self.kanban_filter.to.focused = false;
    }

    /// The header's Filter button.
    pub fn toggle_kanban_filter(&mut self) {
        self.kanban_filter.open = !self.kanban_filter.open;
        if !self.kanban_filter.open {
            self.blur_kanban_filter_fields();
        }
        self.menu = None;
    }

    /// Close the panel without clearing its conditions.
    pub fn close_kanban_filter(&mut self) {
        self.kanban_filter.open = false;
        self.blur_kanban_filter_fields();
    }

    /// The header's Clear control.
    pub fn clear_kanban_filter(&mut self) {
        self.kanban_filter.clear();
        self.blur_kanban_filter_fields();
    }

    /// A tag chip's click: set the tag condition and close Board Find if it
    /// was open, so the chip and the header's summary agree.
    pub fn set_kanban_tag_filter(&mut self, tag: &str) {
        self.kanban_filter.set_tag(tag);
        self.close_kanban_find();
    }

    /// `today` / `overdue` / no date preset.
    pub fn set_kanban_date_preset(&mut self, preset: Option<KanbanDatePreset>) {
        self.kanban_filter.preset = preset;
        if preset.is_some() {
            self.kanban_filter.from.replace_text("");
            self.kanban_filter.to.replace_text("");
        }
        self.blur_kanban_filter_fields();
    }

    /// Open only, done only, or either.
    pub fn set_kanban_done_filter(&mut self, done: Option<bool>) {
        self.kanban_filter.done = done;
        self.blur_kanban_filter_fields();
    }

    /// Keep the selected card on a visible one while a filter is active.
    /// Returns true when the selection moved.
    pub fn kanban_filter_tick(&mut self) -> bool {
        if self.mode != Mode::Kanban || !self.kanban_filter_active() {
            return false;
        }
        let Some(board) = self.kanban_board() else {
            return false;
        };
        let dates = self.kanban_date_settings();
        let today = crate::kanban_dates::local_today();
        let shows = |card: &Card| {
            self.kanban_filter
                .matches(card.text(), card.is_done(), &dates, today)
        };
        if let Some((lane, index)) = self.kanban_selected
            && board
                .lanes()
                .get(lane)
                .and_then(|lane| lane.cards().get(index))
                .is_some_and(shows)
        {
            return false;
        }
        let mut first = None;
        'lanes: for (lane, section) in board.lanes().iter().enumerate() {
            for (index, card) in section.cards().iter().enumerate() {
                if shows(card) {
                    first = Some((lane, index));
                    break 'lanes;
                }
            }
        }
        if first == self.kanban_selected {
            return false;
        }
        self.kanban_selected = first;
        true
    }

    /// `⌘F` on the Kanban page: open the board's Find bar, or refocus it.
    pub fn open_kanban_find(&mut self) {
        if self.kanban.is_none() {
            return;
        }
        self.blur_kanban_filter_fields();
        self.close_picker();
        let saved = self
            .kanban_find
            .as_ref()
            .map_or(self.kanban_selected, |find| find.saved_selection);
        if let Some(find) = self.kanban_find.as_mut() {
            find.field.focused = true;
        } else {
            let mut field = TextEdit::new("");
            field.focused = true;
            self.kanban_find = Some(KanbanFindState {
                field,
                saved_selection: saved,
                ..KanbanFindState::default()
            });
        }
        self.editor.focused = false;
        self.search.focused = false;
        // The card and lane fields are not mounted while the bar has the
        // keyboard; their open text stays, as slice 4 keeps it.
        self.kanban_card_field.focused = false;
        self.kanban_lane_field.focused = false;
        self.kanban_find_tick();
    }

    /// `Esc`: close Board Find and put the selection back where it was.
    pub fn close_kanban_find(&mut self) {
        let Some(find) = self.kanban_find.take() else {
            return;
        };
        let saved = find.saved_selection.filter(|&(lane, index)| {
            self.kanban_board()
                .and_then(|board| board.lanes().get(lane))
                .is_some_and(|lane| index < lane.cards().len())
        });
        self.kanban_selected = saved;
        // The note editor is not mounted on the Kanban page; keep the page's
        // arrows and Alt+arrows alive as slice 10's fix does.
        self.set_text_focus(false, false);
    }

    pub fn kanban_find_next(&mut self) {
        self.kanban_find_step(1);
    }

    pub fn kanban_find_prev(&mut self) {
        self.kanban_find_step(-1);
    }

    fn kanban_find_step(&mut self, delta: isize) {
        self.kanban_find_tick();
        let Some(find) = self.kanban_find.as_mut() else {
            return;
        };
        if find.matches.is_empty() {
            return;
        }
        let len = find.matches.len() as isize;
        find.index = (find.index as isize + delta).rem_euclid(len) as usize;
        let current = find.matches[find.index];
        self.kanban_selected = Some((current.lane, current.card));
    }

    /// The current match's card, and whether the card holds any match.
    /// `(has_match, is_current)`.
    #[must_use]
    pub fn kanban_find_mark(&self, lane: usize, card: usize) -> (bool, bool) {
        let Some(find) = self.kanban_find.as_ref() else {
            return (false, false);
        };
        let mut any = false;
        let mut current = false;
        for (index, found) in find.matches.iter().enumerate() {
            if found.lane == lane && found.card == card {
                any = true;
                current |= index == find.index;
            }
        }
        (any, current)
    }

    /// Recompute Board Find's matches when the query, the board or the
    /// visible cards changed. Runs from `poll`; the view only borrows state.
    pub fn kanban_find_tick(&mut self) -> bool {
        let Some(find) = self.kanban_find.as_ref() else {
            return false;
        };
        let query = find.field.text.clone();
        let Some(shown) = self.kanban.as_ref() else {
            return false;
        };
        let (rel, source) = match shown.board.as_ref() {
            Some(board) => (shown.rel.clone(), board.source().to_owned()),
            None => (shown.rel.clone(), shown.saved.clone()),
        };
        let filter = self.kanban_filter_hash();
        if find.query == query && find.hay == source && find.rel == rel && find.filter == filter {
            return false;
        }
        let dates = self.kanban_date_settings();
        let today = crate::kanban_dates::local_today();
        let mut matches = Vec::new();
        if let Some(board) = shown.board.as_ref()
            && !query.trim().is_empty()
        {
            for (lane, section) in board.lanes().iter().enumerate() {
                for (card, item) in section.cards().iter().enumerate() {
                    if !self.kanban_filter_matches_card(item, &dates, today) {
                        continue;
                    }
                    let (starts, ends) = crate::state::find_matches(item.text(), &query);
                    for (start, end) in starts.into_iter().zip(ends) {
                        matches.push(KanbanMatch {
                            lane,
                            card,
                            start,
                            end,
                        });
                    }
                }
            }
        }
        let first = matches.first().map(|found| (found.lane, found.card));
        let changed = {
            let Some(find) = self.kanban_find.as_mut() else {
                return false;
            };
            let changed = find.matches != matches || find.query != query;
            find.query = query;
            find.hay = source;
            find.rel = rel;
            find.filter = filter;
            find.matches = matches;
            find.index = 0;
            changed
        };
        if let Some(selection) = first {
            self.kanban_selected = Some(selection);
        }
        changed
    }
}

/// `⌘F` on the Kanban page: the board's Find bar under the header.
pub(crate) fn find_bar(s: &State, k: &Kit) -> El {
    let Some(find) = &s.kanban_find else {
        return rune::el();
    };
    let total = find.matches.len();
    let count = if total == 0 {
        "0/0".to_owned()
    } else {
        format!("{}/{}", find.index + 1, total)
    };
    row()
        .key("kanban-find-bar")
        .w_full()
        .h(40.0)
        .px(28.0)
        .gap(8.0)
        .center_y()
        .fill(k.c.layer_01)
        .child(icon(&ic::MAGNIFYING_GLASS, 13.0).color(k.c.text_muted))
        .child(
            text_input(&find.field, k.ty.ui_compact)
                .key("kanban-find")
                .single_line()
                .w(300.0)
                .color(k.c.text_base)
                .caret(k.c.text_accent, k.selection())
                .placeholder("Find in board", k.c.text_faint)
                .on_edit(|s: &mut State| &mut s.kanban_find.as_mut().expect("board find").field)
                .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                    match e.key {
                        Key::Escape => {
                            s.close_kanban_find();
                            return true;
                        }
                        Key::Enter => {
                            if e.mods.shift {
                                s.kanban_find_prev();
                            } else {
                                s.kanban_find_next();
                            }
                            return true;
                        }
                        Key::Up => {
                            s.kanban_find_prev();
                            return true;
                        }
                        Key::Down => {
                            s.kanban_find_next();
                            return true;
                        }
                        _ => {}
                    }
                    crate::shortcut(s, e, tx)
                }),
        )
        .child(
            text(count)
                .key("kanban-find-count")
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
        .child(
            small_button(k, "kanban-find-prev", "‹").on_press(|s: &mut State| s.kanban_find_prev()),
        )
        .child(
            small_button(k, "kanban-find-next", "›").on_press(|s: &mut State| s.kanban_find_next()),
        )
        .child(rune::el().grow(1.0))
        .child(
            small_button(k, "kanban-find-close", "✕")
                .on_press(|s: &mut State| s.close_kanban_find()),
        )
}

/// The Filter panel under the header.
pub(crate) fn filter_panel(s: &State, k: &Kit) -> El {
    let filter = &s.kanban_filter;
    let field = |field: &TextEdit, key: &'static str, placeholder: &'static str| {
        text_input(field, k.ty.ui_compact)
            .key(key)
            .single_line()
            .color(k.c.text_base)
            .caret(k.c.text_accent, k.selection())
            .placeholder(placeholder, k.c.text_faint)
            .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                if e.key == Key::Escape {
                    s.close_kanban_filter();
                    return true;
                }
                crate::shortcut(s, e, tx)
            })
    };
    let mut panel = col()
        .key("kanban-filter-panel")
        .w_full()
        .px(28.0)
        .pb(10.0)
        .gap(6.0)
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(
                    text("Filter cards")
                        .font(k.ty.caption())
                        .color(k.c.text_muted),
                )
                .child(
                    field(&filter.text, "kanban-filter-text", "Text")
                        .w(240.0)
                        .on_edit(|s: &mut State| &mut s.kanban_filter.text),
                )
                .child(
                    field(&filter.tag, "kanban-filter-tag", "#tag")
                        .w(160.0)
                        .on_edit(|s: &mut State| &mut s.kanban_filter.tag),
                ),
        )
        .child(
            row()
                .gap(6.0)
                .center_y()
                .child(label(k, "Date"))
                .child(
                    filter_choice(k, "kanban-filter-any-date", "Any date", {
                        filter.preset.is_none() && filter.date_range().is_none()
                    })
                    .on_press(|s: &mut State| s.set_kanban_date_preset(None)),
                )
                .child(
                    filter_choice(
                        k,
                        "kanban-filter-today",
                        "Today",
                        filter.preset == Some(KanbanDatePreset::Today),
                    )
                    .on_press(|s: &mut State| {
                        s.set_kanban_date_preset(Some(KanbanDatePreset::Today));
                    }),
                )
                .child(
                    filter_choice(
                        k,
                        "kanban-filter-overdue",
                        "Overdue",
                        filter.preset == Some(KanbanDatePreset::Overdue),
                    )
                    .on_press(|s: &mut State| {
                        s.set_kanban_date_preset(Some(KanbanDatePreset::Overdue));
                    }),
                )
                .child(
                    field(&filter.from, "kanban-filter-from", "From YYYY-MM-DD")
                        .w(150.0)
                        .on_edit(|s: &mut State| {
                            s.kanban_filter.preset = None;
                            &mut s.kanban_filter.from
                        }),
                )
                .child(
                    field(&filter.to, "kanban-filter-to", "To YYYY-MM-DD")
                        .w(150.0)
                        .on_edit(|s: &mut State| {
                            s.kanban_filter.preset = None;
                            &mut s.kanban_filter.to
                        }),
                ),
        )
        .child(
            row()
                .gap(6.0)
                .center_y()
                .child(label(k, "Status"))
                .child(
                    filter_choice(k, "kanban-filter-done-any", "Any", filter.done.is_none())
                        .on_press(|s: &mut State| s.set_kanban_done_filter(None)),
                )
                .child(
                    filter_choice(k, "kanban-filter-open", "Open", filter.done == Some(false))
                        .on_press(|s: &mut State| s.set_kanban_done_filter(Some(false))),
                )
                .child(
                    filter_choice(k, "kanban-filter-done", "Done", filter.done == Some(true))
                        .on_press(|s: &mut State| s.set_kanban_done_filter(Some(true))),
                )
                .child(rune::el().grow(1.0))
                .child(
                    filter_choice(k, "kanban-filter-clear-all", "Clear all", false)
                        .on_press(|s: &mut State| s.clear_kanban_filter()),
                ),
        );
    if filter.date_error() {
        panel = panel.child(
            text("A date filter is YYYY-MM-DD; the range is not applied.")
                .key("kanban-filter-error")
                .role("alert")
                .font(k.ty.caption())
                .color(k.c.danger.fg),
        );
    }
    panel
}

fn label(k: &Kit, words: &'static str) -> El {
    text(words)
        .font(k.ty.caption())
        .color(k.c.text_faint)
        .w(46.0)
}

fn filter_choice(k: &Kit, key: &'static str, words: &'static str, selected: bool) -> El {
    row()
        .key(key)
        .h(22.0)
        .px(8.0)
        .radius(5.0)
        .center_y()
        .fill(if selected {
            k.alpha(k.c.text_accent, 0.16)
        } else {
            k.c.layer_02
        })
        .border(
            1.0,
            if selected {
                k.c.text_accent
            } else {
                k.c.border_muted
            },
        )
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(words)
        .child(text(words).font(k.ty.caption()).color(if selected {
            k.c.text_accent
        } else {
            k.c.text_muted
        }))
}

fn small_button(k: &Kit, key: &'static str, glyph: &'static str) -> El {
    crate::view::pressable(k, key.to_owned())
        .size(24.0, 24.0)
        .center()
        .role("button")
        .label(match glyph {
            "‹" => "Previous match",
            "›" => "Next match",
            _ => "Close find",
        })
        .child(
            text(glyph.to_owned())
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
}

fn icon(glyph: &'static rune::Icon, size: f32) -> El {
    rune::icon(glyph, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_range_is_typed_as_iso_and_bad_text_is_ignored() {
        let mut filter = KanbanFilter::default();
        filter.from.replace_text("2026-11-01");
        filter.to.replace_text("2026-11-30");
        assert_eq!(
            filter.date_range(),
            Some(KanbanDateRange {
                from: CivilDate::new(2026, 11, 1),
                to: CivilDate::new(2026, 11, 30),
            })
        );
        assert!(!filter.date_error());
        filter.to.replace_text("soon");
        assert!(filter.date_range().is_none());
        assert!(filter.date_error());
        assert!(
            filter.summary().is_none(),
            "a malformed range alone applies nothing"
        );
    }

    #[test]
    fn conditions_compose_with_and() {
        let dates = DateSettings::default();
        let today = CivilDate::new(2026, 11, 12).expect("today");
        let mut filter = KanbanFilter::default();
        assert!(filter.matches("Ship #release @{2026-11-12}", false, &dates, today));
        filter.text.replace_text("SHIP");
        filter.set_tag("#Release");
        filter.done = Some(false);
        filter.preset = Some(KanbanDatePreset::Today);
        assert!(filter.matches("Ship #release @{2026-11-12}", false, &dates, today));
        assert!(!filter.matches("Ship #release @{2026-11-11}", false, &dates, today));
        assert!(!filter.matches("Ship #release @{2026-11-12}", true, &dates, today));
        assert!(!filter.matches("Ship #docs @{2026-11-12}", false, &dates, today));
        filter.done = Some(true);
        assert!(!filter.matches("Ship #release @{2026-11-12}", false, &dates, today));
    }

    #[test]
    fn overdue_means_before_today_and_undated_cards_do_not_match() {
        let dates = DateSettings::default();
        let today = CivilDate::new(2026, 11, 12).expect("today");
        let filter = KanbanFilter {
            preset: Some(KanbanDatePreset::Overdue),
            ..KanbanFilter::default()
        };
        assert!(filter.matches("old @{2026-11-11}", false, &dates, today));
        assert!(!filter.matches("today @{2026-11-12}", false, &dates, today));
        assert!(!filter.matches("no date", false, &dates, today));
    }
}

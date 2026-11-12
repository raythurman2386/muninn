//! Kanban slice 9: dates and times on cards.
//!
//! - A card's date and time tokens (`@{…}`, `@[[…]]`, `@@{…}`) are read by
//!   the same [`scan_card_text`] sorting uses, outside fences and inline
//!   code.
//! - **Add/Change date…** and **Add/Change time…** in a card's menu open a
//!   small picker. Accepting writes the card's one token of that kind through
//!   `edit_kanban` (replacing an existing token), so it is one undo step and
//!   is written only by `⌘S`.
//! - Typing a trigger at a word boundary in the card editor opens the same
//!   picker; accepting replaces the typed trigger with the token at the
//!   caret. Escape closes the picker and leaves the typed trigger.
//! - Esc, Cancel and a click elsewhere change nothing. The picker is a
//!   display layer; it never writes a file by itself.
//! - Settings are the board footer's, then the app-wide `kanban_defaults`,
//!   then the built-in defaults ([`DateSettings::resolve`]).

use std::cmp::Reverse;
use std::time::{SystemTime, UNIX_EPOCH};

use muninn_core::{
    CardText, CivilDate, CivilTime, DateSettings, Kanban, format_date, format_time, scan_card_text,
};
use rune::{El, Key, KeyEvent, TextEdit, Tx, col, row, text, text_input};
use rune_kit::Kit;

use crate::State;

/// Which picker is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PickerKind {
    Date,
    Time,
}

/// The typed trigger the picker replaces when it accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PickerInsert {
    pub start: usize,
    pub end: usize,
}

/// A small picker under a card, or under the card editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KanbanPicker {
    pub kind: PickerKind,
    /// The card the token is appended to when `insert` is `None`.
    pub card: (usize, usize),
    /// The typed trigger in the card editor, when the picker was opened by
    /// typing rather than from the card menu.
    pub insert: Option<PickerInsert>,
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u8,
    pub minute: u8,
}

impl State {
    /// The date/time settings the shown board displays: its footer first,
    /// then `kanban_defaults` from `settings.json`, then the built-in value.
    #[must_use]
    pub fn kanban_date_settings(&self) -> DateSettings {
        DateSettings::resolve(
            self.kanban_board().and_then(Kanban::footer_json),
            &self.settings.kanban_defaults,
        )
    }

    /// **Add a date…**: the picker for card `lane`/`index`.
    pub fn open_card_date_picker(&mut self, lane: usize, index: usize) {
        self.open_card_picker(PickerKind::Date, lane, index);
    }

    /// **Add a time…**: the picker for card `lane`/`index`.
    pub fn open_card_time_picker(&mut self, lane: usize, index: usize) {
        self.open_card_picker(PickerKind::Time, lane, index);
    }

    fn open_card_picker(&mut self, kind: PickerKind, lane: usize, index: usize) {
        let Some((lane, index)) = self.settle_card_edit(lane, index) else {
            return;
        };
        if self.kanban_card_at(lane, index).is_none() {
            return;
        }
        self.open_picker(kind, lane, index, None);
    }

    fn open_picker(
        &mut self,
        kind: PickerKind,
        lane: usize,
        index: usize,
        insert: Option<PickerInsert>,
    ) {
        let settings = self.kanban_date_settings();
        let found = self.card_text(lane, index, &settings);
        let (today, now) = local_now();
        let mut picker = KanbanPicker {
            kind,
            card: (lane, index),
            insert,
            year: today.year(),
            month: today.month(),
            day: today.day(),
            hour: now.hour(),
            minute: now.minute(),
        };
        match kind {
            PickerKind::Date => {
                if let Some(date) = found
                    .date
                    .as_ref()
                    .and_then(|token| settings.parse_date(&token.value))
                {
                    picker.year = date.year();
                    picker.month = date.month();
                    picker.day = date.day();
                }
            }
            PickerKind::Time => {
                if let Some(time) = found
                    .time
                    .as_ref()
                    .and_then(|token| settings.parse_time(&token.value))
                {
                    picker.hour = time.hour();
                    picker.minute = time.minute();
                }
            }
        }
        self.kanban_hour_field = TextEdit::new(format!("{:02}", picker.hour));
        self.kanban_minute_field = TextEdit::new(format!("{:02}", picker.minute));
        select_all(&mut self.kanban_hour_field);
        select_all(&mut self.kanban_minute_field);
        self.kanban_hour_field.focused = kind == PickerKind::Time;
        self.kanban_minute_field.focused = false;
        if kind == PickerKind::Time {
            self.kanban_card_field.focused = false;
        }
        self.menu = None;
        self.kanban_picker_dismissed = None;
        self.kanban_picker = Some(picker);
        if self.kanban_card_at(lane, index).is_some() {
            self.kanban_selected = Some((lane, index));
        }
        if let Some(shown) = &mut self.kanban {
            shown.error = None;
        }
    }

    fn card_text(&self, lane: usize, index: usize, settings: &DateSettings) -> CardText {
        self.kanban_card_at(lane, index)
            .map_or_else(CardText::default, |card| {
                scan_card_text(card.text(), settings)
            })
    }

    /// Close the picker, changing nothing.
    pub fn close_picker(&mut self) {
        let insert = self
            .kanban_picker
            .as_ref()
            .is_some_and(|picker| picker.insert.is_some());
        self.kanban_picker = None;
        self.kanban_picker_dismissed = None;
        self.kanban_hour_field.focused = false;
        self.kanban_minute_field.focused = false;
        if insert && self.kanban_card.is_some() {
            self.kanban_card_field.focused = true;
        }
    }

    /// Escape or Cancel: close the picker and leave the typed trigger, without
    /// reopening it until the editor's text changes.
    pub fn dismiss_picker(&mut self) {
        let dismissed = self.kanban_picker.as_ref().and_then(|picker| {
            picker
                .insert
                .map(|insert| (self.kanban_card_field.text.clone(), insert.start, false))
        });
        self.close_picker();
        self.kanban_picker_dismissed = dismissed;
    }

    pub fn picker_previous_month(&mut self) {
        let Some(picker) = &mut self.kanban_picker else {
            return;
        };
        if picker.month == 1 {
            picker.year -= 1;
            picker.month = 12;
        } else {
            picker.month -= 1;
        }
        picker.day = picker
            .day
            .min(CivilDate::new(picker.year, picker.month, 1).map_or(28, |d| d.days_in_month()));
    }

    pub fn picker_next_month(&mut self) {
        let Some(picker) = &mut self.kanban_picker else {
            return;
        };
        if picker.month == 12 {
            picker.year += 1;
            picker.month = 1;
        } else {
            picker.month += 1;
        }
        picker.day = picker
            .day
            .min(CivilDate::new(picker.year, picker.month, 1).map_or(28, |d| d.days_in_month()));
    }

    pub fn picker_today(&mut self) {
        let today = local_today();
        if let Some(picker) = &mut self.kanban_picker {
            picker.year = today.year();
            picker.month = today.month();
            picker.day = today.day();
        }
    }

    pub fn picker_pick_day(&mut self, day: u32) {
        if let Some(picker) = &mut self.kanban_picker {
            picker.day = day;
        }
    }

    pub fn picker_now(&mut self) {
        let now = local_now().1;
        self.kanban_hour_field
            .replace_text(format!("{:02}", now.hour()));
        self.kanban_minute_field
            .replace_text(format!("{:02}", now.minute()));
        if let Some(picker) = &mut self.kanban_picker {
            picker.hour = now.hour();
            picker.minute = now.minute();
        }
    }

    /// Accept the picker: append the token, or replace the typed trigger in
    /// the card editor. One undo step either way.
    pub fn picker_accept(&mut self) {
        let Some(picker) = self.kanban_picker.as_ref() else {
            return;
        };
        let settings = self.kanban_date_settings();
        let token = match picker.kind {
            PickerKind::Date => {
                let Some(date) = CivilDate::new(picker.year, picker.month, picker.day) else {
                    return;
                };
                settings.date_token(date)
            }
            PickerKind::Time => {
                let hour = self.kanban_hour_field.text.trim().parse::<u8>();
                let minute = self.kanban_minute_field.text.trim().parse::<u8>();
                let time = hour
                    .ok()
                    .zip(minute.ok())
                    .and_then(|(hour, minute)| CivilTime::new(hour, minute, 0));
                let Some(time) = time else {
                    if let Some(shown) = &mut self.kanban {
                        shown.error =
                            Some("A time needs an hour 0–23 and a minute 0–59".to_owned());
                    }
                    return;
                };
                settings.time_token(time)
            }
        };
        let Some(picker) = self.kanban_picker.take() else {
            return;
        };
        self.kanban_hour_field.focused = false;
        self.kanban_minute_field.focused = false;
        if let Some(insert) = picker.insert {
            if self.kanban_card.is_some() {
                let before = self.kanban_card_field.text.clone();
                self.kanban_card_field
                    .set_selection(insert.start, insert.end);
                self.kanban_card_field.insert(&token);
                self.kanban_card_field.focused = true;
                // Undo in the field returns to the typed trigger; keep it
                // dismissed so the picker does not reopen on that edit.
                self.kanban_picker_dismissed = Some((before, insert.start, true));
            }
        } else {
            let what = match picker.kind {
                PickerKind::Date => "change the date",
                PickerKind::Time => "change the time",
            };
            let (lane, index) = picker.card;
            let Some(text) = self
                .kanban_card_at(lane, index)
                .map(|card| card.text().to_owned())
            else {
                return;
            };
            // A menu picker replaces the card's existing token of this kind;
            // only an editor picker inserts a new one at the caret.
            let text = match card_token_span(&text, &settings, picker.kind) {
                Some(range) => {
                    let mut next = text;
                    next.replace_range(range, &token);
                    next
                }
                None => with_token(&text, &token),
            };
            self.edit_kanban(what, |board| board.edit_card(lane, index, &text));
        }
    }

    /// Clear: remove the card's date or time token. An editor picker removes
    /// it from the field being edited and leaves the typed trigger; a menu
    /// picker removes it with one board edit.
    pub fn picker_clear(&mut self) {
        let Some(picker) = self.kanban_picker.clone() else {
            return;
        };
        let settings = self.kanban_date_settings();
        if picker.insert.is_some() {
            let text = self.kanban_card_field.text.clone();
            if let Some(range) = card_token_span(&text, &settings, picker.kind) {
                let (start, end) = clear_span(&text, range.start, range.end);
                self.kanban_card_field.set_selection(start, end);
                self.kanban_card_field.insert("");
            }
            self.close_picker();
            return;
        }
        let (lane, index) = picker.card;
        let Some(text) = self
            .kanban_card_at(lane, index)
            .map(|card| card.text().to_owned())
        else {
            self.close_picker();
            return;
        };
        let Some(range) = card_token_span(&text, &settings, picker.kind) else {
            self.close_picker();
            return;
        };
        let (start, end) = clear_span(&text, range.start, range.end);
        let next = format!("{}{}", &text[..start], &text[end..]);
        let what = match picker.kind {
            PickerKind::Date => "clear the date",
            PickerKind::Time => "clear the time",
        };
        self.close_picker();
        self.edit_kanban(what, |board| board.edit_card(lane, index, &next));
    }

    /// Follow the card editor's text: a trigger at a word boundary opens the
    /// picker, a second trigger turns a date picker into a time picker, and
    /// typing past the trigger closes it.
    pub fn detect_card_trigger(&mut self) {
        let Some(slot) = self.kanban_card else {
            return;
        };
        let settings = self.kanban_date_settings();
        let text = self.kanban_card_field.text.clone();
        let caret = self.kanban_card_field.focus.min(text.len());
        let found = trigger_before_caret(&text, caret, &settings);
        if let Some((dismissed, start, accepted)) = self.kanban_picker_dismissed.clone() {
            if dismissed == text && found.as_ref().is_some_and(|(_, at)| at == &start) {
                // An accepted marker is used once, by the field undo; an
                // Escape marker stays until the text changes.
                if accepted {
                    self.kanban_picker_dismissed = None;
                }
                return;
            }
            if accepted && found.is_none() {
                // The token's own text, or typing after it, does not clear
                // the accepted marker; its field undo still matches it.
                return;
            }
            self.kanban_picker_dismissed = None;
        }
        match found {
            Some((kind, start)) => {
                if let Some(picker) = &mut self.kanban_picker
                    && picker.insert.is_some()
                {
                    let was = picker.kind;
                    picker.kind = kind;
                    picker.insert = Some(PickerInsert { start, end: caret });
                    if was != kind {
                        self.kanban_hour_field = TextEdit::new(format!("{:02}", picker.hour));
                        self.kanban_minute_field = TextEdit::new(format!("{:02}", picker.minute));
                        select_all(&mut self.kanban_hour_field);
                        select_all(&mut self.kanban_minute_field);
                        if kind == PickerKind::Time {
                            self.kanban_card_field.focused = false;
                            self.kanban_hour_field.focused = true;
                        }
                    }
                } else {
                    self.open_picker(
                        kind,
                        slot.lane,
                        slot.index,
                        Some(PickerInsert { start, end: caret }),
                    );
                }
            }
            None => {
                if self
                    .kanban_picker
                    .as_ref()
                    .is_some_and(|picker| picker.insert.is_some())
                {
                    self.close_picker();
                }
            }
        }
    }
}

/// The byte range of the card's first date or time token.
fn card_token_span(
    text: &str,
    settings: &DateSettings,
    kind: PickerKind,
) -> Option<std::ops::Range<usize>> {
    let found = scan_card_text(text, settings);
    let token = match kind {
        PickerKind::Date => found.date,
        PickerKind::Time => found.time,
    }?;
    Some(token.start..token.end)
}

/// The note a link-form date points at: an alias after `|` is display text,
/// not part of the target.
fn link_target(value: &str) -> &str {
    value.split_once('|').map_or(value, |(target, _)| target)
}

/// Whether the card already has a date or time token, for the menu label.
pub(crate) fn has_token(s: &State, lane: usize, index: usize, kind: PickerKind) -> bool {
    let settings = s.kanban_date_settings();
    s.kanban_card_at(lane, index).is_some_and(|card| {
        let found = scan_card_text(card.text(), &settings);
        match kind {
            PickerKind::Date => found.date.is_some(),
            PickerKind::Time => found.time.is_some(),
        }
    })
}

/// The local date and time now. Rune reads the OS time zone; core stays
/// zone-free and only converts the fields.
pub(crate) fn local_now() -> (CivilDate, CivilTime) {
    let epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        });
    date_time_from_local(rune::LocalTime::at(epoch_ms))
}

pub(crate) fn local_today() -> CivilDate {
    local_now().0
}

fn date_time_from_local(local: rune::LocalTime) -> (CivilDate, CivilTime) {
    let date = CivilDate::new(i64::from(local.year), local.month, local.day)
        .unwrap_or_else(|| CivilDate::from_days_since_epoch(0));
    let time = CivilTime::new(local.hour as u8, local.minute as u8, local.second as u8)
        .unwrap_or_else(|| CivilTime::new(0, 0, 0).expect("midnight is a valid time"));
    (date, time)
}

fn select_all(field: &mut TextEdit) {
    field.set_selection(0, field.text.len());
}

/// The trigger ending at `caret`, at the start of the text or after
/// whitespace. Longest first, so `@@` wins over `@`.
fn trigger_before_caret(
    text: &str,
    caret: usize,
    settings: &DateSettings,
) -> Option<(PickerKind, usize)> {
    let before = text.get(..caret)?;
    let mut triggers = vec![
        (PickerKind::Date, settings.date_trigger.as_str()),
        (PickerKind::Time, settings.time_trigger.as_str()),
    ];
    triggers.retain(|(_, trigger)| !trigger.is_empty());
    triggers.sort_by_key(|(_, trigger)| Reverse(trigger.len()));
    for (kind, trigger) in triggers {
        let Some(start) = before.len().checked_sub(trigger.len()) else {
            continue;
        };
        if !before.is_char_boundary(start) || &before[start..] != trigger {
            continue;
        }
        let boundary = before[..start]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        if boundary {
            return Some((kind, start));
        }
    }
    None
}

fn with_token(text: &str, token: &str) -> String {
    if text.is_empty() {
        token.to_owned()
    } else if text.ends_with(char::is_whitespace) {
        format!("{text}{token}")
    } else {
        format!("{text} {token}")
    }
}

/// Remove one neighbouring space too, so `due @{…} now` becomes `due now`.
fn clear_span(text: &str, start: usize, end: usize) -> (usize, usize) {
    let before = start > 0 && text[..start].ends_with(' ');
    let after = end == text.len() || text[end..].starts_with(' ');
    if before && after {
        (start - 1, end)
    } else {
        (start, end)
    }
}

/// The picker under the card it belongs to.
pub(crate) fn picker(s: &State, k: &Kit) -> El {
    let Some(picker) = &s.kanban_picker else {
        return rune::el();
    };
    match picker.kind {
        PickerKind::Date => date_picker(s, k, picker),
        PickerKind::Time => time_picker(s, k),
    }
}

fn date_picker(s: &State, k: &Kit, picker: &KanbanPicker) -> El {
    let first = CivilDate::new(picker.year, picker.month, 1).unwrap_or_else(local_today);
    let title = format_date(first, "MMMM YYYY");
    let head = row()
        .w_full()
        .gap(4.0)
        .center_y()
        .child(
            picker_button(k, "kanban-picker-prev", "‹")
                .label("Previous month")
                .on_press(|s: &mut State| s.picker_previous_month()),
        )
        .child(
            text(title)
                .key("kanban-picker-title")
                .font(k.ty.ui_compact)
                .color(k.c.text_base)
                .grow(1.0),
        )
        .child(
            picker_button(k, "kanban-picker-next", "›")
                .label("Next month")
                .on_press(|s: &mut State| s.picker_next_month()),
        );
    let mut grid = col().key("kanban-picker-grid").w_full().gap(2.0);
    let mut week = row().w_full().gap(2.0);
    for label in ["S", "M", "T", "W", "T", "F", "S"] {
        week = week.child(
            text(label)
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .w(28.0),
        );
    }
    grid = grid.child(week);
    let offset = first.weekday() as usize;
    let days = first.days_in_month();
    let mut column = 0;
    let mut line = row().w_full().gap(2.0);
    for _ in 0..offset {
        line = line.child(rune::el().w(28.0).h(24.0));
        column += 1;
    }
    for day in 1..=days {
        let selected = day == picker.day;
        let cell = row()
            .key(format!("kanban-picker-day-{day}"))
            .size(28.0, 24.0)
            .center()
            .radius(5.0)
            .cursor(rune::Cursor::Pointer)
            .hover(k.c.hover)
            .press(k.c.pressed)
            .role("button")
            .label(format!("Day {day}"))
            .fill(if selected {
                k.c.text_accent
            } else {
                k.c.layer_02
            })
            .on_press(move |s: &mut State| s.picker_pick_day(day))
            .child(
                text(day.to_string())
                    .font(k.ty.caption())
                    .color(if selected {
                        k.c.text_inverse
                    } else {
                        k.c.text_base
                    }),
            );
        line = line.child(cell);
        column += 1;
        if column == 7 {
            grid = grid.child(line);
            line = row().w_full().gap(2.0);
            column = 0;
        }
    }
    if column > 0 {
        grid = grid.child(line);
    }
    let mut foot = row().w_full().gap(4.0).center_y().child(
        picker_button(k, "kanban-picker-today", "Today").on_press(|s: &mut State| s.picker_today()),
    );
    if s.kanban_card_at(picker.card.0, picker.card.1).is_some() {
        foot = foot.child(
            picker_button(k, "kanban-picker-clear", "Clear")
                .on_press(|s: &mut State| s.picker_clear()),
        );
    }
    foot = foot
        .child(rune::el().grow(1.0))
        .child(
            picker_button(k, "kanban-picker-cancel", "Cancel")
                .on_press(|s: &mut State| s.dismiss_picker()),
        )
        .child(
            picker_button(k, "kanban-picker-set", "Add date")
                .fill(k.c.text_accent)
                .on_press(|s: &mut State| s.picker_accept()),
        );
    col()
        .key("kanban-picker")
        .w(232.0)
        .padding(8.0)
        .gap(6.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .role("dialog")
        .label("Add a date")
        .child(head)
        .child(grid)
        .child(foot)
}

fn time_picker(s: &State, k: &Kit) -> El {
    let field = |edit: &TextEdit, key: &'static str, label: &'static str| {
        text_input(edit, k.ty.ui_compact)
            .key(key)
            .w(52.0)
            .px(6.0)
            .py(4.0)
            .radius(6.0)
            .border(1.0, k.c.border_base)
            .color(k.c.text_base)
            .caret(k.c.text_accent, k.selection())
            .label(label)
            .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                if e.key == Key::Escape {
                    s.dismiss_picker();
                    tx.blur();
                    return true;
                }
                crate::shortcut(s, e, tx)
            })
            .on_submit(|s: &mut State, _| s.picker_accept())
    };
    let hour = field(&s.kanban_hour_field, "kanban-picker-hour", "Hour")
        .on_edit(|s: &mut State| &mut s.kanban_hour_field);
    let minute = field(&s.kanban_minute_field, "kanban-picker-minute", "Minute")
        .on_edit(|s: &mut State| &mut s.kanban_minute_field);
    col()
        .key("kanban-picker")
        .w(210.0)
        .padding(8.0)
        .gap(6.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .role("dialog")
        .label("Add a time")
        .child(
            text("Add a time")
                .key("kanban-picker-title")
                .font(k.ty.ui_compact)
                .color(k.c.text_base),
        )
        .child(
            row()
                .gap(4.0)
                .center_y()
                .child(hour)
                .child(text(":").color(k.c.text_faint))
                .child(minute),
        )
        .child(
            row()
                .w_full()
                .gap(4.0)
                .center_y()
                .child(
                    picker_button(k, "kanban-picker-now", "Now")
                        .on_press(|s: &mut State| s.picker_now()),
                )
                .child(
                    picker_button(k, "kanban-picker-clear", "Clear")
                        .on_press(|s: &mut State| s.picker_clear()),
                )
                .child(rune::el().grow(1.0))
                .child(
                    picker_button(k, "kanban-picker-cancel", "Cancel")
                        .on_press(|s: &mut State| s.dismiss_picker()),
                )
                .child(
                    picker_button(k, "kanban-picker-set", "Add time")
                        .fill(k.c.text_accent)
                        .on_press(|s: &mut State| s.picker_accept()),
                ),
        )
}

fn picker_button(k: &Kit, key: &str, label: &str) -> El {
    row()
        .key(key)
        .h(24.0)
        .px(7.0)
        .radius(5.0)
        .center()
        .cursor(rune::Cursor::Pointer)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .role("button")
        .label(label)
        .child(text(label).font(k.ty.caption()).color(k.c.text_base))
}

/// The card's own date/time row: formatted values, an optional relative day,
/// and a link to the daily note when the board asks.
pub(crate) fn card_when(
    s: &State,
    k: &Kit,
    settings: &DateSettings,
    found: &CardText,
    today: CivilDate,
    i: usize,
    j: usize,
) -> El {
    if found.date.is_none() && found.time.is_none() {
        return rune::el();
    }
    let mut line = row()
        .key(format!("kanban-card-{i}-{j}-when"))
        .w_full()
        .gap(6.0)
        .center_y();
    if let Some(token) = &found.date {
        let raw = link_target(&token.value);
        let date = settings.parse_date(raw);
        let shown = date.map_or_else(
            || raw.to_owned(),
            |date| format_date(date, &settings.date_display_format),
        );
        let shown = match (settings.show_relative_date, date) {
            (true, Some(date)) => {
                let days = date.days_since_epoch() - today.days_since_epoch();
                format!("{shown} · {}", relative_days(days))
            }
            _ => shown,
        };
        let target = (settings.link_date_to_daily_note || token.link).then(|| {
            let (note, _) = raw.split_once('#').unwrap_or((raw, ""));
            if s.vault.resolve(note).is_some() {
                format!("note:{raw}")
            } else {
                format!("missing:{note}")
            }
        });
        line = line.child(match target {
            Some(target) => text(shown)
                .key(format!("kanban-card-{i}-{j}-date"))
                .font(k.ty.caption())
                .color(k.c.text_accent)
                .cursor(rune::Cursor::Pointer)
                .role("link")
                .label("Open the date's daily note")
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.open_card_link(&target, tx)),
            None => text(shown)
                .key(format!("kanban-card-{i}-{j}-date"))
                .font(k.ty.caption())
                .color(k.c.text_faint),
        });
    }
    if let Some(token) = &found.time {
        let shown = settings.parse_time(&token.value).map_or_else(
            || token.value.clone(),
            |time| format_time(time, &settings.time_format),
        );
        line = line.child(
            text(shown)
                .key(format!("kanban-card-{i}-{j}-time"))
                .font(k.ty.caption())
                .color(k.c.text_faint),
        );
    }
    line
}

fn relative_days(days: i64) -> String {
    match days {
        0 => "today".to_owned(),
        1 => "tomorrow".to_owned(),
        -1 => "yesterday".to_owned(),
        n if n > 0 => format!("in {n} days"),
        n => format!("{} days ago", -n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> DateSettings {
        DateSettings::default()
    }

    #[test]
    fn a_trigger_at_a_word_boundary_is_found_but_not_mid_word() {
        let settings = settings();
        assert_eq!(
            trigger_before_caret("due @", 5, &settings),
            Some((PickerKind::Date, 4))
        );
        assert_eq!(
            trigger_before_caret("due @@", 6, &settings),
            Some((PickerKind::Time, 4)),
            "the longer trigger wins"
        );
        assert_eq!(
            trigger_before_caret("@", 1, &settings),
            Some((PickerKind::Date, 0))
        );
        assert_eq!(trigger_before_caret("mail@", 5, &settings), None);
        assert_eq!(trigger_before_caret("a @b", 4, &settings), None);
    }

    #[test]
    fn a_token_is_appended_with_one_space() {
        assert_eq!(with_token("Card", "@{2026-11-12}"), "Card @{2026-11-12}");
        assert_eq!(with_token("Card ", "@{2026-11-12}"), "Card @{2026-11-12}");
        assert_eq!(with_token("", "@{2026-11-12}"), "@{2026-11-12}");
    }

    #[test]
    fn clearing_removes_one_neighbouring_space() {
        assert_eq!(clear_span("a @{x} b", 2, 6), (1, 6));
        assert_eq!(clear_span("a@{x}", 1, 5), (1, 5));
        assert_eq!(clear_span("a @{x}", 2, 6), (1, 6));
    }

    #[test]
    fn a_fixed_offset_maps_to_the_local_date_and_time() {
        let epoch = (CivilDate::new(2026, 11, 12).unwrap().days_since_epoch() * 86_400
            + 23 * 3_600
            + 30 * 60)
            * 1_000;
        assert_eq!(
            date_time_from_local(rune::LocalTime::at_offset(epoch, 2 * 3_600)),
            (
                CivilDate::new(2026, 11, 13).unwrap(),
                CivilTime::new(1, 30, 0).unwrap()
            )
        );
        assert_eq!(
            date_time_from_local(rune::LocalTime::at_offset(epoch, -5 * 3_600)),
            (
                CivilDate::new(2026, 11, 12).unwrap(),
                CivilTime::new(18, 30, 0).unwrap()
            )
        );
    }

    #[test]
    fn a_link_alias_is_not_part_of_the_target() {
        assert_eq!(link_target("2026-11-11|Nov 11"), "2026-11-11");
        assert_eq!(
            link_target("daily/2026-11-11#Today"),
            "daily/2026-11-11#Today"
        );
        assert_eq!(link_target("2026-11-11"), "2026-11-11");
    }

    #[test]
    fn relative_days_read_naturally() {
        assert_eq!(relative_days(0), "today");
        assert_eq!(relative_days(1), "tomorrow");
        assert_eq!(relative_days(-1), "yesterday");
        assert_eq!(relative_days(3), "in 3 days");
        assert_eq!(relative_days(-4), "4 days ago");
    }
}

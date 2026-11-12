//! Kanban slice 5: lanes, card moves, sorting and collapse on the Kanban page.
//!
//! Every action is one edit through slice 4's `edit_kanban`, so it is one
//! undo step, shows the kanban as unsaved, and is written only by `⌘S`. Each
//! changes only its own lines (`docs/kanban-format.md` section 10):
//!
//! - **Lanes** (a lane's menu, and **+ Add lane** after the last lane): Add
//!   lane, Insert lane before / after and Rename lane open a title field;
//!   Enter commits, Esc cancels, and a blank title changes nothing. A new
//!   lane is `## Title`, a blank line, then two blank lines (spec section 4;
//!   one after the last lane). A title ending in ` (N)` is refused: that is
//!   the card limit.
//!   Rename changes the title line and keeps a lane limit.
//! - **Card limit…** opens a number field: a whole number writes
//!   `## Title (N)`, and a blank field or `0` removes the suffix (`(0)` is
//!   never written). Other text is refused.
//! - **Mark as done lane** / **Mark as not done lane** add or remove the
//!   done-lane marker line. Cards keep their done marks.
//! - **Delete lane** removes the lane's title, marker and cards. Its other
//!   text (paragraphs, headings, comments the page doesn't show) stays in the
//!   file (spec section 10.3). A lane with cards asks first; when that text
//!   can't stay, any lane asks and says how many lines go.
//! - Lane add and delete are refused while the note ends inside an open
//!   fence, as a new settings footer is.
//! - **Moves** (a card's menu, or Alt with the arrow keys on the selected
//!   card): up or down one place (Alt+Up/Down), to the top or bottom of its
//!   lane (Alt+Shift+Up/Down), to the previous or next lane at the same place
//!   or the lane's end (Alt+Left/Right), and **Move to list…**, which puts it
//!   at the end of the lane picked. A move takes the card's lines out and
//!   puts the same lines back elsewhere; a card moved into a done lane is
//!   marked done, since cards entering a done lane become done (spec section
//!   1). A card moved out of one keeps its mark. A click selects a card and
//!   the arrow keys move the selection. Alt+Shift+Left/Right move the
//!   selected card's lane instead (slice 6, `kanban_drag.rs`).
//! - **Sort by text / date / tags** (a lane's menu): a stable sort, with cards
//!   that have no date or no tags last in their original order
//!   ([`muninn_core::CardOrder`]).
//! - **Collapse** (the `‹` in a lane's header) draws the lane as a narrow strip
//!   with its title and card count; its `›` expands it. The state is the
//!   footer's `list-collapse` setting (spec section 9), so it is saved by
//!   `⌘S` with the other edits; adding or deleting a lane keeps it aligned.
//! - **Open editors** follow slice 4's rules. Each action first commits an
//!   open card editor or lane field, then finds its card or lane again (a
//!   card added at or above it moves it down one, a lane added at or before
//!   it moves it right one). A refused commit keeps the editor and its text
//!   open, with the reason on the page, and the action waits.

use std::io;

use muninn_core::{CardOrder, Kanban};
use rune::{El, Key, KeyEvent, Tx, col, row, text, text_input};
use rune_kit::Kit;

use crate::kanban_edit::CardCommit;
use crate::{ContextMenu, MenuTarget, Mode, State};

/// What the lane field edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LaneField {
    /// The title of a lane to insert at [`LaneSlot::lane`].
    New,
    Rename,
    Limit,
}

/// Where the lane field is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LaneSlot {
    pub lane: usize,
    pub field: LaneField,
}

/// A keyboard or menu move of one card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardStep {
    Up,
    Down,
    Top,
    Bottom,
    PreviousLane,
    NextLane,
}

/// Why a card limit was refused.
const LIMIT_REFUSED: &str = "A card limit is a whole number";

/// The lane field's `typed` text (trimmed) applied to `board` at `slot`, as
/// committing it does. A blank title changes nothing; a blank limit clears it.
pub(crate) fn apply_lane_field(board: &mut Kanban, slot: LaneSlot, typed: &str) -> io::Result<()> {
    let LaneSlot { lane, field } = slot;
    match field {
        LaneField::New | LaneField::Rename if typed.is_empty() => Ok(()),
        LaneField::New => board.add_lane(lane, typed, None, false),
        LaneField::Rename => board.rename_lane(lane, typed),
        LaneField::Limit if typed.is_empty() => board.set_lane_limit(lane, None),
        LaneField::Limit => match typed.parse::<u32>() {
            Ok(n) => board.set_lane_limit(lane, Some(n)),
            Err(_) => Err(io::Error::new(io::ErrorKind::InvalidInput, LIMIT_REFUSED)),
        },
    }
}

impl State {
    /// Add lane, Insert lane before / after: a title field for a lane at `index`.
    pub fn begin_add_lane(&mut self, index: usize) {
        // A lane added by the open field at or before `index` moves it on.
        let Some(index) = self.settle_lane_edit(index) else {
            return;
        };
        let field = LaneField::New;
        self.open_lane_field(LaneSlot { lane: index, field }, String::new());
    }

    pub fn begin_rename_lane(&mut self, lane: usize) {
        let Some(lane) = self.settle_lane_edit(lane) else {
            return;
        };
        if let Some(title) = self.kanban_lane(lane).map(|l| l.title().to_owned()) {
            let field = LaneField::Rename;
            self.open_lane_field(LaneSlot { lane, field }, title);
        }
    }

    /// Card limit…: a field holding the lane's limit, blank when it has none.
    pub fn begin_lane_limit(&mut self, lane: usize) {
        let Some(lane) = self.settle_lane_edit(lane) else {
            return;
        };
        if let Some(limit) = self.kanban_lane(lane).map(muninn_core::Lane::limit) {
            let field = LaneField::Limit;
            let text = limit.map(|n| n.to_string()).unwrap_or_default();
            self.open_lane_field(LaneSlot { lane, field }, text);
        }
    }

    /// Open the field; the caller has committed any open editor.
    fn open_lane_field(&mut self, slot: LaneSlot, text: String) {
        self.menu = None;
        self.set_text_focus(false, false);
        let mut field = rune::TextEdit::new(text);
        field.set_caret(field.text.len());
        field.focused = true;
        self.kanban_lane_field = field;
        self.kanban_lane_edit = Some(slot);
    }

    /// Close the lane field and apply it as one edit, as slice 4's card
    /// editor does: a refused text (one the model can't write, or a limit
    /// that is not a whole number) keeps the field open with its text, and
    /// the page says why.
    pub(crate) fn commit_lane_edit(&mut self) -> CardCommit {
        let Some(LaneSlot { lane, field }) = self.kanban_lane_edit else {
            return CardCommit::Closed;
        };
        let typed = self.kanban_lane_field.text.trim().to_owned();
        let slot = LaneSlot { lane, field };
        let what = match field {
            LaneField::New => "add the lane",
            LaneField::Rename => "rename the lane",
            LaneField::Limit if typed.is_empty() => "clear the limit",
            LaneField::Limit => "set the limit",
        };
        let applied = if typed.is_empty() && field != LaneField::Limit {
            true
        } else if field == LaneField::Limit && !typed.is_empty() && typed.parse::<u32>().is_err() {
            if let Some(shown) = &mut self.kanban {
                shown.error = Some(LIMIT_REFUSED.to_owned());
            }
            false
        } else {
            self.edit_kanban(what, |b| apply_lane_field(b, slot, &typed))
        };
        if !applied {
            return CardCommit::Refused;
        }
        self.cancel_lane_edit();
        if field == LaneField::New && !typed.is_empty() {
            let commit = CardCommit::LaneAdded(lane);
            self.kanban_selected = self.kanban_selected.and_then(|(l, i)| commit.follow(l, i));
            return commit;
        }
        CardCommit::Done(None)
    }

    pub fn cancel_lane_edit(&mut self) {
        self.kanban_lane_edit = None;
        self.kanban_lane_field = rune::TextEdit::default();
        if let Some(shown) = &mut self.kanban {
            shown.error = None;
        }
    }

    /// Delete lane: at once when it has no cards and its note text can all
    /// stay in the file; otherwise after a confirmation that says what goes.
    pub fn delete_lane(&mut self, lane: usize) {
        let Some(lane) = self.settle_lane_edit(lane) else {
            return;
        };
        let Some(board) = self.kanban_board() else {
            return;
        };
        let empty = board.lanes().get(lane).map(|l| l.cards().is_empty());
        let drops = match board.lane_delete_drops(lane) {
            Ok(drops) => drops,
            Err(error) => {
                // Refused (an open fence): say why, as an edit would.
                let message = format!("Could not delete the lane: {error}");
                if let Some(shown) = &mut self.kanban {
                    shown.error = Some(message);
                }
                return;
            }
        };
        match empty {
            Some(true) if drops.is_empty() => self.delete_lane_now(lane),
            Some(_) => {
                self.menu = Some(ContextMenu {
                    target: MenuTarget::DeleteLane(lane),
                    x: 0.0,
                    y: 0.0,
                });
            }
            None => {}
        }
    }

    pub fn delete_lane_now(&mut self, lane: usize) {
        let Some(lane) = self.settle_lane_edit(lane) else {
            return;
        };
        if self.edit_kanban("delete the lane", |b| b.delete_lane(lane)) {
            self.kanban_selected = None;
        }
    }

    pub fn set_lane_done(&mut self, lane: usize, done: bool) {
        if let Some(lane) = self.settle_lane_edit(lane) {
            self.edit_kanban("change the done lane", |b| b.set_lane_done(lane, done));
        }
    }

    pub fn sort_lane(&mut self, lane: usize, order: CardOrder) {
        if let Some(lane) = self.settle_lane_edit(lane) {
            self.edit_kanban("sort the lane", |b| b.sort_cards(lane, order));
        }
    }

    /// The `‹` / `›` in a lane's header: collapse or expand it.
    pub fn toggle_lane_collapsed(&mut self, lane: usize) {
        let Some(lane) = self.settle_lane_edit(lane) else {
            return;
        };
        let Some(collapsed) = self.kanban_lane(lane).map(muninn_core::Lane::collapsed) else {
            return;
        };
        self.edit_kanban("collapse the lane", |b| {
            b.set_lane_collapsed(lane, !collapsed)
        });
    }

    /// Move a card to `to_index` in `to_lane` (its index after the move), on
    /// indices that match the board as it is now. A card entering a done lane
    /// from another lane is marked done.
    pub(crate) fn move_card_now(
        &mut self,
        lane: usize,
        index: usize,
        to_lane: usize,
        to_index: usize,
    ) {
        let Some(open) = self.kanban_card_at(lane, index).map(|c| !c.is_done()) else {
            return;
        };
        let mark = open && to_lane != lane && self.kanban_lane_is_done(to_lane);
        let moved = self.edit_kanban("move the card", |b| {
            b.move_card(lane, index, to_lane, to_index)?;
            if mark {
                b.toggle_done(to_lane, to_index)?;
            }
            Ok(())
        });
        if moved {
            self.kanban_selected = Some((to_lane, to_index));
        }
    }

    /// The card menu's moves and Alt+arrows. A step with nowhere to go does nothing.
    pub fn step_kanban_card(&mut self, lane: usize, index: usize, step: CardStep) {
        let Some((lane, index)) = self.settle_card_edit(lane, index) else {
            return;
        };
        let len = |l: usize| self.kanban_lane(l).map(|l| l.cards().len());
        let Some(here) = len(lane).filter(|&n| index < n) else {
            return;
        };
        let target = match step {
            CardStep::Up => index.checked_sub(1).map(|i| (lane, i)),
            CardStep::Down => (index + 1 < here).then_some((lane, index + 1)),
            CardStep::Top => Some((lane, 0)),
            CardStep::Bottom => Some((lane, here - 1)),
            CardStep::PreviousLane => lane
                .checked_sub(1)
                .and_then(|l| Some((l, index.min(len(l)?)))),
            CardStep::NextLane => len(lane + 1).map(|n| (lane + 1, index.min(n))),
        };
        if let Some((to_lane, to_index)) = target {
            self.move_card_now(lane, index, to_lane, to_index);
        }
    }

    /// Move to list…: the lane picker under the card.
    pub fn open_move_to_list(&mut self, lane: usize, index: usize) {
        let Some((lane, index)) = self.settle_card_edit(lane, index) else {
            return;
        };
        self.menu = Some(ContextMenu {
            target: MenuTarget::MoveCard(lane, index),
            x: 0.0,
            y: 0.0,
        });
    }

    /// The lane picker's choice: the end of `to_lane`.
    pub fn move_card_to_lane(&mut self, lane: usize, index: usize, to_lane: usize) {
        self.menu = None;
        let commit = self.commit_card_edit();
        let (Some((lane, index)), Some((to_lane, _))) =
            (commit.follow(lane, index), commit.follow(to_lane, 0))
        else {
            return;
        };
        if let Some(len) = self.kanban_lane(to_lane).map(|l| l.cards().len()) {
            let to_index = if to_lane == lane { len - 1 } else { len };
            self.move_card_now(lane, index, to_lane, to_index);
        }
    }

    /// A click on a card: the card the arrow keys and Alt+arrows act on. It
    /// leaves an open editor alone (a double click edits); when the editor
    /// later adds a card or lane, the selection follows its card.
    pub fn select_kanban_card(&mut self, lane: usize, index: usize) {
        self.kanban_selected = Some((lane, index));
    }

    /// Slice 5's lane and card menu actions.
    pub(crate) fn pick_kanban_menu(&mut self, target: MenuTarget, action: &str) {
        let step = |action: &str| {
            Some(match action {
                "move_up" => CardStep::Up,
                "move_down" => CardStep::Down,
                "move_top" => CardStep::Top,
                "move_bottom" => CardStep::Bottom,
                "move_previous" => CardStep::PreviousLane,
                "move_next" => CardStep::NextLane,
                _ => return None,
            })
        };
        match (target, action) {
            (MenuTarget::Card(lane, index), "move_to_list") => self.open_move_to_list(lane, index),
            (MenuTarget::Card(lane, index), action) => {
                if let Some(step) = step(action) {
                    self.step_kanban_card(lane, index, step);
                }
            }
            (MenuTarget::Lane(lane), "insert_lane_before") => self.begin_add_lane(lane),
            (MenuTarget::Lane(lane), "insert_lane_after") => self.begin_add_lane(lane + 1),
            (MenuTarget::Lane(lane), "move_lane_left") => self.step_kanban_lane(lane, false),
            (MenuTarget::Lane(lane), "move_lane_right") => self.step_kanban_lane(lane, true),
            (MenuTarget::Lane(lane), "rename_lane") => self.begin_rename_lane(lane),
            (MenuTarget::Lane(lane), "lane_limit") => self.begin_lane_limit(lane),
            (MenuTarget::Lane(lane), "done_lane") => self.set_lane_done(lane, true),
            (MenuTarget::Lane(lane), "not_done_lane") => self.set_lane_done(lane, false),
            (MenuTarget::Lane(lane), "sort_text") => self.sort_lane(lane, CardOrder::Text),
            (MenuTarget::Lane(lane), "sort_date") => self.sort_lane(lane, CardOrder::Date),
            (MenuTarget::Lane(lane), "sort_tags") => self.sort_lane(lane, CardOrder::Tags),
            (MenuTarget::Lane(lane), "delete_lane") => self.delete_lane(lane),
            (MenuTarget::DeleteLane(lane), "delete") => self.delete_lane_now(lane),
            _ => {}
        }
    }

    fn kanban_lane(&self, lane: usize) -> Option<&muninn_core::Lane> {
        self.kanban_board()?.lanes().get(lane)
    }
}

/// The card menu's move items, for the card at `lane`/`index`.
pub(crate) fn card_move_items(
    s: &State,
    lane: usize,
    index: usize,
) -> Vec<(&'static str, &'static str)> {
    let Some(board) = s.kanban_board() else {
        return Vec::new();
    };
    let lanes = board.lanes().len();
    let len = board.lanes().get(lane).map_or(0, |l| l.cards().len());
    let mut items = Vec::new();
    if index > 0 {
        items.extend([("Move up", "move_up"), ("Move to top", "move_top")]);
    }
    if index + 1 < len {
        items.extend([
            ("Move down", "move_down"),
            ("Move to bottom", "move_bottom"),
        ]);
    }
    if lane > 0 {
        items.push(("Move to previous list", "move_previous"));
    }
    if lane + 1 < lanes {
        items.push(("Move to next list", "move_next"));
    }
    if lanes > 1 {
        items.push(("Move to list…", "move_to_list"));
    }
    items
}

/// The lane menu's slice 5 items.
pub(crate) fn lane_items(s: &State, lane: usize) -> Vec<(&'static str, &'static str)> {
    let done = s
        .kanban_board()
        .and_then(|b| b.lanes().get(lane))
        .is_some_and(|l| l.done_word().is_some());
    let lanes = s.kanban_board().map_or(0, |b| b.lanes().len());
    let mut items = vec![
        ("Insert lane before", "insert_lane_before"),
        ("Insert lane after", "insert_lane_after"),
    ];
    // Slice 6: the keyboard and menu equivalents of dragging a lane.
    if lane > 0 {
        items.push(("Move lane left", "move_lane_left"));
    }
    if lane + 1 < lanes {
        items.push(("Move lane right", "move_lane_right"));
    }
    items.extend([
        ("Rename lane", "rename_lane"),
        ("Card limit…", "lane_limit"),
        if done {
            ("Mark as not done lane", "not_done_lane")
        } else {
            ("Mark as done lane", "done_lane")
        },
        ("Sort by text", "sort_text"),
        ("Sort by date", "sort_date"),
        ("Sort by tags", "sort_tags"),
        ("Delete lane", "delete_lane"),
    ]);
    items
}

/// Delete lane's confirmation, under the lane's header: what goes, what
/// stays, then Delete and Cancel.
pub(crate) fn delete_lane_confirm(s: &State, k: &Kit, lane: usize) -> El {
    let board = s.kanban_board();
    let title = board
        .and_then(|b| b.lanes().get(lane))
        .map(|l| l.title().to_owned())
        .unwrap_or_default();
    let cards = board
        .and_then(|b| b.lanes().get(lane))
        .map_or(0, |l| l.cards().len());
    let drops = board
        .and_then(|b| b.lane_delete_drops(lane).ok())
        .unwrap_or_default();
    let (question, button) = delete_lane_words(&title, cards, &drops);
    col()
        .key("context-menu")
        .w_full()
        .px(10.0)
        .pl(20.0)
        .gap(2.0)
        .py(4.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base)
        .child(
            text(question)
                .key("kanban-delete-lane-question")
                .font(k.ty.caption())
                .color(k.c.text_base)
                .wrap_anywhere()
                .w_full()
                .px(10.0)
                .py(6.0),
        )
        .child(
            row()
                .key("menu-delete")
                .w_full()
                .h(30.0)
                .px(10.0)
                .radius(6.0)
                .center_y()
                .cursor(rune::Cursor::Pointer)
                .hover(k.c.hover)
                .press(k.c.pressed)
                .role("button")
                .label(button)
                .child(text(button).font(k.ty.ui_compact).color(k.c.danger.fg))
                .on_press(|s: &mut State| s.pick_menu("delete")),
        )
        .child(
            menu_row(k, "menu-cancel".to_owned(), "Cancel".to_owned())
                .on_press(|s: &mut State| s.close_menu()),
        )
}

/// The confirmation's question and button. Note text the page doesn't show
/// is named: kept, or (when it can't stay) counted and quoted.
fn delete_lane_words(title: &str, cards: usize, drops: &[String]) -> (String, &'static str) {
    let cards = match cards {
        0 => String::new(),
        1 => " and its card".to_owned(),
        n => format!(" and its {n} cards"),
    };
    let Some(first) = drops.first() else {
        let question = format!("Delete “{title}”{cards}? Any note text in it stays in the file.");
        return (question, "Delete the lane and its cards");
    };
    let mut preview: String = first.chars().take(48).collect();
    if preview.len() < first.len() {
        preview.push('…');
    }
    let lines = match drops.len() {
        1 => "1 line".to_owned(),
        n => format!("{n} lines"),
    };
    let question = format!(
        "Delete “{title}”{cards}? {lines} of note text the board doesn't show will be deleted too, starting “{preview}”."
    );
    (question, "Delete the lane and its note text")
}

/// Move to list…: every other lane, by title, under the card.
pub(crate) fn lane_picker(s: &State, k: &Kit, lane: usize, index: usize) -> El {
    let mut list = col()
        .key("context-menu")
        .w_full()
        .px(10.0)
        .pl(20.0)
        .gap(2.0)
        .py(4.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_base);
    let lanes = s.kanban_board().map(|b| b.lanes()).unwrap_or_default();
    for (to, other) in lanes.iter().enumerate().filter(|(to, _)| *to != lane) {
        list = list.child(
            menu_row(k, format!("move-to-lane-{to}"), other.title().to_owned())
                .on_press(move |s: &mut State| s.move_card_to_lane(lane, index, to)),
        );
    }
    list.child(
        menu_row(k, "menu-cancel".to_owned(), "Cancel".to_owned())
            .on_press(|s: &mut State| s.close_menu()),
    )
}

fn menu_row(k: &Kit, key: String, label: String) -> El {
    row()
        .key(key)
        .w_full()
        .h(30.0)
        .px(10.0)
        .radius(6.0)
        .center_y()
        .cursor(rune::Cursor::Pointer)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .role("button")
        .label(label.clone())
        .child(text(label).font(k.ty.ui_compact).color(k.c.text_base))
}

/// The lane field: a new lane's title, a rename, or a card limit.
pub(crate) fn lane_field(s: &State, k: &Kit) -> El {
    let (label, placeholder) = match s.kanban_lane_edit.map(|slot| slot.field) {
        Some(LaneField::Limit) => ("Card limit", "No limit"),
        _ => ("Lane title", "Lane title"),
    };
    text_input(&s.kanban_lane_field, k.ty.ui_compact)
        .key("kanban-lane-field")
        .single_line()
        .w_full()
        .px(8.0)
        .py(6.0)
        .radius(7.0)
        .border(1.0, k.c.border_base)
        .color(k.c.text_base)
        .caret(k.c.text_accent, k.selection())
        .placeholder(placeholder, k.c.text_faint)
        .label(label)
        .on_edit(|s: &mut State| &mut s.kanban_lane_field)
        .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
            if e.key == Key::Escape {
                s.cancel_lane_edit();
                tx.blur();
                return true;
            }
            crate::shortcut(s, e, tx)
        })
        .on_submit(|s: &mut State, tx: &mut Tx| {
            // A refused text stays in the field, with the reason above.
            if s.commit_lane_edit() != CardCommit::Refused {
                tx.blur();
            }
        })
}

/// Arrow keys on the Kanban page: select a card, or with Alt move the
/// selected one (Shift: to the top or bottom of its lane). Slice 6:
/// Alt+Shift+Left / Right move the selected card's lane.
pub(crate) fn kanban_key(s: &mut State, e: &KeyEvent) -> bool {
    if s.mode != Mode::Kanban || e.mods.cmd {
        return false;
    }
    let (across, down): (isize, isize) = match e.key {
        Key::Up => (0, -1),
        Key::Down => (0, 1),
        Key::Left => (-1, 0),
        Key::Right => (1, 0),
        _ => return false,
    };
    let Some(board) = s.kanban_board() else {
        return false;
    };
    let len = |l: usize| board.lanes().get(l).map_or(0, |l| l.cards().len());
    let selected = s.kanban_selected.filter(|&(l, i)| i < len(l));
    let Some((lane, index)) = selected else {
        s.kanban_selected = (0..board.lanes().len())
            .find(|&l| len(l) > 0)
            .map(|l| (l, 0));
        return true;
    };
    if e.mods.alt && e.mods.shift && across != 0 {
        s.step_kanban_lane(lane, across > 0);
        return true;
    }
    if e.mods.alt {
        let step = match (across, down, e.mods.shift) {
            (-1, _, _) => CardStep::PreviousLane,
            (1, _, _) => CardStep::NextLane,
            (_, -1, false) => CardStep::Up,
            (_, -1, true) => CardStep::Top,
            (_, _, false) => CardStep::Down,
            (_, _, true) => CardStep::Bottom,
        };
        s.step_kanban_card(lane, index, step);
        return true;
    }
    let to_lane = lane.saturating_add_signed(across);
    let to_len = len(to_lane);
    if to_len > 0 {
        let to_index = index.saturating_add_signed(down).min(to_len - 1);
        s.kanban_selected = Some((to_lane, to_index));
    }
    true
}

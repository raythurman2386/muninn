//! Kanban slice 4: editing cards on the Kanban page, with undo, redo and an
//! explicit save.
//!
//! Every action is one edit through `muninn_core::Kanban`'s splice API, so it
//! changes only its own lines (`docs/kanban-format.md` section 10):
//!
//! - **Add card** (a lane's **+ Add card**) and **Insert card before / after**
//!   (a card's menu) open an editor where the card will go. Committing text
//!   that is not blank writes `- [ ] text` there; Add card puts it at the end
//!   of the lane. Blank text adds nothing.
//! - **Edit** (a double click on a card, or Edit in its menu) edits the card
//!   in place. The checkbox character and the block id stay. Committing blank
//!   text keeps the card as it was (Delete removes one).
//! - In the editor Enter commits, Shift+Enter breaks the line and Esc
//!   cancels. Leaving the page, picking another kanban, saving and quitting
//!   commit it. While it is open it owns the keyboard ([`State::is_typing`]),
//!   so `⌘Z` / `⇧⌘Z` there are the field's own undo and redo of the typing.
//! - **Delete** removes the card's lines. **Duplicate** writes a copy of the
//!   card's text right after it, without its block id (a block id names one
//!   card). **Split card** makes each line of a multi-line card its own card,
//!   in order: the first keeps the card's checkbox and block id, and the
//!   lines lose their leading spaces and tabs. A card made by Duplicate or
//!   Split is done when the card it came from is done.
//! - The **checkbox** flips the done mark: one character.
//! - **Done lanes**: a card added to, or duplicated or split inside, a done
//!   lane is born done (`- [x]`). The lane menu's **Mark all cards done**
//!   marks every open card in the lane done, each one character.
//! - **Undo / redo** (`⌘Z`, `⇧⌘Z` / `⌘Y`, and the Ctrl chords) cover every
//!   action above. Each history entry is the whole note text before or after
//!   it, so undo puts back the exact previous bytes. Up to
//!   [`muninn_core::HISTORY_LIMIT`] entries and about
//!   [`muninn_core::HISTORY_BYTES`] per open kanban, as on Board.
//! - **Saving**: `⌘S` / `Ctrl+S` writes the note once through
//!   `Vault::write_note` (the vault's atomic writer), and only inside the
//!   open vault. The file's text is compared with what the page last read or
//!   wrote, not just its `(mtime, len)`: a file that changed on disk since it
//!   was read is not written over, even when another app kept its length and
//!   modified time. The save is refused and the page flags it (slice 7,
//!   `kanban_sync.rs`, which also adds autosave; with `"kanban_autosave":
//!   false` nothing is written until `⌘S`).
//! - **Unsaved edits are never dropped**, as on Board: leaving the page keeps
//!   them in memory, and coming back does not re-read the file over them.
//!   Picking another kanban saves first; when that save fails the page stays
//!   on the edited kanban. Switching vaults and quitting save first too and
//!   are refused, with the reason, when the save fails.
//! - An action the model refuses (an `io::Error`, for example text it can't
//!   place inside the card) changes nothing, in memory or on disk, and the
//!   Kanban page says why. A refused editor stays open with its text.
//! - The page draws the cards as they were when it was built. An action on a
//!   card commits an open editor first; when that adds a card at or above the
//!   one clicked, the action follows the clicked card down one place.

use std::collections::VecDeque;
use std::io;

use muninn_core::{HISTORY_BYTES, HISTORY_LIMIT, Kanban};
use rune::{KeyEvent, TextEdit};

use crate::board_view::BoardStatus;
use crate::doc_sync::inside_vault;
use crate::{Mode, State};

/// Where the card editor is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CardSlot {
    pub lane: usize,
    /// The card edited, or the index a new card will have.
    pub index: usize,
    /// A card not in the file yet.
    pub new: bool,
}

/// What closing the card editor did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CardCommit {
    /// No editor was open.
    Closed,
    /// The editor closed; with the slot of the card it added, if it added one.
    Done(Option<CardSlot>),
    /// The model refused the text: the editor stays open with it.
    Refused,
    /// Slice 5: the lane field closed after adding a lane at this index, so
    /// lanes drawn at or after it moved right one.
    LaneAdded(usize),
}

impl CardCommit {
    /// Where the card drawn at `lane`/`index` before this commit is now:
    /// down one when a card was added at or above it in its lane, right one
    /// when a lane was added at or before its lane. `None` when refused.
    pub(crate) fn follow(self, lane: usize, index: usize) -> Option<(usize, usize)> {
        match self {
            Self::Refused => None,
            Self::Done(Some(added)) if added.lane == lane && added.index <= index => {
                Some((lane, index + 1))
            }
            Self::LaneAdded(at) if at <= lane => Some((lane + 1, index)),
            Self::Closed | Self::Done(_) | Self::LaneAdded(_) => Some((lane, index)),
        }
    }
}

/// Undo and redo for the kanban on the page: whole note texts, newest last.
#[derive(Clone, Debug, Default)]
pub struct KanbanHistory {
    undo: VecDeque<String>,
    redo: Vec<String>,
}

impl KanbanHistory {
    /// Remember the text before an edit. A new edit clears redo.
    fn record(&mut self, before: String) {
        self.redo.clear();
        self.undo.push_back(before);
        let mut bytes: usize = self.undo.iter().map(String::len).sum();
        while self.undo.len() > 1 && (self.undo.len() > HISTORY_LIMIT || bytes > HISTORY_BYTES) {
            if let Some(old) = self.undo.pop_front() {
                bytes -= old.len();
            }
        }
    }
}

impl State {
    /// Whether the kanban on the page holds edits that are not on disk, or
    /// has its card editor or lane field open.
    #[must_use]
    pub fn kanban_dirty(&self) -> bool {
        self.kanban_card.is_some()
            || self.kanban_lane_edit.is_some()
            || self.kanban.as_ref().is_some_and(|k| k.dirty())
    }

    /// + Add card: an editor at the end of `lane`.
    pub fn begin_add_card(&mut self, lane: usize) {
        let Some(lane) = self.settle_lane_edit(lane) else {
            return;
        };
        let Some(len) = self.kanban_lane_len(lane) else {
            return;
        };
        self.open_card_field(
            CardSlot {
                lane,
                index: len,
                new: true,
            },
            String::new(),
        );
    }

    /// Insert card before / after: an editor that will add a card at `index`.
    pub fn begin_insert_card(&mut self, lane: usize, index: usize) {
        let Some((lane, index)) = self.settle_card_edit(lane, index) else {
            return;
        };
        if self.kanban_lane_len(lane).is_some_and(|len| index <= len) {
            self.open_card_field(
                CardSlot {
                    lane,
                    index,
                    new: true,
                },
                String::new(),
            );
        }
    }

    /// Edit: an editor on the card's markdown.
    pub fn begin_edit_card(&mut self, lane: usize, index: usize) {
        let Some((lane, index)) = self.settle_card_edit(lane, index) else {
            return;
        };
        let Some(text) = self
            .kanban_card_at(lane, index)
            .map(|c| c.text().to_owned())
        else {
            return;
        };
        self.open_card_field(
            CardSlot {
                lane,
                index,
                new: false,
            },
            text,
        );
    }

    fn open_card_field(&mut self, slot: CardSlot, text: String) {
        self.menu = None;
        self.set_text_focus(false, false);
        let mut field = TextEdit::new(text);
        field.set_caret(field.text.len());
        field.focused = true;
        self.kanban_card_field = field;
        self.kanban_card = Some(slot);
    }

    /// Close the editor and apply its text as one edit. When the model
    /// refuses the text the editor stays open, holding it. Slice 5: an open
    /// lane field is committed the same way (only one of them is open).
    pub(crate) fn commit_card_edit(&mut self) -> CardCommit {
        if self.kanban_lane_edit.is_some() {
            return self.commit_lane_edit();
        }
        let Some(slot) = self.kanban_card else {
            return CardCommit::Closed;
        };
        let text = self.kanban_card_field.text.clone();
        let added = slot.new && !text.trim().is_empty();
        let what = if slot.new {
            "add the card"
        } else {
            "edit the card"
        };
        let applied = text.trim().is_empty()
            || self.edit_kanban(what, |board| apply_card_field(board, slot, &text));
        if !applied {
            return CardCommit::Refused;
        }
        self.cancel_card_edit();
        let commit = CardCommit::Done(added.then_some(slot));
        self.kanban_selected = self.kanban_selected.and_then(|(l, i)| commit.follow(l, i));
        commit
    }

    /// Commit the editor before an action on the card drawn at `index` in
    /// `lane`, and find that card again: a card the commit added at or above
    /// it in the same lane moves it down one, and a lane added at or before
    /// its lane moves it right one. `None` when the commit was refused, so
    /// the action waits.
    pub(crate) fn settle_card_edit(&mut self, lane: usize, index: usize) -> Option<(usize, usize)> {
        self.commit_card_edit().follow(lane, index)
    }

    /// [`State::settle_card_edit`] for an action on a whole lane.
    pub(crate) fn settle_lane_edit(&mut self, lane: usize) -> Option<usize> {
        self.commit_card_edit()
            .follow(lane, 0)
            .map(|(lane, _)| lane)
    }

    /// Esc in the editor: close it, changing nothing.
    pub fn cancel_card_edit(&mut self) {
        self.kanban_card = None;
        self.kanban_card_field = TextEdit::default();
        if let Some(shown) = &mut self.kanban {
            shown.error = None;
        }
    }

    pub fn delete_card(&mut self, lane: usize, index: usize) {
        self.edit_kanban("delete the card", |board| board.delete_card(lane, index));
    }

    pub fn duplicate_card(&mut self, lane: usize, index: usize) {
        let Some(card) = self.kanban_card_at(lane, index) else {
            return;
        };
        let text = card.text().to_owned();
        let done = card.is_done() || self.kanban_lane_is_done(lane);
        self.edit_kanban("duplicate the card", |board| {
            board.add_card(lane, index + 1, &text)?;
            if done {
                board.toggle_done(lane, index + 1)?;
            }
            Ok(())
        });
    }

    /// Split card: each non-blank line becomes a card, in order.
    pub fn split_card(&mut self, lane: usize, index: usize) {
        let Some(card) = self.kanban_card_at(lane, index) else {
            return;
        };
        let lines: Vec<String> = split_lines(card.text())
            .into_iter()
            .map(str::to_owned)
            .collect();
        let done = card.is_done() || self.kanban_lane_is_done(lane);
        let Some((first, rest)) = lines.split_first() else {
            return;
        };
        if rest.is_empty() {
            return;
        }
        self.edit_kanban("split the card", |board| {
            board.edit_card(lane, index, first)?;
            for (n, line) in rest.iter().enumerate() {
                let at = index + 1 + n;
                board.add_card(lane, at, line)?;
                if done {
                    board.toggle_done(lane, at)?;
                }
            }
            Ok(())
        });
    }

    /// The checkbox: flip the card's done mark.
    pub fn toggle_card_done(&mut self, lane: usize, index: usize) {
        let Some((lane, index)) = self.settle_card_edit(lane, index) else {
            return;
        };
        self.edit_kanban("change the done mark", |board| {
            board.toggle_done(lane, index)
        });
    }

    /// Mark all cards done: every open card in `lane`.
    pub fn mark_lane_done(&mut self, lane: usize) {
        let open: Vec<usize> = self
            .kanban_board()
            .and_then(|board| board.lanes().get(lane))
            .map(|l| {
                l.cards()
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| !card.is_done())
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default();
        self.edit_kanban("mark the cards done", |board| {
            open.iter().try_for_each(|&i| board.toggle_done(lane, i))
        });
    }

    /// Run `op` on a copy of the board and keep the copy only when it
    /// succeeds and changed something: one history entry. A refusal leaves
    /// the board as it was, says why on the page and returns `false`.
    pub(crate) fn edit_kanban(
        &mut self,
        what: &str,
        op: impl FnOnce(&mut Kanban) -> io::Result<()>,
    ) -> bool {
        let Some(mut next) = self.kanban_board().cloned() else {
            return true;
        };
        if let Err(e) = op(&mut next) {
            self.log_event("kanban", &format!("could not {what}: {e}"));
            if let Some(shown) = &mut self.kanban {
                shown.error = Some(format!("Could not {what}: {e}"));
            }
            return false;
        }
        let Some(shown) = &mut self.kanban else {
            return true;
        };
        shown.error = None;
        if shown.board.as_ref().map(Kanban::source) == Some(next.source()) {
            return true;
        }
        if let Some(board) = shown.board.replace(next) {
            shown.history.record(board.source().to_owned());
            shown.mark_changed();
        }
        // Slice 6: a press held on a card or lane may now point elsewhere.
        self.kanban_drag = None;
        true
    }

    /// `⌘Z` on the page: put back the text before the newest edit.
    pub fn undo_kanban(&mut self) {
        self.step_kanban_history(true);
    }

    /// `⇧⌘Z` / `⌘Y` on the page: apply the newest undone edit again.
    pub fn redo_kanban(&mut self) {
        self.step_kanban_history(false);
    }

    fn step_kanban_history(&mut self, undo: bool) {
        if self.commit_card_edit() == CardCommit::Refused {
            return;
        }
        // A menu or confirmation names cards and lanes by place, and the
        // undo can move them: it closes rather than act on another one.
        self.menu = None;
        let Some(shown) = &mut self.kanban else {
            return;
        };
        let Some(board) = &shown.board else {
            return;
        };
        let history = &mut shown.history;
        let Some(text) = (if undo {
            history.undo.pop_back()
        } else {
            history.redo.pop()
        }) else {
            return;
        };
        // Every entry was a parsed board's text, so it parses again.
        let Ok(next) = Kanban::parse(&text) else {
            return;
        };
        let current = board.source().to_owned();
        if undo {
            history.redo.push(current);
        } else {
            history.undo.push_back(current);
        }
        shown.board = Some(next);
        shown.error = None;
        shown.mark_changed();
        self.kanban_drag = None;
    }

    /// `⌘S` on the page: commit the editor, then write the kanban once when
    /// it differs from the file. `false` when it could not be written (the
    /// reason is its status); `true` when written or nothing needed writing.
    pub fn save_kanban(&mut self) -> bool {
        if self.commit_card_edit() == CardCommit::Refused {
            if let Some(shown) = &mut self.kanban {
                let what = if self.kanban_lane_edit.is_some() {
                    "the lane being edited was refused"
                } else {
                    "the card being edited was refused"
                };
                shown.status = BoardStatus::Failed(what.to_owned());
            }
            return false;
        }
        let Some(shown) = &self.kanban else {
            return true;
        };
        let Some(board) = shown.board.as_ref().filter(|_| shown.dirty()) else {
            return true;
        };
        let (rel, source, disk, saved) = (
            shown.rel.clone(),
            board.source().to_owned(),
            shown.disk,
            shown.saved.clone(),
        );
        let path = self.vault.root.join(&rel);
        // Slice 7, the shared disk rule: only inside the open vault, and only
        // over the file as it was read. The bytes are compared, not just
        // `(mtime, len)`: another app's edit can keep both and still change
        // the text. `flag`: the refusal is about the file on disk, so the
        // page flags it for the conflict bar.
        let written = match (self.note_id(&rel), disk) {
            _ if !inside_vault(&self.vault.root, &path) => {
                Err(("the kanban is not inside the open vault".to_owned(), false))
            }
            (None, _) => Err(("the note is no longer in the vault".to_owned(), true)),
            (Some(_), None) => Err(("the file could not be checked".to_owned(), true)),
            (Some(id), Some(_)) => match std::fs::read_to_string(&path) {
                Ok(text) if text == saved => match self.vault.write_note(id, &source) {
                    Ok(()) => Ok(id),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        Err(("the file was removed".to_owned(), true))
                    }
                    Err(e) => Err((e.to_string(), false)),
                },
                Ok(_) => Err(("the file changed on disk".to_owned(), true)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    Err(("the file was removed".to_owned(), true))
                }
                Err(e) => Err((e.to_string(), false)),
            },
        };
        let status = match written {
            Ok(id) => {
                self.kanban_written(id, source);
                self.forget_kanban_recovery(&rel);
                self.log_event("kanban", &format!("saved {rel}"));
                BoardStatus::Saved
            }
            Err((e, flag)) => {
                self.log_event("kanban", &format!("{rel} not saved: {e}"));
                if flag && let Some(shown) = &mut self.kanban {
                    shown.check_disk(&path);
                }
                BoardStatus::Failed(e)
            }
        };
        let saved = status == BoardStatus::Saved;
        if let Some(shown) = &mut self.kanban {
            shown.status = status;
        }
        saved
    }

    /// After `source` was written to note `id`: the page holds it as the
    /// file's text, and the note's editor, links and watcher follow the new
    /// file. Text typed in Write and not saved is left as it is (its own
    /// guarded save then finds the file changed), never dropped.
    pub(crate) fn kanban_written(&mut self, id: usize, source: String) {
        if self.selected == Some(id) && self.editor.text == self.saved {
            let _ = self.load_disk_into_editor(id);
        } else {
            self.graph.sync(&self.vault);
            self.refresh_watch();
        }
        let disk = self.vault.disk_state(id).ok();
        if let Some(shown) = &mut self.kanban {
            shown.saved = source;
            shown.disk = disk;
            shown.conflict = None;
        }
    }

    pub(crate) fn kanban_board(&self) -> Option<&Kanban> {
        self.kanban.as_ref()?.board.as_ref()
    }

    fn kanban_lane_len(&self, lane: usize) -> Option<usize> {
        Some(self.kanban_board()?.lanes().get(lane)?.cards().len())
    }

    pub(crate) fn kanban_lane_is_done(&self, lane: usize) -> bool {
        self.kanban_board()
            .and_then(|board| board.lanes().get(lane))
            .is_some_and(|l| l.done_word().is_some())
    }

    pub(crate) fn kanban_card_at(&self, lane: usize, index: usize) -> Option<&muninn_core::Card> {
        self.kanban_board()?.lanes().get(lane)?.cards().get(index)
    }
}

/// The card editor's `text` applied to `board` at `slot`, as committing it
/// does: a new card (born done in a done lane), or the card's new text.
/// Blank text changes nothing.
pub(crate) fn apply_card_field(board: &mut Kanban, slot: CardSlot, text: &str) -> io::Result<()> {
    let CardSlot { lane, index, new } = slot;
    if text.trim().is_empty() {
        return Ok(());
    }
    if !new {
        return board.edit_card(lane, index, text);
    }
    let done = board
        .lanes()
        .get(lane)
        .is_some_and(|l| l.done_word().is_some());
    board.add_card(lane, index, text)?;
    if done {
        board.toggle_done(lane, index)?;
    }
    Ok(())
}

/// A card's lines for Split card: each line without its leading spaces and
/// tabs, blank lines dropped.
fn split_lines(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|line| line.trim_start_matches([' ', '\t']))
        .filter(|line| !line.trim().is_empty())
        .collect()
}

/// Undo and redo chords on the Kanban page, outside a text field.
pub(crate) fn kanban_command(s: &mut State, e: &KeyEvent) -> bool {
    if s.mode != Mode::Kanban || s.kanban.is_none() || !e.mods.cmd {
        return false;
    }
    let rune::Key::Command(c) = &e.key else {
        return false;
    };
    match c.to_ascii_lowercase() {
        'z' if e.mods.shift => s.redo_kanban(),
        'z' => s.undo_kanban(),
        'y' => s.redo_kanban(),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_keeps_each_line_in_order_without_its_lead() {
        assert_eq!(
            split_lines("one\n  two\n\n\tthree 漢字\n   \nfour"),
            ["one", "two", "three 漢字", "four"]
        );
    }

    #[test]
    fn history_keeps_the_newest_entries() {
        let mut history = KanbanHistory::default();
        for n in 0..HISTORY_LIMIT + 5 {
            history.record(n.to_string());
        }
        assert_eq!(history.undo.len(), HISTORY_LIMIT);
        assert_eq!(history.undo.front().map(String::as_str), Some("5"));
        history.redo.push("r".to_owned());
        history.record("next".to_owned());
        assert!(history.redo.is_empty(), "a new edit clears redo");
    }
}

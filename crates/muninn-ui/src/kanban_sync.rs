//! Kanban slice 7: the Kanban page is as safe as Board, through the disk
//! rule they share (`doc_sync.rs`).
//!
//! - **Autosave.** With `"kanban_autosave"` on (the default, as Board's),
//!   unsaved edits on the page are saved once they have rested for the
//!   autosave delay (at least [`AUTOSAVE_MIN`]), or at most every
//!   [`AUTOSAVE_MAX_AGE`] while they keep changing, through the same guarded
//!   save as `⌘S`. Nothing is saved half-way: not while a card editor or
//!   lane field is open, a drag is under way, or the page is flagged. A
//!   refused save is not retried on its own. Off, slice 4's explicit save
//!   is unchanged.
//! - **Watcher.** Every `WATCH_INTERVAL` on the Kanban page the shown
//!   kanban's file is compared (modified time and length) with what the
//!   page last read or wrote, and the vault watcher's reloads ask the same
//!   rule ([`State::kanban_follow_disk`]). Clean and idle: re-read, keeping
//!   the selected card where it still exists and the scroll. Unsaved edits,
//!   or text typed in an open card editor or lane field: flagged, never
//!   re-read. An open field with nothing typed, or a drag: looked at again
//!   once it is over.
//! - **Conflict bar** (in the page header): Keep mine writes the edits over
//!   the file (recreating it when it is gone), Save copy writes them to a
//!   new `Name (conflict YYYYMMDD-HHMMSS).md` beside it and shows that, and
//!   Load theirs (Discard when the file is gone) drops them for what is on
//!   disk. A file renamed outside Muninn is followed when the vault rescan
//!   finds one new note holding exactly the text the page last read.
//! - **Recovery copy.** Unsaved edits, with the open field's text applied as
//!   its commit would, are copied to the SAF-05 store notes and boards use,
//!   once they rest ([`RECOVERY_IDLE`]) or every [`RECOVERY_MAX_AGE`] while
//!   they do not, so a crash, SIGTERM or logout loses at most that. The next
//!   launch lists it; Restore opens it on the Kanban page, unsaved, and
//!   flagged when the file is gone or holds something else.
//! - **Quit.** Closing saves the kanban; when it can't, its recovery copy is
//!   brought up to date. Only when that fails too is the quit refused, with
//!   the reason and Save copy on the page.

use std::io;
use std::path::Path;
use std::time::Duration;

use muninn_core::Kanban;

use crate::board_view::BoardStatus;
use crate::doc_sync::{
    AUTOSAVE_MAX_AGE, AUTOSAVE_MIN, DiskRule, RECOVERY_IDLE, RECOVERY_MAX_AGE, Rest, copy_path,
    file_state, inside_vault, quit_rule,
};
use crate::kanban_edit::apply_card_field;
use crate::kanban_lanes::{LaneField, apply_lane_field};
use crate::kanban_view::ShownKanban;
use crate::state::{WATCH_INTERVAL, conflict_stamp, safe_rel};
use crate::{Mode, State};

/// Why a quit was refused.
const QUIT_REFUSED: &str = "Kanban not saved and no recovery copy could be written";
/// Why a quit was refused while the open field holds text the kanban
/// refuses (slice 4's words).
const FIELD_REFUSED: &str = "Not quitting: the kanban could not be saved. Fix what stopped the save, or undo the edits, then quit again.";

impl State {
    /// The open card editor's or lane field's text, when committing it would
    /// change the kanban: typed work that is not in the model yet.
    pub(crate) fn kanban_typed(&self) -> Option<String> {
        if let Some(slot) = self.kanban_card {
            let text = &self.kanban_card_field.text;
            let changed = !text.trim().is_empty()
                && (slot.new
                    || self
                        .kanban_card_at(slot.lane, slot.index)
                        .is_none_or(|card| card.text() != text));
            return changed.then(|| text.clone());
        }
        let slot = self.kanban_lane_edit?;
        let typed = self.kanban_lane_field.text.trim();
        let lane = self.kanban_board().and_then(|b| b.lanes().get(slot.lane));
        let changed = match slot.field {
            LaneField::New => !typed.is_empty(),
            LaneField::Rename => !typed.is_empty() && lane.is_none_or(|lane| lane.title() != typed),
            LaneField::Limit => {
                let now = lane.and_then(muninn_core::Lane::limit);
                typed != now.map(|n| n.to_string()).unwrap_or_default()
            }
        };
        changed.then(|| self.kanban_lane_field.text.clone())
    }

    /// What a watcher may do with the shown kanban when its file changed on
    /// disk; the rule Board's watchers follow ([`DiskRule`]).
    pub(crate) fn kanban_disk_rule(&self) -> Option<DiskRule> {
        let shown = self.kanban.as_ref()?;
        let busy = self.kanban_card.is_some()
            || self.kanban_lane_edit.is_some()
            || self.kanban_drag.is_some();
        Some(DiskRule::of(
            shown.dirty() || self.kanban_typed().is_some(),
            busy,
        ))
    }

    /// Both watchers (the kanban watcher, and the vault watcher's reload
    /// through `refresh_kanban`) apply the disk rule to the shown kanban
    /// here. `true` when something on the page changed.
    pub(crate) fn kanban_follow_disk(&mut self) -> bool {
        let Some(rule) = self.kanban_disk_rule() else {
            return false;
        };
        let renamed = self.kanban_renamed();
        let root = self.vault.root.clone();
        let Some(shown) = &mut self.kanban else {
            return false;
        };
        let path = root.join(&shown.rel);
        let now = file_state(&path);
        if now.is_some() && now == shown.disk && shown.conflict.is_none() {
            return false;
        }
        match rule {
            DiskRule::Later => false,
            DiskRule::Flag => {
                if let Some(rel) = renamed {
                    shown.disk = file_state(&root.join(&rel));
                    shown.rel.clone_from(&rel);
                    shown.conflict = None;
                    self.kanban_rel = Some(rel);
                    self.log_event("watch", "followed the shown kanban's rename");
                    return true;
                }
                let was = shown.conflict;
                shown.check_disk(&path);
                let flagged = shown.conflict != was;
                if flagged {
                    self.log_event("watch", "shown kanban changed on disk; kept unsaved");
                }
                flagged
            }
            DiskRule::Reload => {
                // A removed file leaves with the vault watcher's rescan.
                let rel = shown.rel.clone();
                let Some(read) = self.read_kanban(rel) else {
                    return false;
                };
                let Some(shown) = &mut self.kanban else {
                    return false;
                };
                if read.saved == shown.saved && !read.dirty() {
                    shown.disk = read.disk;
                    return false;
                }
                self.kanban = Some(read);
                self.prune_kanban_selection();
                self.log_event("watch", "reloaded the shown kanban");
                true
            }
        }
    }

    /// The vault-relative path the shown kanban was renamed to outside
    /// Muninn: its own file is gone and exactly one other note in the vault
    /// holds the text the page last read or wrote.
    pub(crate) fn kanban_renamed(&self) -> Option<String> {
        let shown = self.kanban.as_ref()?;
        if self.vault.root.join(&shown.rel).exists() {
            return None;
        }
        let mut same = self
            .vault
            .notes
            .iter()
            .filter(|note| note.rel != shown.rel && note.source == shown.saved);
        let found = same.next()?;
        same.next().is_none().then(|| found.rel.clone())
    }

    /// A selected card that is no longer on the page is unselected.
    pub(crate) fn prune_kanban_selection(&mut self) {
        let Some((lane, index)) = self.kanban_selected else {
            return;
        };
        if self.kanban_card_at(lane, index).is_none() {
            self.kanban_selected = None;
        }
    }

    /// Per frame, every `WATCH_INTERVAL` on the Kanban page: notice another
    /// app's change to the shown kanban. `true` when the page changed.
    pub(crate) fn kanban_watch_tick(&mut self, t: f64) -> bool {
        if self.mode != Mode::Kanban || t - self.last_kanban_watch < WATCH_INTERVAL {
            return false;
        }
        self.last_kanban_watch = t;
        self.kanban_follow_disk()
    }

    /// Per frame: autosave the shown kanban once its edits have rested.
    /// `true` when its status changed (saved or refused).
    pub(crate) fn kanban_autosave_tick(&mut self, t: f64) -> bool {
        let delay = self.autosave_delay().max(AUTOSAVE_MIN);
        let busy = !self.settings.kanban_autosave
            || self.mode != Mode::Kanban
            || self.kanban_card.is_some()
            || self.kanban_lane_edit.is_some()
            || self.kanban_drag.is_some();
        let Some(shown) = &self.kanban else {
            self.kanban_autosave_wait = None;
            return false;
        };
        if shown.status != BoardStatus::Unsaved || shown.conflict.is_some() {
            self.kanban_autosave_wait = None;
            return false;
        }
        if busy {
            return false;
        }
        let wait = &mut self.kanban_autosave_wait;
        if Rest::wait(wait, shown.generation, t, delay, AUTOSAVE_MAX_AGE).is_some() {
            return false;
        }
        self.kanban_autosave_wait = None;
        let saved = self.save_kanban();
        let what = if saved {
            "autosaved"
        } else {
            "autosave refused; kept unsaved"
        };
        self.log_event("kanban", what);
        true
    }

    /// When `poll` must run again for the Kanban page's timed work, so a
    /// resting window still does it: soon while an autosave or a recovery
    /// copy is pending, and at the watcher's next look while the page shows.
    pub(crate) fn kanban_wake(&self, t: f64) -> Option<Duration> {
        const PENDING: f64 = 0.25;
        let unwritten = self.kanban.as_ref().is_some_and(|shown| {
            let autosave = self.settings.kanban_autosave
                && shown.status == BoardStatus::Unsaved
                && shown.conflict.is_none();
            let copy = shown.dirty() && shown.recovered != Some((shown.generation, None));
            autosave || copy
        });
        let pending =
            unwritten || self.kanban_autosave_wait.is_some() || self.kanban_recovery_wait.is_some();
        let watch = (self.mode == Mode::Kanban)
            .then(|| (WATCH_INTERVAL - (t - self.last_kanban_watch)).max(0.0));
        let next = match (pending.then_some(PENDING), watch) {
            (Some(a), Some(b)) => a.min(b),
            (a, b) => a.or(b)?,
        };
        Some(Duration::from_secs_f64(next))
    }

    /// The kanban's text with the open card editor's or lane field's text
    /// applied as committing it would, for the recovery copy. `Err` when the
    /// model refuses that text (it stays in the field, unsaved).
    fn kanban_pending_text(&self) -> io::Result<Option<String>> {
        let Some(board) = self.kanban_board() else {
            return Ok(None);
        };
        if self.kanban_typed().is_none() {
            return Ok(Some(board.source().to_owned()));
        }
        let mut next: Kanban = board.clone();
        if let Some(slot) = self.kanban_card {
            apply_card_field(&mut next, slot, &self.kanban_card_field.text)?;
        } else if let Some(slot) = self.kanban_lane_edit {
            apply_lane_field(&mut next, slot, self.kanban_lane_field.text.trim())?;
        }
        Ok(Some(next.source().to_owned()))
    }

    /// Keep the shown kanban's recovery copy in step: written when it holds
    /// unsaved edits (or typed text) not copied yet, removed once it holds
    /// none. Errors leave the edits in memory.
    pub(crate) fn kanban_recovery_tick(&mut self) -> io::Result<()> {
        let typed = self.kanban_typed();
        let Some(shown) = &self.kanban else {
            return Ok(());
        };
        let key = (shown.generation, typed.clone());
        if shown.dirty() || typed.is_some() {
            if shown.recovered.as_ref() == Some(&key) {
                return Ok(());
            }
            let rel = shown.rel.clone();
            let Some(text) = self.kanban_pending_text()? else {
                return Ok(());
            };
            self.recovery.save(&rel, &text)?;
            if let Some(shown) = &mut self.kanban {
                shown.recovered = Some(key);
            }
            return Ok(());
        }
        let rel = shown.rel.clone();
        if let Some(shown) = &mut self.kanban
            && shown.recovered.take().is_some()
        {
            self.recovery.clear(&rel);
            self.recovered.retain(|entry| entry.rel != rel);
        }
        Ok(())
    }

    /// Per frame: the recovery copy, once the edits rest. Returns when
    /// `poll` must run again for it.
    pub(crate) fn kanban_recovery_poll(&mut self, t: f64) -> Option<Duration> {
        // A keystroke lands after the frame's poll: look again soon.
        let editing = (self.kanban_card.is_some() || self.kanban_lane_edit.is_some())
            .then(|| Duration::from_secs_f64(RECOVERY_IDLE));
        if self.kanban_drag.is_some() {
            return editing;
        }
        let typed = self.kanban_typed();
        if let Some(shown) = &self.kanban
            && (shown.dirty() || typed.is_some())
        {
            let key = (shown.generation, typed);
            if shown.recovered.as_ref() != Some(&key) {
                let wait = &mut self.kanban_recovery_wait;
                if let Some(due) = Rest::wait(wait, key, t, RECOVERY_IDLE, RECOVERY_MAX_AGE) {
                    return Some(Duration::from_secs_f64(due.max(0.0)));
                }
            }
        }
        self.kanban_recovery_wait = None;
        if let Err(e) = self.kanban_recovery_tick() {
            let message = format!("Could not persist kanban recovery copy: {e}");
            if self.error.as_deref() != Some(message.as_str()) {
                self.error = Some(message);
            }
        }
        editing
    }

    /// The close and quit flush for the kanban: save it, else bring its
    /// recovery copy up to date. `false` only when neither worked; the quit
    /// is then refused (the reason is in [`State::error`], on the page).
    pub(crate) fn flush_kanban_before_quit(&mut self) -> bool {
        let saved = self.save_kanban();
        if !saved && self.kanban_pending_text().is_err() {
            // Text the model refuses has no kanban to save or copy: slice 4's
            // refusal stands, and the editor keeps it.
            self.error = Some(FIELD_REFUSED.to_owned());
            return false;
        }
        let copied = self.kanban_recovery_tick();
        let typed = self.kanban_typed();
        let current = self.kanban.as_ref().is_some_and(|shown| {
            shown.recovered.as_ref() == Some(&(shown.generation, typed.clone()))
        });
        match quit_rule(saved, &copied, current, "kanban") {
            Ok(()) => {
                if !saved {
                    self.log_event("kanban", "not saved; kept in the recovery copy");
                }
                true
            }
            Err(why) => {
                self.error = Some(format!(
                    "{QUIT_REFUSED}: {why}. Use Save copy or fix the folder, then quit again."
                ));
                false
            }
        }
    }

    /// Whether closing now would lose nothing on the Kanban page: it is
    /// clean, or its recovery copy holds the latest edits.
    pub(crate) fn kanban_can_quit(&self) -> bool {
        let typed = self.kanban_typed();
        self.kanban.as_ref().is_none_or(|shown| {
            (!shown.dirty() && typed.is_none())
                || shown.recovered.as_ref() == Some(&(shown.generation, typed))
        })
    }

    /// Keep mine: write the edits to the kanban's own path even though the
    /// file changed or was removed. Still only inside the open vault.
    pub fn kanban_keep_mine(&mut self) -> bool {
        if self.commit_card_edit() == crate::kanban_edit::CardCommit::Refused {
            return false;
        }
        let root = self.vault.root.clone();
        let Some(shown) = &mut self.kanban else {
            return false;
        };
        let Some(board) = &shown.board else {
            return false;
        };
        let path = root.join(&shown.rel);
        let (rel, source) = (shown.rel.clone(), board.source().to_owned());
        if !inside_vault(&root, &path) {
            shown.status =
                BoardStatus::Failed("the kanban is not inside the open vault".to_owned());
            return false;
        }
        let written = match self.vault.notes.iter().position(|note| note.rel == rel) {
            Some(id) if path.exists() => self.vault.write_note(id, &source).map(|()| Some(id)),
            _ => path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| board.save(&path))
                .map(|()| None),
        };
        let id = match written {
            Ok(Some(id)) => id,
            Ok(None) => {
                // A recreated file: index it again, keeping the page.
                self.refresh_index_only();
                let Some(id) = self.note_id(&rel) else {
                    return false;
                };
                id
            }
            Err(e) => {
                if let Some(shown) = &mut self.kanban {
                    shown.status = BoardStatus::Failed(e.to_string());
                }
                return false;
            }
        };
        self.kanban_written(id, source);
        if let Some(shown) = &mut self.kanban {
            shown.status = BoardStatus::Saved;
        }
        let _ = self.kanban_recovery_tick();
        self.log_event("kanban", "kept mine over the file on disk");
        true
    }

    /// Save copy: write the edits to a new note beside the kanban (or at the
    /// vault root when its folder is gone) and show that. The original file,
    /// whatever it holds now, is left alone.
    pub fn kanban_save_copy(&mut self) -> bool {
        if self.commit_card_edit() == crate::kanban_edit::CardCommit::Refused {
            return false;
        }
        let root = self.vault.root.clone();
        let Some(shown) = &mut self.kanban else {
            return false;
        };
        let Some(board) = &shown.board else {
            return false;
        };
        let path = root.join(&shown.rel);
        if !inside_vault(&root, &path) {
            return false;
        }
        let copy = copy_path(&root, &path, &conflict_stamp());
        if let Err(e) = board.save(&copy) {
            shown.status = BoardStatus::Failed(format!("could not save a copy: {e}"));
            return false;
        }
        let new = rel_of(&root, &copy);
        let old = std::mem::replace(&mut shown.rel, new.clone());
        shown.saved = board.source().to_owned();
        shown.disk = file_state(&copy);
        shown.status = BoardStatus::Saved;
        shown.conflict = None;
        shown.recovered = None;
        self.kanban_rel = Some(new);
        self.recovery.clear(&old);
        self.recovered.retain(|entry| entry.rel != old);
        self.refresh_index_only();
        self.log_event("kanban", "saved a conflict copy");
        self.save_settings();
        true
    }

    /// Load theirs (or Discard when the file is gone): drop the unsaved
    /// edits and their recovery copy for what is on disk.
    pub fn kanban_discard(&mut self) -> bool {
        let Some(rel) = self.kanban.as_ref().map(|shown| shown.rel.clone()) else {
            return false;
        };
        self.cancel_card_edit();
        self.cancel_lane_edit();
        self.kanban_drag = None;
        self.kanban = None;
        self.kanban_autosave_wait = None;
        self.kanban_recovery_wait = None;
        self.recovery.clear(&rel);
        self.recovered.retain(|entry| entry.rel != rel);
        self.log_event("kanban", "dropped the unsaved edits for the file on disk");
        self.refresh_kanban();
        self.prune_kanban_selection();
        true
    }

    /// Restore a kanban from its recovery copy (the startup banner's Restore
    /// for a kanban note's text): show it on the Kanban page, unsaved.
    pub(crate) fn restore_kanban_recovery(&mut self, rel: &str, text: &str) -> bool {
        let root = self.vault.root.clone();
        let path = root.join(rel);
        if !safe_rel(rel) || !inside_vault(&root, &path) {
            self.error = Some(format!("Refusing unsafe recovery path {rel}"));
            return false;
        }
        let Ok(board) = Kanban::parse(text) else {
            return false;
        };
        if self.kanban_dirty() && !self.save_kanban() {
            self.error =
                Some("Save or discard the current kanban before restoring another".to_owned());
            return false;
        }
        self.set_mode(Mode::Kanban);
        let on_disk = std::fs::read_to_string(&path).ok();
        let same = on_disk.as_deref() == Some(text);
        let conflict = match &on_disk {
            None => Some(crate::DocConflict::Missing),
            Some(_) if !same => Some(crate::DocConflict::Changed),
            Some(_) => None,
        };
        let mut shown = ShownKanban {
            rel: rel.to_owned(),
            board: Some(board),
            saved: on_disk.unwrap_or_default(),
            // Unknown unless the file holds exactly the copy: a plain save
            // is refused and the conflict bar decides.
            disk: same.then(|| file_state(&path)).flatten(),
            status: BoardStatus::Clean,
            history: crate::KanbanHistory::default(),
            error: None,
            conflict,
            generation: 0,
            recovered: None,
        };
        shown.mark_changed();
        shown.recovered = Some((shown.generation, None));
        self.kanban = Some(shown);
        self.kanban_rel = Some(rel.to_owned());
        self.kanban_selected = None;
        self.recovered.retain(|entry| entry.rel != rel);
        self.log_event("recovery", &format!("restored kanban {rel}"));
        true
    }

    /// The Write hand-off: the editor's unsaved text now lives on the Kanban
    /// page, so the editor shows the file's text again, clean.
    pub(crate) fn give_editor_text_to_kanban(&mut self) {
        let saved = self.saved.clone();
        self.editor.replace_text(saved);
        self.editor.anchor = 0;
        self.editor.focus = 0;
        self.dirty_at = None;
        if self
            .conflict
            .as_ref()
            .is_some_and(|c| Some(c.note_id) == self.selected)
        {
            self.conflict = None;
        }
    }
}

/// `path`'s vault-relative form with `/` separators.
fn rel_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

//! #38 review B1/B2: unsaved board drawing is never dropped or lost.
//!
//! - **Never dropped.** [`State::refresh_boards`] keeps a dirty board open
//!   when its file was removed, renamed or changed outside Muninn (on
//!   entering Board, after a watcher reload). The heading flags it
//!   ([`crate::BoardConflict`]) and offers the note conflict bar's choices:
//!   **Keep mine** writes the drawing to the board's own path (recreating it
//!   if it was removed; still only inside the vault it was opened from and
//!   that is open now, through s1's atomic save), **Save copy** writes it to
//!   a new sibling `Title (conflict YYYYMMDD-HHMMSS).excalidraw` and makes
//!   that the open board, and **Load theirs** (**Discard** when the file is
//!   gone) drops the drawing for what is on disk.
//! - **Recovery copy.** While a board is dirty its drawing is copied, after
//!   each change, to the same SAF-05 store notes use
//!   (`$XDG_STATE_HOME/muninn/recovery/<vault-id>~<sanitized-rel>.txt` plus
//!   its `.json` sidecar, keyed by the vault and the board's vault-relative
//!   path; temp file, fsync, rename). The copy goes once the board is saved or discarded. On launch
//!   the recovery banner lists it beside notes; Restore opens it on Board,
//!   unsaved and flagged when the file is gone or holds something else, so a
//!   plain ⌘S is refused and only Keep mine writes over it.
//! - **Quit.** Closing or quitting saves the board if it can; if it cannot,
//!   the recovery copy is brought up to date first. Only if that copy cannot
//!   be written either is the quit refused (the window comes back with the
//!   reason), so a dirty board is never discarded silently.
//! - **Contained.** Every write (save, Keep mine, Save copy) and every
//!   restore goes through `doc_sync::inside_vault`: a recovery record naming `..`, an
//!   absolute path, or a symlink out of the vault is refused (#38 review M2).

use std::io;
use std::time::Duration;

use muninn_core::Board;

use crate::board_tools::fingerprint;
use crate::board_view::{BoardStatus, OpenBoard};
use crate::doc_sync::{RECOVERY_IDLE, RECOVERY_MAX_AGE, Rest, copy_path, inside_vault, quit_rule};
use crate::state::conflict_stamp;
use crate::{Mode, State};

/// #40 recheck B1: after a board press or key, poll again this soon, so an
/// edit (or an editor opened) by it is seen by the recovery copy without
/// waiting for more input.
pub(crate) const BOARD_EDIT_WAKE: Duration = Duration::from_millis(50);

/// Why a quit was refused.
const QUIT_REFUSED: &str = "Board not saved and no recovery copy could be written";

/// `true` for a vault-relative path that names a board.
#[must_use]
pub(crate) fn is_board_rel(rel: &str) -> bool {
    rel.to_ascii_lowercase().ends_with(".excalidraw")
}

impl OpenBoard {
    /// The board's vault-relative path with `/` separators.
    pub(crate) fn rel(&self) -> Option<String> {
        let rel = self.path.strip_prefix(&self.vault_root).ok()?;
        Some(rel.to_string_lossy().replace('\\', "/"))
    }
}

impl State {
    /// Keep the open board's recovery copy in step: written when the drawing
    /// is unsaved and changed since the last copy, removed once it is not
    /// dirty. Cheap when nothing changed. Errors leave the drawing in memory.
    pub(crate) fn board_recovery_tick(&mut self) -> io::Result<()> {
        // #40 review M1: text typed but not committed is unsaved drawing too.
        let pending = self.board_text_pending();
        let Some(open) = &self.board_open else {
            return Ok(());
        };
        let Some(rel) = open.rel() else {
            return Ok(());
        };
        if open.dirty() || pending.is_some() {
            if open.recovered_at == Some(open.generation) && self.board_recovered_text == pending {
                return Ok(());
            }
            let generation = open.generation;
            let Some(json) = self.board_json_with_pending_text() else {
                return Ok(());
            };
            self.recovery.save(&rel, &json)?;
            if let Some(open) = &mut self.board_open {
                open.recovered_at = Some(generation);
            }
            self.board_recovered_text = pending;
            return Ok(());
        }
        self.board_recovered_text = None;
        let Some(open) = &mut self.board_open else {
            return Ok(());
        };
        if open.recovered_at.take().is_some() {
            self.recovery.clear(&rel);
            self.recovered.retain(|entry| entry.rel != rel);
        }
        Ok(())
    }

    /// Per frame: the recovery copy, reporting a failed write once. Returns
    /// when `poll` must run again (#40 recheck B1): a resting window polls
    /// only at a wake it asked for, so a deferred copy, or an open editor
    /// whose next keystroke lands after this poll, would otherwise wait for
    /// the next input, and a crash would lose it.
    pub(crate) fn board_recovery_poll(&mut self, t: f64) -> Option<Duration> {
        // While the editor is open, look again soon: a keystroke is applied
        // after the frame's poll, so this poll may not have seen it yet.
        let editing = self
            .board_text
            .is_some()
            .then(|| Duration::from_secs_f64(RECOVERY_IDLE));
        // Slice 5: a move drag changes the model every frame; copy once it
        // lands instead of writing (and fsyncing) per frame.
        if self
            .board_open
            .as_ref()
            .is_some_and(|open| open.edit.gesture.is_some())
        {
            return editing;
        }
        // Slice 7: a stream of edits (a held arrow key) and text being typed
        // (#40 review M1) are copied once they rest, or every
        // RECOVERY_MAX_AGE while they do not. A big board's copy is
        // megabytes, written and fsynced each time.
        let pending = self.board_text_pending();
        if let Some(open) = &self.board_open {
            let drawing = open.dirty() && open.recovered_at != Some(open.generation);
            let typing = pending.is_some() && self.board_recovered_text != pending;
            if drawing || typing {
                let key = (open.generation, pending);
                let wait = &mut self.board_recovery_wait;
                if let Some(due) = Rest::wait(wait, key, t, RECOVERY_IDLE, RECOVERY_MAX_AGE) {
                    return Some(Duration::from_secs_f64(due.max(0.0)));
                }
            }
        }
        self.board_recovery_wait = None;
        if let Err(e) = self.board_recovery_tick() {
            let message = format!("Could not persist board recovery copy: {e}");
            if self.error.as_deref() != Some(message.as_str()) {
                self.error = Some(message);
            }
        }
        editing
    }

    /// Keep mine: write the drawing to the board's own path even though the
    /// file changed or was removed. Still refused outside the open vault.
    pub fn board_keep_mine(&mut self) -> bool {
        let root = self.vault.root.clone();
        let Some(open) = &mut self.board_open else {
            return false;
        };
        let Some(model) = &open.model else {
            return false;
        };
        if open.vault_root != root {
            open.status = BoardStatus::Failed("the board belongs to another vault".to_owned());
            return false;
        }
        if !inside_vault(&root, &open.path) {
            open.status = BoardStatus::Failed("the board is not inside the open vault".to_owned());
            return false;
        }
        if let Some(parent) = open.path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            open.status = BoardStatus::Failed(e.to_string());
            return false;
        }
        let json = model.to_json();
        if let Err(e) = model.save(&open.path) {
            open.status = BoardStatus::Failed(e.to_string());
            return false;
        }
        open.disk = Some(fingerprint(&json));
        open.status = BoardStatus::Saved;
        open.conflict = None;
        let _ = self.board_recovery_tick();
        self.log_event("board", "kept mine over the file on disk");
        self.relist_boards();
        true
    }

    /// Save copy: write the drawing to a new sibling file (or the vault root
    /// when its folder is gone) and make that the open board. The original
    /// file, whatever it holds now, is left alone.
    pub fn board_save_copy(&mut self) -> bool {
        let root = self.vault.root.clone();
        let Some(open) = &mut self.board_open else {
            return false;
        };
        if open.model.is_none() || open.vault_root != root || !inside_vault(&root, &open.path) {
            return false;
        }
        let old_rel = open.rel();
        let copy = copy_path(&root, &open.path, &conflict_stamp());
        let saved = std::mem::replace(&mut open.path, copy.clone());
        // `write_board` checks the fingerprint of the target: a new file has
        // none, so write directly through s1's atomic save.
        let Some(model) = &open.model else {
            return false;
        };
        let json = model.to_json();
        if let Err(e) = model.save(&copy) {
            open.path = saved;
            open.status = BoardStatus::Failed(format!("could not save a copy: {e}"));
            return false;
        }
        open.disk = Some(fingerprint(&json));
        open.status = BoardStatus::Saved;
        open.conflict = None;
        open.recovered_at = None;
        if let Some(rel) = old_rel {
            self.recovery.clear(&rel);
            self.recovered.retain(|entry| entry.rel != rel);
        }
        self.board_selected = Some(copy);
        self.log_event("board", "saved a conflict copy");
        self.relist_boards();
        self.save_settings();
        true
    }

    /// Load theirs (or Discard when the file is gone): drop the unsaved
    /// drawing and its recovery copy for what is on disk.
    pub fn board_discard(&mut self) -> bool {
        let Some(open) = &mut self.board_open else {
            return false;
        };
        open.status = BoardStatus::Clean;
        open.conflict = None;
        let _ = self.board_recovery_tick();
        let gone = self
            .board_open
            .as_ref()
            .is_some_and(|open| !open.path.exists());
        if gone {
            self.board_open = None;
            self.board_selected = None;
        }
        self.refresh_boards();
        true
    }

    /// Restore a board from its recovery copy (the startup banner's Restore
    /// for a `.excalidraw` path): open it on Board, unsaved.
    pub(crate) fn restore_board_recovery(&mut self, rel: &str) -> bool {
        // #38 review M2: the record names the path it will be written to;
        // the same containment as a note's restore, and symlinks followed.
        if !crate::state::safe_rel(rel)
            || !inside_vault(&self.vault.root, &self.vault.root.join(rel))
        {
            self.error = Some(format!("Refusing unsafe recovery path {rel}"));
            return false;
        }
        let text = match self.recovery.load(rel) {
            Ok(text) => text,
            Err(e) => {
                self.error = Some(format!("Could not read recovery copy: {e}"));
                return false;
            }
        };
        let model = match Board::from_json(&text) {
            Ok(model) => model,
            Err(e) => {
                self.error = Some(format!("Could not read recovery copy: {e}"));
                return false;
            }
        };
        if self.board_dirty() && !self.save_board() {
            self.error =
                Some("Save or discard the current board before restoring another".to_owned());
            return false;
        }
        let path = self.vault.root.join(rel);
        self.set_mode(Mode::Board);
        let open = OpenBoard::recovered(path.clone(), model, self.vault.root.clone());
        self.board_open = Some(open);
        self.board_selected = Some(path);
        self.recovered.retain(|entry| entry.rel != rel);
        self.log_event("recovery", &format!("restored board {rel}"));
        true
    }

    /// The close and quit flush for the board: save it, else bring its
    /// recovery copy up to date. `false` only when neither worked; the quit
    /// must then be refused (the reason is in [`State::error`]).
    pub(crate) fn flush_board_before_quit(&mut self) -> bool {
        let saved = self.save_board();
        let copied = self.board_recovery_tick();
        let current = self
            .board_open
            .as_ref()
            .is_some_and(|open| open.recovered_at == Some(open.generation));
        let why = match quit_rule(saved, &copied, current, "board") {
            Ok(()) => {
                if !saved {
                    self.log_event("board", "not saved; kept in the recovery copy");
                }
                return true;
            }
            Err(why) => why,
        };
        self.error = Some(format!(
            "{QUIT_REFUSED}: {why}. Use Save copy or fix the folder, then quit again."
        ));
        false
    }

    /// Whether closing now would lose nothing on the board (and the Kanban
    /// page): it is saved, clean, or its recovery copy holds the latest
    /// drawing.
    #[must_use]
    pub fn can_quit(&self) -> bool {
        let pending = self.board_text_pending();
        let board = self.board_open.as_ref().is_none_or(|open| {
            (!open.dirty() && pending.is_none())
                || (open.recovered_at == Some(open.generation)
                    && self.board_recovered_text == pending)
        });
        // Kanban slice 7: the Kanban page's edits count too.
        board && self.kanban_can_quit()
    }
}

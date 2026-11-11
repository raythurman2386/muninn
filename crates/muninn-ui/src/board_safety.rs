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
//!   restore goes through [`inside_vault`]: a recovery record naming `..`, an
//!   absolute path, or a symlink out of the vault is refused (#38 review M2).

use std::io;
use std::path::{Component, Path, PathBuf};

use muninn_core::Board;

use crate::board_tools::fingerprint;
use crate::board_view::{BoardStatus, OpenBoard};
use crate::state::conflict_stamp;
use crate::{Mode, State};

/// Why a quit was refused.
/// #40 review M1: the recovery copy of text being typed waits until the
/// typing has rested this long, or at most this long while it never does.
const TYPING_REST: f64 = 0.5;
const TYPING_MAX_AGE: f64 = 2.0;

const QUIT_REFUSED: &str = "Board not saved and no recovery copy could be written";

/// #38 review M2: `path` lies inside the vault `root`: below it through
/// plain components only (no `..`, `.`, root or prefix), and, following the
/// symlinks of whatever part of it exists, still below the canonical root.
#[must_use]
pub(crate) fn inside_vault(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    if rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return false;
    }
    let Ok(base) = std::fs::canonicalize(root) else {
        return false;
    };
    let mut probe = path.to_path_buf();
    loop {
        if let Ok(real) = std::fs::canonicalize(&probe) {
            return real.starts_with(&base);
        }
        if !probe.pop() {
            return false;
        }
    }
}

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

    /// Per frame: the recovery copy, reporting a failed write once.
    pub(crate) fn board_recovery_poll(&mut self) {
        // Slice 5: a move drag changes the model every frame; copy once it
        // lands instead of writing (and fsyncing) per frame.
        if self
            .board_open
            .as_ref()
            .is_some_and(|open| open.edit.gesture.is_some())
        {
            return;
        }
        // #40 review M1: text being typed is copied once the typing rests
        // (0.5 s, at most every 2 s), not on every keystroke.
        if let Some(text) = self.board_text_pending()
            && self.board_recovered_text.as_ref() != Some(&text)
            && !self
                .board_open
                .as_ref()
                .is_some_and(|o| o.dirty() && o.recovered_at != Some(o.generation))
        {
            let now = self.now;
            let wait = self.board_text_wait.get_or_insert((text.clone(), now, now));
            if wait.0 != text {
                wait.0 = text;
                wait.2 = now;
            }
            if now - wait.2 < TYPING_REST && now - wait.1 < TYPING_MAX_AGE {
                return;
            }
        }
        self.board_text_wait = None;
        if let Err(e) = self.board_recovery_tick() {
            let message = format!("Could not persist board recovery copy: {e}");
            if self.error.as_deref() != Some(message.as_str()) {
                self.error = Some(message);
            }
        }
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
        let stem = open
            .path
            .file_stem()
            .map_or_else(|| "Board".to_owned(), |s| s.to_string_lossy().into_owned());
        let folder = open
            .path
            .parent()
            .filter(|p| p.is_dir() && inside_vault(&root, p))
            .map_or_else(|| root.clone(), PathBuf::from);
        let stamp = conflict_stamp();
        let mut copy = folder.join(format!("{stem} (conflict {stamp}).excalidraw"));
        let mut n = 2;
        while copy.exists() {
            copy = folder.join(format!("{stem} (conflict {stamp} {n}).excalidraw"));
            n += 1;
        }
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
        if saved {
            return true;
        }
        let kept = copied.is_ok()
            && self
                .board_open
                .as_ref()
                .is_some_and(|open| open.recovered_at == Some(open.generation));
        if kept {
            self.log_event("board", "not saved; kept in the recovery copy");
            return true;
        }
        let why = match copied {
            Err(e) => e.to_string(),
            Ok(()) => "the board could not be read".to_owned(),
        };
        self.error = Some(format!(
            "{QUIT_REFUSED}: {why}. Use Save copy or fix the folder, then quit again."
        ));
        false
    }

    /// Whether closing now would lose nothing on the board: it is saved,
    /// clean, or its recovery copy holds the latest drawing.
    #[must_use]
    pub fn can_quit(&self) -> bool {
        let pending = self.board_text_pending();
        self.board_open.as_ref().is_none_or(|open| {
            (!open.dirty() && pending.is_none())
                || (open.recovered_at == Some(open.generation)
                    && self.board_recovered_text == pending)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::inside_vault;

    fn temp(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("muninn-inside-vault-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn plain_paths_inside_pass_and_dot_dot_or_absolute_fail() {
        let root = temp("plain");
        assert!(inside_vault(&root, &root.join("a/b.excalidraw")));
        assert!(inside_vault(&root, &root.join("new.excalidraw")));
        assert!(!inside_vault(&root, &root.join("../x.excalidraw")));
        assert!(!inside_vault(&root, &root.join("a/../../x.excalidraw")));
        assert!(!inside_vault(
            &root,
            std::path::Path::new("/tmp/x.excalidraw")
        ));
        assert!(!inside_vault(&root, &root));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// #38 recheck: a folder inside the vault that is a symlink out of it.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_folder_pointing_out_of_the_vault_is_outside() {
        let (root, out) = (temp("link-root"), temp("link-out"));
        std::os::unix::fs::symlink(&out, root.join("escape")).unwrap();
        std::fs::create_dir_all(root.join("real")).unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("alias")).unwrap();
        // A missing file below the link resolves through its existing parent.
        assert!(!inside_vault(&root, &root.join("escape/new.excalidraw")));
        std::fs::write(out.join("there.excalidraw"), "{}").unwrap();
        assert!(!inside_vault(&root, &root.join("escape/there.excalidraw")));
        // A link that stays inside the vault is fine.
        assert!(inside_vault(&root, &root.join("alias/new.excalidraw")));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&out);
    }
}

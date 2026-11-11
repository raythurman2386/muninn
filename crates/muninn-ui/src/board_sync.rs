//! Board slice 7: autosave and the board watcher.
//!
//! - **Autosave.** An open board with unsaved drawing is written once it has
//!   rested for the autosave delay (at least [`BOARD_AUTOSAVE_MIN`]; a board
//!   file can be megabytes), or at most every [`BOARD_AUTOSAVE_MAX_AGE`]
//!   while it keeps changing. It goes through the same guarded, atomic save
//!   as `⌘S` (`State::save_board`), so a file that another app changed or
//!   removed is never written over: the save is refused and the conflict bar
//!   (Keep mine / Save copy / Load theirs) decides. A move drag, a drawing
//!   drag or an open text editor is never saved half-way. A failed save is not
//!   retried on its own; the bar or `⌘S` does that. `"board_autosave":
//!   false` in `settings.json` turns it off (`⌘S` and the quit save remain).
//! - **Watcher.** Every `WATCH_INTERVAL` while Board is showing, each listed
//!   board's file metadata (modified time and length; no contents) is
//!   compared with the last look. The open board, when it changed: a clean
//!   board is re-read (the view stays; a write that only echoes Muninn's own
//!   bytes is skipped by its fingerprint), and a board with unsaved drawing is
//!   flagged, never dropped (the s4 rules). Any other changed board has its
//!   list entry re-read. New and removed boards come from the vault watcher,
//!   whose folder times change then.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::board_view::BoardStatus;
use crate::watch::FileState;
use crate::{Mode, State};

/// The shortest rest before a board is autosaved.
pub(crate) const BOARD_AUTOSAVE_MIN: f64 = 1.0;
/// While the drawing never rests, it is still saved this often.
pub(crate) const BOARD_AUTOSAVE_MAX_AGE: f64 = 10.0;

/// A file's `(modified, length)`, or `None` when it cannot be read.
pub(crate) fn file_state(path: &Path) -> Option<FileState> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok(), meta.len()))
}

/// What a watcher may do with the open board when its file changed on disk.
/// Both watchers (the board watcher and the vault watcher's
/// `refresh_boards`) ask [`State::open_board_disk_rule`], so they cannot
/// drift apart (#41 review GK2b: an atomic save reaches the vault watcher
/// first, and it used to reload under the open text editor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenBoardDiskRule {
    /// Unsaved drawing, or text typed into the editor and not committed:
    /// flag their change (`check_disk`), never reload.
    Flag,
    /// A gesture, a draft or an open editor with nothing typed: do not
    /// reload under it (the editor's commit would write the old text back
    /// over theirs); look again once it is over.
    Later,
    /// Clean and idle: re-read the file.
    Reload,
}

impl State {
    /// The rule for the open board, if one is open. See [`OpenBoardDiskRule`].
    pub(crate) fn open_board_disk_rule(&self) -> Option<OpenBoardDiskRule> {
        let open = self.board_open.as_ref()?;
        Some(if open.dirty() || self.board_text_pending().is_some() {
            OpenBoardDiskRule::Flag
        } else if open.edit.gesture.is_some() || open.draft.is_some() || self.board_text.is_some() {
            OpenBoardDiskRule::Later
        } else {
            OpenBoardDiskRule::Reload
        })
    }

    /// Per frame: autosave the open board once its drawing has rested.
    /// `true` when the status line changed (saved or failed).
    pub(crate) fn board_autosave_tick(&mut self, t: f64) -> bool {
        let delay = self.autosave_delay().max(BOARD_AUTOSAVE_MIN);
        let busy = self.board_text.is_some() || !self.settings.board_autosave;
        let Some(open) = &self.board_open else {
            self.board_autosave_wait = None;
            return false;
        };
        if open.status != BoardStatus::Unsaved
            || open.conflict.is_some()
            || open.edit.gesture.is_some()
            || open.draft.is_some()
            || busy
        {
            // Pending until the gesture ends; a new change restarts the rest.
            if open.status != BoardStatus::Unsaved || open.conflict.is_some() {
                self.board_autosave_wait = None;
            }
            return false;
        }
        let generation = open.generation;
        let wait = self.board_autosave_wait.get_or_insert((generation, t, t));
        if wait.0 != generation {
            wait.0 = generation;
            wait.2 = t;
        }
        if t - wait.2 < delay && t - wait.1 < BOARD_AUTOSAVE_MAX_AGE {
            return false;
        }
        self.board_autosave_wait = None;
        let saved = self.save_board();
        self.log_event(
            "board",
            if saved {
                "autosaved"
            } else {
                "autosave refused; kept unsaved"
            },
        );
        true
    }

    /// Per frame, every `WATCH_INTERVAL` on Board: notice boards another app
    /// changed. `true` when something on screen changed.
    pub(crate) fn board_watch_tick(&mut self, t: f64) -> bool {
        if self.mode != Mode::Board {
            return false;
        }
        if t - self.last_board_watch < crate::state::WATCH_INTERVAL {
            return false;
        }
        self.last_board_watch = t;
        let mut paths: Vec<PathBuf> = self.boards.iter().map(|b| b.file.path.clone()).collect();
        if let Some(open) = &self.board_open
            && !paths.contains(&open.path)
        {
            paths.push(open.path.clone());
        }
        let now: BTreeMap<PathBuf, Option<FileState>> = paths
            .into_iter()
            .map(|p| {
                let state = file_state(&p);
                (p, state)
            })
            .collect();
        let before = std::mem::replace(&mut self.board_stats, now);
        let open_path = self.board_open.as_ref().map(|open| open.path.clone());
        // Other boards: changed since the last look. The open board also on
        // the first look (it may have changed since it was read; its
        // fingerprint makes that check cheap to get right).
        let changed: Vec<PathBuf> = self
            .board_stats
            .iter()
            .filter(|(path, state)| match before.get(*path) {
                Some(old) => old != *state,
                None => open_path.as_ref() == Some(*path),
            })
            .map(|(path, _)| path.clone())
            .collect();
        // #41 review GK2: text typed into the open editor and not committed
        // yet is unsaved work too, so their change raises the conflict; and
        // an open editor is never reloaded under, even with nothing typed
        // (its commit would write the old text over theirs).
        let rule = self.open_board_disk_rule();
        let mut visible = false;
        for path in changed {
            let open = self.board_open.as_mut().filter(|open| open.path == path);
            if let Some(open) = open {
                if rule == Some(OpenBoardDiskRule::Flag) {
                    let was = open.conflict;
                    open.check_disk();
                    if open.conflict != was {
                        self.log_event("watch", "open board changed on disk; kept unsaved");
                        visible = true;
                    }
                    continue;
                }
                if rule == Some(OpenBoardDiskRule::Later) {
                    // Look again next time, once the gesture is over or the
                    // editor has closed.
                    self.board_stats.remove(&path);
                    continue;
                }
                let disk = open.disk;
                open.reload_from_watcher();
                if open.disk != disk {
                    self.log_event("watch", "reloaded the open board");
                    self.refresh_board_summary(&path);
                    visible = true;
                }
                continue;
            }
            self.refresh_board_summary(&path);
            visible = true;
        }
        visible
    }

    /// When the next poll is needed for slice 7's timed work, so a resting
    /// window (which polls only at a wake it asked for) still runs it: soon
    /// while an autosave, a recovery copy or a settings write is pending,
    /// and at the watcher's next look while Board shows.
    pub(crate) fn board_wake(&self, t: f64) -> Option<std::time::Duration> {
        const PENDING: f64 = 0.25;
        // An unsaved board that autosave or the recovery copy has yet to
        // reach counts from its first change, before either has started its
        // wait (a key's change lands after this frame's poll).
        let unwritten = self.board_open.as_ref().is_some_and(|open| {
            let autosave = self.settings.board_autosave
                && open.status == BoardStatus::Unsaved
                && open.conflict.is_none();
            let copy = open.dirty() && open.recovered_at != Some(open.generation);
            autosave || copy
        });
        let pending = unwritten
            || self.board_autosave_wait.is_some()
            || self.board_recovery_wait.is_some()
            || self.settings_due.is_some();
        let watch = (self.mode == Mode::Board)
            .then(|| (crate::state::WATCH_INTERVAL - (t - self.last_board_watch)).max(0.0));
        let next = match (pending.then_some(PENDING), watch) {
            (Some(a), Some(b)) => a.min(b),
            (a, b) => a.or(b)?,
        };
        Some(std::time::Duration::from_secs_f64(next))
    }

    /// After Muninn itself wrote `path`: the board watcher and the vault
    /// watcher's folder time take the new state as theirs, so Muninn's own
    /// save is not mistaken for another app's (and does not rescan the vault).
    pub(crate) fn board_written(&mut self, path: &Path) {
        if self.board_stats.contains_key(path) {
            self.board_stats
                .insert(path.to_path_buf(), file_state(path));
        }
        let Some(parent) = path.parent() else {
            return;
        };
        let rel = parent
            .strip_prefix(&self.vault.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .ok();
        if let (Some(rel), Some(watched)) = (rel, self.watched.as_mut())
            && watched.dirs.contains_key(&rel)
        {
            let mtime = std::fs::metadata(parent)
                .ok()
                .and_then(|m| m.modified().ok());
            watched.dirs.insert(rel, mtime);
        }
    }
}

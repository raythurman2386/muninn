//! Kanban slice 7: the disk rule Board and the Kanban page share.
//!
//! A board (`.excalidraw`) and a kanban note are both documents shown on a
//! page with unsaved edits held in memory. Both follow one rule, set out
//! here and used by `board_sync.rs` / `board_safety.rs` and
//! `kanban_sync.rs`:
//!
//! - **Save only over what was read.** A save (`⌘S`, autosave, quit) writes
//!   only while the file still holds what the page last read or wrote, and
//!   only to a path [`inside_vault`]. Otherwise it is refused and the page
//!   flags the file ([`DocConflict`]).
//! - **Follow the file.** When a watcher sees the file change (in place, or
//!   by another app's atomic temp-and-rename save), [`DiskRule`] says what
//!   to do: re-read a clean, idle page; flag unsaved work, never re-read
//!   over it; and wait while a gesture or an editor with nothing typed is
//!   open, since its commit would write the old text over theirs.
//! - **Decide on a flag.** The page's conflict bar ([`conflict_bar`]) offers
//!   Keep mine (write over the file, recreating it when it is gone), Save
//!   copy (a new sibling `Name (conflict YYYYMMDD-HHMMSS).ext`,
//!   [`copy_path`]) and Load theirs (Discard when the file is gone).
//! - **Rest before writing.** Autosave and the recovery copy wait for the
//!   edits to rest, or write at most every so often while they never do
//!   ([`Rest`]).
//! - **Quit loses nothing.** Quitting saves; when it can't, the recovery
//!   copy must hold the latest edits, or the quit is refused ([`quit_rule`]).

use std::io;
use std::path::{Component, Path, PathBuf};

use rune::{Cursor, El, Key, KeyEvent, Tx, el, row, text};
use rune_kit::Kit;

use crate::State;
use crate::watch::FileState;

/// The shortest rest before autosave writes (a board file can be
/// megabytes)…
pub(crate) const AUTOSAVE_MIN: f64 = 1.0;
/// …and while the edits never rest, they are still saved this often.
pub(crate) const AUTOSAVE_MAX_AGE: f64 = 10.0;
/// The recovery copy of edits that keep changing is written once they have
/// rested this long…
pub(crate) const RECOVERY_IDLE: f64 = 0.5;
/// …or, while they never rest, at least this often.
pub(crate) const RECOVERY_MAX_AGE: f64 = 2.0;

/// A file's `(modified, length)`, or `None` when it cannot be read.
pub(crate) fn file_state(path: &Path) -> Option<FileState> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok(), meta.len()))
}

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

/// Why a document's file no longer matches its unsaved edits (#38 review
/// B1/B2). Muninn never writes over it on its own: the page's conflict bar
/// decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocConflict {
    /// Another app changed the file since Muninn read or wrote it.
    Changed,
    /// The file was removed, renamed or moved away.
    Missing,
}

impl DocConflict {
    /// The conflict bar's words for it.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            DocConflict::Changed => "Changed on disk by another app",
            DocConflict::Missing => "Removed or renamed on disk",
        }
    }
}

/// Compare the file at `path` with what the page last read or wrote
/// (`same`): `None` when it still holds that, else why not. An unreadable
/// file keeps the flag it had (`was`).
pub(crate) fn disk_conflict(
    path: &Path,
    same: impl FnOnce(&str) -> bool,
    was: Option<DocConflict>,
) -> Option<DocConflict> {
    match std::fs::read_to_string(path) {
        Ok(text) if same(&text) => None,
        Ok(_) => Some(DocConflict::Changed),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Some(DocConflict::Missing),
        Err(_) => was,
    }
}

/// What a watcher may do with the shown document when its file changed on
/// disk. Every watcher of a page asks the page's one rule, so they cannot
/// drift apart (#41 review GK2b: an atomic save reaches the vault watcher
/// first, and it used to reload under an open editor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiskRule {
    /// Unsaved edits, or text typed into an editor and not committed: flag
    /// their change, never reload.
    Flag,
    /// A gesture, or an open editor with nothing typed: do not reload under
    /// it; look again once it is over.
    Later,
    /// Clean and idle: re-read the file.
    Reload,
}

impl DiskRule {
    /// The rule for a page with `unsaved` work, `busy` with a gesture or an
    /// open editor.
    pub(crate) fn of(unsaved: bool, busy: bool) -> DiskRule {
        if unsaved {
            DiskRule::Flag
        } else if busy {
            DiskRule::Later
        } else {
            DiskRule::Reload
        }
    }
}

/// Edits waiting to rest before a write: the state `key` not written yet,
/// when it first changed and when it last did.
#[derive(Clone, Debug)]
pub(crate) struct Rest<K> {
    key: K,
    first: f64,
    last: f64,
}

impl<K: PartialEq> Rest<K> {
    /// Track `key` at time `t` in `slot`. `None` once it has rested `idle`
    /// seconds, or has waited `max_age` since its first change: write it now
    /// (the caller clears the slot). Otherwise how long until it is due.
    pub(crate) fn wait(
        slot: &mut Option<Rest<K>>,
        key: K,
        t: f64,
        idle: f64,
        max_age: f64,
    ) -> Option<f64> {
        let rest = match slot {
            Some(rest) => {
                if rest.key != key {
                    rest.key = key;
                    rest.last = t;
                }
                rest
            }
            None => slot.insert(Rest {
                key,
                first: t,
                last: t,
            }),
        };
        let (rested, age) = (t - rest.last, t - rest.first);
        (rested < idle && age < max_age).then(|| (idle - rested).min(max_age - age))
    }
}

/// Save copy's file: `Name (conflict STAMP).ext` beside `path`, or at the
/// vault `root` when that folder is gone or outside it, with ` 2`, ` 3`…
/// after the stamp until the name is free.
pub(crate) fn copy_path(root: &Path, path: &Path, stamp: &str) -> PathBuf {
    let stem = path.file_stem().map_or_else(
        || "Untitled".to_owned(),
        |s| s.to_string_lossy().into_owned(),
    );
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let folder = path
        .parent()
        .filter(|p| p.is_dir() && inside_vault(root, p))
        .map_or_else(|| root.to_path_buf(), PathBuf::from);
    let mut copy = folder.join(format!("{stem} (conflict {stamp}){ext}"));
    let mut n = 2;
    while copy.exists() {
        copy = folder.join(format!("{stem} (conflict {stamp} {n}){ext}"));
        n += 1;
    }
    copy
}

/// The close and quit rule: nothing is lost when the edits were `saved`, or
/// their recovery copy was written (`copied`) and holds the latest edits
/// (`current`). Otherwise why the quit must be refused (`what` names the
/// document).
pub(crate) fn quit_rule(
    saved: bool,
    copied: &io::Result<()>,
    current: bool,
    what: &str,
) -> Result<(), String> {
    match copied {
        _ if saved => Ok(()),
        Ok(()) if current => Ok(()),
        Ok(()) => Err(format!("the {what} could not be read")),
        Err(e) => Err(e.to_string()),
    }
}

/// The page's choices for unsaved edits whose file changed or vanished, as
/// the note conflict bar offers them. Keys are `{page}-conflict`,
/// `{page}-conflict-text`, `{page}-keep-mine`, `{page}-save-copy` and
/// `{page}-load-theirs`.
pub(crate) fn conflict_bar(
    k: &Kit,
    page: &str,
    conflict: DocConflict,
    actions: [fn(&mut State) -> bool; 3],
) -> El {
    let [keep, copy, theirs] = actions;
    let theirs_label = match conflict {
        DocConflict::Changed => "Load theirs",
        DocConflict::Missing => "Discard",
    };
    row()
        .key(format!("{page}-conflict"))
        .gap(6.0)
        .center_y()
        // Slice 16: a conflict is an alert dialog (its choices are the way
        // out), its sentence an alert, and each choice a real button.
        .role("alertdialog")
        .label(conflict.message())
        .child(el().size(6.0, 6.0).radius(3.0).fill(k.c.danger.fg))
        .child(
            text(conflict.message())
                .key(format!("{page}-conflict-text"))
                .role("alert")
                .font(k.ty.caption())
                .color(k.c.danger.fg),
        )
        .child(choice(k, "Keep mine", format!("{page}-keep-mine"), keep))
        .child(choice(k, "Save copy", format!("{page}-save-copy"), copy))
        .child(choice(
            k,
            theirs_label,
            format!("{page}-load-theirs"),
            theirs,
        ))
}

fn choice(k: &Kit, label: &str, key: String, action: fn(&mut State) -> bool) -> El {
    el().key(key)
        .radius(6.0)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .px(8.0)
        .py(4.0)
        .center_y()
        .role("button")
        .label(label.to_owned())
        .help(label.to_owned())
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .on_key(move |s: &mut State, e: &KeyEvent, _: &mut Tx| {
            if e.key == Key::Enter {
                let _ = action(s);
                true
            } else {
                false
            }
        })
        .child(
            text(label.to_owned())
                .font(k.ty.caption())
                .color(k.c.text_base),
        )
        .on_press(move |s: &mut State| {
            let _ = action(s);
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
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
        assert!(!inside_vault(&root, Path::new("/x.excalidraw")));
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

    #[test]
    fn edits_rest_before_they_are_due_and_never_wait_past_the_max_age() {
        let mut slot = None;
        assert!(Rest::wait(&mut slot, 1, 0.0, 0.5, 2.0).is_some());
        assert!(Rest::wait(&mut slot, 1, 0.4, 0.5, 2.0).is_some());
        assert_eq!(Rest::wait(&mut slot, 1, 0.5, 0.5, 2.0), None, "rested");
        let mut slot = None;
        for n in 0..8 {
            let t = f64::from(n) * 0.3;
            let due = Rest::wait(&mut slot, n, t, 0.5, 2.0);
            assert_eq!(due.is_none(), t >= 2.0, "at {t}");
        }
    }

    #[test]
    fn a_copy_gets_a_free_name_beside_the_file_or_at_the_root() {
        let root = temp("copy");
        std::fs::create_dir_all(root.join("Work")).unwrap();
        let file = root.join("Work/Board.md");
        let first = copy_path(&root, &file, "20261112-120000");
        assert_eq!(first, root.join("Work/Board (conflict 20261112-120000).md"));
        std::fs::write(&first, "x").unwrap();
        let second = copy_path(&root, &file, "20261112-120000");
        assert_eq!(
            second,
            root.join("Work/Board (conflict 20261112-120000 2).md")
        );
        let gone = copy_path(&root, &root.join("Gone/Plan.excalidraw"), "s");
        assert_eq!(gone, root.join("Plan (conflict s).excalidraw"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_quit_needs_a_save_or_a_current_copy() {
        let (ok, failed) = (Ok(()), Err(io::Error::other("read-only")));
        assert_eq!(quit_rule(true, &failed, false, "kanban"), Ok(()));
        assert_eq!(quit_rule(false, &ok, true, "kanban"), Ok(()));
        assert_eq!(
            quit_rule(false, &ok, false, "kanban"),
            Err("the kanban could not be read".to_owned())
        );
        assert_eq!(
            quit_rule(false, &failed, true, "kanban"),
            Err("read-only".to_owned())
        );
    }
}

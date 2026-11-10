//! SAF-03 external-change watcher: metadata-only vault snapshots.
//!
//! There is no `notify` dependency. Every [`crate::state::WATCH_INTERVAL`]
//! seconds `poll` asks for a snapshot and compares it with the previous one.
//! A snapshot costs exactly one `fs::metadata` per note plus one per known
//! folder: ~2,000 stats for a large vault, which is roughly 1–3 ms with a warm
//! dentry cache on Linux and reads **no file contents**. That is cheap enough
//! to run every 1.5 s on the UI thread while keeping external edits visible
//! within roughly two seconds.
//!
//! Directory mtimes are part of the snapshot because Linux updates a folder's
//! mtime when an entry is created, renamed or removed. That is what makes a
//! brand-new note (or a deleted/renamed one) visible without reading the
//! directory or scanning every file: the folder changed, so the vault is
//! rescanned. Missing files and folders are represented as `None` instead of
//! failing the whole snapshot, so a transient error keeps the watcher quiet
//! rather than spamming the status bar.

use std::collections::BTreeMap;
use std::fs;
use std::time::SystemTime;

use muninn_core::Vault;

/// `(mtime, length)`, the same identity [`Vault::disk_state`] exposes.
pub type FileState = (Option<SystemTime>, u64);

/// One metadata-only picture of the vault.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VaultSnapshot {
    /// Note `rel` → `Some(identity)`, or `None` when the file is missing.
    pub notes: BTreeMap<String, Option<FileState>>,
    /// Folder `rel` (`""` is the root) → `Some(mtime)`, or `None` when missing.
    pub dirs: BTreeMap<String, Option<SystemTime>>,
}

impl VaultSnapshot {
    #[must_use]
    pub fn note(&self, rel: &str) -> Option<FileState> {
        self.notes.get(rel).copied().flatten()
    }
}

/// Read the vault's metadata picture. Never fails: an unreadable or missing
/// entry becomes `None`, so callers can compare and decide quietly.
#[must_use]
pub fn snapshot(vault: &Vault) -> VaultSnapshot {
    let mut notes = BTreeMap::new();
    for note in &vault.notes {
        let state = fs::metadata(&note.path)
            .ok()
            .map(|meta| (meta.modified().ok(), meta.len()));
        notes.insert(note.rel.clone(), state);
    }
    let mut dirs = BTreeMap::new();
    for rel in &vault.dirs {
        let path = if rel.is_empty() {
            vault.root.clone()
        } else {
            vault.root.join(rel)
        };
        dirs.insert(
            rel.clone(),
            fs::metadata(path).ok().and_then(|m| m.modified().ok()),
        );
    }
    VaultSnapshot { notes, dirs }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::testing::TempVault;
    use std::fs;

    #[test]
    fn snapshot_tracks_content_and_new_entries() {
        let tmp = TempVault::new(&[("Alpha.md", "alpha")]);
        let vault = tmp.load();
        let first = snapshot(&vault);
        assert_eq!(
            first.note("Alpha.md"),
            Some((first.note("Alpha.md").unwrap().0, 5))
        );

        // The directory mtime must advance so an entry creation can be picked
        // up by snapshot diff alone. Windows reports directory mtimes with
        // coarse ticks, and rewriting the same file often does not move the
        // folder mtime, so create a new unique file each wait (bounded).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut after = None;
        let mut n = 0u32;
        while std::time::Instant::now() < deadline {
            let vault = tmp.load();
            let snap = snapshot(&vault);
            if snap.dirs != first.dirs {
                after = Some(snap);
                break;
            }
            let name = if n == 0 {
                "Beta.md".to_string()
            } else {
                format!("Beta-{n}.md")
            };
            fs::write(tmp.path().join(name), "beta!").expect("create");
            n += 1;
        }
        let second = after.expect("directory mtime should move after creating a file");
        assert!(second.note("Beta.md").is_some());
        assert_ne!(first.dirs, second.dirs, "the root mtime moved");
    }

    #[test]
    fn missing_files_become_none_instead_of_failing() {
        let tmp = TempVault::new(&[("Alpha.md", "alpha")]);
        let vault = tmp.load();
        let stale = vault.notes[0].path.clone();
        fs::remove_file(&stale).expect("remove");
        let snap = snapshot(&vault);
        assert_eq!(snap.note("Alpha.md"), None);
    }
}

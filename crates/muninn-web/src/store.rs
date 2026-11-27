//! The OPFS-backed browser vault's synchronous half (#240, slice 6b).
//!
//! OPFS is a Promise API; only a dedicated worker gets synchronous handles, and
//! a worker would need COOP/CORP headers and a cross-thread bridge for a
//! synchronous [`VaultStore`]. The browser build already holds the whole vault
//! in memory, so this store keeps the same model and writes behind it:
//!
//! - the bytes and folder identity live in a [`MemoryStore`], which is exactly
//!   the semantics the vault's 6a contract pins (case-insensitive identity,
//!   per-write stamps, no parent invention, POSIX rename, best-effort
//!   `read_dir`);
//! - every successful mutation marks its vault-relative path in a dirty set;
//! - the async half in `opfs.rs` drains that set into the real OPFS handles
//!   when the app's poll loop asks, and a reload rebuilds the mirror from OPFS.
//!
//! The honest limitation is the flush window: between two flushes OPFS lags
//! the mirror, so a crash can lose the edits since the last drain. Slice 6c's
//! recovery store is where that window gets a safety net.

use std::collections::BTreeSet;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use muninn_core::{EntryKind, MemoryStore, StoreEntry, StoreMeta, VaultStore};

/// A vault store served from an in-memory mirror of OPFS.
///
/// The root is the vault label the notes carry (`/Muninn sample` in the demo);
/// OPFS itself is mounted elsewhere and the async side strips the root before
/// touching a handle.
pub struct OpfsStore {
    mirror: MemoryStore,
    writable: AtomicBool,
    /// Vault-relative paths whose mirror state has not reached OPFS yet.
    dirty: Mutex<BTreeSet<String>>,
}

impl OpfsStore {
    /// An empty store serving `root`. `writable` is false for the second tab
    /// that could not take the single-writer lock: reads still work and every
    /// mutation is refused with [`io::ErrorKind::PermissionDenied`].
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, writable: bool) -> OpfsStore {
        OpfsStore {
            mirror: MemoryStore::new(root),
            writable: AtomicBool::new(writable),
            dirty: Mutex::new(BTreeSet::new()),
        }
    }

    /// Seed a file without marking it dirty: the initial snapshot of bytes
    /// already in OPFS.
    pub fn load(&self, rel: &str, bytes: &[u8]) {
        self.mirror.put_file(rel, bytes);
    }

    /// Seed an existing folder without marking it dirty. OPFS can hold empty
    /// folders, so the snapshot has to restore them too; otherwise a folder a
    /// user created would vanish on reload.
    pub fn load_dir(&self, rel: &str) {
        let _ = self.mirror.create_dir_all(&self.mirror.root().join(rel));
    }

    /// Seed a file and mark it for the next flush: the first-run sample.
    pub fn seed(&self, rel: &str, bytes: &[u8]) {
        self.mirror.put_file(rel, bytes);
        self.mark_rel(rel);
    }

    /// Whether this tab holds the write lock.
    #[must_use]
    pub fn is_writable(&self) -> bool {
        self.writable.load(Ordering::Relaxed)
    }

    /// Stop accepting and flushing writes. The lock-loss watcher calls this;
    /// edits already in the mirror stay readable for the rest of the session.
    pub fn set_writable(&self, writable: bool) {
        self.writable.store(writable, Ordering::Relaxed);
    }

    /// Whether a flush has anything to do.
    #[must_use]
    pub fn has_dirty(&self) -> bool {
        !self.dirty().is_empty()
    }

    /// How many distinct paths are waiting to be flushed. Tests use it; the
    /// flush itself only needs [`OpfsStore::has_dirty`].
    #[must_use]
    pub fn dirty_count(&self) -> usize {
        self.dirty().len()
    }

    /// The bytes the mirror holds for a vault-relative path, binary included.
    #[must_use]
    pub fn read_bytes(&self, rel: &str) -> Option<Vec<u8>> {
        self.mirror.read_bytes(rel)
    }

    /// The next flush's work, in execution order.
    ///
    /// Taking the plan also clears the dirty set, so a mutation that lands
    /// while the async flush is awaiting re-marks its path and survives to the
    /// next drain. On a failed flush the caller re-marks every planned path
    /// with [`OpfsStore::requeue`].
    #[must_use]
    pub fn take_flush_plan(&self) -> FlushPlan {
        let dirty = {
            let mut dirty = self.dirty();
            std::mem::take(&mut *dirty)
        };
        self.plan(dirty)
    }

    /// Put a failed plan back in the dirty set.
    pub fn requeue(&self, plan: &FlushPlan) {
        for rel in plan.removals.iter().chain(&plan.dirs).chain(&plan.files) {
            self.mark_rel(rel);
        }
    }

    fn dirty(&self) -> std::sync::MutexGuard<'_, BTreeSet<String>> {
        self.dirty
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn require_writable(&self) -> io::Result<()> {
        if self.is_writable() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the browser vault is open read-only: another tab holds its write lock",
            ))
        }
    }

    /// Mark a path when it is under the root. A path the mirror accepted but
    /// this cannot spell is left for the read path to reject; it cannot happen
    /// for the vault's own paths.
    fn mark_path(&self, path: &Path) {
        if let Some(rel) = self.rel_of(path) {
            self.mark_rel(&rel);
        }
    }

    fn mark_rel(&self, rel: &str) {
        let rel = normalise_rel(rel);
        if rel.is_empty() {
            return;
        }
        self.dirty().insert(rel);
    }

    fn rel_of(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(self.mirror.root()).ok()?;
        let mut parts: Vec<String> = Vec::new();
        for component in rel.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            }
        }
        Some(parts.join("/"))
    }

    /// Turn the dirty paths into concrete work. A dirty folder contributes its
    /// whole mirror subtree: a rename marks the old and the new spelling, and
    /// the new side then has to be materialised in OPFS rather than only the
    /// folder itself.
    fn plan(&self, dirty: BTreeSet<String>) -> FlushPlan {
        let mut removals = BTreeSet::new();
        let mut dirs = BTreeSet::new();
        let mut files = BTreeSet::new();
        for rel in dirty {
            if rel.is_empty() {
                continue;
            }
            match self.mirror.metadata(&self.mirror.root().join(&rel)) {
                Ok(meta) if meta.is_dir() => {
                    dirs.insert(rel.clone());
                    self.collect_tree(&rel, &mut dirs, &mut files);
                }
                Ok(_) => {
                    files.insert(rel);
                }
                Err(_) => {
                    removals.insert(rel);
                }
            }
        }
        let mut removals: Vec<String> = removals.into_iter().collect();
        // Children first: a recursive remove of an ancestor makes the deeper
        // entries no-ops, and the other order never removes a folder before
        // something that would be stranded inside it.
        removals.sort_by(|a, b| depth(b).cmp(&depth(a)).then_with(|| b.cmp(a)));
        let mut dirs: Vec<String> = dirs.into_iter().collect();
        dirs.sort_by(|a, b| depth(a).cmp(&depth(b)).then_with(|| a.cmp(b)));
        FlushPlan {
            removals,
            dirs,
            files: files.into_iter().collect(),
        }
    }

    fn collect_tree(&self, rel: &str, dirs: &mut BTreeSet<String>, files: &mut BTreeSet<String>) {
        let Ok(entries) = self.mirror.read_dir(&self.mirror.root().join(rel)) else {
            return;
        };
        for entry in entries {
            let name = entry.name.to_string_lossy();
            let child = if rel.is_empty() {
                name.into_owned()
            } else {
                format!("{rel}/{name}")
            };
            match entry.kind {
                EntryKind::Dir => {
                    dirs.insert(child.clone());
                    self.collect_tree(&child, dirs, files);
                }
                EntryKind::File => {
                    files.insert(child);
                }
            }
        }
    }
}

impl VaultStore for OpfsStore {
    fn root(&self) -> &Path {
        self.mirror.root()
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.mirror.read_to_string(path)
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.require_writable()?;
        self.mirror.write_atomic(path, bytes)?;
        self.mark_path(path);
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.require_writable()?;
        self.mirror.rename(from, to)?;
        self.mark_path(from);
        self.mark_path(to);
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.require_writable()?;
        self.mirror.remove_file(path)?;
        self.mark_path(path);
        Ok(())
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.require_writable()?;
        self.mirror.create_dir(path)?;
        self.mark_path(path);
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        self.require_writable()?;
        // A folder that already exists is the vault's ordinary "make sure"
        // call before writing a note. It changes nothing, so re-marking it
        // would make the flush rewrite the whole subtree.
        let existed = self.mirror.is_dir(path);
        self.mirror.create_dir_all(path)?;
        if !existed {
            self.mark_path(path);
        }
        Ok(())
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        self.require_writable()?;
        self.mirror.remove_dir(path)?;
        self.mark_path(path);
        Ok(())
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.require_writable()?;
        self.mirror.remove_dir_all(path)?;
        self.mark_path(path);
        Ok(())
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<StoreEntry>> {
        self.mirror.read_dir(path)
    }

    fn metadata(&self, path: &Path) -> io::Result<StoreMeta> {
        self.mirror.metadata(path)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<StoreMeta> {
        self.mirror.symlink_metadata(path)
    }

    fn same_file(&self, a: &Path, b: &Path) -> bool {
        self.mirror.same_file(a, b)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.mirror.canonicalize(path)
    }
}

/// One flush's work: removals deepest first, folders shallow first, files
/// sorted. The async side executes removals, then folders, then file writes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlushPlan {
    pub removals: Vec<String>,
    pub dirs: Vec<String>,
    pub files: Vec<String>,
}

impl FlushPlan {
    /// True when the dirty set was empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.removals.is_empty() && self.dirs.is_empty() && self.files.is_empty()
    }
}

fn depth(rel: &str) -> usize {
    rel.split('/').filter(|part| !part.is_empty()).count()
}

/// Normalise a caller-supplied relative path the way [`MemoryStore`] does:
/// `/` separators, no `.`/`..`/empty segments.
fn normalise_rel(rel: &str) -> String {
    rel.replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> OpfsStore {
        OpfsStore::new("/Muninn sample", true)
    }

    fn names(store: &OpfsStore, rel: &str) -> Vec<String> {
        store
            .read_dir(&store.root().join(rel))
            .expect("read dir")
            .into_iter()
            .map(|entry| entry.name.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn an_existing_folder_is_not_re_dirtied_by_make_sure() {
        let store = store();
        store.load_dir("notes");
        store.load("notes/One.md", b"one");
        store
            .create_dir_all(&store.root().join("notes"))
            .expect("an existing folder is a no-op");
        assert!(!store.has_dirty(), "no journal entry for a no-op");
        store
            .create_dir_all(&store.root().join("notes/deep"))
            .expect("a new folder is dirty");
        assert_eq!(store.take_flush_plan().dirs, ["notes/deep"]);
    }

    #[test]
    fn an_empty_folder_loaded_from_opfs_is_listed() {
        let store = store();
        store.load_dir("empty/deep");
        assert_eq!(names(&store, ""), ["empty"]);
        assert_eq!(names(&store, "empty"), ["deep"]);
        assert!(!store.has_dirty(), "a snapshot load writes nothing");
    }

    #[test]
    fn the_mirror_keeps_the_memory_store_contract() {
        let store = store();
        store.load("notes/Alpha.md", b"# Alpha\n");
        store.load("Root.md", b"# Root\n");

        assert_eq!(
            store
                .read_to_string(&store.root().join("notes/Alpha.md"))
                .unwrap(),
            "# Alpha\n"
        );
        assert_eq!(names(&store, ""), ["Root.md", "notes"]);
        assert_eq!(names(&store, "notes"), ["Alpha.md"]);
        assert!(
            store.is_file(&store.root().join("notes/alpha.md")),
            "case-insensitive identity"
        );
        assert!(store.is_dir(&store.root().join("NOTES")));
        assert!(!store.exists(&store.root().join("missing.md")));
    }

    #[test]
    fn a_case_only_rename_keeps_the_spelling_and_a_folder_rename_moves_the_subtree() {
        let store = store();
        store.load("leaves.md", b"body");
        store
            .rename(
                &store.root().join("leaves.md"),
                &store.root().join("Leaves.md"),
            )
            .unwrap();
        assert_eq!(names(&store, ""), ["leaves.md"]);

        store.load("notes/One.md", b"one");
        store.load("notes/deep/Two.md", b"two");
        store
            .rename(&store.root().join("notes"), &store.root().join("Archive"))
            .unwrap();
        assert_eq!(names(&store, ""), ["Archive", "leaves.md"]);
        assert_eq!(names(&store, "archive"), ["One.md", "deep"]);
        assert_eq!(
            store
                .read_to_string(&store.root().join("Archive/deep/Two.md"))
                .unwrap(),
            "two"
        );
    }

    #[test]
    fn the_write_stamp_changes_even_when_the_length_does_not() {
        let store = store();
        let path = store.root().join("Same.md");
        store.write_atomic(&path, b"a").unwrap();
        let first = store.metadata(&path).unwrap();
        store.write_atomic(&path, b"b").unwrap();
        let second = store.metadata(&path).unwrap();
        assert_eq!(first.len, second.len);
        assert_ne!(first.modified, second.modified);
    }

    #[test]
    fn a_write_under_a_missing_folder_is_refused_and_leaves_no_dirty_path() {
        let store = store();
        let missing = store.root().join("nowhere/Note.md");
        assert_eq!(
            store.write_atomic(&missing, b"x").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(!store.has_dirty());
        store.create_dir_all(&store.root().join("nowhere")).unwrap();
        store.write_atomic(&missing, b"x").unwrap();
        assert!(store.is_file(&missing));
        assert!(store.has_dirty());
    }

    #[test]
    fn invalid_utf8_reads_like_the_filesystem_but_flushes_as_bytes() {
        let store = store();
        store.load("assets/pic.png", &[0xff, 0xfe]);
        assert_eq!(
            store
                .read_to_string(&store.root().join("assets/pic.png"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        store
            .write_atomic(&store.root().join("assets/pic.png"), &[0x89, 0x50])
            .unwrap();
        assert_eq!(store.read_bytes("assets/pic.png"), Some(vec![0x89, 0x50]));
    }

    #[test]
    fn every_mutation_marks_and_taking_the_plan_drains() {
        let store = store();
        store.seed("Welcome.md", b"# Welcome\n");
        assert_eq!(store.dirty_count(), 1);

        store.create_dir_all(&store.root().join("notes")).unwrap();
        store
            .write_atomic(&store.root().join("notes/One.md"), b"one")
            .unwrap();
        store
            .rename(
                &store.root().join("notes/One.md"),
                &store.root().join("notes/Two.md"),
            )
            .unwrap();
        store
            .remove_file(&store.root().join("notes/Two.md"))
            .unwrap();
        let plan = store.take_flush_plan();
        assert!(!store.has_dirty(), "the plan drains the journal");
        assert_eq!(plan.dirs, ["notes"]);
        assert!(plan.files.contains(&"Welcome.md".to_owned()));
        assert!(
            plan.removals.contains(&"notes/Two.md".to_owned()),
            "the renamed-and-deleted path is a removal: {plan:?}"
        );
        assert!(
            !plan.files.contains(&"notes/Two.md".to_owned()),
            "the mirror no longer has it: {plan:?}"
        );
    }

    #[test]
    fn a_dirty_folder_contributes_its_whole_subtree_in_execution_order() {
        let store = store();
        store.load("notes/One.md", b"one");
        store.load("notes/deep/Two.md", b"two");
        store
            .rename(&store.root().join("notes"), &store.root().join("Archive"))
            .unwrap();

        let plan = store.take_flush_plan();
        assert_eq!(plan.removals, ["notes"], "the old side is removed first");
        assert_eq!(
            plan.dirs,
            ["Archive", "Archive/deep"],
            "folders shallow first"
        );
        assert_eq!(
            plan.files,
            ["Archive/One.md", "Archive/deep/Two.md"],
            "the new subtree's files are rewritten"
        );
    }

    #[test]
    fn requeue_puts_a_failed_flush_back() {
        let store = store();
        store
            .write_atomic(&store.root().join("A.md"), b"a")
            .unwrap();
        let plan = store.take_flush_plan();
        assert!(!store.has_dirty());
        store.requeue(&plan);
        assert!(store.is_file(&store.root().join("A.md")));
        assert_eq!(store.take_flush_plan().files, ["A.md"]);
    }

    #[test]
    fn a_read_only_store_reads_but_refuses_every_mutation() {
        let store = OpfsStore::new("/Muninn sample", false);
        store.load("Welcome.md", b"# Welcome\n");
        assert_eq!(
            store
                .read_to_string(&store.root().join("Welcome.md"))
                .unwrap(),
            "# Welcome\n"
        );
        let path = store.root().join("Welcome.md");
        for result in [
            store.write_atomic(&path, b"x"),
            store.rename(&path, &store.root().join("Other.md")),
            store.remove_file(&path),
            store.create_dir(&store.root().join("new")),
            store.create_dir_all(&store.root().join("new/deep")),
            store.remove_dir(&store.root().join("new")),
            store.remove_dir_all(&store.root().join("new")),
        ] {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        }
        assert!(!store.has_dirty());
        assert_eq!(
            store.read_to_string(&path).unwrap(),
            "# Welcome\n",
            "a refused mutation leaves the mirror untouched"
        );
    }

    #[test]
    fn losing_the_lock_stops_writes_and_flushes() {
        let store = store();
        store
            .write_atomic(&store.root().join("A.md"), b"a")
            .unwrap();
        assert!(store.is_writable());
        store.set_writable(false);
        assert!(!store.is_writable());
        assert_eq!(
            store
                .write_atomic(&store.root().join("B.md"), b"b")
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        // The already-dirty path is still there, but the flush loop checks
        // `is_writable` before taking a plan, so it is not drained.
        assert!(store.has_dirty());
    }

    #[test]
    fn the_plan_skips_the_root_and_dot_free_paths_stay_distinct() {
        let store = store();
        store.load("A.md", b"a");
        store.load("a.md", b"case");
        let plan = store.take_flush_plan();
        // Only one spelling exists in a case-insensitive mirror.
        assert!(plan.is_empty(), "load does not dirty: {plan:?}");
        store
            .write_atomic(&store.root().join("A.md"), b"new")
            .unwrap();
        let plan = store.take_flush_plan();
        assert_eq!(plan.files, ["A.md"]);
    }
}

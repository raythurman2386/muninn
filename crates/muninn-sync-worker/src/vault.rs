//! The worker's vault store: a synchronous mirror of `muninn-vault/` with a
//! dirty journal (#240 slice 8a).
//!
//! The sync engine is synchronous and calls [`muninn_core::VaultStore`] while
//! it blocks on the provider. A dedicated worker is the only place with OPFS
//! sync access handles, but even there *acquiring* one is a Promise. The shape
//! is therefore the one the state home and the page's vault already use: a
//! memory mirror answers the engine synchronously, and the run's changes are
//! written back through sync handles when the run yields.
//!
//! Unlike the page's [`muninn_web::OpfsStore`], this store is **case-
//! sensitive**, because the OPFS implementations this slice targets are
//! (Chromium's OPFS compares names case-sensitively and preserves their
//! spelling). `Notes.md` and `notes.md` are different entries there, so folding
//! them in the mirror would let a flush write a second file or lose the
//! spelling the engine planned against. The sync inventory independently
//! rejects two paths that collide when folded (its `fold_key` check), so the
//! engine never hands this store such a pair. The identity rules below say
//! what case sensitivity means for each trait method.
//!
//! # Lifecycle, honestly
//!
//! A worker is spawned for exactly one run. The community therefore is much
//! smaller than the page's:
//!
//! 1. `VaultDisk` (crate::vault_opfs) walks OPFS into an empty
//!    store at worker boot (no dirty paths: the snapshot is not a write);
//! 2. the engine reads and writes the mirror for the run;
//! 3. at the end, the worker takes [`WorkerVaultStore::take_flush_ops`] and
//!    applies them through sync access handles, then exits.
//!
//! **Durability.** Nothing this run wrote is durable until step 3 finishes.
//! A worker killed mid-run leaves OPFS exactly as the run found it: no partial
//! vault, because no mirror write touched OPFS. A failure during step 3 puts
//! the unfinished ops back in the dirty set and is reported to the page; the
//! next run re-reads OPFS and replans. This is a different trade from the
//! page's write-behind (where a crash can lose the last few edits) and is the
//! honest one for a run-scoped writer.
//!
//! **`write_atomic`.** The mirror replaces a file's bytes atomically. The
//! OPFS flush truncates then writes through one sync access handle, which is
//! not a filesystem transaction: a crash *during* the flush can leave a
//! partially written file. That file is a normal local change to the next
//! run's inventory, not a corrupt state journal, so the next run replans it.
//!
//! # OPFS semantics, per method
//!
//! | method | what it does here, on OPFS |
//! | --- | --- |
//! | `root` / `canonicalize` | the OPFS directory label (`muninn-vault`) is the only path; `canonicalize` returns the spelled mirror path, or the root itself |
//! | `read_to_string` / `read_bytes` | mirror read; invalid UTF-8 is `InvalidData` like `std::fs::read_to_string` |
//! | `write_atomic` | mirror replace; the file's parents must exist; the flush writes the whole file |
//! | `rename` | whole-entry move with POSIX file-replace semantics, in the mirror; a case-only spelling is a *real* move because OPFS is case-sensitive |
//! | `remove_file` / `remove_dir` / `remove_dir_all` | mirror removal; a missing path is `NotFound` for a file/non-empty check and success for `remove_dir_all`, matching `std::fs` |
//! | `create_dir` / `create_dir_all` | mirror folders; OPFS creates them in the flush because a sync handle needs its parent to exist |
//! | `read_dir` | direct children only, sorted by path; OPFS has real directory order but the engine sorts by name anyway |
//! | `metadata` / `symlink_metadata` | kind and length from the mirror; `links` is always 1 (OPFS has no hard links), `symlink` always false (OPFS has no symbolic links) |
//! | `same_file` | OPFS exposes no inode. Because the store is case-sensitive, two distinct spellings are two entries, so identity is *identical spelled path and existence*; see below for why the rename checks stay safe |
//! | `modified` | a per-write logical stamp, not OPFS's `File.lastModified`: reading that needs an async `getFile`, and the mirror must change the identity on every write so a guarded write never mistakes a rewrite for a no-op |
//!
//! # `same_file` and the #292 rename checks
//!
//! The vault's rename paths ask `same_file(old, target)` only when `target`
//! already exists. On a case-insensitive volume the two spellings can be one
//! file, and a plain rename would be a silent no-op; the vault then hops
//! through a hidden temp name. OPFS compares names case-sensitively, so
//! `leaves.md` and `Leaves.md` are two entries: `same_file` must be false for
//! them (they really are different files) and true only for the same spelling.
//! The plain `rename` then does the right thing — a real move in OPFS — and
//! the temp hop is never needed, exactly as on a case-sensitive native
//! filesystem. Reporting a false positive here would risk the vault moving a
//! file over a different note.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use muninn_core::{EntryKind, StoreEntry, StoreMeta, VaultStore};

/// One unit of work for the OPFS flush.
///
/// The wasm half executes these in order; the native tests replay them against
/// an in-memory fake of the sync-handle layer, which is the same contract the
/// real OPFS writes obey: removals first (deepest first), then folders
/// (shallow first), then whole-file writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultOp {
    /// Remove the entry and, when it is a folder, its subtree.
    Remove(String),
    /// Create the folder and any missing parents.
    EnsureDir(String),
    /// Replace the file with these bytes, creating missing parents.
    Write { rel: String, bytes: Vec<u8> },
}

impl VaultOp {
    /// The vault-relative path this op touches.
    #[must_use]
    pub fn rel(&self) -> &str {
        match self {
            VaultOp::Remove(rel) | VaultOp::EnsureDir(rel) => rel,
            VaultOp::Write { rel, .. } => rel,
        }
    }
}

struct Mirror {
    files: BTreeMap<String, MirrorFile>,
    /// Folder spellings, always including the root (`""`).
    dirs: BTreeSet<String>,
    /// Paths whose mirror state has not reached OPFS yet.
    dirty: BTreeSet<String>,
}

struct MirrorFile {
    bytes: Vec<u8>,
    stamp: u64,
}

impl Mirror {
    fn new() -> Mirror {
        Mirror {
            files: BTreeMap::new(),
            dirs: BTreeSet::from([String::new()]),
            dirty: BTreeSet::new(),
        }
    }
}

/// The worker's [`VaultStore`]: the mirror, its dirty journal and the label
/// the engine hashes for the vault identity.
pub struct WorkerVaultStore {
    root: PathBuf,
    clock: AtomicU64,
    mirror: Mutex<Mirror>,
}

impl WorkerVaultStore {
    /// An empty store serving `label`. The label is the OPFS directory name;
    /// see [`crate::request::VAULT_LABEL`] for why it is the identity.
    #[must_use]
    pub fn new(label: impl Into<PathBuf>) -> WorkerVaultStore {
        WorkerVaultStore {
            root: label.into(),
            clock: AtomicU64::new(0),
            mirror: Mutex::new(Mirror::new()),
        }
    }

    /// Seed a file from the boot walk without marking it dirty, creating its
    /// parents in the mirror as OPFS has them.
    pub fn load(&self, rel: &str, bytes: &[u8]) {
        let rel = normalise_rel(rel);
        if rel.is_empty() {
            return;
        }
        let mut mirror = self.mirror();
        ensure_parents(&mut mirror, &rel);
        mirror.files.insert(
            rel,
            MirrorFile {
                bytes: bytes.to_vec(),
                stamp: self.next_stamp(),
            },
        );
    }

    /// Seed an existing (possibly empty) folder from the boot walk without
    /// marking it dirty. OPFS holds empty folders, so the walk restores them.
    pub fn load_dir(&self, rel: &str) {
        let rel = normalise_rel(rel);
        let mut mirror = self.mirror();
        for part in components(&rel) {
            mirror.dirs.insert(part);
        }
    }

    /// Whether a flush has anything to do.
    #[must_use]
    pub fn has_dirty(&self) -> bool {
        !self.mirror().dirty.is_empty()
    }

    /// How many distinct paths are waiting for the flush.
    #[must_use]
    pub fn dirty_count(&self) -> usize {
        self.mirror().dirty.len()
    }

    /// The bytes the mirror holds for a relative path.
    #[must_use]
    pub fn read_rel(&self, rel: &str) -> Option<Vec<u8>> {
        self.mirror()
            .files
            .get(&normalise_rel(rel))
            .map(|file| file.bytes.clone())
    }

    /// The next flush's work, in execution order.
    ///
    /// Taking the ops clears the dirty set, so a mutation that lands while the
    /// async flush awaits re-marks its path. On a failed flush the caller puts
    /// the unfinished ops back with [`WorkerVaultStore::requeue`].
    #[must_use]
    pub fn take_flush_ops(&self) -> Vec<VaultOp> {
        let dirty = {
            let mut mirror = self.mirror();
            std::mem::take(&mut mirror.dirty)
        };
        let mirror = self.mirror();
        let mut removals = BTreeSet::new();
        let mut dirs = BTreeSet::new();
        let mut writes = BTreeMap::new();
        for rel in dirty {
            if rel.is_empty() {
                continue;
            }
            if let Some(file) = mirror.files.get(&rel) {
                writes.insert(rel, file.bytes.clone());
            } else if mirror.dirs.contains(&rel) {
                dirs.insert(rel.clone());
                collect_subtree(&mirror, &rel, &mut dirs, &mut writes);
            } else {
                removals.insert(rel);
            }
        }
        let mut ops = Vec::new();
        let mut removals: Vec<String> = removals.into_iter().collect();
        // Children first: a recursive remove of an ancestor makes the deeper
        // entries no-ops, and the other order never removes a folder before
        // something that would be stranded inside it.
        removals.sort_by(|a, b| depth(b).cmp(&depth(a)).then_with(|| b.cmp(a)));
        for rel in removals {
            ops.push(VaultOp::Remove(rel));
        }
        let mut dirs: Vec<String> = dirs.into_iter().collect();
        dirs.sort_by(|a, b| depth(a).cmp(&depth(b)).then_with(|| a.cmp(b)));
        for rel in dirs {
            ops.push(VaultOp::EnsureDir(rel));
        }
        for (rel, bytes) in writes {
            ops.push(VaultOp::Write { rel, bytes });
        }
        ops
    }

    /// Put a failed flush's ops back in the dirty set.
    pub fn requeue(&self, ops: &[VaultOp]) {
        let mut mirror = self.mirror();
        for op in ops {
            mirror.dirty.insert(op.rel().to_owned());
        }
    }

    fn mirror(&self) -> std::sync::MutexGuard<'_, Mirror> {
        self.mirror
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn next_stamp(&self) -> u64 {
        self.clock.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// The `/`-separated path below the root, or `NotFound` when the path is
    /// outside it. `..` cannot escape the vault.
    fn rel_of(&self, path: &Path) -> io::Result<String> {
        let rel = path.strip_prefix(&self.root).map_err(|_| not_found(path))?;
        let mut parts: Vec<String> = Vec::new();
        for component in rel.components() {
            match component {
                Component::Normal(part) => {
                    let part = part.to_str().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("{} has a name that is not UTF-8", path.display()),
                        )
                    })?;
                    parts.push(part.to_owned());
                }
                Component::CurDir => {}
                Component::ParentDir => return Err(permission_denied(path)),
                Component::RootDir | Component::Prefix(_) => return Err(not_found(path)),
            }
        }
        Ok(parts.join("/"))
    }

    fn spelled(&self, rel: &str) -> PathBuf {
        if rel.is_empty() {
            self.root.clone()
        } else {
            self.root.join(rel)
        }
    }
}

impl VaultStore for WorkerVaultStore {
    fn root(&self) -> &Path {
        &self.root
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let rel = self.rel_of(path)?;
        let mirror = self.mirror();
        let Some(file) = mirror.files.get(&rel) else {
            return Err(not_found(path));
        };
        String::from_utf8(file.bytes.clone()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )
        })
    }

    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        let rel = self.rel_of(path)?;
        let mirror = self.mirror();
        mirror
            .files
            .get(&rel)
            .map(|file| file.bytes.clone())
            .ok_or_else(|| not_found(path))
    }

    /// Replace the mirror's bytes for `path`. The OPFS side of this write is
    /// the end-of-run flush: one sync access handle, truncate + write +
    /// flush, which is a whole-file replace but not a filesystem transaction
    /// (see the module's durability note).
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        if rel.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::IsADirectory,
                format!("{} is the vault root", path.display()),
            ));
        }
        let mut mirror = self.mirror();
        if mirror.dirs.contains(&rel) {
            return Err(io::Error::new(
                io::ErrorKind::IsADirectory,
                format!("{} is a folder", path.display()),
            ));
        }
        let Some(parent) = parent_of(&rel) else {
            return Err(not_found(path));
        };
        if !mirror.dirs.contains(parent) {
            return Err(not_found(path));
        }
        let stamp = self.next_stamp();
        mirror.files.insert(
            rel.clone(),
            MirrorFile {
                bytes: bytes.to_vec(),
                stamp,
            },
        );
        mirror.dirty.insert(rel);
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let from_rel = self.rel_of(from)?;
        let to_rel = self.rel_of(to)?;
        if from_rel.is_empty() || to_rel.is_empty() {
            return Err(permission_denied(from));
        }
        let mut mirror = self.mirror();
        if from_rel == to_rel {
            // POSIX `rename(2)` of one name to itself is a no-op; a missing
            // source is still an error.
            let exists = mirror.files.contains_key(&from_rel) || mirror.dirs.contains(&from_rel);
            return if exists { Ok(()) } else { Err(not_found(from)) };
        }
        let Some(to_parent) = parent_of(&to_rel) else {
            return Err(not_found(to));
        };
        if !mirror.dirs.contains(to_parent) {
            return Err(not_found(to));
        }
        if let Some(file) = mirror.files.remove(&from_rel) {
            if mirror.dirs.contains(&to_rel) {
                mirror.files.insert(from_rel, file);
                return Err(io::Error::new(
                    io::ErrorKind::IsADirectory,
                    format!("{} is a folder", to.display()),
                ));
            }
            // A file replaces a file at the destination, as POSIX rename does.
            mirror.files.insert(to_rel.clone(), file);
            mirror.dirty.insert(from_rel);
            mirror.dirty.insert(to_rel);
            return Ok(());
        }
        if !mirror.dirs.contains(&from_rel) {
            return Err(not_found(from));
        }
        if to_rel.starts_with(&format!("{from_rel}/")) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is inside {}", to.display(), from.display()),
            ));
        }
        if mirror.files.contains_key(&to_rel) {
            return Err(already_exists(to));
        }
        if mirror.dirs.contains(&to_rel) {
            // POSIX replaces an existing empty folder; a non-empty one fails.
            if !is_empty_dir(&mirror, &to_rel) {
                return Err(io::Error::new(
                    io::ErrorKind::DirectoryNotEmpty,
                    format!("{} is not empty", to.display()),
                ));
            }
            mirror.dirs.remove(&to_rel);
        }
        let prefix = format!("{from_rel}/");
        let mut moved_dirs: Vec<(String, String)> = Vec::new();
        for dir in &mirror.dirs {
            if *dir == from_rel || dir.starts_with(&prefix) {
                let suffix = dir.strip_prefix(&from_rel).unwrap_or("");
                moved_dirs.push((dir.clone(), format!("{to_rel}{suffix}")));
            }
        }
        let mut moved_files: Vec<(String, String)> = Vec::new();
        for key in mirror.files.keys() {
            if key.starts_with(&prefix) {
                let suffix = key.strip_prefix(&from_rel).unwrap_or("");
                moved_files.push((key.clone(), format!("{to_rel}{suffix}")));
            }
        }
        for (old, _) in &moved_dirs {
            mirror.dirs.remove(old);
        }
        let mut files = BTreeMap::new();
        for (old, new) in moved_files {
            if let Some(file) = mirror.files.remove(&old) {
                files.insert(new, file);
            }
        }
        for (_old, new) in moved_dirs {
            mirror.dirs.insert(new);
        }
        mirror.files.extend(files);
        mirror.dirty.insert(from_rel);
        mirror.dirty.insert(to_rel);
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        let mut mirror = self.mirror();
        if mirror.files.remove(&rel).is_some() {
            mirror.dirty.insert(rel);
            Ok(())
        } else {
            Err(not_found(path))
        }
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        if rel.is_empty() {
            return Err(already_exists(path));
        }
        let mut mirror = self.mirror();
        if mirror.dirs.contains(&rel) || mirror.files.contains_key(&rel) {
            return Err(already_exists(path));
        }
        let Some(parent) = parent_of(&rel) else {
            return Err(not_found(path));
        };
        if !mirror.dirs.contains(parent) {
            return Err(not_found(path));
        }
        mirror.dirs.insert(rel.clone());
        mirror.dirty.insert(rel);
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        let mut mirror = self.mirror();
        let mut created = false;
        for part in components(&rel) {
            if mirror.files.contains_key(&part) {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    format!("{} is a file", path.display()),
                ));
            }
            created |= mirror.dirs.insert(part);
        }
        if created && !rel.is_empty() {
            mirror.dirty.insert(rel);
        }
        Ok(())
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        if rel.is_empty() {
            return Err(permission_denied(path));
        }
        let mut mirror = self.mirror();
        if !mirror.dirs.contains(&rel) {
            return Err(not_found(path));
        }
        if !is_empty_dir(&mirror, &rel) {
            return Err(io::Error::new(
                io::ErrorKind::DirectoryNotEmpty,
                format!("{} is not empty", path.display()),
            ));
        }
        mirror.dirs.remove(&rel);
        mirror.dirty.insert(rel);
        Ok(())
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        if rel.is_empty() {
            return Err(permission_denied(path));
        }
        let mut mirror = self.mirror();
        if mirror.files.contains_key(&rel) {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} is a file", path.display()),
            ));
        }
        if !mirror.dirs.contains(&rel) {
            // `std::fs::remove_dir_all` treats a missing path as success.
            return Ok(());
        }
        let prefix = format!("{rel}/");
        mirror.files.retain(|key, _| !key.starts_with(&prefix));
        mirror
            .dirs
            .retain(|key| key != &rel && !key.starts_with(&prefix));
        mirror.dirty.insert(rel);
        Ok(())
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<StoreEntry>> {
        let rel = self.rel_of(path)?;
        let mirror = self.mirror();
        if mirror.files.contains_key(&rel) {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} is a file", path.display()),
            ));
        }
        if !mirror.dirs.contains(&rel) {
            return Err(not_found(path));
        }
        let prefix = if rel.is_empty() {
            String::new()
        } else {
            format!("{rel}/")
        };
        let mut out = Vec::new();
        for dir in &mirror.dirs {
            if let Some(name) = direct_child(dir, &prefix) {
                out.push(StoreEntry {
                    path: self.root.join(dir),
                    name: name.into(),
                    kind: EntryKind::Dir,
                });
            }
        }
        for key in mirror.files.keys() {
            if let Some(name) = direct_child(key, &prefix) {
                out.push(StoreEntry {
                    path: self.root.join(key),
                    name: name.into(),
                    kind: EntryKind::File,
                });
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// Kind and length from the mirror. OPFS has no hard links (`links` is
    /// always 1) and no symbolic links (`symlink` is always false), so
    /// `symlink_metadata` is this same call.
    fn metadata(&self, path: &Path) -> io::Result<StoreMeta> {
        let rel = self.rel_of(path)?;
        let mirror = self.mirror();
        if let Some(file) = mirror.files.get(&rel) {
            return Ok(StoreMeta {
                kind: EntryKind::File,
                symlink: false,
                len: file.bytes.len() as u64,
                // OPFS has no hard links; an ordinary file has one name.
                links: 1,
                // A logical stamp: OPFS's `File.lastModified` needs an async
                // read, and the guarded-write identity must change on every
                // write, including one that keeps the length.
                modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(file.stamp)),
            });
        }
        if mirror.dirs.contains(&rel) {
            return Ok(StoreMeta {
                kind: EntryKind::Dir,
                symlink: false,
                len: 0,
                links: 1,
                modified: None,
            });
        }
        Err(not_found(path))
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<StoreMeta> {
        // OPFS has no symbolic links, so there is nothing to distinguish.
        self.metadata(path)
    }

    /// OPFS exposes no inode, so identity here is the spelled relative path
    /// and its existence. Because the store is case-sensitive, two different
    /// spellings are two entries and this is false for them; the vault's
    /// case-only rename temp hop is then never needed (see the module docs).
    fn same_file(&self, a: &Path, b: &Path) -> bool {
        let (Ok(a), Ok(b)) = (self.rel_of(a), self.rel_of(b)) else {
            return false;
        };
        if a != b {
            // Case-sensitive OPFS: a different spelling is a different entry.
            return false;
        }
        let mirror = self.mirror();
        mirror.files.contains_key(&a) || mirror.dirs.contains(&a)
    }

    /// OPFS has no `realpath`; the mirror's spelling is already canonical
    /// (one entry per case-sensitive name). The root itself returns the label
    /// the engine hashes for the vault identity.
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let rel = self.rel_of(path)?;
        let mirror = self.mirror();
        if mirror.files.contains_key(&rel) || mirror.dirs.contains(&rel) {
            return Ok(self.spelled(&rel));
        }
        Err(not_found(path))
    }
}

/// The `/`-separated folder chain of `rel`, root first, excluding `""`.
fn components(rel: &str) -> Vec<String> {
    let mut out = Vec::new();
    if rel.is_empty() {
        return out;
    }
    let mut current = String::new();
    for part in rel.split('/') {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(part);
        out.push(current.clone());
    }
    out
}

/// The parent of a vault-relative path: `""` for a top-level name.
fn parent_of(rel: &str) -> Option<&str> {
    if rel.is_empty() {
        return None;
    }
    Some(rel.rsplit_once('/').map_or("", |(parent, _)| parent))
}

/// Create the parents of `rel` in the mirror, as `MemoryStore::put_file` does
/// for a seed. Nothing is marked dirty: this is a boot snapshot.
fn ensure_parents(mirror: &mut Mirror, rel: &str) {
    let Some(parent) = parent_of(rel) else {
        return;
    };
    for part in components(parent) {
        mirror.dirs.insert(part);
    }
}

/// True when no file or folder lives below `rel`.
fn is_empty_dir(mirror: &Mirror, rel: &str) -> bool {
    let prefix = format!("{rel}/");
    !mirror.files.keys().any(|key| key.starts_with(&prefix))
        && !mirror.dirs.iter().any(|dir| dir.starts_with(&prefix))
}

/// The direct child's name when `key` sits immediately below `prefix`, else
/// `None`. The root's prefix is empty.
fn direct_child<'a>(key: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = key.strip_prefix(prefix)?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    Some(rest)
}

fn collect_subtree(
    mirror: &Mirror,
    rel: &str,
    dirs: &mut BTreeSet<String>,
    files: &mut BTreeMap<String, Vec<u8>>,
) {
    let prefix = format!("{rel}/");
    for dir in &mirror.dirs {
        if dir.starts_with(&prefix) {
            dirs.insert(dir.clone());
        }
    }
    for (key, file) in &mirror.files {
        if key.starts_with(&prefix) {
            files.insert(key.clone(), file.bytes.clone());
        }
    }
}

fn depth(rel: &str) -> usize {
    rel.split('/').filter(|part| !part.is_empty()).count()
}

/// Normalise a caller-supplied relative path: `/` separators and no `.` or
/// empty segments. `..` is dropped here as well; the seed path is trusted
/// (it comes from the OPFS walk or a test), unlike a vault API path.
fn normalise_rel(rel: &str) -> String {
    rel.replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .collect::<Vec<_>>()
        .join("/")
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{} does not exist", path.display()),
    )
}

fn permission_denied(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("{} is outside the store root", path.display()),
    )
}

fn already_exists(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("{} already exists", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> WorkerVaultStore {
        WorkerVaultStore::new("/Muninn sample")
    }

    fn names(store: &WorkerVaultStore, rel: &str) -> Vec<String> {
        store
            .read_dir(&store.root().join(rel))
            .expect("read dir")
            .into_iter()
            .map(|entry| entry.name.to_string_lossy().into_owned())
            .collect()
    }

    /// An in-memory stand-in for the worker's sync-access-handle layer: the
    /// same VaultOps, applied to a map, exactly as the OPFS flush applies them
    /// to a directory.
    #[derive(Default, PartialEq, Eq, Debug)]
    struct FakeDisk {
        files: BTreeMap<String, Vec<u8>>,
        dirs: BTreeSet<String>,
    }

    impl FakeDisk {
        fn apply(&mut self, ops: &[VaultOp]) {
            for op in ops {
                match op {
                    VaultOp::Remove(rel) => {
                        let prefix = format!("{rel}/");
                        self.files
                            .retain(|key, _| key != rel && !key.starts_with(&prefix));
                        self.dirs
                            .retain(|key| key != rel && !key.starts_with(&prefix));
                    }
                    VaultOp::EnsureDir(rel) => {
                        for part in components(rel) {
                            self.dirs.insert(part);
                        }
                    }
                    VaultOp::Write { rel, bytes } => {
                        for part in components(parent_of(rel).unwrap_or("")) {
                            self.dirs.insert(part);
                        }
                        self.files.insert(rel.clone(), bytes.clone());
                    }
                }
            }
        }
    }

    fn snapshot(store: &WorkerVaultStore) -> FakeDisk {
        let mut disk = FakeDisk::default();
        let mut pending = vec![String::new()];
        while let Some(dir) = pending.pop() {
            for entry in store.read_dir(&store.root().join(&dir)).expect("read dir") {
                let name = entry.name.to_string_lossy().into_owned();
                let rel = if dir.is_empty() {
                    name
                } else {
                    format!("{dir}/{name}")
                };
                match entry.kind {
                    EntryKind::Dir => {
                        disk.dirs.insert(rel.clone());
                        pending.push(rel);
                    }
                    EntryKind::File => {
                        disk.files
                            .insert(rel.clone(), store.read_bytes(&entry.path).unwrap());
                    }
                }
            }
        }
        disk
    }

    #[test]
    fn reads_and_writes_answer_from_the_mirror_and_dirty_only_writes() {
        let store = store();
        store.load("Notes/One.md", b"# One\n");
        assert!(!store.has_dirty(), "a boot snapshot writes nothing");
        assert_eq!(
            store
                .read_to_string(&store.root().join("Notes/One.md"))
                .unwrap(),
            "# One\n"
        );
        store
            .write_atomic(&store.root().join("Notes/Two.md"), b"two")
            .unwrap();
        assert_eq!(store.dirty_count(), 1);
        let path = store.root().join("Notes/One.md");
        store.write_atomic(&path, b"# One (edited)\n").unwrap();
        assert_eq!(store.read_to_string(&path).unwrap(), "# One (edited)\n");
        // Wrong root: a path outside the store is not addressable.
        assert_eq!(
            store
                .write_atomic(Path::new("/elsewhere/x.md"), b"x")
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn writing_never_invents_a_missing_folder() {
        let store = store();
        assert_eq!(
            store
                .write_atomic(&store.root().join("No/Note.md"), b"x")
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        store.create_dir_all(&store.root().join("No/Deep")).unwrap();
        store
            .write_atomic(&store.root().join("No/Deep/Note.md"), b"x")
            .unwrap();
    }

    #[test]
    fn opfs_identity_is_case_sensitive_and_same_file_is_the_same_spelling() {
        let store = store();
        store.load("Notes.md", b"upper");
        let root = store.root();
        assert!(
            !store.exists(&root.join("notes.md")),
            "OPFS keeps case apart"
        );
        assert!(store.same_file(&root.join("Notes.md"), &root.join("Notes.md")));
        assert!(
            !store.same_file(&root.join("Notes.md"), &root.join("notes.md")),
            "distinct spellings are distinct entries (#292 needs this false)"
        );
        store
            .write_atomic(&root.join("notes.md"), b"lower")
            .unwrap();
        assert_eq!(names(&store, ""), ["Notes.md", "notes.md"]);
    }

    #[test]
    fn rename_moves_a_file_and_a_folder_with_posix_file_replace() {
        let store = store();
        store.load("Notes/One.md", b"one");
        store.load("Notes/Two.md", b"two");
        store.load("Other.md", b"other");
        let root = store.root();
        store
            .rename(&root.join("Notes/One.md"), &root.join("Other.md"))
            .expect("a file replaces a file");
        assert_eq!(store.read_to_string(&root.join("Other.md")).unwrap(), "one");
        assert_eq!(names(&store, "Notes"), ["Two.md"]);
        store
            .rename(&root.join("Notes"), &root.join("Moved"))
            .expect("a folder moves its subtree");
        assert_eq!(
            store.read_to_string(&root.join("Moved/Two.md")).unwrap(),
            "two"
        );
        assert!(!store.exists(&root.join("Notes")), "the old folder is gone");
        // A file onto a folder is an error, not a replacement.
        assert_eq!(
            store
                .rename(&root.join("Other.md"), &root.join("Moved"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::IsADirectory
        );
    }

    #[test]
    fn directories_list_direct_children_and_report_opfs_metadata() {
        let store = store();
        store.load("A/One.md", b"1");
        store.load_dir("A/Empty");
        store.load_dir("Empty");
        assert_eq!(names(&store, ""), ["A", "Empty"]);
        assert_eq!(names(&store, "A"), ["Empty", "One.md"]);
        let meta = store.metadata(&store.root().join("A/One.md")).unwrap();
        assert_eq!(meta.kind, EntryKind::File);
        assert_eq!(meta.len, 1);
        assert_eq!(meta.links, 1, "OPFS has no hard links");
        assert!(!meta.symlink, "OPFS has no symlinks");
        let again = store
            .symlink_metadata(&store.root().join("A/One.md"))
            .unwrap();
        assert_eq!(meta, again, "there is no link/file distinction on OPFS");
        let dir = store.metadata(&store.root().join("A")).unwrap();
        assert_eq!(dir.kind, EntryKind::Dir);
        assert_eq!(dir.len, 0);
    }

    #[test]
    fn a_write_changes_the_logical_modified_stamp_even_at_equal_length() {
        let store = store();
        store.load("One.md", b"aa");
        let path = store.root().join("One.md");
        let before = store.metadata(&path).unwrap();
        store.write_atomic(&path, b"bb").unwrap();
        let after = store.metadata(&path).unwrap();
        assert_eq!(before.len, after.len);
        assert_ne!(
            before.modified, after.modified,
            "the guarded-write identity must change"
        );
    }

    #[test]
    fn remove_file_and_remove_dir_keep_std_fs_errors() {
        let store = store();
        store.load("A/One.md", b"1");
        let root = store.root();
        assert_eq!(
            store
                .remove_file(&root.join("Missing.md"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            store.remove_dir(&root.join("A")).unwrap_err().kind(),
            io::ErrorKind::DirectoryNotEmpty
        );
        store.remove_file(&root.join("A/One.md")).unwrap();
        store.remove_dir(&root.join("A")).unwrap();
        assert!(!store.exists(&root.join("A")));
        // `remove_dir_all` treats a missing folder as success.
        store.remove_dir_all(&root.join("A")).unwrap();
    }

    #[test]
    fn the_flush_ops_make_the_fake_disk_match_the_mirror() {
        let store = store();
        store.load("Notes/Keep.md", b"keep");
        store.load("Notes/Delete.md", b"delete");
        store.load("Gone/Deep.md", b"deep");
        store.load("Renamed.md", b"renamed");
        // The fake starts as OPFS was before the run: the boot snapshot.
        let mut disk = snapshot(&store);
        let root = store.root();
        store
            .write_atomic(&root.join("Notes/New.md"), b"new")
            .unwrap();
        store.remove_file(&root.join("Notes/Delete.md")).unwrap();
        store.remove_dir_all(&root.join("Gone")).unwrap();
        store.create_dir_all(&root.join("Fresh/Deep")).unwrap();
        store
            .rename(&root.join("Renamed.md"), &root.join("Notes/Moved.md"))
            .unwrap();

        let ops = store.take_flush_ops();
        assert!(!store.has_dirty(), "taking the ops drains the journal");
        // Order is part of the contract the wasm flush relies on.
        let first_write = ops
            .iter()
            .position(|op| matches!(op, VaultOp::Write { .. }))
            .unwrap();
        assert!(
            ops.iter()
                .take(first_write)
                .all(|op| !matches!(op, VaultOp::Write { .. }))
        );
        assert!(
            ops.iter()
                .position(|op| op == &VaultOp::Remove("Gone".to_owned()))
                .is_some()
        );

        disk.apply(&ops);
        assert_eq!(disk, snapshot(&store), "the flush reproduces the mirror");
        assert_eq!(
            disk.files.get("Notes/New.md").map(Vec::as_slice),
            Some(&b"new"[..])
        );
        assert!(!disk.files.contains_key("Notes/Delete.md"));
        assert!(!disk.dirs.contains("Gone"));
        assert!(
            disk.dirs.contains("Fresh/Deep"),
            "empty folders are flushed"
        );
        assert_eq!(
            disk.files.get("Notes/Moved.md").map(Vec::as_slice),
            Some(&b"renamed"[..])
        );
    }

    #[test]
    fn a_failed_flush_is_requeued() {
        let store = store();
        store
            .write_atomic(&store.root().join("A.md"), b"a")
            .unwrap();
        let ops = store.take_flush_ops();
        assert!(!store.has_dirty());
        store.requeue(&ops);
        assert_eq!(store.take_flush_ops(), ops);
    }
}

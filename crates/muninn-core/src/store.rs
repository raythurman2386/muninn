//! The storage seam behind a [`Vault`](crate::Vault).
//!
//! Every filesystem operation the vault performs — reading a note, saving it
//! atomically, renaming through the case-only temp hop, walking folders, the
//! trash budget, the `(mtime, length)` identity the guarded writes compare —
//! goes through one [`VaultStore`]. That keeps the vault model independent of
//! *where* the bytes live:
//!
//! - [`FsStore`] is the native filesystem and the default for [`Vault::load`].
//!   It wraps exactly the calls the vault used before this seam, so native
//!   behaviour is byte for byte unchanged.
//! - [`MemoryStore`] is a pure in-memory implementation with no filesystem
//!   access. [`Vault::from_sources`] builds one from its sources, and the
//!   headless mutation tests use it as the contract a browser (OPFS) store must
//!   reproduce.
//!
//! A store serves one root (its [`VaultStore::root`]) and answers the paths
//! under it: the paths on [`crate::Note::path`] and the paths the vault's
//! public API returns. [`FsStore`] uses them directly; [`MemoryStore`] strips
//! the root prefix to find its entry.
//!
//! [`Vault::load`]: crate::Vault::load
//! [`Vault::from_sources`]: crate::Vault::from_sources

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

/// Distinguishes concurrent temp files from the same process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// What a store reports for one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
}

/// The metadata a [`VaultStore`] reports.
///
/// This is what [`Vault::disk_state`](crate::Vault::disk_state) turns into the
/// `(mtime, length)` write identity, and what the trash budget and attachment
/// sizes read. [`VaultStore::metadata`] follows symbolic links;
/// [`VaultStore::symlink_metadata`] does not, so the rename safety checks can
/// still tell a link apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoreMeta {
    pub kind: EntryKind,
    /// True when the path itself is a symbolic link (only from
    /// [`VaultStore::symlink_metadata`]).
    pub symlink: bool,
    pub len: u64,
    /// How many directory entries name this file. An ordinary file reports 1; a
    /// hard link reports more. A logical store that cannot tell reports 1.
    /// Sync refuses a multiply-linked file because replacing one name would not
    /// change the bytes behind the others.
    pub links: u64,
    /// The file's modification time when the store has one. A logical store may
    /// supply its own monotonic stamp; see [`MemoryStore`].
    pub modified: Option<SystemTime>,
}

impl StoreMeta {
    /// True for a regular file.
    #[must_use]
    pub fn is_file(&self) -> bool {
        self.kind == EntryKind::File
    }

    /// True for a directory.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Dir
    }
}

/// One entry from [`VaultStore::read_dir`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreEntry {
    /// The entry's full path (the directory joined with its name).
    pub path: PathBuf,
    /// The final path segment as the store spells it.
    pub name: OsString,
    pub kind: EntryKind,
}

/// The filesystem operations a vault performs.
///
/// Paths are the vault's paths under [`VaultStore::root`] (absolute for a
/// loaded folder, root-prefixed for a source vault). Implementations must
/// preserve the semantics the native vault relies on:
///
/// - [`write_atomic`](VaultStore::write_atomic) replaces a file with either its
///   old or its new bytes, never a partial file.
/// - [`rename`](VaultStore::rename) is a whole-entry move. A rename whose two
///   spellings name the same entry (case-only on a case-insensitive store) must
///   keep the existing spelling, exactly like POSIX `rename(2)`; the vault's
///   temp hop then flips the case in two hops.
/// - [`read_dir`](VaultStore::read_dir) lists direct children only and does not
///   follow symbolic links.
///
/// The trait is object-safe: [`Vault`](crate::Vault) holds an
/// `Arc<dyn VaultStore>` so its public API stays non-generic.
pub trait VaultStore: Send + Sync {
    /// The root this store serves. Every path handed to the store is under it.
    fn root(&self) -> &Path;

    /// Read a file as UTF-8. Invalid UTF-8 is
    /// [`io::ErrorKind::InvalidData`], like [`std::fs::read_to_string`].
    fn read_to_string(&self, path: &Path) -> io::Result<String>;

    /// Read a file's raw bytes, binary included: the counterpart to
    /// [`read_to_string`](VaultStore::read_to_string) for attachments, sync hashes
    /// and any caller that must not assume UTF-8.
    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// Replace `path` with `bytes` atomically.
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;

    /// Move `from` to `to`, replacing an existing file at `to` (POSIX rename).
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Remove one file.
    fn remove_file(&self, path: &Path) -> io::Result<()>;

    /// Create one directory. Its parent must already exist.
    fn create_dir(&self, path: &Path) -> io::Result<()>;

    /// Create `path` and any missing parents.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;

    /// Remove one empty directory.
    fn remove_dir(&self, path: &Path) -> io::Result<()>;

    /// Remove `path` and everything below it.
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;

    /// List the direct children of a directory.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<StoreEntry>>;

    /// Metadata for `path`, following symbolic links.
    fn metadata(&self, path: &Path) -> io::Result<StoreMeta>;

    /// Metadata for `path` without following a final symbolic link. `symlink`
    /// in the result tells a link apart from the file it names.
    fn symlink_metadata(&self, path: &Path) -> io::Result<StoreMeta>;

    /// Whether `a` and `b` name the same entry. The rename safety checks use
    /// this to tell a case-only rename on a case-insensitive store from a real
    /// collision (issue #292).
    fn same_file(&self, a: &Path, b: &Path) -> bool;

    /// The store's canonical spelling of `path`. Errors when it does not exist.
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;

    /// Whether `path` exists (following symbolic links).
    fn exists(&self, path: &Path) -> bool {
        self.metadata(path).is_ok()
    }

    /// Whether `path` is a regular file (following symbolic links).
    fn is_file(&self, path: &Path) -> bool {
        self.metadata(path)
            .is_ok_and(|meta| meta.kind == EntryKind::File)
    }

    /// Whether `path` is a directory.
    fn is_dir(&self, path: &Path) -> bool {
        self.metadata(path)
            .is_ok_and(|meta| meta.kind == EntryKind::Dir)
    }
}

/// The native filesystem, the default store for [`Vault::load`](crate::Vault::load).
///
/// Every method is the `std::fs` call the vault used before the seam, including
/// [`atomic_write`]'s hidden temp file + fsync + rename.
#[derive(Clone, Debug)]
pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    /// A store serving `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> FsStore {
        FsStore { root: root.into() }
    }
}

impl VaultStore for FsStore {
    fn root(&self) -> &Path {
        &self.root
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }

    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        atomic_write(path, bytes)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        fs::create_dir(path)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<StoreEntry>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            // The walk, the trash budget and the prune all skip an unreadable
            // entry rather than failing the whole listing; keep that posture.
            let Ok(entry) = entry else {
                continue;
            };
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            // A symbolic link is neither walked into nor counted: the existing
            // callers used `file_type().is_dir()` / `is_file()`, which are both
            // false for a link.
            let kind = if file_type.is_dir() {
                EntryKind::Dir
            } else if file_type.is_file() {
                EntryKind::File
            } else {
                continue;
            };
            entries.push(StoreEntry {
                path: entry.path(),
                name: entry.file_name(),
                kind,
            });
        }
        Ok(entries)
    }

    fn metadata(&self, path: &Path) -> io::Result<StoreMeta> {
        let meta = fs::metadata(path)?;
        Ok(StoreMeta {
            kind: if meta.is_dir() {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
            symlink: false,
            len: meta.len(),
            links: link_count(&meta),
            modified: meta.modified().ok(),
        })
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<StoreMeta> {
        let meta = fs::symlink_metadata(path)?;
        Ok(StoreMeta {
            kind: if meta.is_dir() {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
            symlink: meta.file_type().is_symlink(),
            len: meta.len(),
            links: link_count(&meta),
            modified: meta.modified().ok(),
        })
    }

    fn same_file(&self, a: &Path, b: &Path) -> bool {
        same_file_meta(a, b)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        fs::canonicalize(path)
    }
}

/// The number of names a file has, where the platform can tell.
#[cfg(unix)]
fn link_count(meta: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.nlink()
}

#[cfg(not(unix))]
fn link_count(_meta: &fs::Metadata) -> u64 {
    // Stable `std` exposes no link count on Windows; report the ordinary case,
    // exactly as the pre-seam inventory did there.
    1
}

/// #292: whether two paths name one file on this filesystem.
///
/// On Unix the inode says so. On Windows stable std has no file index, but
/// `canonicalize` resolves the on-disk case, so two spellings of one file give
/// the same path. Elsewhere there is no portable identity: refuse rather than
/// risk a rename.
#[cfg(unix)]
fn same_file_meta(source: &Path, target: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let (Ok(source), Ok(target)) = (fs::metadata(source), fs::metadata(target)) else {
        return false;
    };
    source.dev() == target.dev() && source.ino() == target.ino()
}

#[cfg(windows)]
fn same_file_meta(source: &Path, target: &Path) -> bool {
    match (fs::canonicalize(source), fs::canonicalize(target)) {
        (Ok(source), Ok(target)) => source == target,
        _ => false,
    }
}

#[cfg(not(any(unix, windows)))]
fn same_file_meta(_source: &Path, _target: &Path) -> bool {
    false
}

/// A pure in-memory [`VaultStore`] with no filesystem access.
///
/// Storage is a map from a case-folded vault-relative path to its bytes, plus
/// the directories that exist. The store is deliberately **case-insensitive**
/// about identity (as a default macOS/Windows volume is): lookups, existence,
/// renames and `canonicalize` fold ASCII/Unicode case, while the spelling a
/// file was created with is preserved for listings. That is the stricter
/// contract for the browser store — it makes the #292 case-only rename take
/// its two-hop path instead of a silent no-op.
///
/// # Write identity
///
/// Every write (including [`MemoryStore::put_file`]) increments a logical
/// clock, and [`VaultStore::metadata`] reports that stamp as `modified` with
/// the byte length. [`Vault::disk_state`](crate::Vault::disk_state) therefore
/// returns a `(Some(time), length)` identity that changes on every write, even
/// one that keeps the same length. A native filesystem can miss such a write
/// when its timestamp granularity and length both happen to match; the memory
/// store never does, which is what the guarded-write tests pin.
pub struct MemoryStore {
    root: PathBuf,
    clock: AtomicU64,
    entries: Mutex<Entries>,
}

#[derive(Default)]
struct Entries {
    /// Case-folded relative path → file.
    files: BTreeMap<String, MemoryFile>,
    /// Case-folded relative path → spelled relative path. Always holds `""`.
    dirs: BTreeMap<String, String>,
}

struct MemoryFile {
    /// The spelling to list, with `/` separators.
    rel: String,
    bytes: Vec<u8>,
    stamp: u64,
}

impl MemoryStore {
    /// An empty store serving `root`. The root directory always exists.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> MemoryStore {
        let mut dirs = BTreeMap::new();
        dirs.insert(String::new(), String::new());
        MemoryStore {
            root: root.into(),
            clock: AtomicU64::new(0),
            entries: Mutex::new(Entries {
                files: BTreeMap::new(),
                dirs,
            }),
        }
    }

    /// Seed or replace `rel` with `bytes`, creating its parent folders.
    ///
    /// This is the store-level writer used by `Vault::from_sources`
    /// (crate::Vault::from_sources) and by tests standing in for an external
    /// writer. A relative path is normalised like a vault folder: `.` and empty
    /// segments are dropped. An existing file keeps the spelling it was
    /// created with, exactly as a write to a case-insensitive volume does.
    pub fn put_file(&self, rel: &str, bytes: &[u8]) {
        let rel = normalise_rel(rel);
        if rel.is_empty() {
            // A seed of the root itself has no meaning.
            return;
        }
        let mut entries = self.entries();
        // Seeding creates the folders a file implies, so a caller can hand over
        // `assets/pic.png` without a separate directory pass.
        let parent = entries.ensure_parents(&rel);
        let (_, name) = split_rel(&rel);
        let display = join_display(&parent, name);
        let key = rel.to_lowercase();
        let stamp = self.next_stamp();
        match entries.files.get_mut(&key) {
            Some(file) => {
                file.bytes = bytes.to_vec();
                file.stamp = stamp;
            }
            None => {
                entries.files.insert(
                    key,
                    MemoryFile {
                        rel: display,
                        bytes: bytes.to_vec(),
                        stamp,
                    },
                );
            }
        }
    }

    /// The raw bytes stored for `rel`, for a caller that keeps binary files in
    /// the same store (the browser vault's OPFS flush writes attachments back
    /// unchanged). The path is normalised like [`MemoryStore::put_file`].
    #[must_use]
    pub fn read_bytes(&self, rel: &str) -> Option<Vec<u8>> {
        let rel = normalise_rel(rel);
        self.entries()
            .files
            .get(&rel.to_lowercase())
            .map(|file| file.bytes.clone())
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, Entries> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn next_stamp(&self) -> u64 {
        self.clock.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// The `/`-separated path relative to the root, or `NotFound` when the path
    /// is not under it. `..` cannot escape the vault.
    fn rel_of(&self, path: &Path) -> io::Result<String> {
        let rel = path.strip_prefix(&self.root).map_err(|_| not_found(path))?;
        let mut parts: Vec<String> = Vec::new();
        for component in rel.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::CurDir => {}
                Component::ParentDir => return Err(permission_denied(path)),
                Component::RootDir | Component::Prefix(_) => return Err(not_found(path)),
            }
        }
        Ok(parts.join("/"))
    }
}

impl Entries {
    /// The spelled path of an existing parent directory plus `rel`'s file name,
    /// or `None` when the parent is missing (or `rel` names the root).
    fn existing_parent_display(&self, rel: &str) -> Option<String> {
        let (parent, name) = split_rel(rel);
        if name.is_empty() {
            return None;
        }
        let parent_display = self.dirs.get(&parent.to_lowercase())?;
        Some(join_display(parent_display, name))
    }

    /// Make every directory on `rel`'s parent chain, returning the spelled path
    /// of the deepest one.
    fn ensure_parents(&mut self, rel: &str) -> String {
        let (parent, _) = split_rel(rel);
        self.ensure_dir(parent)
    }

    fn ensure_dir(&mut self, rel: &str) -> String {
        if rel.is_empty() {
            return String::new();
        }
        let key = rel.to_lowercase();
        if let Some(display) = self.dirs.get(&key) {
            return display.clone();
        }
        let (parent, name) = split_rel(rel);
        let parent_display = self.ensure_dir(parent);
        let display = join_display(&parent_display, name);
        self.dirs.insert(key, display.clone());
        display
    }
}

impl VaultStore for MemoryStore {
    fn root(&self) -> &Path {
        &self.root
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let rel = self.rel_of(path)?;
        let entries = self.entries();
        let Some(file) = entries.files.get(&rel.to_lowercase()) else {
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
        let entries = self.entries();
        entries
            .files
            .get(&rel.to_lowercase())
            .map(|file| file.bytes.clone())
            .ok_or_else(|| not_found(path))
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        let mut entries = self.entries();
        let key = rel.to_lowercase();
        let stamp = self.next_stamp();
        if let Some(file) = entries.files.get_mut(&key) {
            file.bytes = bytes.to_vec();
            file.stamp = stamp;
            return Ok(());
        }
        // Writing never creates parents: the vault's callers make folders with
        // `create_dir_all` first, and a missing folder must surface as an error.
        let Some(display) = entries.existing_parent_display(&rel) else {
            return Err(not_found(path));
        };
        entries.files.insert(
            key,
            MemoryFile {
                rel: display,
                bytes: bytes.to_vec(),
                stamp,
            },
        );
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let from_rel = self.rel_of(from)?;
        let to_rel = self.rel_of(to)?;
        let from_key = from_rel.to_lowercase();
        let to_key = to_rel.to_lowercase();
        if from_rel.is_empty() || to_rel.is_empty() {
            return Err(permission_denied(from));
        }
        let mut entries = self.entries();
        if from_key == to_key {
            // POSIX `rename(2)` on a case-insensitive volume: both spellings
            // name one entry, so the call succeeds and the spelling is kept.
            // `rename_through_temp` is what flips the case in two hops.
            let exists =
                entries.files.contains_key(&from_key) || entries.dirs.contains_key(&from_key);
            return if exists { Ok(()) } else { Err(not_found(from)) };
        }
        // The destination's parent must exist before anything moves.
        let Some(to_display) = entries.existing_parent_display(&to_rel) else {
            return Err(not_found(to));
        };
        if let Some(file) = entries.files.remove(&from_key) {
            if entries.dirs.contains_key(&to_key) {
                entries.files.insert(from_key, file);
                return Err(io::Error::new(
                    io::ErrorKind::IsADirectory,
                    format!("{} is a folder", to.display()),
                ));
            }
            // A file replaces a file at the destination, as POSIX rename does.
            entries.files.insert(
                to_key,
                MemoryFile {
                    rel: to_display,
                    bytes: file.bytes,
                    stamp: file.stamp,
                },
            );
            return Ok(());
        }
        if !entries.dirs.contains_key(&from_key) {
            return Err(not_found(from));
        }
        if entries.dirs.contains_key(&to_key) || entries.files.contains_key(&to_key) {
            return Err(already_exists(to));
        }
        let Some(from_display) = entries.dirs.get(&from_key).cloned() else {
            return Err(not_found(from));
        };
        // Move the directory and every descendant, rebasing their spellings on
        // the destination while keeping the descendants' own names/case.
        let prefix = format!("{from_key}/");
        let mut dirs: Vec<(String, String)> = Vec::new();
        let mut files: Vec<(String, MemoryFile)> = Vec::new();
        for (key, display) in &entries.dirs {
            if key == &from_key || key.starts_with(&prefix) {
                let suffix = display_segment_suffix(display, &from_display);
                let new_key = rebase_key(key, &from_key, &to_key);
                dirs.push((new_key, join_display(&to_display, suffix)));
            }
        }
        for (key, file) in &entries.files {
            if key.starts_with(&prefix) {
                let suffix = display_segment_suffix(&file.rel, &from_display);
                let new_key = rebase_key(key, &from_key, &to_key);
                files.push((
                    new_key,
                    MemoryFile {
                        rel: join_display(&to_display, suffix),
                        bytes: file.bytes.clone(),
                        stamp: file.stamp,
                    },
                ));
            }
        }
        entries
            .dirs
            .retain(|key, _| key != &from_key && !key.starts_with(&prefix));
        entries.files.retain(|key, _| !key.starts_with(&prefix));
        for (key, display) in dirs {
            entries.dirs.insert(key, display);
        }
        for (key, file) in files {
            entries.files.insert(key, file);
        }
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        let mut entries = self.entries();
        if entries.files.remove(&rel.to_lowercase()).is_some() {
            Ok(())
        } else {
            Err(not_found(path))
        }
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        let key = rel.to_lowercase();
        let mut entries = self.entries();
        if entries.dirs.contains_key(&key) || entries.files.contains_key(&key) {
            return Err(already_exists(path));
        }
        let Some(display) = entries.existing_parent_display(&rel) else {
            return Err(not_found(path));
        };
        entries.dirs.insert(key, display);
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        let mut entries = self.entries();
        entries.ensure_dir(&rel);
        Ok(())
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        if rel.is_empty() {
            return Err(permission_denied(path));
        }
        let key = rel.to_lowercase();
        let mut entries = self.entries();
        if !entries.dirs.contains_key(&key) {
            return Err(not_found(path));
        }
        let prefix = format!("{key}/");
        if entries.files.keys().any(|k| k.starts_with(&prefix))
            || entries.dirs.keys().any(|k| k.starts_with(&prefix))
        {
            return Err(io::Error::new(
                io::ErrorKind::DirectoryNotEmpty,
                format!("{} is not empty", path.display()),
            ));
        }
        entries.dirs.remove(&key);
        Ok(())
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let rel = self.rel_of(path)?;
        let key = rel.to_lowercase();
        let mut entries = self.entries();
        if entries.files.contains_key(&key) {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} is a file", path.display()),
            ));
        }
        if key.is_empty() || !entries.dirs.contains_key(&key) {
            // `std::fs::remove_dir_all` treats a missing path as success.
            return Ok(());
        }
        let prefix = format!("{key}/");
        entries
            .files
            .retain(|candidate, _| !candidate.starts_with(&prefix));
        entries
            .dirs
            .retain(|candidate, _| candidate != &key && !candidate.starts_with(&prefix));
        Ok(())
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<StoreEntry>> {
        let rel = self.rel_of(path)?;
        let key = rel.to_lowercase();
        let entries = self.entries();
        if !entries.dirs.contains_key(&key) {
            return Err(not_found(path));
        }
        let prefix = if key.is_empty() {
            String::new()
        } else {
            format!("{key}/")
        };
        let mut out = Vec::new();
        for (child_key, display) in &entries.dirs {
            if child_key.is_empty() || child_key == &key {
                continue;
            }
            let Some(suffix) = child_key.strip_prefix(&prefix) else {
                continue;
            };
            if suffix.contains('/') {
                continue;
            }
            let name = display.rsplit('/').next().unwrap_or(display);
            out.push(StoreEntry {
                path: self.root.join(display),
                name: OsString::from(name),
                kind: EntryKind::Dir,
            });
        }
        for (child_key, file) in &entries.files {
            let Some(suffix) = child_key.strip_prefix(&prefix) else {
                continue;
            };
            if suffix.contains('/') {
                continue;
            }
            let name = file.rel.rsplit('/').next().unwrap_or(&file.rel);
            out.push(StoreEntry {
                path: self.root.join(&file.rel),
                name: OsString::from(name),
                kind: EntryKind::File,
            });
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn metadata(&self, path: &Path) -> io::Result<StoreMeta> {
        let rel = self.rel_of(path)?;
        let entries = self.entries();
        let key = rel.to_lowercase();
        if let Some(file) = entries.files.get(&key) {
            return Ok(StoreMeta {
                kind: EntryKind::File,
                symlink: false,
                len: file.bytes.len() as u64,
                links: 1,
                // The logical write stamp, expressed as a time so the guarded
                // writes keep their `(mtime, length)` shape.
                modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(file.stamp)),
            });
        }
        if entries.dirs.contains_key(&key) {
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
        // There are no symbolic links in memory.
        self.metadata(path)
    }

    fn same_file(&self, a: &Path, b: &Path) -> bool {
        let (Ok(a), Ok(b)) = (self.rel_of(a), self.rel_of(b)) else {
            return false;
        };
        let a_key = a.to_lowercase();
        if a_key != b.to_lowercase() {
            return false;
        }
        let entries = self.entries();
        entries.files.contains_key(&a_key) || entries.dirs.contains_key(&a_key)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let rel = self.rel_of(path)?;
        let entries = self.entries();
        let key = rel.to_lowercase();
        if let Some(file) = entries.files.get(&key) {
            return Ok(self.root.join(&file.rel));
        }
        if let Some(display) = entries.dirs.get(&key) {
            return Ok(if display.is_empty() {
                self.root.clone()
            } else {
                self.root.join(display)
            });
        }
        Err(not_found(path))
    }
}

/// Split a `/`-separated relative path into `(parent, name)`; the parent of a
/// top-level name is `""`.
fn split_rel(rel: &str) -> (&str, &str) {
    rel.rsplit_once('/').unwrap_or(("", rel))
}

/// Join a spelled parent with a child name, leaving the root un-prefixed.
fn join_display(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else if name.is_empty() {
        parent.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

/// The part of `display` below `base` (both `/`-separated, same depth prefix).
/// Used when a directory subtree is renamed so descendants keep their spelling.
fn display_segment_suffix<'a>(display: &'a str, base: &str) -> &'a str {
    let depth = base.split('/').filter(|part| !part.is_empty()).count();
    let mut rest = display;
    for _ in 0..depth {
        match rest.split_once('/') {
            Some((_, tail)) => rest = tail,
            None => return "",
        }
    }
    rest
}

/// Rebase a case-folded key from `from` to `to`, keeping its canonical suffix.
fn rebase_key(key: &str, from: &str, to: &str) -> String {
    format!("{to}{}", &key[from.len()..])
}

/// Normalise a caller-supplied relative path: `/` separators and no `.`/empty
/// segments.
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

/// Write `data` to `path` atomically: a hidden, uniquely named temp file in
/// the same directory is flushed and fsynced, then renamed over `path`.
///
/// A crash at any point leaves either the old bytes or the new bytes at
/// `path`, never a truncated file. A failed write removes the temp file. The
/// temp name starts with `.` so the vault walk ignores it even if a killed
/// process leaves one behind.
pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    atomic_replace(path, true, |file| file.write_all(data))
}

/// Copy `source` into `dest` through the same hidden temp + fsync + rename path
/// as note saves. An existing `dest` is never replaced.
///
/// Drag-drop imports use this so a half-copied attachment can never appear in
/// the vault and a same-named file keeps its bytes.
pub fn atomic_copy(source: &Path, dest: &Path) -> io::Result<()> {
    atomic_replace(dest, false, |file| {
        let mut reader = fs::File::open(source)?;
        io::copy(&mut reader, file).map(|_| ())
    })
}

/// The one temp-file + rename implementation behind note saves and attachment
/// imports. `replace` is false for imports: the destination is re-checked just
/// before the rename so a racing copy cannot overwrite a file that appeared.
fn atomic_replace(
    path: &Path,
    replace: bool,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map_or_else(|| "note".into(), |n| n.to_string_lossy().into_owned());
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    // pid + counter keeps concurrent writers (and repeated saves) apart.
    let temp = dir.join(format!(".{name}.{}.{counter}.tmp", std::process::id()));

    let write = (|| -> io::Result<()> {
        {
            let mut file = fs::File::create(&temp)?;
            write(&mut file)?;
            file.flush()?;
            // fsync before rename: otherwise the rename can reach disk first
            // and a crash could expose an empty or partial file.
            file.sync_all()?;
            // Close before rename; Windows cannot rename an open file.
        }
        if !replace && path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                muninn_i18n::t(muninn_i18n::Key::VaultDestinationExists),
            ));
        }
        fs::rename(&temp, path)
    })();

    if write.is_err() {
        // Best-effort cleanup: report the original error, not the removal's.
        let _ = fs::remove_file(&temp);
        return write;
    }
    sync_parent(dir);
    Ok(())
}

/// fsync the directory a rename landed in, best-effort.
///
/// Some filesystems and platforms reject fsync on a directory. The rename is
/// already atomic; skipping (or failing) this sync only weakens durability
/// across a power loss, so the error is deliberately ignored — there is no
/// logger yet. Windows has no portable directory fsync, so it is a no-op.
fn sync_parent(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> MemoryStore {
        MemoryStore::new("/mem")
    }

    fn names(store: &MemoryStore, rel: &str) -> Vec<String> {
        store
            .read_dir(&store.root().join(rel))
            .expect("read dir")
            .into_iter()
            .map(|entry| entry.name.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_memory_store_writes_reads_and_lists_without_a_filesystem() {
        let store = store();
        store.put_file("notes/Alpha.md", b"# Alpha\n");
        store.put_file("Root.md", b"# Root\n");

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
            "case-insensitive"
        );
        assert!(
            store.is_dir(&store.root().join("NOTES")),
            "case-insensitive"
        );
        assert!(!store.exists(&store.root().join("missing.md")));
    }

    #[test]
    fn a_memory_store_rename_is_a_no_op_on_one_identity_and_moves_a_subtree() {
        let store = store();
        store.put_file("leaves.md", b"body");
        // Case-only: the identity already matches, so a plain rename keeps the
        // old spelling (which is exactly why the vault hops through a temp).
        store
            .rename(
                &store.root().join("leaves.md"),
                &store.root().join("Leaves.md"),
            )
            .unwrap();
        assert_eq!(names(&store, ""), ["leaves.md"]);

        store.put_file("notes/One.md", b"one");
        store.put_file("notes/deep/Two.md", b"two");
        store
            .rename(&store.root().join("notes"), &store.root().join("Archive"))
            .unwrap();
        assert_eq!(names(&store, ""), ["Archive", "leaves.md"]);
        assert_eq!(names(&store, "archive"), ["One.md", "deep"]);
        assert_eq!(names(&store, "Archive/deep"), ["Two.md"]);
        assert_eq!(
            store
                .read_to_string(&store.root().join("Archive/deep/Two.md"))
                .unwrap(),
            "two"
        );
    }

    #[test]
    fn a_memory_store_write_stamp_changes_even_when_the_length_does_not() {
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
    fn a_memory_store_refuses_to_write_under_a_missing_folder() {
        let store = store();
        let missing = store.root().join("nowhere/Note.md");
        assert_eq!(
            store.write_atomic(&missing, b"x").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        store.create_dir_all(&store.root().join("nowhere")).unwrap();
        store.write_atomic(&missing, b"x").unwrap();
        assert!(store.is_file(&missing));
    }

    #[test]
    fn a_memory_store_reports_invalid_utf8_like_the_filesystem() {
        let store = store();
        store.put_file("bad.md", &[0xff, 0xfe]);
        assert_eq!(
            store
                .read_to_string(&store.root().join("bad.md"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_memory_store_reads_raw_bytes_and_reports_one_link() {
        let store = store();
        let path = store.root().join("assets/pic.png");
        store.put_file("assets/pic.png", &[0xff, 0xfe]);
        assert_eq!(
            VaultStore::read_bytes(&store, &path).unwrap(),
            vec![0xff, 0xfe],
            "binary bytes come back unchanged"
        );
        assert_eq!(store.metadata(&path).unwrap().links, 1);
    }

    #[cfg(unix)]
    #[test]
    fn the_native_store_reports_a_hard_link_count() {
        let dir = std::env::temp_dir().join(format!(
            "muninn-store-links-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.md"), b"a").unwrap();
        let store = FsStore::new(dir.canonicalize().unwrap());
        let path = store.root().join("a.md");
        assert_eq!(store.metadata(&path).unwrap().links, 1);
        fs::hard_link(&path, dir.join("b.md")).unwrap();
        assert_eq!(store.metadata(&path).unwrap().links, 2);
        let _ = fs::remove_dir_all(&dir);
    }
}

//! Directory walks and per-file writes that do not follow symlinks or
//! reparse points. Callers pass a [`RelativePath`] that has already been
//! validated; these helpers still refuse to leave `root`.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use crate::error::SyncError;
use crate::pathutil::RelativePath;

/// Open `root` as a real directory. `root` must already be canonical.
pub fn open_root(root: &Path) -> Result<RootDir, SyncError> {
    if !root.is_absolute() {
        return Err(SyncError::Path {
            detail: format!("{} is not absolute", root.display()),
        });
    }
    let metadata = fs::symlink_metadata(root).map_err(SyncError::io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SyncError::Unsupported {
            path: root.display().to_string(),
            detail: "vault root is not a real directory".to_owned(),
        });
    }
    #[cfg(unix)]
    {
        unix::open_root(root)
    }
    #[cfg(not(unix))]
    {
        Ok(RootDir {
            path: root.to_path_buf(),
        })
    }
}

pub struct RootDir {
    #[cfg(unix)]
    fd: std::os::fd::OwnedFd,
    #[cfg(not(unix))]
    path: std::path::PathBuf,
}

impl RootDir {
    pub fn list(&self, rel: Option<&RelativePath>) -> Result<Vec<DirName>, SyncError> {
        #[cfg(unix)]
        {
            unix::list(self, rel)
        }
        #[cfg(not(unix))]
        {
            windows::list(self, rel)
        }
    }

    pub fn read_file(&self, rel: &RelativePath, max_bytes: u64) -> Result<Vec<u8>, SyncError> {
        #[cfg(unix)]
        {
            unix::read_file(self, rel, max_bytes)
        }
        #[cfg(not(unix))]
        {
            windows::read_file(self, rel, max_bytes)
        }
    }

    pub fn metadata(&self, rel: &RelativePath) -> Result<Option<EntryMeta>, SyncError> {
        #[cfg(unix)]
        {
            unix::metadata(self, rel)
        }
        #[cfg(not(unix))]
        {
            windows::metadata(self, rel)
        }
    }

    /// Create or replace a regular file without following a symlink at the
    /// destination. Parent directories are created only as real directories.
    pub fn write_file(&self, rel: &RelativePath, bytes: &[u8]) -> Result<(), SyncError> {
        #[cfg(unix)]
        {
            unix::write_file(self, rel, bytes)
        }
        #[cfg(not(unix))]
        {
            windows::write_file(self, rel, bytes)
        }
    }

    pub fn remove_file(&self, rel: &RelativePath) -> Result<(), SyncError> {
        #[cfg(unix)]
        {
            unix::remove_file(self, rel)
        }
        #[cfg(not(unix))]
        {
            windows::remove_file(self, rel)
        }
    }
}

#[derive(Clone, Debug)]
pub struct DirName {
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File { bytes: u64, hardlinked: bool },
    Dir,
}

#[derive(Clone, Copy, Debug)]
pub struct EntryMeta {
    pub kind: EntryKind,
}

#[cfg(unix)]
mod unix {
    use std::ffi::OsStr;
    use std::io::{Read, Write};
    use std::os::fd::OwnedFd;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    use rustix::fs::{
        AtFlags, Dir, FileType, Mode, OFlags, fsync, mkdirat, open, openat, renameat, statat,
        unlinkat,
    };

    use super::{DirName, EntryKind, EntryMeta, RootDir};
    use crate::error::SyncError;
    use crate::pathutil::RelativePath;

    pub fn open_root(root: &Path) -> Result<RootDir, SyncError> {
        let fd = open(
            root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| SyncError::Unsupported {
            path: root.display().to_string(),
            detail: format!("could not open the vault without following links: {error}"),
        })?;
        Ok(RootDir { fd })
    }

    fn parent_fd(root: &RootDir, rel: &RelativePath, create: bool) -> Result<OwnedFd, SyncError> {
        let mut fd = rustix::fs::openat(
            &root.fd,
            ".",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(SyncError::io)?;
        if let Some(parent) = rel.parent() {
            for part in parent.components() {
                fd = open_child_dir(&fd, part, create)?;
            }
        }
        Ok(fd)
    }

    fn open_child_dir(parent: &OwnedFd, name: &str, create: bool) -> Result<OwnedFd, SyncError> {
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        match openat(parent, name, flags, Mode::empty()) {
            Ok(fd) => Ok(fd),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
                mkdirat(parent, name, Mode::from_raw_mode(0o755)).map_err(|error| {
                    SyncError::Io {
                        detail: format!("could not create {name}: {error}"),
                    }
                })?;
                openat(parent, name, flags, Mode::empty()).map_err(|error| SyncError::Unsupported {
                    path: name.to_owned(),
                    detail: format!("created directory is not a real directory: {error}"),
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(SyncError::Inventory {
                    detail: format!("{name} does not exist"),
                })
            }
            Err(error) => Err(SyncError::Unsupported {
                path: name.to_owned(),
                detail: format!("refusing to follow or replace {name}: {error}"),
            }),
        }
    }

    pub fn list(root: &RootDir, rel: Option<&RelativePath>) -> Result<Vec<DirName>, SyncError> {
        let fd = match rel {
            Some(rel) => {
                let parent = parent_fd(root, rel, false)?;
                open_child_dir(&parent, rel.file_name(), false)?
            }
            None => rustix::fs::openat(
                &root.fd,
                ".",
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(SyncError::io)?,
        };
        let dir = Dir::read_from(fd).map_err(|error| SyncError::Inventory {
            detail: format!("could not read {}: {error}", display_rel(rel)),
        })?;
        let mut names = Vec::new();
        for entry in dir {
            let entry = entry.map_err(|error| SyncError::Inventory {
                detail: format!("could not read {}: {error}", display_rel(rel)),
            })?;
            let name = entry.file_name().to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            let Some(text) = OsStr::from_bytes(name).to_str() else {
                return Err(SyncError::Path {
                    detail: format!("non-utf8 name under {}", display_rel(rel)),
                });
            };
            names.push(DirName {
                name: text.to_owned(),
            });
        }
        names.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(names)
    }

    pub fn metadata(root: &RootDir, rel: &RelativePath) -> Result<Option<EntryMeta>, SyncError> {
        let parent = match parent_fd(root, rel, false) {
            Ok(parent) => parent,
            Err(SyncError::Inventory { .. }) => return Ok(None),
            Err(error) => return Err(error),
        };
        match statat(&parent, rel.file_name(), AtFlags::SYMLINK_NOFOLLOW) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(SyncError::io(error)),
            Ok(stat) => Ok(Some(meta_from_stat(&stat, rel)?)),
        }
    }

    fn meta_from_stat(stat: &rustix::fs::Stat, rel: &RelativePath) -> Result<EntryMeta, SyncError> {
        let kind = FileType::from_raw_mode(stat.st_mode);
        if kind.is_symlink() {
            return Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "symlink".to_owned(),
            });
        }
        if kind.is_dir() {
            return Ok(EntryMeta {
                kind: EntryKind::Dir,
            });
        }
        if kind.is_file() {
            return Ok(EntryMeta {
                kind: EntryKind::File {
                    bytes: stat.st_size as u64,
                    hardlinked: stat.st_nlink > 1,
                },
            });
        }
        Err(SyncError::Unsupported {
            path: rel.to_string(),
            detail: "special file".to_owned(),
        })
    }

    pub fn read_file(
        root: &RootDir,
        rel: &RelativePath,
        max_bytes: u64,
    ) -> Result<Vec<u8>, SyncError> {
        let meta = metadata(root, rel)?.ok_or_else(|| SyncError::Inventory {
            detail: format!("{rel} disappeared while it was being read"),
        })?;
        let EntryKind::File { bytes, hardlinked } = meta.kind else {
            return Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "not a regular file".to_owned(),
            });
        };
        if hardlinked {
            return Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "hard link; sync would change another name".to_owned(),
            });
        }
        if bytes > max_bytes {
            return Err(SyncError::Limit {
                detail: format!("{rel} is {bytes} bytes; the limit is {max_bytes}"),
            });
        }
        let parent = parent_fd(root, rel, false)?;
        let fd = openat(
            &parent,
            rel.file_name(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| SyncError::Unsupported {
            path: rel.to_string(),
            detail: format!("refusing to open: {error}"),
        })?;
        let mut file = std::fs::File::from(fd);
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|error| SyncError::Inventory {
                detail: format!("could not read {rel}: {error}"),
            })?;
        if buf.len() as u64 > max_bytes {
            return Err(SyncError::Limit {
                detail: format!("{rel} grew past {max_bytes} bytes while it was being read"),
            });
        }
        Ok(buf)
    }

    pub fn write_file(root: &RootDir, rel: &RelativePath, bytes: &[u8]) -> Result<(), SyncError> {
        let parent = parent_fd(root, rel, true)?;
        if let Some(meta) = metadata(root, rel)?
            && !matches!(meta.kind, EntryKind::File { .. })
        {
            return Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "refusing to replace a non-file".to_owned(),
            });
        }
        let temp_name = format!(".muninn-sync-{}-{}.tmp", std::process::id(), unique());
        let fd = openat(
            &parent,
            temp_name.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o644),
        )
        .map_err(|error| SyncError::Io {
            detail: format!("could not create a temp file for {rel}: {error}"),
        })?;
        let write_result = (|| -> Result<(), SyncError> {
            let mut file = std::fs::File::from(fd);
            file.write_all(bytes).map_err(SyncError::io)?;
            file.sync_all().map_err(SyncError::io)?;
            Ok(())
        })();
        if let Err(error) = write_result {
            let _ = unlinkat(&parent, temp_name.as_str(), AtFlags::empty());
            return Err(error);
        }
        renameat(&parent, temp_name.as_str(), &parent, rel.file_name()).map_err(|error| {
            let _ = unlinkat(&parent, temp_name.as_str(), AtFlags::empty());
            SyncError::Io {
                detail: format!("could not replace {rel}: {error}"),
            }
        })?;
        fsync(&parent).map_err(SyncError::io)?;
        Ok(())
    }

    pub fn remove_file(root: &RootDir, rel: &RelativePath) -> Result<(), SyncError> {
        let meta = metadata(root, rel)?;
        match meta {
            None => Ok(()),
            Some(meta)
                if matches!(
                    meta.kind,
                    EntryKind::File {
                        hardlinked: false,
                        ..
                    }
                ) =>
            {
                let parent = parent_fd(root, rel, false)?;
                unlinkat(&parent, rel.file_name(), AtFlags::empty()).map_err(SyncError::io)?;
                fsync(&parent).map_err(SyncError::io)?;
                Ok(())
            }
            Some(_) => Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "refusing to delete a directory, link, or hard link".to_owned(),
            }),
        }
    }

    fn display_rel(rel: Option<&RelativePath>) -> String {
        rel.map(|path| path.to_string())
            .unwrap_or_else(|| "/".to_owned())
    }

    fn unique() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }
}

#[cfg(not(unix))]
mod windows {
    use std::fs::{self, OpenOptions};
    use std::io::{Read, Write};
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};

    use super::{DirName, EntryKind, EntryMeta, RootDir};
    use crate::error::SyncError;
    use crate::pathutil::RelativePath;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    pub fn list(root: &RootDir, rel: Option<&RelativePath>) -> Result<Vec<DirName>, SyncError> {
        let path = path_of(root, rel);
        confirm_real_dir(&path)?;
        let entries = fs::read_dir(&path).map_err(|error| SyncError::Inventory {
            detail: format!("could not read {}: {error}", path.display()),
        })?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| SyncError::Inventory {
                detail: format!("could not read {}: {error}", path.display()),
            })?;
            let name = entry.file_name();
            let Some(text) = name.to_str() else {
                return Err(SyncError::Path {
                    detail: format!("non-utf8 name under {}", path.display()),
                });
            };
            names.push(DirName {
                name: text.to_owned(),
            });
        }
        names.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(names)
    }

    pub fn metadata(root: &RootDir, rel: &RelativePath) -> Result<Option<EntryMeta>, SyncError> {
        let path = root.path.join(rel_path(rel));
        let meta = match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(SyncError::io(error)),
            Ok(meta) => meta,
        };
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || meta.file_type().is_symlink()
        {
            return Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "reparse point or symlink".to_owned(),
            });
        }
        if meta.is_dir() {
            return Ok(Some(EntryMeta {
                kind: EntryKind::Dir,
            }));
        }
        if meta.is_file() {
            return Ok(Some(EntryMeta {
                kind: EntryKind::File {
                    bytes: meta.len(),
                    // `number_of_links` is still behind an unstable Rust feature
                    // on Windows. Hard links there are rejected only when the
                    // file is not a normal file. Unix uses `st_nlink`.
                    hardlinked: false,
                },
            }));
        }
        Err(SyncError::Unsupported {
            path: rel.to_string(),
            detail: "special file".to_owned(),
        })
    }

    pub fn read_file(
        root: &RootDir,
        rel: &RelativePath,
        max_bytes: u64,
    ) -> Result<Vec<u8>, SyncError> {
        let meta = metadata(root, rel)?.ok_or_else(|| SyncError::Inventory {
            detail: format!("{rel} disappeared while it was being read"),
        })?;
        let EntryKind::File { bytes, hardlinked } = meta.kind else {
            return Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "not a regular file".to_owned(),
            });
        };
        if hardlinked {
            return Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "hard link; sync would change another name".to_owned(),
            });
        }
        if bytes > max_bytes {
            return Err(SyncError::Limit {
                detail: format!("{rel} is {bytes} bytes; the limit is {max_bytes}"),
            });
        }
        let path = root.path.join(rel_path(rel));
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
            .map_err(|error| SyncError::Inventory {
                detail: format!("could not read {rel}: {error}"),
            })?;
        let opened = file.metadata().map_err(SyncError::io)?;
        if opened.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "reparse point".to_owned(),
            });
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(SyncError::io)?;
        Ok(buf)
    }

    pub fn write_file(root: &RootDir, rel: &RelativePath, bytes: &[u8]) -> Result<(), SyncError> {
        if let Some(parent) = rel.parent() {
            ensure_dir(root, &parent)?;
        }
        if metadata(root, rel)?.is_some_and(|meta| !matches!(meta.kind, EntryKind::File { .. })) {
            return Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "refusing to replace a non-file".to_owned(),
            });
        }
        let dest = root.path.join(rel_path(rel));
        let temp = dest.with_file_name(format!(
            ".muninn-sync-{}-{}.tmp",
            std::process::id(),
            unique()
        ));
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|error| SyncError::Io {
                    detail: format!("could not create a temp file for {rel}: {error}"),
                })?;
            file.write_all(bytes).map_err(SyncError::io)?;
            file.sync_all().map_err(SyncError::io)?;
        }
        // Windows `rename` does not replace. Move the previous file aside first;
        // the journal's checkpoint is the recovery copy if we crash in between.
        let backup = dest.with_file_name(format!(
            ".muninn-sync-backup-{}-{}.tmp",
            std::process::id(),
            unique()
        ));
        let had_dest = dest.exists();
        if had_dest {
            fs::rename(&dest, &backup).map_err(SyncError::io)?;
        }
        if let Err(error) = fs::rename(&temp, &dest) {
            if had_dest {
                let _ = fs::rename(&backup, &dest);
            }
            let _ = fs::remove_file(&temp);
            return Err(SyncError::io(error));
        }
        if had_dest {
            let _ = fs::remove_file(&backup);
        }
        Ok(())
    }

    pub fn remove_file(root: &RootDir, rel: &RelativePath) -> Result<(), SyncError> {
        match metadata(root, rel)? {
            None => Ok(()),
            Some(meta)
                if matches!(
                    meta.kind,
                    EntryKind::File {
                        hardlinked: false,
                        ..
                    }
                ) =>
            {
                let path = root.path.join(rel_path(rel));
                fs::remove_file(path).map_err(SyncError::io)
            }
            Some(_) => Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "refusing to delete a directory, link, or hard link".to_owned(),
            }),
        }
    }

    fn ensure_dir(root: &RootDir, rel: &RelativePath) -> Result<(), SyncError> {
        let mut built = root.path.clone();
        for part in rel.components() {
            built.push(part);
            if built.exists() {
                confirm_real_dir(&built)?;
            } else {
                fs::create_dir(&built).map_err(SyncError::io)?;
            }
        }
        Ok(())
    }

    fn confirm_real_dir(path: &Path) -> Result<(), SyncError> {
        let meta = fs::symlink_metadata(path).map_err(SyncError::io)?;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !meta.is_dir() {
            return Err(SyncError::Unsupported {
                path: path.display().to_string(),
                detail: "not a real directory".to_owned(),
            });
        }
        Ok(())
    }

    fn path_of(root: &RootDir, rel: Option<&RelativePath>) -> PathBuf {
        match rel {
            Some(rel) => root.path.join(rel_path(rel)),
            None => root.path.clone(),
        }
    }

    fn rel_path(rel: &RelativePath) -> PathBuf {
        let mut out = PathBuf::new();
        for part in rel.components() {
            out.push(part);
        }
        out
    }

    fn unique() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }
}

/// Copy bytes to `dest` using the same temp-file + rename rule, creating parents.
pub fn write_outside(dest: &Path, bytes: &[u8]) -> Result<(), SyncError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(SyncError::io)?;
    }
    let temp = dest.with_extension(format!("tmp-{}", std::process::id()));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)
            .map_err(SyncError::io)?;
        file.write_all(bytes).map_err(SyncError::io)?;
        file.sync_all().map_err(SyncError::io)?;
    }
    fs::rename(&temp, dest).map_err(|error| {
        let _ = fs::remove_file(&temp);
        SyncError::io(error)
    })
}

pub fn read_outside(path: &Path) -> Result<Vec<u8>, SyncError> {
    let mut file = File::open(path).map_err(SyncError::io)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).map_err(SyncError::io)?;
    Ok(buf)
}

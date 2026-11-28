//! Small native filesystem helpers for the state home.
//!
//! Vault bytes go through [`muninn_core::VaultStore`] (`FsStore` natively), not
//! this module: the sync engine's inventory, reads and apply never call
//! `std::fs` directly. What remains here is the host-state writer and the one
//! root check the native wrappers keep from the pre-seam code.
//!
//! [`write_outside`] keeps the temp-file + fsync + rename rule earlier builds
//! used for a state file, so on-disk state bytes are unchanged.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use crate::error::SyncError;

/// Refuse a vault root that is not an ordinary directory.
///
/// The native wrappers call this after canonicalising the root, reproducing the
/// pre-seam `open_root` check (a link or a non-folder is not a vault) without
/// reopening the directory. The store itself resolves every vault path.
pub fn require_real_dir(root: &Path) -> Result<(), SyncError> {
    let metadata = fs::symlink_metadata(root).map_err(SyncError::io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SyncError::Unsupported {
            path: root.display().to_string(),
            detail: "vault root is not a real directory".to_owned(),
        });
    }
    Ok(())
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

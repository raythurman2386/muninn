//! One advisory exclusive lease per canonical vault.
//!
//! The lock is held by an open file in the state directory, not by a sentinel
//! file inside the vault. The operating system drops it when the process dies.
//! A second Muninn process must not steal it after a timeout.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::error::SyncError;
use crate::model::VaultIdentity;

pub struct VaultLease {
    _file: File,
}

impl VaultLease {
    pub fn try_acquire(state_dir: &Path, identity: &VaultIdentity) -> Result<Self, SyncError> {
        let path = lock_path(state_dir, identity);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(SyncError::io)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(SyncError::io)?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(SyncError::LeaseBusy),
            Err(std::fs::TryLockError::Error(error)) => Err(SyncError::Io {
                detail: format!("could not lock {}: {error}", path.display()),
            }),
        }
    }
}

impl Drop for VaultLease {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

pub fn lock_path(state_dir: &Path, identity: &VaultIdentity) -> PathBuf {
    state_dir
        .join("leases")
        .join(format!("{}.lock", identity.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "muninn-lease-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn lease_child_probe() {
        let Ok(dir) = std::env::var("MUNINN_SYNC_LEASE_DIR") else {
            return;
        };
        let Ok(id) = std::env::var("MUNINN_SYNC_LEASE_ID") else {
            return;
        };
        let identity = VaultIdentity::from_known_hex(&id);
        match VaultLease::try_acquire(Path::new(&dir), &identity) {
            Err(SyncError::LeaseBusy) => std::process::exit(11),
            Ok(_lease) => std::process::exit(0),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
    }

    #[test]
    fn a_second_process_cannot_take_the_lease_and_drop_releases_it() {
        let dir = scratch("hold");
        let vault = scratch("vault");
        fs::write(vault.join("note.md"), "x").unwrap();
        let identity = VaultIdentity::of_canonical(&vault).unwrap();
        let lease = VaultLease::try_acquire(&dir, &identity).unwrap();
        let busy = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("lease::tests::lease_child_probe")
            .env("MUNINN_SYNC_LEASE_DIR", &dir)
            .env("MUNINN_SYNC_LEASE_ID", identity.as_str())
            .output()
            .unwrap();
        assert_eq!(
            busy.status.code(),
            Some(11),
            "stderr {}",
            String::from_utf8_lossy(&busy.stderr)
        );
        drop(lease);
        let free = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("lease::tests::lease_child_probe")
            .env("MUNINN_SYNC_LEASE_DIR", &dir)
            .env("MUNINN_SYNC_LEASE_ID", identity.as_str())
            .output()
            .unwrap();
        assert_eq!(
            free.status.code(),
            Some(0),
            "stderr {}",
            String::from_utf8_lossy(&free.stderr)
        );
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&vault);
    }
}

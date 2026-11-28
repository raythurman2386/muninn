//! One advisory exclusive lease per canonical vault.
//!
//! The lease is a seam, not a filesystem detail: a run must hold the one lease
//! for its vault while it reads or writes the vault, and the *host* decides
//! what that lease is. [`SyncLease`] is the contract; the engine only ever sees
//! the trait.
//!
//! - [`FsLease`] is the desktop implementation and keeps the existing
//!   behaviour: an open file in the state directory, locked by the operating
//!   system and dropped when the process dies. It is not a sentinel file inside
//!   the vault, and a second Muninn process must not steal it after a timeout.
//! - The browser implementation wraps its named Web Lock (`RunLock`, slice 8a)
//!   and lives in `muninn-web`, where the JavaScript binding can reach it.
//!
//! Dropping the returned guard releases the lease.

#[cfg(feature = "native")]
use std::fs::{self, File, OpenOptions};
#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

use crate::error::SyncError;
use crate::model::VaultIdentity;

/// A held run lease. Dropping it ends this guard's hold; the host decides what
/// that releases (the desktop guard releases the lock file, while a guard over a
/// whole-run browser lock only drops its borrow).
pub trait LeaseGuard {}

/// Take the one exclusive lease for a vault, as this host implements it.
///
/// The engine calls [`acquire`](SyncLease::acquire) with the identity it read
/// from the vault store and keeps the guard alive while the vault is read or
/// written. It takes the lease twice in a run — once around the inventory read,
/// once around the apply — and drops the guard in between, as the desktop
/// implementation always has. A host whose lock is held for the whole run (a
/// browser's Web Lock must be taken before the blocking engine call, because
/// the API is asynchronous) returns a guard that borrows that outer hold and
/// does not release it.
///
/// The trait is where the host's serialisation lives. A caller that supplies a
/// lease must not pass a lock-free implementation for a real run.
pub trait SyncLease {
    /// Acquire the lease for `identity`, or report that another run holds it.
    ///
    /// # Errors
    ///
    /// [`SyncError::LeaseBusy`] when the vault is already leased, and
    /// [`SyncError::Io`] when the host cannot create or open its lock.
    fn acquire(&self, identity: &VaultIdentity) -> Result<Box<dyn LeaseGuard>, SyncError>;
}

/// The desktop lease: an open, exclusively locked file in the state directory.
#[cfg(feature = "native")]
pub struct FsLease {
    state_dir: PathBuf,
}

#[cfg(feature = "native")]
impl FsLease {
    /// A lease that locks below `state_dir`, the same directory the native
    /// state home uses.
    #[must_use]
    pub fn new(state_dir: PathBuf) -> Self {
        Self { state_dir }
    }
}

#[cfg(feature = "native")]
struct HeldFsLease {
    _lease: VaultLease,
}

#[cfg(feature = "native")]
impl LeaseGuard for HeldFsLease {}

#[cfg(feature = "native")]
impl SyncLease for FsLease {
    fn acquire(&self, identity: &VaultIdentity) -> Result<Box<dyn LeaseGuard>, SyncError> {
        Ok(Box::new(HeldFsLease {
            _lease: VaultLease::try_acquire(&self.state_dir, identity)?,
        }))
    }
}

#[cfg(feature = "native")]
pub struct VaultLease {
    _file: File,
}

#[cfg(feature = "native")]
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

#[cfg(feature = "native")]
impl Drop for VaultLease {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

#[cfg(feature = "native")]
pub fn lock_path(state_dir: &Path, identity: &VaultIdentity) -> PathBuf {
    state_dir
        .join("leases")
        .join(format!("{}.lock", identity.as_str()))
}

#[cfg(all(test, feature = "native"))]
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
        match FsLease::new(PathBuf::from(&dir)).acquire(&identity) {
            Err(SyncError::LeaseBusy) => std::process::exit(11),
            Ok(_guard) => std::process::exit(0),
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
        let lease = FsLease::new(dir.clone())
            .acquire(&identity)
            .expect("the first holder");
        assert!(
            lock_path(&dir, &identity).exists(),
            "the trait keeps the same lock file"
        );
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

//! The worker's OPFS half: mount `muninn-vault/` into the mirror and flush it
//! back with sync access handles (#240 slice 8a).
//!
//! [`VaultDisk::open`] walks the vault before the engine starts (the page has
//! quiesced its own flush, so this snapshot is stable), and
//! [`VaultDisk::flush`] is the run's only durable step. Both use the shared
//! primitives in `muninn_web::sync::opfs_util`; the vault's own semantics live
//! in [`crate::vault`].
//!
//! The walk skips the first-run marker `.muninn-vault`, the same way the page
//! boot keeps it out of its mirror. It is page bookkeeping, not content; if it
//! entered the mirror the sync inventory would list it as a hidden exclusion
//! in every report.

use web_sys::FileSystemDirectoryHandle;

use muninn_web::sync::opfs_util;

use crate::request::VAULT_LABEL;
use crate::vault::{VaultOp, WorkerVaultStore};

/// The page's first-run marker inside `muninn-vault/`.
const MARKER_REL: &str = ".muninn-vault";

/// The OPFS directory the mirrored vault lives in.
pub struct VaultDisk {
    root: FileSystemDirectoryHandle,
}

impl VaultDisk {
    /// Open `muninn-vault/` and load it into `store`.
    ///
    /// The directory must already exist: the page's boot creates and seeds it,
    /// and a sync before that has nothing to sync. A read failure is returned
    /// rather than presented as an empty vault, because an empty mirror would
    /// let the engine plan every remote file as a new addition over a vault it
    /// could not read.
    ///
    /// # Errors
    ///
    /// A short diagnostic when OPFS is unavailable, the vault directory is
    /// missing, or the walk fails.
    pub async fn open(store: &WorkerVaultStore) -> Result<VaultDisk, String> {
        let origin = opfs_util::origin_directory().await?;
        let root = opfs_util::dir_handle(&origin, VAULT_LABEL, false)
            .await
            .map_err(|error| {
                format!(
                    "{VAULT_LABEL}/ is not in origin-private storage: {}",
                    opfs_util::js_message(error)
                )
            })?;
        let (files, dirs) = opfs_util::walk(&root).await?;
        for dir in dirs {
            store.load_dir(&dir);
        }
        for file in files {
            if file.rel != MARKER_REL {
                store.load(&file.rel, &file.bytes);
            }
        }
        Ok(VaultDisk { root })
    }

    /// Apply the mirror's pending changes to OPFS.
    ///
    /// Taking the ops clears the dirty set; on a failure the ops are put back
    /// so the caller can report a durable failure while leaving a retryable
    /// record. The worker is run-scoped and does not retry, but the next run
    /// re-reads OPFS and replans from whatever reached it, so the requeue is
    /// also what keeps this API honest for a future resumable shape.
    ///
    /// # Errors
    ///
    /// A short diagnostic when an OPFS operation fails.
    pub async fn flush(&self, store: &WorkerVaultStore) -> Result<(), String> {
        let ops = store.take_flush_ops();
        if ops.is_empty() {
            return Ok(());
        }
        if let Err(error) = self.apply(&ops).await {
            store.requeue(&ops);
            return Err(error);
        }
        Ok(())
    }

    async fn apply(&self, ops: &[VaultOp]) -> Result<(), String> {
        for op in ops {
            match op {
                VaultOp::Remove(rel) => opfs_util::remove_entry(&self.root, rel)
                    .await
                    .map_err(opfs_util::js_message)?,
                VaultOp::EnsureDir(rel) => {
                    opfs_util::dir_handle(&self.root, rel, true)
                        .await
                        .map_err(opfs_util::js_message)?;
                }
                VaultOp::Write { rel, bytes } => opfs_util::write_file(&self.root, rel, bytes)
                    .await
                    .map_err(opfs_util::js_message)?,
            }
        }
        Ok(())
    }
}

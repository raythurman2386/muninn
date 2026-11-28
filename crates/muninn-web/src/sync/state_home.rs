//! The sync engine's state home in the worker: a memory mirror over a
//! dedicated OPFS directory (#240 slice 8a).
//!
//! The engine's state IO is synchronous (`muninn_core::StateHome`), while OPFS
//! is a Promise API. A dedicated worker is the only place with synchronous
//! OPFS access handles, but even there *acquiring* a handle is a Promise:
//! `FileSystemFileHandle.createSyncAccessHandle()` returns one. The engine
//! writes keys the worker cannot know in advance (`staged/<path>`,
//! `checkpoints/<path>`), so a handle cannot be opened lazily inside a
//! synchronous `put`. Verified in current Chrome: inside a dedicated worker,
//! `getFileHandle` → `createSyncAccessHandle` → `write` → `flush` → `getSize`
//! → `close` all work; the round trip is the basis of the private `write_key`
//! helper below.
//!
//! The shape is therefore the same one the page's state home already uses
//! (`crate::OpfsStateHome`): the mirror answers the engine synchronously and a
//! flush drains the dirty keys. The flush uses a sync access handle — truncate,
//! write, flush, close — because that is the durable primitive a worker has;
//! `createSyncAccessHandle()` has no synchronous equivalent. Keys are never
//! re-spelled: the engine and this home both use `muninn-sync`'s state-key
//! functions, and the flush uses each key as the file's path below
//! `muninn-sync-state/`.
//!
//! # Durability, honestly
//!
//! What this guarantees:
//!
//! - a run's state is complete and correct once [`SyncStateHome::flush`]
//!   returns;
//! - the sync access handle flushes to the origin's backing store before it is
//!   closed, so a completed flush survives a worker restart and a reload;
//! - a failed flush puts its keys back in the dirty set, so the caller can
//!   retry without losing the mirror.
//!
//! What it does not guarantee:
//!
//! - a key written by the engine is not durable until the flush runs. If the
//!   worker is terminated mid-run, the state written by *that run* is lost
//!   (the previous flush's files remain). Native's journal-before-apply
//!   barrier is therefore weaker here; a later slice can narrow the window by
//!   pre-opening handles for the four fixed keys (`base.json`, `journal.json`,
//!   `prepared.json`, `github-blob-cache.json`), whose names are known when the
//!   home opens.
//! - this is not a filesystem transaction and does not make the vault apply
//!   atomic. The vault's own store owns that problem.
//!
//! Nothing else may touch this directory. It is host state (base, journal,
//! prepared list, sha cache, staged bytes, checkpoints), not a vault folder,
//! and it sits beside `muninn-vault/` and `muninn-state/` rather than inside
//! either.

use std::sync::Arc;

use muninn_core::StateHome;
use web_sys::FileSystemDirectoryHandle;

use crate::OpfsStateHome;
use crate::sync::SYNC_STATE_DIR;
use crate::sync::opfs_util;

/// The worker's state home: the synchronous mirror plus the OPFS directory it
/// flushes to.
pub struct SyncStateHome {
    home: Arc<OpfsStateHome>,
    root: FileSystemDirectoryHandle,
}

impl SyncStateHome {
    /// Open `muninn-sync-state/` and load every key into the mirror.
    ///
    /// The directory is created when absent: a worker is only started to run a
    /// sync, and an empty state directory is a valid first run. A read failure
    /// is returned rather than presented as an empty home, because an empty
    /// base would let the engine plan a first bind over a state it could not
    /// read.
    ///
    /// # Errors
    ///
    /// A short diagnostic when OPFS is not available or the walk fails.
    pub async fn open() -> Result<SyncStateHome, String> {
        let origin = opfs_util::origin_directory().await?;
        let root = opfs_util::dir_handle(&origin, SYNC_STATE_DIR, true)
            .await
            .map_err(opfs_util::js_message)?;

        let home = OpfsStateHome::new(true);
        let (entries, _dirs) = opfs_util::walk(&root).await?;
        for entry in entries {
            home.load(&entry.rel, &entry.bytes);
        }
        Ok(SyncStateHome {
            home: Arc::new(home),
            root,
        })
    }

    /// The synchronous home the engine reads and writes.
    #[must_use]
    pub fn home(&self) -> &Arc<OpfsStateHome> {
        &self.home
    }

    /// Write this run's dirty keys into OPFS with sync access handles.
    ///
    /// Call after `run_sync_with_state_home` returns (success or failure) and
    /// again after a resumed apply. Taking the plan clears the dirty set; on a
    /// failed flush the plan is put back so a retry repeats it.
    ///
    /// # Errors
    ///
    /// A short diagnostic when an OPFS operation fails.
    pub async fn flush(&self) -> Result<(), String> {
        let plan = self.home.take_flush_plan();
        if plan.is_empty() {
            return Ok(());
        }
        if let Err(error) = self.apply_plan(&plan).await {
            self.home.requeue(&plan);
            return Err(error);
        }
        Ok(())
    }

    /// True when a flush has work. The worker checks this before scheduling a
    /// drain, exactly as the page's poll loop does.
    #[must_use]
    pub fn has_dirty(&self) -> bool {
        self.home.has_dirty()
    }

    /// How many distinct keys are waiting for the flush.
    #[must_use]
    pub fn dirty_count(&self) -> usize {
        self.home.dirty_count()
    }

    async fn apply_plan(&self, plan: &crate::StateFlushPlan) -> Result<(), String> {
        for key in &plan.removals {
            opfs_util::remove_entry(&self.root, key)
                .await
                .map_err(opfs_util::js_message)?;
        }
        for key in &plan.writes {
            let bytes = self
                .home
                .read_bytes(key)
                .ok_or_else(|| format!("the sync state mirror lost {key} before the flush"))?;
            opfs_util::write_file(&self.root, key, &bytes)
                .await
                .map_err(opfs_util::js_message)?;
        }
        Ok(())
    }
}

/// The engine sees this as its [`StateHome`]: the synchronous mirror, with the
/// browser's clock for records that carry one. The OPFS half is
/// [`SyncStateHome::flush`], which is async and therefore outside the trait.
impl StateHome for SyncStateHome {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.home.get(key)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> std::io::Result<()> {
        self.home.put(key, bytes)
    }

    fn remove(&self, key: &str) -> std::io::Result<()> {
        self.home.remove(key)
    }

    fn list(&self, prefix: &str) -> Vec<String> {
        self.home.list(prefix)
    }

    /// Unix seconds from `Date.now()`. The trait's default reads
    /// `SystemTime`, which panics on `wasm32-unknown-unknown`.
    fn now_secs(&self) -> u64 {
        (js_sys::Date::now() / 1000.0) as u64
    }
}

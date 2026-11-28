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

use js_sys::Array;
use muninn_core::StateHome;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemHandle, FileSystemHandleKind, FileSystemReadWriteOptions,
    FileSystemRemoveOptions, FileSystemSyncAccessHandle, StorageManager,
};

use crate::OpfsStateHome;
use crate::sync::SYNC_STATE_DIR;

/// The worker's state home: the synchronous mirror plus the OPFS directory it
/// flushes to.
pub struct SyncStateHome {
    home: Arc<OpfsStateHome>,
    root: FileSystemDirectoryHandle,
}

impl SyncStateHome {
    /// Open `muninn-sync-state/` and load every key into the mirror.    ///
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
        let storage = storage().await?;
        let origin: FileSystemDirectoryHandle = JsFuture::from(storage.get_directory())
            .await
            .map_err(js_message)?
            .dyn_into()
            .map_err(|_| "origin-private storage did not return a directory".to_owned())?;
        let options = FileSystemGetDirectoryOptions::new();
        options.set_create(true);
        let root: FileSystemDirectoryHandle =
            JsFuture::from(origin.get_directory_handle_with_options(SYNC_STATE_DIR, &options))
                .await
                .map_err(js_message)?
                .dyn_into()
                .map_err(|_| format!("OPFS did not return {SYNC_STATE_DIR}"))?;

        let home = OpfsStateHome::new(true);
        let mut entries = Vec::new();
        walk(&root, "", &mut entries).await.map_err(js_message)?;
        for entry in entries {
            home.load(&entry.key, &entry.bytes);
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
            remove_entry(&self.root, key).await.map_err(js_message)?;
        }
        for key in &plan.writes {
            let bytes = self
                .home
                .read_bytes(key)
                .ok_or_else(|| format!("the sync state mirror lost {key} before the flush"))?;
            write_key(&self.root, key, &bytes)
                .await
                .map_err(js_message)?;
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

/// `navigator.storage` in a worker. `WorkerNavigator` is the worker's
/// navigator; Reflect keeps this working in both a worker and a window so the
/// module can be exercised by the browser gate from either side.
async fn storage() -> Result<StorageManager, String> {
    let navigator = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("navigator"))
        .map_err(js_message)?;
    let storage =
        js_sys::Reflect::get(&navigator, &JsValue::from_str("storage")).map_err(js_message)?;
    storage.dyn_into::<StorageManager>().map_err(|_| {
        "this browser has no origin-private storage, so sync state cannot be kept".to_owned()
    })
}

struct Loaded {
    key: String,
    bytes: Vec<u8>,
}

async fn walk(
    dir: &FileSystemDirectoryHandle,
    prefix: &str,
    out: &mut Vec<Loaded>,
) -> Result<(), JsValue> {
    let entries = dir.entries();
    loop {
        let next = match JsFuture::from(entries.next()?).await {
            Ok(next) => next,
            // Another tab removed the directory between entries: what is left
            // is still a consistent snapshot.
            Err(error) if is_not_found(&error) => break,
            Err(error) => return Err(error),
        };
        if get(&next, "done")?.as_bool().unwrap_or(true) {
            break;
        }
        let value = get(&next, "value")?;
        let pair = Array::from(&value);
        let Some(name) = pair.get(0).as_string() else {
            continue;
        };
        let handle: FileSystemHandle = pair.get(1).dyn_into()?;
        let key = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if handle.kind() == FileSystemHandleKind::Directory {
            let child: FileSystemDirectoryHandle = handle.unchecked_into();
            match Box::pin(walk(&child, &key, out)).await {
                Ok(()) => {}
                Err(error) if is_not_found(&error) => {}
                Err(error) => return Err(error),
            }
        } else {
            let file: FileSystemFileHandle = handle.unchecked_into();
            match read_file(&file).await {
                Ok(bytes) => out.push(Loaded { key, bytes }),
                // A file another writer removed mid-walk is skipped; the
                // engine treats a missing state file as "none".
                Err(error) if is_not_found(&error) => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}

async fn read_file(handle: &FileSystemFileHandle) -> Result<Vec<u8>, JsValue> {
    let blob: web_sys::Blob = JsFuture::from(handle.get_file()).await?.unchecked_into();
    let buffer = JsFuture::from(blob.array_buffer()).await?;
    let view = js_sys::Uint8Array::new(&buffer);
    let mut bytes = vec![0; view.length() as usize];
    view.copy_to(&mut bytes);
    Ok(bytes)
}

/// Replace one key's file with `bytes` through a sync access handle.
///
/// `truncate(0)` before the write makes a shorter value replace a longer one;
/// `truncate(len)` after it covers a write that stopped short (which is an
/// error, but truncating keeps the file consistent).
async fn write_key(
    root: &FileSystemDirectoryHandle,
    key: &str,
    bytes: &[u8],
) -> Result<(), JsValue> {
    let (parent, name) = key.rsplit_once('/').unwrap_or(("", key));
    let dir = dir_handle(root, parent, true).await?;
    let options = FileSystemGetFileOptions::new();
    options.set_create(true);
    let file: FileSystemFileHandle =
        JsFuture::from(dir.get_file_handle_with_options(name, &options))
            .await?
            .dyn_into()?;
    let handle: FileSystemSyncAccessHandle = JsFuture::from(file.create_sync_access_handle())
        .await?
        .dyn_into()?;
    let result = write_through(&handle, bytes);
    // Close even when a step failed: an open handle pins the file and its
    // flush; the caller puts the key back in the dirty set.
    handle.close();
    result
}

fn write_through(handle: &FileSystemSyncAccessHandle, bytes: &[u8]) -> Result<(), JsValue> {
    handle.truncate_with_f64(0.0)?;
    if !bytes.is_empty() {
        let options = FileSystemReadWriteOptions::new();
        options.set_at(0.0);
        let written = handle.write_with_u8_array_and_options(bytes, &options)?;
        if written as usize != bytes.len() {
            return Err(JsValue::from_str(&format!(
                "a sync access handle wrote {written} of {} bytes",
                bytes.len()
            )));
        }
        handle.truncate_with_f64(bytes.len() as f64)?;
    }
    handle.flush()
}

async fn dir_handle(
    root: &FileSystemDirectoryHandle,
    rel: &str,
    create: bool,
) -> Result<FileSystemDirectoryHandle, JsValue> {
    let mut current = root.clone();
    for part in rel.split('/').filter(|part| !part.is_empty()) {
        let options = FileSystemGetDirectoryOptions::new();
        options.set_create(create);
        current = JsFuture::from(current.get_directory_handle_with_options(part, &options))
            .await?
            .dyn_into()?;
    }
    Ok(current)
}

async fn remove_entry(root: &FileSystemDirectoryHandle, key: &str) -> Result<(), JsValue> {
    let (parent, name) = key.rsplit_once('/').unwrap_or(("", key));
    let dir = match dir_handle(root, parent, false).await {
        Ok(dir) => dir,
        // The parent is already gone: so is the entry.
        Err(_) => return Ok(()),
    };
    let options = FileSystemRemoveOptions::new();
    options.set_recursive(true);
    match JsFuture::from(dir.remove_entry_with_options(name, &options)).await {
        Ok(_) => Ok(()),
        Err(error) if is_not_found(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

fn get(object: &JsValue, key: &str) -> Result<JsValue, JsValue> {
    js_sys::Reflect::get(object, &JsValue::from_str(key))
}

fn is_not_found(error: &JsValue) -> bool {
    get(error, "name")
        .ok()
        .and_then(|value| value.as_string())
        .as_deref()
        == Some("NotFoundError")
}

fn js_message(error: JsValue) -> String {
    if let Some(message) = error.as_string() {
        return message;
    }
    get(&error, "message")
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_else(|| format!("{error:?}"))
}

//! The OPFS primitives the worker's stores share (#240 slice 8a).
//!
//! Both the sync state home and the worker's vault store need the same four
//! operations, and only a dedicated worker has them synchronously:
//!
//! - the origin-private root (`navigator.storage.getDirectory()`), which is
//!   the parent of `muninn-vault/`, `muninn-state/` and
//!   `muninn-sync-state/`;
//! - a directory walk that loads files (and, for the vault, empty folders)
//!   into an in-memory mirror;
//! - a whole-file replace through a **sync access handle** — truncate, write,
//!   flush, close — which is the only durable primitive available while the
//!   engine blocks;
//! - a recursive removal and a create-directories hop for the flush.
//!
//! Acquiring a handle is still a Promise (`createSyncAccessHandle()` has no
//! synchronous form), so everything here is async; the mirror answers the
//! engine synchronously and the flush awaits between ops, never inside one of
//! the engine's calls.

use js_sys::Array;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemHandle, FileSystemHandleKind, FileSystemReadWriteOptions,
    FileSystemRemoveOptions, FileSystemSyncAccessHandle, StorageManager,
};

/// One file loaded from an OPFS walk, spelling its path relative to the walk's
/// root with `/` separators.
pub struct Loaded {
    pub rel: String,
    pub bytes: Vec<u8>,
}

/// The origin-private file system root.
///
/// # Errors
///
/// A short diagnostic when the browser has no OPFS.
pub async fn origin_directory() -> Result<FileSystemDirectoryHandle, String> {
    let navigator = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("navigator"))
        .map_err(js_message)?;
    let storage =
        js_sys::Reflect::get(&navigator, &JsValue::from_str("storage")).map_err(js_message)?;
    let storage: StorageManager = storage.dyn_into().map_err(|_| {
        "this browser has no origin-private storage, so the vault cannot be reached".to_owned()
    })?;
    let origin: FileSystemDirectoryHandle = JsFuture::from(storage.get_directory())
        .await
        .map_err(js_message)?
        .dyn_into()
        .map_err(|_| "origin-private storage did not return a directory".to_owned())?;
    Ok(origin)
}

/// Walk `root` into its files and folders. `dirs` carries every folder's
/// relative spelling (never the root itself); `files` carries the bytes.
///
/// # Errors
///
/// A short diagnostic when the walk cannot continue. A folder or file another
/// writer removed mid-walk is skipped: what is left is still a snapshot.
pub async fn walk(root: &FileSystemDirectoryHandle) -> Result<(Vec<Loaded>, Vec<String>), String> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    walk_dir(root, "", &mut files, &mut dirs)
        .await
        .map_err(js_message)?;
    Ok((files, dirs))
}

async fn walk_dir(
    dir: &FileSystemDirectoryHandle,
    prefix: &str,
    files: &mut Vec<Loaded>,
    dirs: &mut Vec<String>,
) -> Result<(), JsValue> {
    let entries = dir.entries();
    loop {
        let next = match JsFuture::from(entries.next()?).await {
            Ok(next) => next,
            // Another writer removed the directory between entries.
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
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if handle.kind() == FileSystemHandleKind::Directory {
            let child: FileSystemDirectoryHandle = handle.unchecked_into();
            dirs.push(rel.clone());
            match Box::pin(walk_dir(&child, &rel, files, dirs)).await {
                Ok(()) => {}
                Err(error) if is_not_found(&error) => {}
                Err(error) => return Err(error),
            }
        } else {
            let file: FileSystemFileHandle = handle.unchecked_into();
            match read_file(&file).await {
                Ok(bytes) => files.push(Loaded { rel, bytes }),
                // A file another writer removed mid-walk is skipped.
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

/// Replace one file with `bytes` through a sync access handle.
///
/// `truncate(0)` before the write makes a shorter value replace a longer one;
/// `truncate(len)` after it covers a write that stopped short (which is an
/// error, but truncating keeps the file consistent). `flush()` is what makes
/// the bytes durable when the handle closes.
///
/// # Errors
///
/// The browser's OPFS error when a handle cannot be opened or a write fails.
pub async fn write_file(
    root: &FileSystemDirectoryHandle,
    rel: &str,
    bytes: &[u8],
) -> Result<(), JsValue> {
    let (parent, name) = rel.rsplit_once('/').unwrap_or(("", rel));
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
    // flush; the caller puts the path back in the dirty set.
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

/// Walk to (and with `create`, make) the directory `rel` below `root`.
///
/// # Errors
///
/// The browser's OPFS error; a missing directory with `create` false is one.
pub async fn dir_handle(
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

/// Remove `rel` (file or whole folder) below `root`. A missing entry is not
/// an error: the flush is replaying a mirror, not a filesystem.
///
/// # Errors
///
/// The browser's OPFS error when the removal fails for another reason.
pub async fn remove_entry(root: &FileSystemDirectoryHandle, rel: &str) -> Result<(), JsValue> {
    let (parent, name) = rel.rsplit_once('/').unwrap_or(("", rel));
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

/// Whether the OPFS error is a missing entry.
#[must_use]
pub fn is_not_found(error: &JsValue) -> bool {
    get(error, "name")
        .ok()
        .and_then(|value| value.as_string())
        .as_deref()
        == Some("NotFoundError")
}

/// The best message a JS error value carries, for a diagnostic.
#[must_use]
pub fn js_message(error: JsValue) -> String {
    if let Some(message) = error.as_string() {
        return message;
    }
    get(&error, "message")
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_else(|| format!("{error:?}"))
}

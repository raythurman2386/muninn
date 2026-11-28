//! The asynchronous half of the browser vault: OPFS boot, Web Locks, flush.
//!
//! [`crate::store::OpfsStore`] is synchronous from the vault's point of view;
//! this module does everything that must await a browser Promise:
//!
//! 1. mount the origin-private file system and walk it into the store's mirror
//!    (or seed the embedded sample on the first run and write it immediately);
//! 2. take the single-writer Web Lock, or report the vault read-only;
//! 3. request persistent storage before the first write;
//! 4. drain the store's and the state home's dirty sets back to OPFS from the
//!    app's poll loop.
//!
//! There is no worker and no `SharedArrayBuffer`, so the page needs no
//! COOP/CORP headers. The price is the flush window: OPFS lags the mirror
//! between drains, and a crash in that window loses the edits since the last
//! one. The state home's recovery copies (#240 slice 6d) are written into that
//! same mirror, so they narrow the window rather than widen it.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use js_sys::{Array, Promise, Reflect};
use muninn_i18n::{Key, t};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemHandle, FileSystemHandleKind, FileSystemRemoveOptions,
    FileSystemWritableFileStream, StorageManager, WritableStream,
};

use crate::state_home::{OpfsStateHome, StateFlushPlan};
use crate::store::OpfsStore;
use crate::{SAMPLE_ROOT, SAMPLE_SOURCES};

/// The browser vault is open read-only because another tab holds the lock.
///
/// The English text lives in the catalog (`web.vault_read_only`), like every
/// other status line the browser build can draw.
pub fn read_only_message() -> &'static str {
    t(Key::WebVaultReadOnly)
}

/// The browser cannot offer the single-writer guarantee at all.
pub fn no_lock_message() -> &'static str {
    t(Key::WebVaultNoLock)
}

/// The directory inside OPFS. It is one vault: the demo has one root.
const OPFS_VAULT_DIR: &str = "muninn-vault";

/// The state home's directory beside the vault: recovery copies, settings and
/// the lock high-water record (#240 slice 6d). Its keys spell the app's folder
/// inside the XDG-style root (`muninn/settings.json`, ...).
const OPFS_STATE_DIR: &str = "muninn-state";

/// First-run marker. Written after the sample seed reaches OPFS so a vault the
/// user has emptied is loaded empty instead of re-seeded. Hidden from the
/// vault's own walk, which skips dot files.
const MARKER_REL: &str = ".muninn-vault";
const MARKER_BYTES: &[u8] = b"muninn browser vault v1\n";

const FLUSH_INTERVAL_MS: f64 = 250.0;
/// A failed flush is retried less often so one broken OPFS operation does not
/// spin the poll loop.
const FLUSH_RETRY_MS: f64 = 5_000.0;

struct Session {
    store: Arc<OpfsStore>,
    home: Arc<OpfsStateHome>,
    root: FileSystemDirectoryHandle,
    /// The state home's own OPFS directory, beside the vault's.
    state_root: FileSystemDirectoryHandle,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    static FLUSHING: Cell<bool> = const { Cell::new(false) };
    static NEXT_FLUSH: Cell<f64> = const { Cell::new(0.0) };
    static FLUSH_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// What [`open`] found. `store` is `None` only when OPFS itself could not be
/// mounted; the demo then falls back to the embedded sample in memory. `home`
/// is `None` when the state home could not be mounted; the three stores then
/// keep their honest refusal instead of pretending a copy is gone.
pub struct Opened {
    pub store: Option<Arc<OpfsStore>>,
    pub home: Option<Arc<OpfsStateHome>>,
    pub read_only: bool,
    pub notice: Option<String>,
    pub error: Option<String>,
}

/// Mount the vault: read OPFS into the mirror, take the lock, and seed the
/// sample on the first run. Failures are reported in [`Opened`] rather than
/// aborting the app: the demo still runs, read-only, with the sample.
pub async fn open() -> Opened {
    match open_inner().await {
        Ok(opened) => opened,
        Err(error) => Opened {
            store: None,
            home: None,
            read_only: true,
            notice: None,
            error: Some(error),
        },
    }
}

/// Drop the active session, e.g. when the store could not become the vault.
pub fn deactivate() {
    SESSION.with(|slot| {
        if let Some(session) = slot.borrow_mut().take() {
            // An in-flight flush holds its own clone; stop it from writing to a
            // vault this tab does not serve.
            session.store.set_writable(false);
            session.home.set_writable(false);
        }
    });
    // The state home is registered with the shared UI, not only parked here:
    // drop it so a recovery or settings write fails honestly instead of
    // filling a mirror nobody drains.
    muninn_ui::state_home::clear();
}

/// Called once per poll frame by the app wrapper: start a flush when either
/// journal is dirty and the last one is old enough.
pub fn schedule_flush() {
    let Some((store, home, root, state_root)) = session() else {
        return;
    };
    let vault_dirty = store.is_writable() && store.has_dirty();
    let state_dirty = home.is_writable() && home.has_dirty();
    if (!vault_dirty && !state_dirty) || FLUSHING.with(Cell::get) {
        return;
    }
    let now = js_sys::Date::now();
    if now < NEXT_FLUSH.with(Cell::get) {
        return;
    }
    FLUSHING.with(|flag| flag.set(true));
    spawn_local(async move {
        // Drain both journals independently: a failing vault write must not
        // starve the recovery copies, which exist for exactly that window.
        let mut failure = flush_store(&store, &root).await.err();
        if let Err(error) = flush_home(&home, &state_root).await {
            failure.get_or_insert(error);
        }
        match failure {
            None => {
                NEXT_FLUSH.with(|next| next.set(js_sys::Date::now() + FLUSH_INTERVAL_MS));
                FLUSH_ERROR.with(|slot| *slot.borrow_mut() = None);
            }
            Some(error) => {
                // The failed half put its plan back; try again later and keep
                // the message for the status bar.
                NEXT_FLUSH.with(|next| next.set(js_sys::Date::now() + FLUSH_RETRY_MS));
                web_sys::console::error_1(&format!("muninn: flush failed: {error}").into());
                FLUSH_ERROR.with(|slot| *slot.borrow_mut() = Some(error));
            }
        }
        FLUSHING.with(|flag| flag.set(false));
    });
}

/// The last flush failure, for the app wrapper's status bar.
pub fn take_flush_error() -> Option<String> {
    FLUSH_ERROR.with(|slot| slot.borrow_mut().take())
}

/// The origin-private root, the parent of the vault's own folder.
async fn storage_parent(storage: &StorageManager) -> Result<FileSystemDirectoryHandle, String> {
    let directory = JsFuture::from(storage.get_directory())
        .await
        .map_err(js_message)?;
    directory
        .dyn_into()
        .map_err(|_| "origin-private storage did not return a directory".to_owned())
}

async fn open_inner() -> Result<Opened, String> {
    let window = web_sys::window().ok_or("this page has no window")?;
    let navigator = window.navigator();
    let storage =
        Reflect::get(navigator.as_ref(), &JsValue::from_str("storage")).map_err(js_message)?;
    let storage: StorageManager = storage.dyn_into().map_err(|_| {
        "this browser has no origin-private storage, so the vault cannot be opened".to_owned()
    })?;

    // Take the single-writer lock before touching the vault directory: a tab
    // that cannot write must not even create the folder.
    let lock = acquire_write_lock(&format!("muninn-vault-{SAMPLE_ROOT}")).await;
    let writable = lock.granted;

    // The state home mounts beside the vault and follows the same lock: a
    // read-only tab reads it, never creates or writes it. A state home that
    // cannot mount leaves the three stores on their honest refusal; it does
    // not stop the vault from opening.
    let (home, state_root, home_error) = match open_state_home(&storage, writable).await {
        Ok((home, root)) => (Some(Arc::new(home)), root, None),
        Err(error) => {
            web_sys::console::error_1(
                &format!("muninn: the state home could not be opened: {error}").into(),
            );
            (None, None, Some(error))
        }
    };

    let mut notice = None;
    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(writable);
    let vault_dir = JsFuture::from(
        storage_parent(&storage)
            .await?
            .get_directory_handle_with_options(OPFS_VAULT_DIR, &options),
    )
    .await;
    let root: FileSystemDirectoryHandle = match vault_dir {
        Ok(directory) => directory
            .dyn_into()
            .map_err(|_| "OPFS did not return the vault directory".to_owned())?,
        // A read-only tab on a first run has no vault to read yet. Show the
        // embedded sample rather than create a folder the writer owns.
        Err(error) if !writable && error_name(&error).as_deref() == Some("NotFoundError") => {
            let store = Arc::new(OpfsStore::new(SAMPLE_ROOT, false));
            for source in SAMPLE_SOURCES {
                store.load(source.rel, source.bytes);
            }
            return Ok(Opened {
                store: Some(store),
                home,
                read_only: true,
                notice: Some(
                    "This read-only tab shows the embedded sample until the writing tab saves it."
                        .to_owned(),
                ),
                error: lock.error.or(home_error),
            });
        }
        Err(error) => return Err(js_message(error)),
    };

    if writable {
        let persisted = request_persist(&storage).await;
        if !persisted {
            notice = Some(t(Key::WebPersistDenied).to_owned());
        }
    }

    let mut entries = Vec::new();
    let mut dirs = Vec::new();
    walk(&root, "", &mut entries, &mut dirs)
        .await
        .map_err(js_message)?;
    let has_marker = entries.iter().any(|entry| entry.rel == MARKER_REL);
    let empty = entries.iter().all(|entry| entry.rel == MARKER_REL);
    let store = Arc::new(OpfsStore::new(SAMPLE_ROOT, writable));
    for rel in &dirs {
        store.load_dir(rel);
    }
    for entry in &entries {
        if entry.rel != MARKER_REL {
            store.load(&entry.rel, &entry.bytes);
        }
    }

    if writable && !has_marker && empty {
        // First run: seed every committed sample byte (notes, kanban
        // frontmatter and the `.excalidraw` board), flush it before the app
        // starts, then drop the marker.
        for source in SAMPLE_SOURCES {
            store.seed(source.rel, source.bytes);
        }
        flush_store(&store, &root)
            .await
            .map_err(|e| format!("the sample vault could not be written to OPFS: {e}"))?;
        match write_file(&root, MARKER_REL, MARKER_BYTES).await {
            Ok(()) => {}
            Err(e) => {
                notice = Some(format!(
                    "the OPFS vault marker could not be written: {}",
                    js_message(e)
                ));
            }
        }
    } else if writable && !has_marker {
        // A vault from an earlier build (or a first run that died before the
        // marker): adopt what is there rather than re-seeding over it.
        if let Err(e) = write_file(&root, MARKER_REL, MARKER_BYTES).await {
            notice = Some(format!(
                "the OPFS vault marker could not be written: {}",
                js_message(e)
            ));
        }
    } else if !writable && !has_marker && empty {
        // Second tab on a first run: show the sample so the board still opens,
        // but never write it; the tab holding the lock will seed OPFS.
        for source in SAMPLE_SOURCES {
            store.load(source.rel, source.bytes);
        }
        notice = Some(t(Key::WebReadOnlySample).to_owned());
    }

    if let Some(lost) = lock.lost {
        let store = Arc::clone(&store);
        let home_lost = home.clone();
        spawn_local(async move {
            // The page gave the lock up (unload is the ordinary case). Stop
            // accepting and flushing writes; the tab is read-only from here.
            let _ = JsFuture::from(lost).await;
            store.set_writable(false);
            if let Some(home) = home_lost {
                home.set_writable(false);
            }
        });
    }

    // Park the session only when both halves are there. A state home that
    // mounted alone writes nothing (the vault is the thing being served); the
    // boot registers the home separately with the shared UI.
    if let (Some(home), Some(state_root)) = (&home, state_root) {
        SESSION.with(|slot| {
            *slot.borrow_mut() = Some(Session {
                store: Arc::clone(&store),
                home: Arc::clone(home),
                root,
                state_root,
            });
        });
    }

    Ok(Opened {
        store: Some(store),
        home,
        read_only: !writable,
        notice,
        error: lock.error.or(home_error),
    })
}

/// Mount the state home's directory and walk it into its mirror. A read-only
/// first run (no directory yet) is an empty home, not an error.
async fn open_state_home(
    storage: &StorageManager,
    writable: bool,
) -> Result<(OpfsStateHome, Option<FileSystemDirectoryHandle>), String> {
    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(writable);
    let state_dir = JsFuture::from(
        storage_parent(storage)
            .await?
            .get_directory_handle_with_options(OPFS_STATE_DIR, &options),
    )
    .await;
    let root: FileSystemDirectoryHandle = match state_dir {
        Ok(directory) => directory
            .dyn_into()
            .map_err(|_| "OPFS did not return the state directory".to_owned())?,
        // A read-only tab on a first run has no state to read yet.
        Err(error) if !writable && error_name(&error).as_deref() == Some("NotFoundError") => {
            return Ok((OpfsStateHome::new(false), None));
        }
        Err(error) => return Err(js_message(error)),
    };
    let home = OpfsStateHome::new(writable);
    let mut entries = Vec::new();
    let mut dirs = Vec::new();
    walk(&root, "", &mut entries, &mut dirs)
        .await
        .map_err(js_message)?;
    for entry in &entries {
        home.load(&entry.rel, &entry.bytes);
    }
    Ok((home, Some(root)))
}

struct Lock {
    granted: bool,
    lost: Option<Promise>,
    error: Option<String>,
}

async fn acquire_write_lock(name: &str) -> Lock {
    let control = lock_control(name);
    let supported = get(&control, "supported")
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !supported {
        return Lock {
            granted: false,
            lost: None,
            error: Some(no_lock_message().to_owned()),
        };
    }
    let acquired = get(&control, "acquired")
        .ok()
        .and_then(|value| value.dyn_into::<Promise>().ok());
    let Some(acquired) = acquired else {
        return Lock {
            granted: false,
            lost: None,
            error: Some("the browser vault lock could not be requested".to_owned()),
        };
    };
    let granted = JsFuture::from(acquired)
        .await
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let lost = if granted {
        get(&control, "lost")
            .ok()
            .and_then(|value| value.dyn_into::<Promise>().ok())
    } else {
        None
    };
    Lock {
        granted,
        lost,
        error: None,
    }
}

async fn request_persist(storage: &StorageManager) -> bool {
    let Ok(value) = Reflect::get(storage.as_ref(), &JsValue::from_str("persist")) else {
        return false;
    };
    let Ok(function) = value.dyn_into::<js_sys::Function>() else {
        return false;
    };
    let Ok(promise) = function.call0(storage.as_ref()) else {
        return false;
    };
    let Ok(promise) = promise.dyn_into::<Promise>() else {
        return false;
    };
    JsFuture::from(promise)
        .await
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

struct Loaded {
    rel: String,
    bytes: Vec<u8>,
}

async fn walk(
    dir: &FileSystemDirectoryHandle,
    prefix: &str,
    out: &mut Vec<Loaded>,
    dirs: &mut Vec<String>,
) -> Result<(), JsValue> {
    let entries = dir.entries();
    loop {
        let next = match JsFuture::from(entries.next()?).await {
            Ok(next) => next,
            // Another tab removed the folder between entries: what is left is
            // still a consistent snapshot.
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
            match Box::pin(walk(&child, &rel, out, dirs)).await {
                Ok(()) => {}
                Err(error) if is_not_found(&error) => {}
                Err(error) => return Err(error),
            }
        } else {
            let file: FileSystemFileHandle = handle.unchecked_into();
            match read_file(&file).await {
                Ok(bytes) => out.push(Loaded { rel, bytes }),
                // A file another tab deleted or renamed mid-walk is skipped,
                // the same best-effort posture as the store's `read_dir`.
                Err(error) if is_not_found(&error) => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}

async fn read_file(handle: &FileSystemFileHandle) -> Result<Vec<u8>, JsValue> {
    // `File` is a `Blob`; the array buffer keeps binary attachments byte exact.
    let blob: web_sys::Blob = JsFuture::from(handle.get_file()).await?.unchecked_into();
    let buffer = JsFuture::from(blob.array_buffer()).await?;
    let view = js_sys::Uint8Array::new(&buffer);
    let mut bytes = vec![0; view.length() as usize];
    view.copy_to(&mut bytes);
    Ok(bytes)
}

async fn flush_store(store: &OpfsStore, root: &FileSystemDirectoryHandle) -> Result<(), String> {
    if !store.is_writable() {
        return Ok(());
    }
    let plan = store.take_flush_plan();
    if plan.is_empty() {
        return Ok(());
    }
    if let Err(error) = apply_plan(store, root, &plan).await {
        store.requeue(&plan);
        return Err(error);
    }
    Ok(())
}

async fn apply_plan(
    store: &OpfsStore,
    root: &FileSystemDirectoryHandle,
    plan: &crate::store::FlushPlan,
) -> Result<(), String> {
    for rel in &plan.removals {
        remove_entry(root, rel).await.map_err(js_message)?;
    }
    for rel in &plan.dirs {
        // The plan carries the folder's whole current subtree. Replacing it
        // wholesale removes anything stale on the OPFS side too (a folder
        // deleted and recreated before a flush, or a rename into a path that
        // already had files), then recreates the exact mirror shape.
        remove_entry(root, rel).await.map_err(js_message)?;
        ensure_dir(root, rel).await.map_err(js_message)?;
    }
    for rel in &plan.files {
        let bytes = store
            .read_bytes(rel)
            .ok_or_else(|| format!("the mirror lost {rel} before the flush"))?;
        write_file(root, rel, &bytes).await.map_err(js_message)?;
    }
    Ok(())
}

/// The state home's drain: the same shape as the vault's, over keys instead of
/// vault-relative paths.
async fn flush_home(home: &OpfsStateHome, root: &FileSystemDirectoryHandle) -> Result<(), String> {
    if !home.is_writable() {
        return Ok(());
    }
    let plan = home.take_flush_plan();
    if plan.is_empty() {
        return Ok(());
    }
    if let Err(error) = apply_home_plan(home, root, &plan).await {
        home.requeue(&plan);
        return Err(error);
    }
    Ok(())
}

async fn apply_home_plan(
    home: &OpfsStateHome,
    root: &FileSystemDirectoryHandle,
    plan: &StateFlushPlan,
) -> Result<(), String> {
    for key in &plan.removals {
        remove_entry(root, key).await.map_err(js_message)?;
    }
    for key in &plan.writes {
        let bytes = home
            .read_bytes(key)
            .ok_or_else(|| format!("the state mirror lost {key} before the flush"))?;
        write_file(root, key, &bytes).await.map_err(js_message)?;
    }
    Ok(())
}

async fn ensure_dir(
    root: &FileSystemDirectoryHandle,
    rel: &str,
) -> Result<FileSystemDirectoryHandle, JsValue> {
    dir_handle(root, rel, true).await
}

/// Walk to a directory without creating anything: a removal must not leave
/// empty parents behind when the entry's folder is already gone.
async fn find_dir(
    root: &FileSystemDirectoryHandle,
    rel: &str,
) -> Result<FileSystemDirectoryHandle, JsValue> {
    dir_handle(root, rel, false).await
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

async fn write_file(
    root: &FileSystemDirectoryHandle,
    rel: &str,
    bytes: &[u8],
) -> Result<(), JsValue> {
    let (parent, name) = rel.rsplit_once('/').unwrap_or(("", rel));
    let dir = ensure_dir(root, parent).await?;
    let options = FileSystemGetFileOptions::new();
    options.set_create(true);
    let handle: FileSystemFileHandle =
        JsFuture::from(dir.get_file_handle_with_options(name, &options))
            .await?
            .dyn_into()?;
    let writable: FileSystemWritableFileStream =
        JsFuture::from(handle.create_writable()).await?.dyn_into()?;
    JsFuture::from(writable.write_with_u8_array(bytes)?).await?;
    let stream: WritableStream = writable.unchecked_into();
    JsFuture::from(stream.close()).await?;
    Ok(())
}

async fn remove_entry(root: &FileSystemDirectoryHandle, rel: &str) -> Result<(), JsValue> {
    let (parent, name) = rel.rsplit_once('/').unwrap_or(("", rel));
    let dir = match find_dir(root, parent).await {
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

fn session() -> Option<(
    Arc<OpfsStore>,
    Arc<OpfsStateHome>,
    FileSystemDirectoryHandle,
    FileSystemDirectoryHandle,
)> {
    SESSION.with(|slot| {
        slot.borrow().as_ref().map(|session| {
            (
                Arc::clone(&session.store),
                Arc::clone(&session.home),
                session.root.clone(),
                session.state_root.clone(),
            )
        })
    })
}

/// The vault store this tab is serving, for export and import. `None` when
/// OPFS could not mount and the demo is running on the embedded sample.
pub(crate) fn session_store() -> Option<Arc<OpfsStore>> {
    SESSION.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|session| Arc::clone(&session.store))
    })
}

fn get(object: &JsValue, key: &str) -> Result<JsValue, JsValue> {
    Reflect::get(object, &JsValue::from_str(key))
}

fn error_name(error: &JsValue) -> Option<String> {
    get(error, "name").ok().and_then(|value| value.as_string())
}

fn is_not_found(error: &JsValue) -> bool {
    error_name(error).as_deref() == Some("NotFoundError")
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

/// The Web Lock control object, in JS because `LockManager` is still behind
/// web-sys's unstable-API cfg. The callback keeps the lock for the page by
/// returning a promise that only resolves on [`release`]; `ifAvailable` makes
/// the second tab fail fast instead of waiting.
#[wasm_bindgen(inline_js = r#"
export function muninn_lock(name) {
  let grant;
  const acquired = new Promise((resolve) => { grant = resolve; });
  let lose;
  const lost = new Promise((resolve) => { lose = resolve; });
  const control = { acquired, lost, release: null, supported: false };
  control.supported = !!(navigator.locks && navigator.locks.request);
  if (!control.supported) {
    grant(false);
    return control;
  }
  try {
    navigator.locks.request(name, { mode: "exclusive", ifAvailable: true }, (lock) => {
      if (!lock) {
        grant(false);
        return;
      }
      grant(true);
      return new Promise((resolve) => {
        control.release = () => { lose(); resolve(); };
      });
    }).catch(() => grant(false));
  } catch (_) {
    grant(false);
  }
  return control;
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = muninn_lock)]
    fn lock_control(name: &str) -> JsValue;
}

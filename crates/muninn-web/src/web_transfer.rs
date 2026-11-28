//! The browser's file half of vault export and import (#240 slice 6e).
//!
//! [`crate::transfer`] owns the archive bytes and [`muninn_ui::vault_transfer`]
//! owns the palette seam and the import guard; this module is the only part
//! that touches the page. It registers two callbacks at boot:
//!
//! - **export** reads the OPFS mirror and downloads the archive, using
//!   `showSaveFilePicker` where the browser has it and a Blob + anchor click
//!   everywhere else. Export only reads, so a read-only tab can use it.
//! - **import** opens a file picker. The picked bytes are parsed on a
//!   `spawn_local` task and parked; the app's poll loop applies them with the
//!   `State` in hand, so the ordinary save guard runs before anything is
//!   written and the vault is re-indexed afterwards.
//!
//! A read-only tab refuses import with the same lock message the boot shows.
//! Messages cross from the page back to the status bar through the pending
//! slots: the callbacks have no borrow of `State`.

use std::cell::RefCell;

use js_sys::{Promise, Reflect, Uint8Array};
use muninn_i18n::{Key, t};
use muninn_ui::State;
use muninn_ui::vault_transfer::VaultTransferHooks;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use crate::transfer::{self, ArchiveEntry};

/// The archive's suggested download name. A file name, not copy.
const ARCHIVE_NAME: &str = "muninn-vault.zip";
/// What the picker offers: the app's archive and a single Markdown note.
const IMPORT_ACCEPT: &str = ".zip,.md,application/zip,text/markdown";

thread_local! {
    static PENDING_IMPORT: RefCell<Option<Vec<ArchiveEntry>>> = const { RefCell::new(None) };
    static PENDING_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
    static PENDING_NOTICE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Register the export/import hooks with the shared UI. Called once from the
/// boot, before `rune::run` asks for the first frame.
pub fn install() {
    muninn_ui::vault_transfer::install(VaultTransferHooks {
        export,
        import: request_import,
    });
}

fn export() {
    let Some(store) = crate::opfs::session_store() else {
        set_error(t(Key::WebExportUnavailable));
        return;
    };
    let bytes = match transfer::export_zip(&*store) {
        Ok(bytes) => bytes,
        Err(error) => {
            web_sys::console::error_1(&format!("muninn: vault export failed: {error}").into());
            set_error(t(Key::WebExportFailed));
            return;
        }
    };
    // Start the save inside the palette key's own task: `showSaveFilePicker`
    // wants transient activation, and only a microtask hop is safe.
    let array = Uint8Array::from(bytes.as_slice());
    let promise = match save_in_page(ARCHIVE_NAME, &array, t(Key::WebArchiveFileType))
        .dyn_into::<Promise>()
    {
        Ok(promise) => promise,
        Err(error) => {
            web_sys::console::error_1(
                &format!(
                    "muninn: vault export download failed: {}",
                    js_message(error)
                )
                .into(),
            );
            set_error(t(Key::WebExportFailed));
            return;
        }
    };
    spawn_local(async move {
        match JsFuture::from(promise).await {
            Ok(saved) if saved.as_bool() == Some(true) => set_notice(t(Key::WebExportDone)),
            // The user closed the browser's save picker.
            Ok(_) => {}
            Err(error) => {
                web_sys::console::error_1(
                    &format!(
                        "muninn: vault export download failed: {}",
                        js_message(error)
                    )
                    .into(),
                );
                set_error(t(Key::WebExportFailed));
            }
        }
    });
}

fn request_import() {
    let Some(store) = crate::opfs::session_store() else {
        set_error(t(Key::WebImportUnavailable));
        return;
    };
    if !store.is_writable() {
        // The same copy the boot shows for this tab's read-only state.
        set_notice(crate::opfs::read_only_message());
        return;
    }
    // The hidden input's `click()` runs in the palette key's own task: a file
    // picker opened from a later microtask loses the user activation some
    // browsers require.
    let promise = match pick_in_page(IMPORT_ACCEPT).dyn_into::<Promise>() {
        Ok(promise) => promise,
        Err(error) => {
            web_sys::console::error_1(
                &format!(
                    "muninn: the vault file picker failed: {}",
                    js_message(error)
                )
                .into(),
            );
            set_error(t(Key::WebImportFailed));
            return;
        }
    };
    spawn_local(async move {
        let picked = match JsFuture::from(promise).await {
            Ok(picked) if !picked.is_null() && !picked.is_undefined() => picked,
            Ok(_) => return,
            Err(error) => {
                web_sys::console::error_1(
                    &format!(
                        "muninn: the vault file picker failed: {}",
                        js_message(error)
                    )
                    .into(),
                );
                set_error(t(Key::WebImportFailed));
                return;
            }
        };
        let name = field(&picked, "name")
            .ok()
            .and_then(|value| value.as_string());
        let bytes = match picked_bytes(&picked) {
            Ok(bytes) => bytes,
            Err(error) => {
                web_sys::console::error_1(
                    &format!(
                        "muninn: the picked file could not be read: {}",
                        js_message(error)
                    )
                    .into(),
                );
                set_error(t(Key::WebImportUnreadable));
                return;
            }
        };
        match transfer::read_archive(&bytes, name.as_deref()) {
            Ok(entries) => PENDING_IMPORT.with(|slot| *slot.borrow_mut() = Some(entries)),
            Err(error) => {
                web_sys::console::error_1(
                    &format!("muninn: the picked file is not a vault archive: {error}").into(),
                );
                set_error(t(Key::WebImportUnreadable));
            }
        }
    });
}

/// Drain the page's pending messages and apply a picked archive, if one
/// arrived. Called from the app's poll wrapper with the live `State`.
pub fn poll(state: &mut State) {
    if let Some(error) = PENDING_ERROR.with(|slot| slot.borrow_mut().take()) {
        state.error = Some(error);
    }
    if let Some(notice) = PENDING_NOTICE.with(|slot| slot.borrow_mut().take()) {
        state.notice = Some(notice);
    }
    let Some(entries) = PENDING_IMPORT.with(|slot| slot.borrow_mut().take()) else {
        return;
    };
    let store = match state.begin_vault_import() {
        Ok(store) => store,
        Err(error) => {
            state.error = Some(error.message().to_owned());
            return;
        }
    };
    // Write first, then re-index: a member that fails mid-archive leaves the
    // files already written visible instead of an index that disagrees with
    // the store. The flush loop persists whatever the mirror accepted.
    let written = transfer::write_archive(&*store, &entries);
    let reloaded = state.finish_vault_import(store);
    match (written, reloaded) {
        (Ok(_), Ok(())) => state.notice = Some(t(Key::WebImportDone).to_owned()),
        (Err(error), _) => {
            web_sys::console::error_1(&format!("muninn: vault import failed: {error}").into());
            state.error = Some(t(Key::WebImportFailed).to_owned());
        }
        (Ok(_), Err(error)) => state.error = Some(error.message().to_owned()),
    }
}

fn set_error(message: &str) {
    PENDING_ERROR.with(|slot| *slot.borrow_mut() = Some(message.to_owned()));
}

fn set_notice(message: &str) {
    PENDING_NOTICE.with(|slot| *slot.borrow_mut() = Some(message.to_owned()));
}

fn picked_bytes(picked: &JsValue) -> Result<Vec<u8>, JsValue> {
    let bytes = field(picked, "bytes")?;
    Ok(bytes.dyn_into::<Uint8Array>()?.to_vec())
}

fn field(value: &JsValue, key: &str) -> Result<JsValue, JsValue> {
    Reflect::get(value, &JsValue::from_str(key))
}

fn js_message(error: JsValue) -> String {
    if let Some(message) = error.as_string() {
        return message;
    }
    field(&error, "message")
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_else(|| format!("{error:?}"))
}

#[wasm_bindgen(inline_js = r#"
// Download `bytes` as `name`. `showSaveFilePicker` is the File System Access
// path where a browser has it; a Blob URL and an anchor click is the fallback.
// Resolves false when the user cancelled the save picker.
export function muninn_save_file(name, bytes, description) {
  const blob = new Blob([bytes], { type: "application/zip" });
  if (window.showSaveFilePicker) {
    return window.showSaveFilePicker({
      suggestedName: name,
      types: [{ description, accept: { "application/zip": [".zip"] } }],
    }).then(async (handle) => {
      const writable = await handle.createWritable();
      await writable.write(blob);
      await writable.close();
      return true;
    }).catch((error) => {
      if (error && error.name === "AbortError") return false;
      throw error;
    });
  }
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
  return Promise.resolve(true);
}

// One file, chosen through a hidden input. Resolves { name, bytes } or null.
// A cancelled dialog fires no change event in most browsers, so the promise
// simply stays pending: cancelling imports nothing.
export function muninn_pick_file(accept) {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    if (accept) input.accept = accept;
    input.style.display = "none";
    document.body.appendChild(input);
    input.addEventListener("change", async () => {
      const file = input.files && input.files[0];
      input.remove();
      if (!file) { resolve(null); return; }
      const buffer = await file.arrayBuffer();
      resolve({ name: file.name, bytes: new Uint8Array(buffer) });
    });
    input.click();
  });
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = muninn_save_file)]
    fn save_in_page(name: &str, bytes: &Uint8Array, description: &str) -> JsValue;
    #[wasm_bindgen(js_name = muninn_pick_file)]
    fn pick_in_page(accept: &str) -> JsValue;
}

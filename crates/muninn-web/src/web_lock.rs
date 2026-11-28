//! The one Web Locks control object, shared by the page boot and the sync
//! worker (#240 slices 6b and 8a).
//!
//! Both the page's single-writer vault lock and the sync worker's run lock
//! wrap `navigator.locks` the same way: request a named, exclusive lock
//! fail-fast (`ifAvailable`), and keep it for the lifetime of a promise the
//! callback returns. `LockManager` is still behind web-sys's unstable-API cfg,
//! so the callback that holds the lock open is written in JS here and the Rust
//! halves talk to the returned control object with `Reflect`.
//!
//! The object has four fields:
//!
//! - `supported`: whether this browser exposes Web Locks at all;
//! - `acquired`: a promise resolving `true` when the lock was granted and
//!   `false` when another document holds it;
//! - `lost`: a promise resolving when this document's hold ends (a page
//!   unload, or an explicit [`release`](Link)); it never resolves when the
//!   request was not granted;
//! - `release`: a function that ends the hold early, or `null`.
//!
//! This module is compiled in both the page and the worker build: it has no
//! dependency on the page's OPFS session or the shared UI.

use wasm_bindgen::prelude::wasm_bindgen;

/// The Web Lock control object, in JS because `LockManager` is still behind
/// web-sys's unstable-API cfg. The callback keeps the lock for the document by
/// returning a promise that only resolves on `release`; `ifAvailable` makes a
/// second holder fail fast instead of waiting.
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
    pub(crate) fn lock_control(name: &str) -> wasm_bindgen::JsValue;
}

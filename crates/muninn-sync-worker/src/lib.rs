//! The sync worker's crate (#240 slice 8a).
//!
//! This is the worker half of the browser sync: a second wasm-bindgen bundle
//! (`scripts/web-build.sh` builds it beside the page as `worker.js` plus its
//! wasm) that runs the one blocking sync engine in a dedicated Web Worker.
//! The page bundle never links it — the page calls
//! `muninn_web::page_sync::spawn` instead.
//!
//! Three pieces live here:
//!
//! - [`vault`]: the worker's [`muninn_core::VaultStore`] over `muninn-vault/`.
//!   A synchronous, case-sensitive mirror of OPFS with a dirty journal; the
//!   wasm half ([`vault`]'s `VaultDisk`) loads it at worker boot and flushes it
//!   with sync access handles when the run ends. This is where the vault's
//!   bytes are actually read and written for a sync.
//! - [`request`]: the wire request → [`muninn_sync::RunRequest`] mapping. The
//!   vault's identity is a hash of the root label it passes, so this module is
//!   where the label's meaning (one OPFS directory per origin) is documented.
//! - [`report`]: the engine's result → [`muninn_web::sync_protocol::WorkerEvent`]
//!   mapping.
//!
//! `entry` is the wasm-only message loop and lease flow; it is the module
//! that calls `run_sync_with_parts`. The engine itself is not changed by this
//! crate.
//!
//! # Why a separate crate
//!
//! A second `#[wasm_bindgen]` entry in `muninn-web` would be compiled into the
//! page's cdylib together with the page's `start`, UI and Rune. A dedicated
//! cdylib gives the worker its own wasm-bindgen output and keeps the page's
//! `app_bg.wasm` byte-for-byte the page build's. The crate depends on
//! `muninn-web` only for the shared message contract and the worker feature's
//! transport/state-home/lock machinery; `page` is off, so no UI or Rune code
//! is even compiled for the worker.

#![forbid(unsafe_code)]

pub mod credentials;
pub mod report;
pub mod request;
pub mod vault;

#[cfg(target_arch = "wasm32")]
pub mod entry;
#[cfg(target_arch = "wasm32")]
pub mod vault_opfs;

//! The browser's sync transport machinery (#240 slice 8a).
//!
//! The engine is synchronous: it blocks on HTTP and on the vault. A page
//! cannot run it, so the design puts it in a dedicated Web Worker where two
//! browser facilities make the blocking shape work:
//!
//! - **sync XHR** for the provider calls ([`transport::WebTransport`]), and
//! - **synchronous OPFS access handles** for the run's state files
//!   ([`state_home`]) and for the vault ([`opfs_util`] plus the worker crate's
//!   `vault` module).
//!
//! `sync_protocol` is the message contract between the page and the worker.
//! The worker entry itself lives in the `muninn-sync-worker` crate: a second
//! wasm-bindgen bundle so the page's `app_bg.wasm` never grows by the engine
//! or this machinery. This module is the shared half the worker crate calls:
//! the state home, the transport, the sleeper, the run lock and the OPFS
//! primitives.
//!
//! # The lease
//!
//! There is no filesystem lease in a browser. The page's boot already holds a
//! `navigator.locks` lock for the vault, which serializes writing tabs, and a
//! dedicated worker processes one message at a time, so two runs cannot
//! overlap inside a tab. The cross-tab guard is [`SYNC_LOCK_NAME`], a named Web
//! Lock taken by [`lock::RunLock`] for the run and released when it finishes.
//! It is fail-fast (`ifAvailable`), matching the vault's boot lock: a second
//! tab is told a run is already in progress instead of silently queueing.
//! There is no long-lived lease and no lock file; a worker that dies drops the
//! lock with its document. The worker's `entry` module wraps a held
//! [`lock::RunLock`] in the engine's `SyncLease` and takes it **before** the
//! blocking `run_sync_with_parts` call, because Web Locks are asynchronous.
//!
//! # Durability
//!
//! `StateHome` is synchronous and `createSyncAccessHandle()` is a Promise. The
//! state home and the vault store therefore answer the engine from memory
//! mirrors and flush with sync handles when the run ends; the windows are
//! documented on [`state_home::SyncStateHome`] and the worker crate's `vault`
//! module.

pub mod lock;
pub mod opfs_util;
pub mod sleeper;
pub mod state_home;
pub mod transport;

pub use crate::sync_state::{SYNC_LOCK_NAME, SYNC_STATE_DIR};

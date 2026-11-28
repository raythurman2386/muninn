//! The browser's sync transport machinery (#240 slice 8a).
//!
//! The engine is synchronous: it blocks on HTTP and on the vault. A page
//! cannot run it, so the design puts it in a dedicated Web Worker where two
//! browser facilities make the blocking shape work:
//!
//! - **sync XHR** for the provider calls ([`transport::WebTransport`]), and
//! - **synchronous OPFS access handles** for the run's state files
//!   ([`state_home`]).
//!
//! This module is that machinery. `sync_protocol` is the message contract
//! between the page and the worker; the pieces here are the worker half.
//!
//! # What this slice does and does not wire
//!
//! This slice compiles and tests the transport, the state home, the run-scoped
//! credential store and the sleeper for `wasm32-unknown-unknown`. It does
//! **not** ship the worker entry that calls `run_sync_with_state_home`, for a
//! reason outside the transport itself: the engine still reaches the *vault*
//! through `std::fs` (`inventory`, `read_local_bytes`, `apply_plan_with`, the
//! filesystem lease). `RunRequest.root` is a real path, and a browser has no
//! filesystem path to hand it — the vault lives behind `muninn_core`'s
//! `VaultStore`/`MemoryStore` seam. Running the engine unchanged therefore
//! needs a vault-byte seam first; until it lands, a worker entry would accept
//! a run and fail at its first inventory. See `docs/web.md` slice 8 for the
//! recorded seam.
//!
//! The state home has a second, smaller honesty note: `StateHome` is
//! synchronous and `createSyncAccessHandle()` is a Promise. The mirror below
//! therefore answers the engine synchronously and flushes with sync handles
//! when the run yields; the durability window is documented on
//! [`state_home::SyncStateHome`].
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
//! lock with its document.

pub mod credentials;
pub mod lock;
pub mod request;
pub mod sleeper;
pub mod state_home;
pub mod transport;

pub use crate::sync_state::{SYNC_LOCK_NAME, SYNC_STATE_DIR};

//! The browser's registered state home (#240 slice 6d).
//!
//! The shared UI's crash recovery, settings and lock high-water code is
//! synchronous Rust; a browser's durable store (OPFS) is Promise-based and can
//! only be read before the first frame. So the browser boot loads the whole
//! state home into an in-memory mirror, registers it here, and the app's poll
//! loop drains the mirror back to OPFS. This is the same split as the vault's
//! [`VaultStore`](muninn_core::VaultStore) in slice 6b, and the same
//! registration shape: one handle per session, installed before the `State` is
//! built so the three stores see it from their first read.
//!
//! The native build does not use this module at all: it keeps the XDG
//! directories and its own `std::fs` code, byte for byte.

use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use muninn_core::StateHome;

static HOME: OnceLock<Mutex<Option<Arc<dyn StateHome>>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Arc<dyn StateHome>>> {
    HOME.get_or_init(|| Mutex::new(None))
}

fn lock() -> std::sync::MutexGuard<'static, Option<Arc<dyn StateHome>>> {
    slot().lock().unwrap_or_else(PoisonError::into_inner)
}

/// Make `home` the store this session's recovery, settings and lock records
/// use. The browser boot calls this after loading OPFS and before building
/// the [`State`](crate::State), the same order the vault store is installed.
pub fn install(home: Arc<dyn StateHome>) {
    *lock() = Some(home);
}

/// Drop the registered store. The boot calls this when OPFS mounted but the
/// state home could not become the vault's, so a later save fails honestly
/// instead of writing into a mirror nobody drains.
pub fn clear() {
    *lock() = None;
}

/// The registered store, if this session has one.
pub(crate) fn registered() -> Option<Arc<dyn StateHome>> {
    lock().as_ref().map(Arc::clone)
}

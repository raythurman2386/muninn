//! The run's cross-tab lock: a fail-fast `navigator.locks` hold (#240 slice
//! 8a).
//!
//! The native engine holds a filesystem lease for a run. A browser has no
//! lease file and no advisory lock, so the web twin is the smallest honest
//! substitute: one named Web Lock held for the run and released when it ends.
//! It is not long-lived, has no timeout to steal and cannot go stale — the
//! browser drops it when the worker's document goes away.
//!
//! The lock is fail-fast (`ifAvailable`), like the vault's boot lock: a second
//! tab is told a run is already in progress instead of queueing behind a
//! potentially slow provider call. Inside one worker, runs are already
//! serialized because the worker processes one message at a time.
//!
//! The control object comes from `crate::opfs`, which owns the one inline-JS
//! implementation of the Web Lock callback: `LockManager` is still behind
//! web-sys's unstable-API cfg, so the callback that holds the lock open is
//! written in JS there.

use js_sys::{Function, Promise, Reflect};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

use crate::sync::SYNC_LOCK_NAME;

/// A held run lock. Dropping it releases the Web Lock.
pub struct RunLock {
    control: JsValue,
}

impl RunLock {
    /// Take the run lock, or report that another tab holds it.
    ///
    /// # Errors
    ///
    /// A short diagnostic when the browser has no Web Locks, when the lock
    /// request fails, or when another tab is already running a sync.
    pub async fn acquire() -> Result<RunLock, String> {
        let control = crate::web_lock::lock_control(SYNC_LOCK_NAME);
        let supported = get(&control, "supported")
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !supported {
            return Err(
                "this browser has no Web Locks, so two tabs cannot serialize a sync".to_owned(),
            );
        }
        let acquired = get(&control, "acquired")
            .ok()
            .and_then(|value| value.dyn_into::<Promise>().ok())
            .ok_or_else(|| "the sync lock could not be requested".to_owned())?;
        let granted = JsFuture::from(acquired)
            .await
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !granted {
            return Err("another tab is already syncing this vault".to_owned());
        }
        Ok(RunLock { control })
    }
}

impl Drop for RunLock {
    fn drop(&mut self) {
        // The JS control's `release` resolves the promise the lock callback is
        // holding; calling it twice would be harmless, but Drop runs once.
        let release = get(&self.control, "release")
            .ok()
            .and_then(|value| value.dyn_into::<Function>().ok());
        if let Some(release) = release {
            let _ = release.call0(&JsValue::UNDEFINED);
        }
    }
}

fn get(object: &JsValue, key: &str) -> Result<JsValue, JsValue> {
    Reflect::get(object, &JsValue::from_str(key))
}

//! The worker's sleeper: `Atomics.wait` where the page is isolated, a short
//! bounded spin otherwise (#240 slice 8a).
//!
//! The engine's only sleeps are read-retry backoff (`muninn-sync`'s HTTP
//! retry gives 50/100/200/400 ms, and at most 30 s for a `Retry-After`) and
//! the publish-round pause (100–400 ms). PAT auth removes device-flow polling,
//! so there is no long sleep in the ordinary path.
//!
//! `Atomics.wait` needs a `SharedArrayBuffer`, which is only exposed when
//! `crossOriginIsolated` is true. Verified in current Chrome: on a plain dev
//! server (no COOP/COEP) `new SharedArrayBuffer(4)` throws
//! `ReferenceError: SharedArrayBuffer is not defined`, while the deployed
//! slice-7 image is isolated and exposes it. Both branches below are therefore
//! required, not defensive:
//!
//! - **isolated:** sleep the whole delay in 25 ms slices with `Atomics.wait`,
//!   checking cancel between slices; no CPU is burned.
//! - **not isolated:** spin for at most [`SPIN_CAP`] of the requested delay
//!   (25 ms cancel-check slices), then return. A longer delay is retried
//!   *sooner* than asked rather than blocking the worker for seconds. The worst
//!   case is 250 ms of CPU per sleep; the engine's bounded retry budget means
//!   this happens a handful of times at most.
//!
//! Neither path can be driven headlessly; the in-browser gate must observe a
//! rate-limited read against a real `Retry-After` on both an isolated and a
//! non-isolated page.

use std::time::Duration;

use muninn_sync::{CancelToken, Sleeper, SyncError};
use wasm_bindgen::JsValue;

/// The most CPU a non-isolated fallback will burn for one sleep.
pub const SPIN_CAP: Duration = Duration::from_millis(250);

/// Granularity of a cancel check while sleeping.
const SLICE: Duration = Duration::from_millis(25);

/// A [`Sleeper`] backed by `Atomics.wait` when the page is cross-origin
/// isolated, and by a bounded spin when it is not.
pub struct AtomicsSleeper {
    /// `Some` only when `crossOriginIsolated` was true at construction and a
    /// `SharedArrayBuffer` could be allocated. The array is never written; it
    /// exists so `Atomics.wait` has a shared location to wait on.
    shared: Option<js_sys::Int32Array>,
}

impl AtomicsSleeper {
    /// Build a sleeper for this worker's page.
    #[must_use]
    pub fn new() -> AtomicsSleeper {
        let shared = if cross_origin_isolated() {
            let buffer = js_sys::SharedArrayBuffer::new(4);
            Some(js_sys::Int32Array::new(buffer.as_ref()))
        } else {
            None
        };
        AtomicsSleeper { shared }
    }

    /// True when this sleeper can block without spinning.
    #[must_use]
    pub fn is_isolated(&self) -> bool {
        self.shared.is_some()
    }
}

impl Default for AtomicsSleeper {
    fn default() -> Self {
        Self::new()
    }
}

impl Sleeper for AtomicsSleeper {
    fn sleep(&self, delay: Duration, cancel: &CancelToken) -> Result<(), SyncError> {
        cancel.check()?;
        match &self.shared {
            Some(shared) => {
                let mut left = delay;
                while !left.is_zero() {
                    let step = left.min(SLICE);
                    // "timed-out", "not-equal" and "ok" all just end the
                    // slice; cancellation is checked immediately after. If the
                    // wait itself is refused (a context that disallows it),
                    // fall back to the bounded spin instead of looping on a
                    // call that always throws.
                    if js_sys::Atomics::wait_with_timeout(shared, 0, 0, step.as_secs_f64() * 1000.0)
                        .is_err()
                    {
                        return spin(delay, cancel);
                    }
                    cancel.check()?;
                    left = left.saturating_sub(step);
                }
                Ok(())
            }
            None => spin(delay, cancel),
        }
    }
}

/// Burn CPU for at most [`SPIN_CAP`] of `delay`, checking cancel every slice.
///
/// This is the non-isolated fallback and the last resort if `Atomics.wait` is
/// refused: a dev server has no `SharedArrayBuffer` (verified), and a worker
/// must not block for a 30 s `Retry-After`. Returning early means the retry is
/// attempted sooner than the provider asked; the client's bounded retry budget
/// still caps the total.
fn spin(delay: Duration, cancel: &CancelToken) -> Result<(), SyncError> {
    let deadline = now_ms() + delay.min(SPIN_CAP).as_secs_f64() * 1000.0;
    while now_ms() < deadline {
        cancel.check()?;
    }
    cancel.check()
}

/// `crossOriginIsolated` is a boolean on the global scope in both a window and
/// a worker. web-sys does not expose it on every global type, so read it by
/// name.
fn cross_origin_isolated() -> bool {
    js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("crossOriginIsolated"))
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}

//! The worker message loop (#240 slice 8a): one `Run` at a time, the Web Lock
//! lease, and the OPFS flush.
//!
//! The worker is a dedicated module worker created by
//! [`muninn_web::page_sync::spawn`]. [`start`] registers `onmessage`; a `Run`
//! is executed on a local task so the message handler returns immediately,
//! and a `Cancel` flips the run's [`CancelToken`].
//!
//! # The ready handshake
//!
//! A dedicated worker's event loop is not dispatching while the shim's
//! top-level `await init()` is pending, and a message delivered with no
//! `onmessage` handler installed is **discarded**, not queued. Posting a run
//! immediately after `new Worker(...)` is therefore a silent loss. [`start`]
//! posts [`WorkerEvent::Ready`] only after `onmessage` is installed; the page
//! holds the request until then. See `muninn_web::sync_protocol` for what a
//! page may rely on. The generated `worker.js` shim additionally buffers any
//! message a non-Muninn caller posts before init, so that caller is not
//! surprised either.
//!
//! # The order of a run
//!
//! 1. **Validate** the wire request ([`PreparedRun::new`]). Failures are a
//!    `Failed` event with the engine's own code; no provider call is made.
//! 2. **Take the Web Lock** (`muninn-sync-run`). Web Locks are asynchronous,
//!    so this must happen *before* the blocking engine call; the engine's
//!    [`SyncLease`] then hands it the already-held guard for both of its
//!    acquire phases. A second tab is told the run is already in progress
//!    (the lock is fail-fast, like the vault's boot lock).
//! 3. **Mount** `muninn-vault/` and `muninn-sync-state/` into their mirrors.
//! 4. Post [`WorkerEvent::Started`], then call `run_sync_with_parts` with the
//!    vault store, state home and lease. This is the only blocking call in the
//!    worker: sync XHR and sync OPFS handles are what make it possible.
//! 5. **Flush** the vault, then the state. Vault first: a journal that says
//!    "applied" over vault bytes that never reached OPFS is the mismatch that
//!    would mislead the next run; a vault that is ahead of the journal is
//!    recoverable, because the next run re-reads OPFS and replans. If the
//!    vault flush fails, the state flush is skipped: the pre-run state is the
//!    one that matches the bytes OPFS still has. A failure part-way through
//!    the vault flush can leave some files new and some old; that is a normal
//!    local change to the next run's inventory, not a corrupted journal.
//! 6. Drop the lock, post the terminal event, yield one task so the message
//!    reaches the page, and close the worker.
//!
//! # Cancel, honestly
//!
//! A message can only be handled while the worker's event loop runs. During
//! step 4 the worker is inside a synchronous call, so a `Cancel` message waits
//! until the run returns; it then finds no run and does nothing. The page's
//! immediate cancel is terminating the worker, which is safe by construction:
//! the vault and state are mirrors and nothing this run wrote is in OPFS until
//! step 5. `Cancel` is still handled because it covers the setup awaits and
//! because the contract has it; the sleeper and the GitHub client poll the
//! same token.

use std::cell::{Cell, RefCell};

use js_sys::{Function, Reflect};
use muninn_sync::{
    CancelToken, EngineParts, LeaseGuard, SyncError, SyncLease, VaultIdentity, run_sync_with_parts,
};
use muninn_web::sync::lock::RunLock;
use muninn_web::sync::sleeper::AtomicsSleeper;
use muninn_web::sync::state_home::SyncStateHome;
use muninn_web::sync::transport::WebTransport;
use muninn_web::sync_protocol::{RunRequest, WorkerEvent, WorkerRequest};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_futures::spawn_local;

use crate::report;
use crate::request::{PreparedRun, VAULT_LABEL};
use crate::vault::WorkerVaultStore;
use crate::vault_opfs::VaultDisk;

/// The worker's entry point. The generated `worker.js` imports the
/// wasm-bindgen glue and awaits `init()`, which runs this.
///
/// Registering `onmessage` is synchronous; every run is a local task. The
/// callback is leaked for the worker's lifetime on purpose: a dedicated
/// worker exists for one page message stream and has no unload hook to drop
/// it from.
#[wasm_bindgen(start)]
pub fn start() {
    let callback = Closure::<dyn FnMut(JsValue)>::new(|event: JsValue| {
        let data = Reflect::get(&event, &JsValue::from_str("data")).unwrap_or(JsValue::NULL);
        let Some(text) = data.as_string() else {
            post(&WorkerEvent::Failed {
                code: "protocol".to_owned(),
                detail: "the page sent a sync message that is not a string".to_owned(),
            });
            return;
        };
        let request = match serde_json::from_str::<WorkerRequest>(&text) {
            Ok(request) => request,
            Err(error) => {
                post(&WorkerEvent::Failed {
                    code: "protocol".to_owned(),
                    detail: format!("the page sent a message that is not a sync request: {error}"),
                });
                return;
            }
        };
        match request {
            WorkerRequest::Run(request) => dispatch_run(request),
            WorkerRequest::Cancel => cancel(),
        }
    });
    if Reflect::set(
        &js_sys::global(),
        &JsValue::from_str("onmessage"),
        callback.as_ref(),
    )
    .is_ok()
    {
        callback.forget();
        // The page may send the run only now. A message posted before this
        // point had no listener and was dropped by the browser, which is why
        // the page-side session holds it until this handshake.
        post(&WorkerEvent::Ready);
    } else {
        // Without a handler the worker cannot run anything; tell the page so
        // it does not wait for a ready that will never come.
        post(&WorkerEvent::Failed {
            code: "worker".to_owned(),
            detail: "the sync worker could not install its message handler".to_owned(),
        });
    }
}

thread_local! {
    static RUNNING: Cell<bool> = const { Cell::new(false) };
    /// The token of the run in flight. A `Cancel` message flips it; see the
    /// module docs for why that only reaches a run that has not blocked yet.
    static CURRENT: RefCell<Option<CancelToken>> = const { RefCell::new(None) };
}

fn dispatch_run(request: RunRequest) {
    if RUNNING.with(Cell::get) {
        post(&WorkerEvent::Failed {
            code: "busy".to_owned(),
            detail: "a sync is already running in this worker".to_owned(),
        });
        return;
    }
    RUNNING.with(|flag| flag.set(true));
    let cancel = CancelToken::new();
    CURRENT.with(|slot| *slot.borrow_mut() = Some(cancel.clone()));
    spawn_local(async move {
        let event = execute(&request, &cancel).await;
        post(&event);
        RUNNING.with(|flag| flag.set(false));
        CURRENT.with(|slot| *slot.borrow_mut() = None);
        // Give the posted terminal event a turn to reach the page, then let
        // the worker exit. The page also terminates it after the event, so a
        // browser that refuses `close()` leaves nothing running.
        next_task().await;
        close_scope();
    });
}

fn cancel() {
    CURRENT.with(|slot| {
        if let Some(cancel) = slot.borrow().as_ref() {
            cancel.cancel();
        }
    });
}

async fn execute(request: &RunRequest, cancel: &CancelToken) -> WorkerEvent {
    let prepared = match PreparedRun::new(request) {
        Ok(prepared) => prepared,
        Err(error) => return report::failed_event(&error),
    };
    let engine_request = prepared.engine_request();

    // The Web Lock is the run lease. It must be held before the blocking
    // engine call because the API is async; the engine's `acquire` below only
    // borrows this hold.
    let lock = match RunLock::acquire().await {
        Ok(lock) => lock,
        Err(detail) => {
            return WorkerEvent::Failed {
                code: "lease_busy".to_owned(),
                detail,
            };
        }
    };

    let vault = WorkerVaultStore::new(VAULT_LABEL);
    let disk = match VaultDisk::open(&vault).await {
        Ok(disk) => disk,
        Err(detail) => {
            return WorkerEvent::Failed {
                code: "io".to_owned(),
                detail,
            };
        }
    };
    let home = match SyncStateHome::open().await {
        Ok(home) => home,
        Err(detail) => {
            return WorkerEvent::Failed {
                code: "io".to_owned(),
                detail,
            };
        }
    };

    // The run's credential store moves into the engine so `Drop` wipes the
    // PAT as soon as the engine is dropped.
    let PreparedRun { credentials, .. } = prepared;
    let engine = EngineParts {
        transport: WebTransport::new(),
        credentials,
        cancel: cancel.clone(),
        sleeper: Box::new(AtomicsSleeper::new()),
        github_api: "https://api.github.com".to_owned(),
        github_oauth: "https://github.com".to_owned(),
        // The same three bases the desktop app uses; the `web` allowlist admits
        // exactly these Google hosts.
        drive_api: muninn_sync::DRIVE_API_BASE.to_owned(),
        drive_upload: muninn_sync::DRIVE_UPLOAD_BASE.to_owned(),
        drive_oauth: muninn_sync::DRIVE_OAUTH_BASE.to_owned(),
        github_client_id: String::new(),
        // The public Web-application client id the page's GIS popup signed in
        // with. This is the one place the worker learns it; empty means the
        // build is unconfigured and a Drive refresh fails closed.
        google_web_client_id: muninn_web::google_auth::web_client_id()
            .unwrap_or_default()
            .to_owned(),
    };

    // About to make the run's first provider request. The engine's own
    // inventory read happens first, but nothing leaves the browser until the
    // GitHub client's first call, and this is the last setup step.
    post(&WorkerEvent::Started);

    let lease = HeldLease { _lock: &lock };
    let outcome = run_sync_with_parts(&engine, &engine_request, &vault, &home, &lease);
    let event = report::outcome_event(outcome);

    // Vault first, then state; see the module docs. If the vault could not be
    // saved, do not write the run's state: the pre-run state is what matches
    // the vault bytes OPFS still holds, and a journal that claimed an apply
    // which never reached the vault would mislead the next run.
    let vault_flush = disk.flush(&vault).await;
    let state_flush = match &vault_flush {
        Ok(()) => home.flush().await,
        Err(_) => Ok(()),
    };
    drop(lock);

    report::final_event(event, &vault_flush, &state_flush)
}

/// The run's [`SyncLease`]: the Web Lock is already held for the whole run.
///
/// The engine acquires the lease around the inventory read and again around
/// the apply, dropping the guard in between. Both guards borrow the outer
/// hold and release nothing; [`RunLock`]'s `Drop` at the end of the run is the
/// one release.
struct HeldLease<'a> {
    /// The lock the guard borrows. The field is never read: keeping it alive
    /// is the point (the guard cannot outlive the held Web Lock).
    _lock: &'a RunLock,
}

impl SyncLease for HeldLease<'_> {
    fn acquire(&self, _identity: &VaultIdentity) -> Result<Box<dyn LeaseGuard>, SyncError> {
        Ok(Box::new(HeldGuard))
    }
}

struct HeldGuard;

impl LeaseGuard for HeldGuard {}

fn post(event: &WorkerEvent) {
    let Ok(text) = serde_json::to_string(event) else {
        return;
    };
    let Ok(post) = Reflect::get(&js_sys::global(), &JsValue::from_str("postMessage")) else {
        return;
    };
    let Ok(post) = post.dyn_into::<Function>() else {
        return;
    };
    let _ = post.call1(&JsValue::UNDEFINED, &JsValue::from_str(&text));
}

fn close_scope() {
    let Ok(close) = Reflect::get(&js_sys::global(), &JsValue::from_str("close")) else {
        return;
    };
    let Ok(close) = close.dyn_into::<Function>() else {
        return;
    };
    let _ = close.call0(&JsValue::UNDEFINED);
}

/// Yield to the browser's task queue once (a `setTimeout(0)` promise).
async fn next_task() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let Ok(set_timeout) = Reflect::get(&js_sys::global(), &JsValue::from_str("setTimeout"))
        else {
            let _ = resolve.call0(&JsValue::UNDEFINED);
            return;
        };
        let Ok(set_timeout) = set_timeout.dyn_into::<Function>() else {
            let _ = resolve.call0(&JsValue::UNDEFINED);
            return;
        };
        let _ = set_timeout.call2(&JsValue::UNDEFINED, &resolve, &JsValue::from_f64(0.0));
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

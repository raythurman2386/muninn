//! The page half of the browser's GitHub sync surface (#240 slice 8b).
//!
//! [`muninn_ui::web_sync`] owns the form, the saved config, the PAT and the
//! status line. This module is the only part that touches the browser: it
//! registers the shared UI's two callbacks at boot, turns a form config into a
//! [`RunRequest`](crate::sync_protocol::RunRequest), spawns the run through
//! [`crate::page_sync::spawn`], and relays the worker's events back.
//!
//! # Why the terminal event waits
//!
//! `spawn`'s gate restores the page's flush by re-reading OPFS into the store
//! mirror asynchronously. The UI's terminal handling re-indexes the vault from
//! that mirror, so handing it the terminal event straight from the worker's
//! callback would sometimes index the pre-run snapshot. The page therefore
//! holds the terminal event in [`TERMINAL`] until
//! [`crate::opfs::sync_resnapshot_pending`] is false, then enqueues it for the
//! next poll. The job is one frame behind at most.
//!
//! # Cancel
//!
//! The shared UI's Cancel drops the run handle. Per `page_sync`'s contract that
//! is the immediate cancel: the worker stages every vault write in its mirror
//! and only touches OPFS at the end of the run, so termination leaves the vault
//! as the run found it, and dropping the handle restores the page's flush gate
//! through the session's abort path. The resulting terminal `Failed` is
//! rewritten to the `cancelled` code, so the person sees "cancelled" rather
//! than "worker stopped".

use std::cell::{Cell, RefCell};

use muninn_ui::State;
use muninn_ui::web_sync::{WebSyncConfig, WebSyncEvent, WebSyncHooks};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

use crate::page_request::{self, PageRequest};
use crate::page_sync::{SyncRunHandle, spawn};
use crate::sync_protocol::WorkerEvent;

thread_local! {
    /// The live run's handle. Dropping it is the immediate cancel; taking it
    /// also releases the worker and its closures once the run has ended.
    static RUN: RefCell<Option<SyncRunHandle>> = const { RefCell::new(None) };
    /// The terminal event, waiting for the post-run OPFS re-read.
    static TERMINAL: RefCell<Option<WebSyncEvent>> = const { RefCell::new(None) };
    /// The person pressed Cancel: rewrite the next terminal event's code so
    /// the status reads "cancelled", not "the worker stopped".
    static CANCELLED: Cell<bool> = const { Cell::new(false) };
}

/// Register the shared UI's sync callbacks. Called once from the boot, before
/// `rune::run` asks for the first frame.
pub fn install() {
    muninn_ui::web_sync::install(WebSyncHooks { start, cancel });
}

/// Start one run. The shared UI has already refused a second concurrent run,
/// but the page guards too: two handles would fight over the flush gate.
fn start(config: WebSyncConfig) {
    let running = RUN.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(SyncRunHandle::is_running)
    });
    if running {
        muninn_ui::web_sync::enqueue(WebSyncEvent::Failed {
            code: "busy".to_owned(),
            detail: String::new(),
        });
        return;
    }
    let operation_id = fresh_operation_id();
    let request = page_request::request_for(PageRequest {
        binding_id: &config.binding_id,
        operation_id: &operation_id,
        owner: &config.owner,
        repo: &config.repo,
        branch: &config.branch,
        prefix: &config.prefix,
        token: config.token.as_str(),
    });
    // The wire request owns one copy of the PAT and the spawn session's
    // serialized message owns another; the config's copy is wiped when it
    // drops at the end of this function.
    CANCELLED.with(|flag| flag.set(false));
    spawn_local(async move {
        match spawn(request, relay).await {
            Ok(handle) if CANCELLED.with(Cell::get) => {
                // Cancel was pressed while the vault was quiescing. Dropping
                // the fresh handle aborts the run and restores the gate; the
                // terminal event is relayed and rewritten to `cancelled`.
                drop(handle);
            }
            Ok(handle) => RUN.with(|slot| *slot.borrow_mut() = Some(handle)),
            Err(error) => {
                // Quiescing or creating the worker failed; nothing reached the
                // provider. The detail names the page's reason (a read-only
                // tab, OPFS unavailable), so it is shown inside the worker
                // failure's copy.
                muninn_ui::web_sync::enqueue(WebSyncEvent::Failed {
                    code: "worker".to_owned(),
                    detail: error,
                });
            }
        }
    });
}

/// Stop the run in flight by dropping its handle, the documented immediate
/// cancel. A run that has already ended has nothing to interrupt; its terminal
/// event is already waiting in [`TERMINAL`]. A cancel pressed while the spawn
/// task is still quiescing the vault sets [`CANCELLED`], and the task drops the
/// handle as soon as it arrives.
fn cancel() {
    if TERMINAL.with(|slot| slot.borrow().is_some()) {
        return;
    }
    CANCELLED.with(|flag| flag.set(true));
    let running = RUN.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(SyncRunHandle::is_running)
    });
    if !running {
        return;
    }
    let handle = RUN.with(|slot| slot.borrow_mut().take());
    drop(handle);
}

/// Relay one worker event: non-terminal events go straight to the UI's queue,
/// the terminal one waits for the page's post-run re-read.
fn relay(event: WorkerEvent) {
    let Some(event) = translate(event) else {
        return;
    };
    let event = if event.is_terminal() && CANCELLED.with(Cell::get) {
        WebSyncEvent::Failed {
            code: "cancelled".to_owned(),
            detail: String::new(),
        }
    } else {
        event
    };
    if event.is_terminal() {
        TERMINAL.with(|slot| *slot.borrow_mut() = Some(event));
    } else {
        muninn_ui::web_sync::enqueue(event);
    }
}

fn translate(event: WorkerEvent) -> Option<WebSyncEvent> {
    Some(match event {
        // The spawn session consumes the handshake; a direct `WorkerEvent`
        // never carries it here, but a future relay path must not draw it.
        WorkerEvent::Ready => return None,
        WorkerEvent::Started => WebSyncEvent::Started,
        WorkerEvent::Completed {
            head,
            exclusions,
            conflict_copies,
        } => WebSyncEvent::Completed {
            head,
            exclusions,
            conflict_copies,
        },
        WorkerEvent::Conflicts { paths } => WebSyncEvent::Conflicts { paths },
        WorkerEvent::Failed { code, detail } => WebSyncEvent::Failed { code, detail },
    })
}

/// Drain the page's sync state into the UI: hand over a finished terminal
/// event, release a finished handle, then apply every queued event.
pub(crate) fn poll(state: &mut State) -> bool {
    if RUN.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|handle| !handle.is_running())
    }) {
        // The worker closed itself after its terminal event; dropping the
        // handle releases its closures. The session is already terminal, so
        // this cannot abort a run.
        let handle = RUN.with(|slot| slot.borrow_mut().take());
        drop(handle);
    }
    if !crate::opfs::sync_resnapshot_pending()
        && let Some(event) = TERMINAL.with(|slot| slot.borrow_mut().take())
    {
        muninn_ui::web_sync::enqueue(event);
    }
    muninn_ui::web_sync::poll(state)
}

/// A fresh operation id: the publish idempotency key for one Sync now.
///
/// `crypto.randomUUID` where the browser has it; otherwise a hex id from
/// `Date.now` and `Math.random`. It is an idempotency key, not a secret, so a
/// non-cryptographic fallback is honest. Hyphens are removed because the
/// engine's `OperationId` accepts hex and dashes either way.
fn fresh_operation_id() -> String {
    let global = js_sys::global();
    if let Ok(crypto) = js_sys::Reflect::get(&global, &JsValue::from_str("crypto"))
        && let Ok(function) = js_sys::Reflect::get(&crypto, &JsValue::from_str("randomUUID"))
        && let Ok(function) = function.dyn_into::<js_sys::Function>()
        && let Ok(value) = function.call0(&crypto)
        && let Some(uuid) = value.as_string()
    {
        return uuid.replace('-', "");
    }
    let time = js_sys::Date::now() as u64;
    let noise = (js_sys::Math::random() * 9.0e15) as u64;
    format!("{time:016x}{noise:016x}")
}

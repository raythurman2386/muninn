//! Spawning the sync worker from the page (#240 slice 8a).
//!
//! [`spawn`] is the function the Sync surface (slice 8b) calls. It:
//!
//! 1. quiesces the page's vault flush and drains the dirty journal
//!    ([`crate::opfs::quiesce_for_sync`]) — the worker and the page must never
//!    write the same OPFS directory at once;
//! 2. creates the dedicated module worker (`worker.js`, built beside the page
//!    by `scripts/web-build.sh`) and hands it the serialized `RunRequest`;
//! 3. relays every `WorkerEvent` to the caller and, on the terminal event,
//!    re-reads OPFS into the page's mirror before letting the flush resume
//!    ([`crate::opfs::finish_sync_run`]).
//!
//! The state machine that decides when to restore is native-tested in
//! [`crate::sync_spawn`]; this module is the wasm glue around it.
//!
//! [`muninn_sync_run`] is a small `#[wasm_bindgen]` bridge for the browser
//! gate: one call runs a request and resolves with every event. It is not the
//! UI API — slice 8b calls [`spawn`] and draws from its callback.
//!
//! # Cancel
//!
//! [`SyncRunHandle::cancel`] posts `WorkerRequest::Cancel`. A run already
//! blocked in synchronous XHR cannot process that message until it returns,
//! so the immediate cancel is [`SyncRunHandle`]'s drop (which terminates the
//! worker). That is safe by construction: the worker stages every vault write
//! in its mirror and only touches OPFS at the end of the run, so termination
//! leaves the vault exactly as it was before the run.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_futures::spawn_local;
use web_sys::{MessageEvent, Worker, WorkerOptions, WorkerType};

use crate::opfs;
use crate::sync_protocol::{RunRequest, WorkerEvent};
use crate::sync_spawn::{RunGate, SyncSession, WorkerPort};

/// The page's worker-backed port.
#[derive(Clone)]
struct WebPort {
    worker: Worker,
}

impl WorkerPort for WebPort {
    fn post(&self, message: &str) -> Result<(), String> {
        self.worker
            .post_message(&JsValue::from_str(message))
            .map(|_| ())
            .map_err(|error| format!("the sync worker refused the message: {error:?}"))
    }

    fn terminate(&self) {
        self.worker.terminate();
    }
}

/// Restoring the gate is the page's post-run reconciliation.
struct PageGate;

impl RunGate for PageGate {
    fn restore(&self) {
        opfs::finish_sync_run();
    }
}

type Session = SyncSession<WebPort, PageGate>;

/// A live sync run. Drop it to stop the worker; the vault gate is restored
/// either way.
pub struct SyncRunHandle {
    worker: Worker,
    session: Rc<RefCell<Option<Session>>>,
    // The closures must outlive the worker's callbacks. Keeping them here also
    // breaks the reference cycle a closure stored on the worker's own port
    // would create.
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(web_sys::Event)>,
}

impl SyncRunHandle {
    /// Ask the run in flight to stop at its next cancel check. See the module
    /// docs: termination is the immediate cancel.
    ///
    /// # Errors
    ///
    /// The worker refused the message (already terminated).
    pub fn cancel(&self) -> Result<(), String> {
        match self.session.borrow().as_ref() {
            Some(session) => session.cancel(),
            None => Ok(()),
        }
    }

    /// Whether the run has not reached a terminal event yet.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.session
            .borrow()
            .as_ref()
            .is_some_and(SyncSession::is_running)
    }
}

impl Drop for SyncRunHandle {
    fn drop(&mut self) {
        // Dropping the handle while a run is in flight is a cancel. Abort the
        // page session first so the vault gate is restored and the flush
        // resumes, then stop the worker. If the run already ended, the gate
        // was restored by its terminal event and abort does nothing.
        if let Some(session) = self.session.borrow_mut().as_mut() {
            session.abort("the sync was stopped before it finished".to_owned());
        }
        self.worker.terminate();
    }
}

/// Start one sync in a dedicated worker.
///
/// The caller owns the returned handle for the run's length. Events are
/// delivered in the order the worker posts them: [`WorkerEvent::Started`] once
/// setup succeeds, then exactly one terminal event. A request that fails
/// validation posts only the terminal `Failed` — nothing reached the provider.
///
/// # Errors
///
/// The vault could not be quiesced (another tab holds its write lock, OPFS is
/// not open, or a flush failed), the worker could not be created, or the run
/// request could not be posted. On any of these the page's flush is left
/// running as before.
pub async fn spawn(
    request: RunRequest,
    on_event: impl FnMut(WorkerEvent) + 'static,
) -> Result<SyncRunHandle, String> {
    opfs::quiesce_for_sync().await?;

    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    let worker = match Worker::new_with_options("./worker.js", &options) {
        Ok(worker) => worker,
        Err(error) => {
            opfs::finish_sync_run();
            return Err(format!("the sync worker could not be created: {error:?}"));
        }
    };

    let session: Rc<RefCell<Option<Session>>> = Rc::new(RefCell::new(None));

    // The worker's messages. A non-string payload is handed to `receive` as an
    // empty string so the session reports the contract violation rather than
    // silently ignoring the run's only answer.
    let on_message = {
        let session = Rc::clone(&session);
        Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let data = event.data();
            let raw = data.as_string().unwrap_or_default();
            if let Some(session) = session.borrow_mut().as_mut() {
                // `receive` restores the gate itself on a protocol error.
                let _ = session.receive(&raw);
            }
        })
    };
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

    // A worker that fails to load or dies mid-run never posts an event. Report
    // it as a Failed event so the page is not left quiesced.
    let on_error = {
        let session = Rc::clone(&session);
        Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let detail = format!(
                "the sync worker stopped unexpectedly (a {} event)",
                event.type_()
            );
            if let Some(session) = session.borrow_mut().as_mut() {
                session.abort(detail);
            }
        })
    };
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));

    match SyncSession::start(
        WebPort {
            worker: worker.clone(),
        },
        PageGate,
        request,
        Box::new(on_event),
    ) {
        Ok(started) => {
            *session.borrow_mut() = Some(started);
            Ok(SyncRunHandle {
                worker,
                session,
                _on_message: on_message,
                _on_error: on_error,
            })
        }
        Err(error) => {
            // `start` already restored the gate.
            worker.terminate();
            Err(error)
        }
    }
}

/// The console gate's one-call entry: parse a `RunRequest` JSON, run it, and
/// resolve with the JSON array of every event, in order.
///
/// This is a testing seam, not the UI API: slice 8b calls [`spawn`] and draws
/// from its callback. The promise resolves on the terminal event; the page's
/// post-run OPFS re-read continues in the background, so a caller that wants
/// to inspect OPFS after a completed run should yield first (the gate's
/// unauthenticated run changes nothing, so this only matters for a real pull).
///
/// ```js
/// const events = await muninnSyncRun(JSON.stringify({ kind: "run",
///   binding_id: "…", operation_id: "…", owner: "…", repo: "…",
///   branch: "main", token: "not-a-pat" }));
/// ```
#[wasm_bindgen(js_name = muninnSyncRun)]
pub fn muninn_sync_run(request_json: String) -> js_sys::Promise {
    js_sys::Promise::new(&mut |resolve, reject| {
        // The executor is `FnMut`; clone the one-shot input into the task.
        let request_json = request_json.clone();
        spawn_local(async move {
            let request = match serde_json::from_str::<RunRequest>(&request_json) {
                Ok(request) => request,
                Err(error) => {
                    let detail = format!("the sync request is not valid JSON: {error}");
                    let _ = reject.call1(&JsValue::UNDEFINED, &JsValue::from_str(&detail));
                    return;
                }
            };
            let events: Rc<RefCell<Vec<WorkerEvent>>> = Rc::new(RefCell::new(Vec::new()));
            let collected = Rc::clone(&events);
            let handle = match spawn(request, move |event| collected.borrow_mut().push(event)).await
            {
                Ok(handle) => handle,
                Err(error) => {
                    let _ = reject.call1(&JsValue::UNDEFINED, &JsValue::from_str(&error));
                    return;
                }
            };
            // Let the page's event loop deliver the worker's messages. The
            // closure above appends each one before the session marks itself
            // terminal, so the vec is complete when the loop ends.
            while handle.is_running() {
                next_task().await;
            }
            drop(handle);
            let json = serde_json::to_string(&*events.borrow()).unwrap_or_else(|_| "[]".to_owned());
            let _ = resolve.call1(&JsValue::UNDEFINED, &JsValue::from_str(&json));
        });
    })
}

/// Yield to the browser's task queue once, so worker messages are delivered.
async fn next_task() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let Some(set_timeout) =
            js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("setTimeout"))
                .ok()
                .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
        else {
            let _ = resolve.call0(&JsValue::UNDEFINED);
            return;
        };
        let _ = set_timeout.call2(&JsValue::UNDEFINED, &resolve, &JsValue::from_f64(0.0));
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

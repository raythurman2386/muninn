//! The page side of one sync run: dispatch, relay and the flush gate (#240
//! slice 8a).
//!
//! The browser sync runs in a dedicated worker (the `page_sync` module holds the
//! wasm glue that spawns it). This module is the part that can be tested
//! natively: it encodes the [`RunRequest`] and holds it until the worker's
//! `Ready` handshake, posts it once, relays every run [`WorkerEvent`] the
//! worker posts to the caller, and restores the page's vault flush exactly
//! once when the run reaches a terminal event.
//!
//! # The ready handshake (a real race the browser forces)
//!
//! A dedicated worker's `onmessage` is installed at the end of the shim's
//! top-level `await init()`. A message posted between `new Worker(...)` and
//! that point is dispatched with no listener and **dropped** — it is not
//! queued for a later handler. Posting eagerly therefore hangs the caller
//! waiting for an event that can never arrive. [`SyncSession`] never posts a
//! run before [`WorkerEvent::Ready`]; the fake port used by the tests models
//! the browser and would record (and drop) such an eager message, so the race
//! is pinned natively.
//!
//! # Why a gate
//!
//! The main thread's [`crate::OpfsStore`] and the worker's vault store write
//! the same OPFS directory, and an OPFS sync access handle is exclusive per
//! file **across contexts**: a worker that opens one while the page is writing
//! gets an error, and the page would otherwise flush a stale mirror over the
//! worker's result. The page therefore quiesces its vault flush before the
//! worker starts, and after the terminal event re-reads OPFS into its mirror
//! (keeping any edit made while the run was in flight) before flushing again.
//! [`RunGate::restore`] is that second half. The quiesce half is async — it
//! waits for an in-flight flush and drains the dirty set — so it belongs to
//! the wasm glue, not this state machine.
//!
//! # Failure never hangs the page
//!
//! Every way a run can end restores the gate: a terminal event, a message the
//! contract cannot parse, a worker `error` event ([`SyncSession::abort`]), a
//! failed post after `Ready`, or a cancel before the handshake. The tests
//! below pin each one.

use crate::sync_protocol::{RunRequest, WorkerEvent, WorkerRequest};
use std::fmt;

/// The worker, as this state machine needs it.
///
/// The wasm implementation wraps `web_sys::Worker`; the tests use a recording
/// fake, so the relay logic does not need a browser.
pub trait WorkerPort {
    /// Post one already-serialized message to the worker.
    ///
    /// # Errors
    ///
    /// The browser refused the message (a terminated worker, a clone error).
    fn post(&self, message: &str) -> Result<(), String>;

    /// Stop the worker immediately. Used for a cancel before the handshake and
    /// for dropping a handle; a run blocked in synchronous XHR can only be
    /// stopped this way. Safe because the worker stages vault writes in memory
    /// until the run ends.
    fn terminate(&self);
}

/// The page's vault-flush gate.
///
/// The wasm implementation starts the post-run OPFS re-read and only then
/// lets the poll loop flush again; dropping it without a restore would leave
/// the page's vault permanently unflushed.
pub trait RunGate {
    /// The worker is no longer running: reconcile the page's mirror with OPFS
    /// and resume the flush. Called exactly once per session.
    fn restore(&self);
}

/// One sync run's page-side state.
pub struct SyncSession<P: WorkerPort, G: RunGate> {
    port: P,
    gate: G,
    on_event: Box<dyn FnMut(WorkerEvent)>,
    /// The encoded `Run` request, held until the worker's `Ready` handshake.
    ///
    /// A dedicated worker discards a message that arrives before its
    /// `onmessage` handler is installed (the handler is registered at the end
    /// of the shim's `await init()`), so posting immediately after
    /// `new Worker(...)` is a real, silent loss. Holding the request until the
    /// worker says it can receive is the fix.
    pending: Option<String>,
    running: bool,
    ready: bool,
}

impl<P: WorkerPort, G: RunGate> SyncSession<P, G> {
    /// Prepare the run against a worker whose starts are already quiesced.
    ///
    /// The request is **encoded and held**, not posted: the session posts it
    /// when the worker's [`WorkerEvent::Ready`] arrives. The caller must have
    /// quiesced the page's vault flush before calling this. A serialization
    /// failure restores the gate, so a failed spawn cannot strand the page.
    ///
    /// # Errors
    ///
    /// The run request could not be encoded.
    pub fn start(
        port: P,
        gate: G,
        request: RunRequest,
        on_event: Box<dyn FnMut(WorkerEvent)>,
    ) -> Result<SyncSession<P, G>, String> {
        let message = match serde_json::to_string(&WorkerRequest::Run(request)) {
            Ok(message) => message,
            Err(error) => {
                gate.restore();
                return Err(format!("the sync request could not be encoded: {error}"));
            }
        };
        Ok(SyncSession {
            port,
            gate,
            on_event,
            pending: Some(message),
            running: true,
            ready: false,
        })
    }

    /// Relay one raw worker message.
    ///
    /// [`WorkerEvent::Ready`] is the handshake: it is consumed here (the
    /// caller's callback never sees it) and releases the held request exactly
    /// once. A second `Ready` is ignored rather than re-sending the PAT. A
    /// message that is not a [`WorkerEvent`] is reported to the caller as a
    /// `Failed` event before the error is returned: the worker is not speaking
    /// the contract, and leaving the page quiesced would strand its vault.
    ///
    /// # Errors
    ///
    /// The raw message was not a [`WorkerEvent`]. The session is terminal in
    /// that case; the gate has been restored.
    pub fn receive(&mut self, raw: &str) -> Result<(), String> {
        match serde_json::from_str::<WorkerEvent>(raw) {
            Ok(WorkerEvent::Ready) => {
                self.arrive();
                Ok(())
            }
            Ok(event) => {
                self.deliver(event);
                Ok(())
            }
            Err(error) => {
                let detail =
                    format!("the sync worker sent a message that is not an event: {error}");
                self.deliver(WorkerEvent::Failed {
                    code: "protocol".to_owned(),
                    detail: detail.clone(),
                });
                Err(detail)
            }
        }
    }

    /// End the session from the page's side (a worker `error` event, a load
    /// failure): report a `Failed` event, restore the gate and stop.
    pub fn abort(&mut self, detail: String) {
        if self.running {
            self.deliver(WorkerEvent::Failed {
                code: "worker".to_owned(),
                detail,
            });
        }
    }

    /// Ask the run to cancel.
    ///
    /// Before the handshake nothing has been sent, so there is nothing to ask:
    /// the worker is terminated and the caller gets a terminal `cancelled`
    /// event, which also restores the gate. After the handshake this posts
    /// `WorkerRequest::Cancel`; see the module docs for why a run already
    /// blocked in synchronous XHR cannot read it (termination is the immediate
    /// cancel, and it is safe because the worker stages every vault write in
    /// memory until the run ends).
    ///
    /// # Errors
    ///
    /// The port refused the cancel message.
    pub fn cancel(&mut self) -> Result<(), String> {
        if !self.running {
            return Ok(());
        }
        if !self.ready {
            self.port.terminate();
            self.deliver(WorkerEvent::Failed {
                code: "cancelled".to_owned(),
                detail: "the sync was cancelled before it started".to_owned(),
            });
            return Ok(());
        }
        let message = serde_json::to_string(&WorkerRequest::Cancel)
            .map_err(|error| format!("the cancel message could not be encoded: {error}"))?;
        self.port.post(&message)
    }

    /// Whether a run is still in flight (no terminal event yet). True while
    /// the request is still held for the handshake.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// The worker says its handler is installed: post the held run exactly
    /// once.
    fn arrive(&mut self) {
        if self.ready || !self.running {
            return;
        }
        self.ready = true;
        let Some(message) = self.pending.take() else {
            return;
        };
        if let Err(detail) = self.port.post(&message) {
            // The worker cannot take the run; report it as the terminal event
            // so the caller is not waiting on a post that never happened.
            self.deliver(WorkerEvent::Failed {
                code: "worker".to_owned(),
                detail,
            });
        }
    }

    fn deliver(&mut self, event: WorkerEvent) {
        let terminal = is_terminal(&event);
        (self.on_event)(event);
        if terminal && self.running {
            self.running = false;
            // The serialized request holds the PAT; drop it with the run.
            self.pending = None;
            self.gate.restore();
        }
    }
}

impl<P: WorkerPort, G: RunGate> fmt::Debug for SyncSession<P, G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncSession")
            .field("running", &self.running)
            .field("ready", &self.ready)
            .finish_non_exhaustive()
    }
}

/// Exactly one of `Completed`, `Conflicts` and `Failed` ends a run. Spelled
/// exhaustively so a future event variant must decide this question.
fn is_terminal(event: &WorkerEvent) -> bool {
    match event {
        WorkerEvent::Ready | WorkerEvent::Started => false,
        WorkerEvent::Completed { .. }
        | WorkerEvent::Conflicts { .. }
        | WorkerEvent::Failed { .. } => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// A worker port that behaves like the browser: a message posted before
    /// the worker's `onmessage` handler exists is **dropped**, not queued.
    /// `ready` is the fake worker's own capability flag; the tests set it when
    /// the fake is about to deliver `WorkerEvent::Ready`.
    #[derive(Default)]
    struct Recorder {
        ready: Cell<bool>,
        posted: RefCell<Vec<String>>,
        dropped: RefCell<Vec<String>>,
        terminated: RefCell<u32>,
        fail_post: RefCell<bool>,
        restores: Rc<RefCell<u32>>,
        delivered: Rc<RefCell<Vec<WorkerEvent>>>,
    }

    impl WorkerPort for Rc<Recorder> {
        fn post(&self, message: &str) -> Result<(), String> {
            if *self.fail_post.borrow() {
                return Err("the worker is gone".to_owned());
            }
            if !self.ready.get() {
                self.dropped.borrow_mut().push(message.to_owned());
                return Ok(());
            }
            self.posted.borrow_mut().push(message.to_owned());
            Ok(())
        }

        fn terminate(&self) {
            *self.terminated.borrow_mut() += 1;
        }
    }

    impl RunGate for Rc<Recorder> {
        fn restore(&self) {
            *self.restores.borrow_mut() += 1;
        }
    }

    fn request() -> RunRequest {
        RunRequest {
            binding_id: "0123456789abcdef".to_owned(),
            operation_id: "fedcba9876543210".to_owned(),
            owner: "octocat".to_owned(),
            repo: "notes".to_owned(),
            branch: "main".to_owned(),
            prefix: "vault".to_owned(),
            token: "github_pat_example".to_owned(),
            keep_both: true,
        }
    }

    fn recorder() -> Rc<Recorder> {
        Rc::new(Recorder::default())
    }

    type Session = SyncSession<Rc<Recorder>, Rc<Recorder>>;

    fn session(port: Rc<Recorder>) -> Session {
        let delivered = Rc::clone(&port.delivered);
        let on_event = Box::new(move |event| delivered.borrow_mut().push(event));
        SyncSession::start(Rc::clone(&port), Rc::clone(&port), request(), on_event)
            .expect("the fake port accepts the request")
    }

    fn raw(event: &WorkerEvent) -> String {
        serde_json::to_string(event).unwrap()
    }

    /// Deliver the worker's boot handshake: the fake becomes able to receive,
    /// then the session sees `Ready`.
    fn handshake(port: &Rc<Recorder>, session: &mut Session) {
        port.ready.set(true);
        session.receive(&raw(&WorkerEvent::Ready)).expect("ready");
    }

    #[test]
    fn start_holds_the_request_and_posts_nothing() {
        let port = recorder();
        let session = session(Rc::clone(&port));
        assert!(
            port.posted.borrow().is_empty(),
            "nothing may be posted before the worker is ready"
        );
        assert!(
            port.dropped.borrow().is_empty(),
            "an implementation that posted now would be dropped by the browser;              the session must not even attempt it"
        );
        assert!(session.is_running(), "the run is pending the handshake");
        assert_eq!(*port.restores.borrow(), 0);
    }

    #[test]
    fn ready_unlocks_one_request_after_the_handshake_and_is_not_relayed() {
        let port = recorder();
        let mut session = session(Rc::clone(&port));
        handshake(&port, &mut session);

        let posted = port.posted.borrow();
        assert_eq!(posted.len(), 1, "the handshake releases the held request");
        assert_eq!(
            serde_json::from_str::<WorkerRequest>(&posted[0]).unwrap(),
            WorkerRequest::Run(request())
        );
        assert!(
            port.dropped.borrow().is_empty(),
            "the request went out after the worker was ready"
        );
        assert!(
            port.delivered.borrow().is_empty(),
            "Ready is the spawn API's handshake, not a run event for the caller"
        );
        assert!(session.is_running());
        assert_eq!(*port.restores.borrow(), 0);
    }

    #[test]
    fn a_second_ready_does_not_re_send_the_pat() {
        let port = recorder();
        let mut session = session(Rc::clone(&port));
        handshake(&port, &mut session);
        session.receive(&raw(&WorkerEvent::Ready)).unwrap();
        assert_eq!(port.posted.borrow().len(), 1);
    }

    #[test]
    fn events_are_relayed_in_order_and_only_the_terminal_one_restores() {
        let port = recorder();
        let mut session = session(Rc::clone(&port));
        handshake(&port, &mut session);
        let started = WorkerEvent::Started;
        let completed = WorkerEvent::Completed {
            head: "abc123".to_owned(),
            exclusions: vec![".git".to_owned()],
            conflict_copies: Vec::new(),
        };
        session.receive(&raw(&started)).unwrap();
        assert_eq!(*port.restores.borrow(), 0);
        assert!(session.is_running());
        session.receive(&raw(&completed)).unwrap();
        assert_eq!(*port.restores.borrow(), 1);
        assert!(!session.is_running());
        assert_eq!(*port.delivered.borrow(), vec![started, completed]);
        // A late duplicate must not restore a second time.
        session
            .receive(&raw(&WorkerEvent::Failed {
                code: "auth".to_owned(),
                detail: "no".to_owned(),
            }))
            .unwrap();
        assert_eq!(*port.restores.borrow(), 1, "one gate restore per session");
    }

    #[test]
    fn a_post_failure_when_ready_arrives_restores_the_gate_and_reports_it() {
        let port = recorder();
        let mut session = session(Rc::clone(&port));
        *port.fail_post.borrow_mut() = true;
        handshake(&port, &mut session);
        assert_eq!(*port.restores.borrow(), 1);
        assert!(!session.is_running());
        let delivered = port.delivered.borrow();
        assert!(matches!(
            delivered.last(),
            Some(WorkerEvent::Failed { code, .. }) if code == "worker"
        ));
    }

    #[test]
    fn a_malformed_message_is_reported_as_a_failure_and_ends_the_session() {
        let port = recorder();
        let mut session = session(Rc::clone(&port));
        let error = session.receive("{\"kind\":\"surprise\"}").unwrap_err();
        assert!(error.contains("not an event"));
        assert_eq!(*port.restores.borrow(), 1);
        assert!(!session.is_running());
        let delivered = port.delivered.borrow();
        assert!(matches!(
            delivered.last(),
            Some(WorkerEvent::Failed { code, .. }) if code == "protocol"
        ));
    }

    #[test]
    fn abort_relays_a_worker_failure_once_and_restores() {
        let port = recorder();
        let mut session = session(Rc::clone(&port));
        session.abort("the worker crashed".to_owned());
        session.abort("a second error must not replay".to_owned());
        assert_eq!(*port.restores.borrow(), 1);
        assert_eq!(port.delivered.borrow().len(), 1);
        assert!(!session.is_running());
    }

    #[test]
    fn cancelling_before_ready_terminates_and_reports_a_cancelled_run() {
        let port = recorder();
        let mut session = session(Rc::clone(&port));
        session.cancel().unwrap();
        assert_eq!(*port.terminated.borrow(), 1, "the idle worker is stopped");
        assert!(
            port.posted.borrow().is_empty() && port.dropped.borrow().is_empty(),
            "a cancel must not send the held run"
        );
        assert_eq!(*port.restores.borrow(), 1);
        assert!(!session.is_running());
        let delivered = port.delivered.borrow();
        assert!(matches!(
            delivered.last(),
            Some(WorkerEvent::Failed { code, .. }) if code == "cancelled"
        ));
    }

    #[test]
    fn cancelling_after_ready_posts_the_cancel_message() {
        let port = recorder();
        let mut session = session(Rc::clone(&port));
        handshake(&port, &mut session);
        session.cancel().unwrap();
        let posted = port.posted.borrow();
        assert_eq!(posted.len(), 2);
        assert_eq!(
            serde_json::from_str::<WorkerRequest>(&posted[1]).unwrap(),
            WorkerRequest::Cancel
        );
        assert_eq!(*port.restores.borrow(), 0, "cancel is not a completion");
        assert_eq!(*port.terminated.borrow(), 0);
    }
}

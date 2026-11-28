//! The page side of one sync run: dispatch, relay and the flush gate (#240
//! slice 8a).
//!
//! The browser sync runs in a dedicated worker (the `page_sync` module holds the
//! wasm glue that spawns it). This module is the part that can be tested
//! natively: it turns a [`RunRequest`] into the worker's first message, relays
//! every [`WorkerEvent`] the worker posts to the caller, and restores the
//! page's vault flush exactly once when the run reaches a terminal event.
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
//! contract cannot parse, a worker `error` event ([`SyncSession::abort`]), or
//! the initial `postMessage` failing. The tests below pin each one.

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

    /// Stop the worker immediately. The page uses this after a terminal event
    /// so a finished worker does not linger, and as the only immediate cancel
    /// for a run blocked in synchronous XHR.
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
    running: bool,
}

impl<P: WorkerPort, G: RunGate> SyncSession<P, G> {
    /// Send the run to a worker whose starts are already quiesced, and wrap
    /// the port and gate for the response relay.
    ///
    /// The caller must have quiesced the page's vault flush before calling
    /// this. If the first message cannot be posted the gate is restored here,
    /// so a failed spawn cannot strand the page.
    ///
    /// # Errors
    ///
    /// A serialization failure or the port's own post error.
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
        if let Err(error) = port.post(&message) {
            gate.restore();
            return Err(error);
        }
        Ok(SyncSession {
            port,
            gate,
            on_event,
            running: true,
        })
    }

    /// Relay one raw worker message.
    ///
    /// A message that is not a [`WorkerEvent`] is reported to the caller as a
    /// `Failed` event before the error is returned: the worker is not speaking
    /// the contract, and leaving the page quiesced would strand its vault.
    ///
    /// # Errors
    ///
    /// The raw message was not a [`WorkerEvent`]. The session is terminal in
    /// that case; the gate has been restored.
    pub fn receive(&mut self, raw: &str) -> Result<(), String> {
        match serde_json::from_str::<WorkerEvent>(raw) {
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

    /// Ask the worker to cancel. See the module docs: a run already blocked in
    /// synchronous XHR cannot read this message until it returns; terminating
    /// the worker is the immediate cancel, and it is safe because the worker
    /// stages every vault write in memory until the run ends.
    ///
    /// # Errors
    ///
    /// The port refused the message.
    pub fn cancel(&self) -> Result<(), String> {
        let message = serde_json::to_string(&WorkerRequest::Cancel)
            .map_err(|error| format!("the cancel message could not be encoded: {error}"))?;
        self.port.post(&message)
    }

    /// Whether a run is still in flight (no terminal event yet).
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }

    fn deliver(&mut self, event: WorkerEvent) {
        let terminal = is_terminal(&event);
        (self.on_event)(event);
        if terminal && self.running {
            self.running = false;
            self.gate.restore();
        }
    }
}

impl<P: WorkerPort, G: RunGate> fmt::Debug for SyncSession<P, G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncSession")
            .field("running", &self.running)
            .finish_non_exhaustive()
    }
}

/// Exactly one of `Completed`, `Conflicts` and `Failed` ends a run. Spelled
/// exhaustively so a future event variant must decide this question.
fn is_terminal(event: &WorkerEvent) -> bool {
    match event {
        WorkerEvent::Started => false,
        WorkerEvent::Completed { .. }
        | WorkerEvent::Conflicts { .. }
        | WorkerEvent::Failed { .. } => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct Recorder {
        posted: RefCell<Vec<String>>,
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

    fn session(port: Rc<Recorder>) -> SyncSession<Rc<Recorder>, Rc<Recorder>> {
        let delivered = Rc::clone(&port.delivered);
        let on_event = Box::new(move |event| delivered.borrow_mut().push(event));
        SyncSession::start(Rc::clone(&port), Rc::clone(&port), request(), on_event)
            .expect("the fake port accepts the request")
    }

    fn raw(event: &WorkerEvent) -> String {
        serde_json::to_string(event).unwrap()
    }

    #[test]
    fn start_posts_one_run_message_and_does_not_restore() {
        let port = recorder();
        let session = session(Rc::clone(&port));
        let posted = port.posted.borrow();
        assert_eq!(posted.len(), 1);
        let decoded: WorkerRequest = serde_json::from_str(&posted[0]).unwrap();
        assert_eq!(decoded, WorkerRequest::Run(request()));
        assert_eq!(*port.restores.borrow(), 0, "the run is still in flight");
        assert!(session.is_running());
    }

    #[test]
    fn events_are_relayed_in_order_and_only_the_terminal_one_restores() {
        let port = recorder();
        let mut session = session(Rc::clone(&port));
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
    fn a_post_failure_restores_the_gate() {
        let port = recorder();
        *port.fail_post.borrow_mut() = true;
        let delivered = Rc::clone(&port.delivered);
        let result = SyncSession::start(
            Rc::clone(&port),
            Rc::clone(&port),
            request(),
            Box::new(move |event| delivered.borrow_mut().push(event)),
        );
        assert!(result.is_err());
        assert_eq!(*port.restores.borrow(), 1);
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
    fn cancel_posts_the_cancel_message() {
        let port = recorder();
        let session = session(Rc::clone(&port));
        session.cancel().unwrap();
        let posted = port.posted.borrow();
        assert_eq!(posted.len(), 2);
        assert_eq!(
            serde_json::from_str::<WorkerRequest>(&posted[1]).unwrap(),
            WorkerRequest::Cancel
        );
        assert_eq!(*port.restores.borrow(), 0, "cancel is not a completion");
    }
}

//! The engine's result → the page's event (#240 slice 8a).
//!
//! The worker posts exactly one terminal [`WorkerEvent`] per run. This module
//! is the mapping, kept free of browser types so the shapes are pinned
//! natively: the page's Sync surface (slice 8b) draws from these fields, so a
//! silent rename here would be a UI regression rather than a compile error.
//!
//! `Failed.code` is `SyncError::code` — the stable machine string the native
//! app already keys on — and `Failed.detail` is the error's `Display`, which
//! may carry the provider's text. The worker never constructs a code of its
//! own for an engine failure: a fake class would tell 8b a different story
//! than the desktop.

use muninn_sync::{SyncError, SyncReport};
use muninn_web::sync_protocol::WorkerEvent;

/// Map one engine outcome onto the worker's terminal event.
#[must_use]
pub fn outcome_event(outcome: Result<SyncReport, SyncError>) -> WorkerEvent {
    match outcome {
        Ok(SyncReport::Succeeded {
            head,
            exclusions,
            conflict_copies,
        }) => WorkerEvent::Completed {
            head,
            exclusions: exclusions.into_iter().map(|entry| entry.path).collect(),
            conflict_copies: conflict_copies
                .into_iter()
                .map(|path| path.to_string())
                .collect(),
        },
        Ok(SyncReport::Conflicts { conflicts }) => WorkerEvent::Conflicts {
            paths: conflicts
                .into_iter()
                .map(|conflict| conflict.path.to_string())
                .collect(),
        },
        Err(error) => failed_event(&error),
    }
}

/// The `Failed` event for an engine error, using the engine's own code and
/// rendering.
#[must_use]
pub fn failed_event(error: &SyncError) -> WorkerEvent {
    WorkerEvent::Failed {
        code: error.code().to_owned(),
        detail: error.to_string(),
    }
}

/// The event for a run whose *engine* outcome was fine but whose local result
/// did not reach OPFS.
///
/// The engine may already have published the commit, so reporting success
/// would tell the person the vault matches the remote when the worker is
/// about to exit without having written those bytes. This reports the local
/// half honestly; the next run re-reads OPFS and replans.
#[must_use]
pub fn not_durable_event(step: &str, detail: &str) -> WorkerEvent {
    WorkerEvent::Failed {
        code: "io".to_owned(),
        detail: format!("the sync finished but the {step} could not be saved: {detail}"),
    }
}

/// The one event the worker posts, given the engine's outcome and the two
/// flushes.
///
/// A failed engine run keeps its own code and rendering: learning that a run
/// was rejected for a bad PAT is more useful than learning that the local
/// flush also failed, and the next run retries the local side anyway. Only a
/// run the engine considered finished can be downgraded to `io` by a lost
/// local write.
#[must_use]
pub fn final_event(
    outcome: WorkerEvent,
    vault_flush: &Result<(), String>,
    state_flush: &Result<(), String>,
) -> WorkerEvent {
    let not_durable = match (vault_flush, state_flush) {
        (Err(detail), _) => Some(("vault", detail.as_str())),
        (Ok(()), Err(detail)) => Some(("sync state", detail.as_str())),
        _ => None,
    };
    match (outcome, not_durable) {
        (failed @ WorkerEvent::Failed { .. }, _) => failed,
        (_, Some((step, detail))) => not_durable_event(step, detail),
        (event, None) => event,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_sync::{Conflict, ConflictKind, RelativePath, SyncError};

    fn path(text: &str) -> RelativePath {
        RelativePath::parse(text).unwrap()
    }

    #[test]
    fn a_success_carries_the_head_exclusions_and_conflict_copies() {
        // A real inventory supplies the exclusions, so the test does not need
        // the crate-private `Exclusion` constructor.
        let store = muninn_core::MemoryStore::new("/mem");
        store.put_file(".git/config", b"x");
        store.put_file("Notes.md", b"# Notes\n");
        let inventory = muninn_sync::inventory_with(&store, muninn_sync::DEFAULT_LIMITS).unwrap();
        assert_eq!(inventory.exclusions.len(), 1, "the vault's `.git`");
        let event = outcome_event(Ok(SyncReport::Succeeded {
            head: "abc123".to_owned(),
            exclusions: inventory.exclusions,
            conflict_copies: vec![path("Notes/One (remote).md")],
        }));
        assert_eq!(
            event,
            WorkerEvent::Completed {
                head: "abc123".to_owned(),
                exclusions: vec![".git".to_owned()],
                conflict_copies: vec!["Notes/One (remote).md".to_owned()],
            }
        );
    }

    #[test]
    fn a_conflict_stop_lists_the_paths() {
        let event = outcome_event(Ok(SyncReport::Conflicts {
            conflicts: vec![Conflict {
                path: path("Notes/One.md"),
                kind: ConflictKind::EditEdit,
                base: None,
                local: None,
                remote: None,
                other_path: None,
            }],
        }));
        assert_eq!(
            event,
            WorkerEvent::Conflicts {
                paths: vec!["Notes/One.md".to_owned()]
            }
        );
    }

    #[test]
    fn an_engine_error_keeps_the_engines_code_and_rendering() {
        let error = SyncError::Auth {
            detail: "Bad credentials".to_owned(),
        };
        let event = outcome_event(Err(error.clone()));
        let WorkerEvent::Failed { code, detail } = event else {
            panic!("expected a failure");
        };
        assert_eq!(code, "auth");
        assert_eq!(detail, error.to_string());
    }

    #[test]
    fn a_lost_local_write_is_a_failure_not_a_success() {
        let WorkerEvent::Failed { code, detail } =
            not_durable_event("vault", "a sync access handle was refused")
        else {
            panic!("expected a failure");
        };
        assert_eq!(code, "io");
        assert!(detail.contains("vault"));
    }

    #[test]
    fn a_finished_run_is_downgraded_when_a_flush_fails_but_an_engine_failure_is_kept() {
        let completed = WorkerEvent::Completed {
            head: "abc".to_owned(),
            exclusions: Vec::new(),
            conflict_copies: Vec::new(),
        };
        let ok = Ok(());
        let refused = || Err::<(), String>("OPFS refused a handle".to_owned());

        assert_eq!(
            final_event(completed.clone(), &refused(), &ok),
            WorkerEvent::Failed {
                code: "io".to_owned(),
                detail: "the sync finished but the vault could not be saved: OPFS refused a handle"
                    .to_owned(),
            }
        );
        assert_eq!(
            final_event(completed.clone(), &ok, &refused()),
            WorkerEvent::Failed {
                code: "io".to_owned(),
                detail:
                    "the sync finished but the sync state could not be saved: OPFS refused a handle"
                        .to_owned(),
            }
        );
        assert_eq!(final_event(completed.clone(), &ok, &ok), completed);

        let auth = WorkerEvent::Failed {
            code: "auth".to_owned(),
            detail: "Authentication needed".to_owned(),
        };
        assert_eq!(
            final_event(auth.clone(), &refused(), &refused()),
            auth,
            "the engine's failure is the one the page draws"
        );
    }
}

//! The engine's result → the page's event (#240 slice 8a).
//!
//! The worker posts exactly one terminal [`WorkerEvent`] per run. This module
//! is the mapping, kept free of browser types so the shapes are pinned
//! natively: the page's Sync surface (slice 8b) draws from these fields, so a
//! silent rename here would be a UI regression rather than a compile error.
//!
//! `Failed.code` is `SyncError::code` — the stable machine string the native
//! app already keys on. The worker never constructs a code of its own for an
//! engine failure: a fake class would tell 8b a different story than the
//! desktop.
//!
//! # `Failed.detail`: the inner detail, not the `Display` rendering
//!
//! The page maps `code` to a catalogued phrase and interpolates `detail` as
//! data, exactly as the desktop's `sync_ui::sync_error_text` interpolates the
//! variant's field. The desktop passes the **raw inner detail** (`Auth {
//! detail }` → `sync_authentication_needed(detail)`), so the worker must too;
//! passing `Display` would double the catalogue's prefix ("Authentication
//! needed: Authentication needed: …"). [`page_detail`] therefore extracts the
//! same field the desktop's mapping passes for every code whose copy has a
//! detail slot: `offline`, `auth`, `permission`, `scope`, `not_found`,
//! `remote_changed`, `outcome_unknown`, `listing_incomplete`, `limit`,
//! `credentials`, `dirty_buffer`, `local_changed`, `config` and `io`.
//!
//! Codes whose page copy has no detail slot (`timeout`, `cancelled`,
//! `rate_limited`, `lease_busy`, and the page's own `worker`/`busy`) carry the
//! `Display` rendering; the page ignores it. The developer-facing composites
//! the desktop renders raw (`path`, `inventory`, `journal`, `protocol`,
//! `injected_fault`) and the ones with no single inner field (`server`,
//! `unsupported`) also keep `Display`, and the page renders them inside its
//! generic translated frame.

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

/// The `Failed` event for an engine error, using the engine's own code and the
/// detail field the page's copy interpolates.
#[must_use]
pub fn failed_event(error: &SyncError) -> WorkerEvent {
    WorkerEvent::Failed {
        code: error.code().to_owned(),
        detail: page_detail(error),
    }
}

/// The payload `WorkerEvent::Failed.detail` carries for `error`.
///
/// For a code whose catalogued copy takes a detail, this is the same inner
/// field the desktop's `sync_ui::sync_error_text` passes: the variant's
/// `detail`, or its `path` for `dirty_buffer`/`local_changed`. Everything else
/// keeps the engine's `Display` rendering, which the page draws inside its
/// generic frame (or ignores, when its copy has no detail slot). See the
/// module docs for why this is not `Display` everywhere.
#[must_use]
pub fn page_detail(error: &SyncError) -> String {
    match error {
        SyncError::Offline { detail }
        | SyncError::Auth { detail }
        | SyncError::Permission { detail }
        | SyncError::Scope { detail }
        | SyncError::NotFound { detail }
        | SyncError::ConflictRemote { detail }
        | SyncError::OutcomeUnknown { detail }
        | SyncError::ListingIncomplete { detail }
        | SyncError::Limit { detail }
        | SyncError::Credentials { detail }
        | SyncError::Config { detail }
        | SyncError::Io { detail } => detail.clone(),
        // The page's copy takes the path, as the desktop's does.
        SyncError::DirtyBuffer { path } | SyncError::LocalChanged { path } => path.clone(),
        // No single inner field the page interpolates, or no detail slot at
        // all: the rendering is the honest payload and the page frames it.
        _ => error.to_string(),
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
    fn an_engine_error_carries_the_code_and_the_pages_inner_detail() {
        let error = SyncError::Auth {
            detail: "Bad credentials".to_owned(),
        };
        let event = outcome_event(Err(error.clone()));
        let WorkerEvent::Failed { code, detail } = event else {
            panic!("expected a failure");
        };
        assert_eq!(code, "auth");
        assert_eq!(
            detail, "Bad credentials",
            "the page's copy owns the prefix; the detail is the raw field"
        );
        assert_ne!(
            detail,
            error.to_string(),
            "Display would double the catalogued prefix"
        );
    }

    #[test]
    fn every_interpolated_code_carries_the_raw_inner_field() {
        let detail = "the provider's own text";
        for error in [
            SyncError::Offline {
                detail: detail.to_owned(),
            },
            SyncError::Auth {
                detail: detail.to_owned(),
            },
            SyncError::Permission {
                detail: detail.to_owned(),
            },
            SyncError::Scope {
                detail: detail.to_owned(),
            },
            SyncError::NotFound {
                detail: detail.to_owned(),
            },
            SyncError::ConflictRemote {
                detail: detail.to_owned(),
            },
            SyncError::OutcomeUnknown {
                detail: detail.to_owned(),
            },
            SyncError::ListingIncomplete {
                detail: detail.to_owned(),
            },
            SyncError::Limit {
                detail: detail.to_owned(),
            },
            SyncError::Credentials {
                detail: detail.to_owned(),
            },
            SyncError::Config {
                detail: detail.to_owned(),
            },
            SyncError::Io {
                detail: detail.to_owned(),
            },
        ] {
            assert_eq!(
                page_detail(&error),
                detail,
                "{} must pass its inner detail, not its Display",
                error.code()
            );
        }
        // The two path-carrying codes interpolate the path, as the desktop's
        // `sync_dirty_buffer`/`sync_local_changed` do.
        assert_eq!(
            page_detail(&SyncError::DirtyBuffer {
                path: "Notes/A.md".to_owned()
            }),
            "Notes/A.md"
        );
        assert_eq!(
            page_detail(&SyncError::LocalChanged {
                path: "Notes/A.md".to_owned()
            }),
            "Notes/A.md"
        );
    }

    #[test]
    fn codes_the_page_does_not_interpolate_keep_the_display_rendering() {
        // No detail slot in the page's copy: the payload is unused, and the
        // rendering is the simplest honest value.
        for error in [
            SyncError::Timeout { mutation: false },
            SyncError::Cancelled,
            SyncError::RateLimited {
                retry_after_ms: Some(1000),
            },
            SyncError::LeaseBusy,
        ] {
            assert_eq!(page_detail(&error), error.to_string(), "{}", error.code());
        }
        // Composites the page renders inside its generic frame.
        for error in [
            SyncError::Server {
                status: 500,
                detail: "upstream".to_owned(),
            },
            SyncError::Unsupported {
                path: "Notes/A.docx".to_owned(),
                detail: "not convertible".to_owned(),
            },
            SyncError::Path {
                detail: "escapes the vault".to_owned(),
            },
            SyncError::Inventory {
                detail: "unreadable".to_owned(),
            },
            SyncError::Journal {
                detail: "corrupt".to_owned(),
            },
            SyncError::Protocol {
                detail: "unexpected body".to_owned(),
            },
            SyncError::InjectedFault {
                at: "test".to_owned(),
            },
        ] {
            assert_eq!(
                page_detail(&error),
                error.to_string(),
                "{} keeps its diagnostic rendering",
                error.code()
            );
        }
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
            detail: "Bad credentials".to_owned(),
        };
        assert_eq!(
            final_event(auth.clone(), &refused(), &refused()),
            auth,
            "the engine's failure is the one the page draws"
        );
    }
}

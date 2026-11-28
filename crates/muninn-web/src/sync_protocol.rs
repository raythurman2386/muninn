//! The message contract between the page and the sync worker (#240 slice 8a).
//!
//! The worker is a separate wasm module, so the only thing the two sides share
//! is a serialized message. These types are the contract: a request carries
//! the binding (owner/repo/branch/prefix), the run ids and the PAT; an event
//! carries the outcome classes the page's Sync surface will draw.
//!
//! The protocol is deliberately free of `muninn-sync` types so it can be
//! tested in a native `cargo test -p muninn-web` without the wasm transport.
//! Parsing a request into `GitHubScope`, `BindingId` and the credential store
//! is the worker's job, and that conversion is where `muninn-sync`'s
//! validation applies.
//!
//! ## What a request cannot carry
//!
//! - **Progress.** The engine reports only completion: it has no progress
//!   callback, and adding one would be a change to the engine. The worker
//!   therefore emits [`WorkerEvent::Started`] and then exactly one terminal
//!   event. A future slice can add steps if the engine grows a hook.
//! - **Vault bytes.** A run reads and writes the vault through the engine's
//!   filesystem path, not through this message. Sending the vault here would
//!   be a second transport for the same bytes and is not part of the design.
//! - **A refresh token.** PAT auth has no refresh token; the worker receives
//!   one access token and keeps it in a run-scoped in-memory store.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One message from the page to the worker.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerRequest {
    /// Run one sync now.
    Run(RunRequest),
    /// Ask a run in flight to stop at its next cancel check. The engine's
    /// `CancelToken` is polled between requests and between files.
    Cancel,
}

/// Everything one sync run needs from the page.
///
/// This type intentionally does not implement `Display`, and its [`Debug`]
/// redacts the token, so an accidental log or panic message cannot leak the
/// PAT.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRequest {
    /// The binding id the page uses in settings; must parse as a
    /// `muninn_sync::BindingId`.
    pub binding_id: String,
    /// The operation id for this run; it is the publish idempotency key, so
    /// the page generates it once per Sync Now and reuses it on retry.
    pub operation_id: String,
    /// `owner/name` of the GitHub repository.
    pub owner: String,
    /// The repository name.
    pub repo: String,
    /// The branch to publish to and read from.
    pub branch: String,
    /// Vault root inside the repository; empty means the repository root.
    #[serde(default)]
    pub prefix: String,
    /// The personal access token. Memory only; the worker's credential store
    /// never writes it to OPFS, settings or a log.
    pub token: String,
    /// Resolve every conflict as "keep both": the local version keeps the
    /// path and the remote one lands beside it. The app's Sync Now always
    /// chooses this, so a note edited on two devices never stops a run.
    #[serde(default = "keep_both_default")]
    pub keep_both: bool,
}

fn keep_both_default() -> bool {
    true
}

impl RunRequest {
    /// Check the fields the worker cannot parse into empty provider scopes.
    ///
    /// The ids are validated by `muninn-sync` when the worker builds its
    /// request; this only rejects the shapes that would make the HTTP calls
    /// meaningless (an empty owner, repo, branch or token).
    ///
    /// # Errors
    ///
    /// A short diagnostic naming the empty field.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("binding id", self.binding_id.as_str()),
            ("operation id", self.operation_id.as_str()),
            ("repository owner", self.owner.as_str()),
            ("repository name", self.repo.as_str()),
            ("branch", self.branch.as_str()),
            ("access token", self.token.as_str()),
        ] {
            if value.is_empty() {
                return Err(format!("the sync request is missing its {name}"));
            }
        }
        if self.owner.contains('/') || self.repo.contains('/') {
            return Err("repository owner and name are separate fields".to_owned());
        }
        Ok(())
    }
}

impl fmt::Debug for RunRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunRequest")
            .field("binding_id", &self.binding_id)
            .field("operation_id", &self.operation_id)
            .field("owner", &self.owner)
            .field("repo", &self.repo)
            .field("branch", &self.branch)
            .field("prefix", &self.prefix)
            .field("token", &"Secret([redacted])")
            .field("keep_both", &self.keep_both)
            .finish()
    }
}

/// One message from the worker back to the page.
///
/// There is no `Progress` variant on purpose: see the module docs. A run
/// produces [`WorkerEvent::Started`] and then exactly one of `Completed`,
/// `Conflicts` or `Failed`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerEvent {
    /// The run is about to make its first provider request.
    Started,
    /// The run converged; `head` is the published commit.
    Completed {
        /// The remote head the local vault now matches.
        head: String,
        /// Vault paths the inventory excluded (dot files, temp files, OS
        /// junk), in the inventory's order.
        exclusions: Vec<String>,
        /// Paths this run wrote as conflict copies (`<stem> (remote)<ext>`).
        conflict_copies: Vec<String>,
    },
    /// The plan found conflicts and the request did not resolve them. With
    /// `keep_both` set this should not happen; it exists so a future UI can
    /// offer per-file choices.
    Conflicts {
        /// Vault paths with a conflict.
        paths: Vec<String>,
    },
    /// The run stopped. `code` is a stable machine string; `detail` is the
    /// engine's diagnostic rendering, which 8b maps to catalog copy.
    Failed {
        /// Stable failure class, e.g. `auth` or `rate_limited`.
        code: String,
        /// The engine's `Display` rendering, including the provider's text.
        detail: String,
    },
}

// There is deliberately no constructor for `WorkerEvent::Failed` here: the
// worker fills `code` from `muninn_sync::SyncError::code` and `detail` from
// the error's `Display`. Keeping the protocol free of `muninn-sync` types is
// what lets `cargo test -p muninn-web` run natively.
#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_request_round_trips_through_json() {
        let request = request();
        let encoded = serde_json::to_string(&WorkerRequest::Run(request.clone())).unwrap();
        let decoded: WorkerRequest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, WorkerRequest::Run(request));
        assert!(encoded.contains("\"kind\":\"run\""));
    }

    #[test]
    fn an_event_round_trips_through_json() {
        let events = [
            WorkerEvent::Started,
            WorkerEvent::Completed {
                head: "abc123".to_owned(),
                exclusions: vec![".git".to_owned()],
                conflict_copies: vec!["Notes/a (remote).md".to_owned()],
            },
            WorkerEvent::Conflicts {
                paths: vec!["Notes/a.md".to_owned()],
            },
            WorkerEvent::Failed {
                code: "auth".to_owned(),
                detail: "Authentication needed: bad token".to_owned(),
            },
        ];
        for event in events {
            let encoded = serde_json::to_string(&event).unwrap();
            let decoded: WorkerEvent = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, event);
        }
    }

    #[test]
    fn the_default_resolution_is_keep_both_and_prefix_is_optional() {
        let encoded = serde_json::json!({
            "kind": "run",
            "binding_id": "0123456789abcdef",
            "operation_id": "fedcba9876543210",
            "owner": "octocat",
            "repo": "notes",
            "branch": "main",
            "token": "pat"
        });
        let decoded: WorkerRequest = serde_json::from_value(encoded).unwrap();
        let WorkerRequest::Run(run) = decoded else {
            panic!("expected a run request");
        };
        assert!(run.keep_both);
        assert!(run.prefix.is_empty());
    }

    #[test]
    fn debug_never_reveals_the_token() {
        let request = request();
        let rendered = format!("{request:?}");
        assert!(!rendered.contains("github_pat_example"));
        assert!(rendered.contains("redacted"));
        assert!(rendered.contains("octocat"));
    }

    #[test]
    fn validate_names_the_empty_field() {
        let mut request = request();
        request.token.clear();
        assert_eq!(
            request.validate().unwrap_err(),
            "the sync request is missing its access token"
        );
        request.token = "pat".to_owned();
        request.branch.clear();
        assert!(request.validate().unwrap_err().contains("branch"));
        request.branch = "main".to_owned();
        request.owner = "octo/cat".to_owned();
        assert!(request.validate().is_err(), "owner and repo are separate");
    }
}

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
//!   therefore emits [`WorkerEvent::Ready`] once at boot and then, per run,
//!   [`WorkerEvent::Started`] and exactly one terminal event. A future slice
//!   can add steps if the engine grows a hook.
//! - **Vault bytes.** A run reads and writes the vault through the engine's
//!   filesystem path, not through this message. Sending the vault here would
//!   be a second transport for the same bytes and is not part of the design.
//! - **A refresh token.** GitHub's PAT auth has no refresh token. Drive
//!   (slice 9) may: Google Identity Services can hand the page an access
//!   token, a refresh token and an expiry, so the wire carries all three and
//!   the worker keeps them in a run-scoped in-memory store. A Drive run
//!   without a refresh token is still valid; the engine fails closed with
//!   "connect again" when the access token expires.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Which provider one browser run syncs.
///
/// The browser's Sync tab offers GitHub and Google Drive (slice 9). Tailscale
/// stays a desktop door: a browser cannot reach a tailnet commit store without
/// a relay (#240 slice 10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// A personal access token against a GitHub repository.
    #[default]
    Github,
    /// Google Drive's hidden app data, signed in through GIS.
    Drive,
}

impl Provider {
    /// The provider's `muninn_sync::ProviderKind` spelling, for tests and
    /// diagnostics.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Github => "github",
            Provider::Drive => "google_drive",
        }
    }
}

/// One message from the page to the worker.
///
/// The request variant is a few hundred bytes and crosses the channel once per
/// run; boxing it would change the public wire type for no runtime gain.
#[allow(clippy::large_enum_variant)]
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
    /// Which provider this run syncs. Absent means GitHub, so a slice-8 gate
    /// call keeps working unchanged.
    #[serde(default)]
    pub provider: Provider,
    /// The binding id the page uses in settings; must parse as a
    /// `muninn_sync::BindingId`.
    pub binding_id: String,
    /// The operation id for this run; it is the publish idempotency key, so
    /// the page generates it once per Sync Now and reuses it on retry.
    pub operation_id: String,
    /// `owner/name` of the GitHub repository. Empty for a Drive run.
    #[serde(default)]
    pub owner: String,
    /// The repository name. Empty for a Drive run.
    #[serde(default)]
    pub repo: String,
    /// The branch to publish to and read from. Empty for a Drive run.
    #[serde(default)]
    pub branch: String,
    /// Vault root inside the repository; empty means the repository root.
    #[serde(default)]
    pub prefix: String,
    /// The Drive store name inside the app's hidden `appDataFolder`. Empty for
    /// a GitHub run.
    #[serde(default)]
    pub drive_store: String,
    /// The access token (GitHub PAT or a Drive access token). Memory only; the
    /// worker's credential store never writes it to OPFS, settings or a log.
    pub token: String,
    /// Drive only: the refresh token GIS returned, when it returned one.
    /// Memory only, and redacted from `Debug` like `token`.
    #[serde(default)]
    pub refresh_token: String,
    /// Drive only: absolute Unix milliseconds when `token` expires. The engine
    /// refreshes within five minutes of this, or on the first 401.
    #[serde(default)]
    pub expires_unix_ms: Option<u64>,
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
    /// meaningless (an empty id, token, or provider field).
    ///
    /// # Errors
    ///
    /// A short diagnostic naming the empty field.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("binding id", self.binding_id.as_str()),
            ("operation id", self.operation_id.as_str()),
            ("access token", self.token.as_str()),
        ] {
            if value.is_empty() {
                return Err(format!("the sync request is missing its {name}"));
            }
        }
        match self.provider {
            Provider::Github => {
                for (name, value) in [
                    ("repository owner", self.owner.as_str()),
                    ("repository name", self.repo.as_str()),
                    ("branch", self.branch.as_str()),
                ] {
                    if value.is_empty() {
                        return Err(format!("the sync request is missing its {name}"));
                    }
                }
                if self.owner.contains('/') || self.repo.contains('/') {
                    return Err("repository owner and name are separate fields".to_owned());
                }
            }
            Provider::Drive => {
                if self.drive_store.is_empty() {
                    return Err("the sync request is missing its Drive store name".to_owned());
                }
            }
        }
        Ok(())
    }
}

impl fmt::Debug for RunRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunRequest")
            .field("provider", &self.provider)
            .field("binding_id", &self.binding_id)
            .field("operation_id", &self.operation_id)
            .field("owner", &self.owner)
            .field("repo", &self.repo)
            .field("branch", &self.branch)
            .field("prefix", &self.prefix)
            .field("drive_store", &self.drive_store)
            .field("token", &"Secret([redacted])")
            .field(
                "refresh_token",
                &(!self.refresh_token.is_empty()).then_some("Secret([redacted])"),
            )
            .field("expires_unix_ms", &self.expires_unix_ms)
            .field("keep_both", &self.keep_both)
            .finish()
    }
}

/// One message from the worker back to the page.
///
/// The worker's very first message is [`WorkerEvent::Ready`]: its message
/// handler is installed and it can accept a run. **A page must not post a
/// `Run` before it arrives.** A dedicated worker's event loop is not running
/// while its shim's top-level `await init()` is pending, so a message that
/// arrives before the handler is installed is dispatched with no listener and
/// discarded; there is no queue to replay it from. The page-side spawn state
/// machine holds the request until `Ready` and then posts it exactly once.
/// `Ready` is the worker's boot handshake, not a drawable run event: the
/// spawn API consumes it and does not relay it to the caller's event callback.
///
/// There is no `Progress` variant on purpose: see the module docs. After the
/// handshake a run produces [`WorkerEvent::Started`] and then exactly one of
/// `Completed`, `Conflicts` or `Failed`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerEvent {
    /// The worker's message handler is installed. Sent exactly once, before
    /// any run; the page sends the run request after it.
    Ready,
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
    /// payload the page interpolates, not a whole sentence. 8b maps `code` to
    /// catalog copy and never renders `detail` as the line.
    Failed {
        /// Stable failure class, e.g. `auth` or `rate_limited`.
        code: String,
        /// The engine's **inner detail** for a code whose catalogued copy
        /// takes one (`auth`, `permission`, `config`, …), the path for
        /// `dirty_buffer`/`local_changed`, and otherwise the engine's `Display`
        /// rendering (which the page frames or ignores). The page owns the
        /// prefix, so a detail never repeats it.
        detail: String,
    },
}

// There is deliberately no constructor for `WorkerEvent::Failed` here: the
// worker fills `code` from `muninn_sync::SyncError::code` and `detail` from
// `muninn_sync_worker::report::page_detail` — the inner field the page's copy
// interpolates, not the whole `Display` rendering (which would double the
// catalogue's prefix). Keeping the protocol free of `muninn-sync` types is
// what lets `cargo test -p muninn-web` run natively.
#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> RunRequest {
        RunRequest {
            provider: Provider::Github,
            binding_id: "0123456789abcdef".to_owned(),
            operation_id: "fedcba9876543210".to_owned(),
            owner: "octocat".to_owned(),
            repo: "notes".to_owned(),
            branch: "main".to_owned(),
            prefix: "vault".to_owned(),
            drive_store: String::new(),
            token: "github_pat_example".to_owned(),
            refresh_token: String::new(),
            expires_unix_ms: None,
            keep_both: true,
        }
    }

    fn drive_request() -> RunRequest {
        RunRequest {
            provider: Provider::Drive,
            binding_id: "0123456789abcdef".to_owned(),
            operation_id: "fedcba9876543210".to_owned(),
            owner: String::new(),
            repo: String::new(),
            branch: String::new(),
            prefix: String::new(),
            drive_store: "Muninn".to_owned(),
            token: "ya29.access".to_owned(),
            refresh_token: "1//refresh".to_owned(),
            expires_unix_ms: Some(1_700_000_000_000),
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
            WorkerEvent::Ready,
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
                // The inner detail, not a whole sentence: the page owns the
                // framing (see `Failed`).
                detail: "Bad credentials".to_owned(),
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
    fn debug_never_reveals_either_token() {
        let drive = drive_request();
        let rendered = format!("{drive:?}");
        assert!(!rendered.contains("ya29.access"));
        assert!(!rendered.contains("1//refresh"));
        assert!(rendered.contains("redacted"));
        assert!(rendered.contains("Muninn"));
        let github = request();
        let rendered = format!("{github:?}");
        assert!(!rendered.contains("github_pat_example"));
        assert!(rendered.contains("octocat"));
    }

    #[test]
    fn a_drive_request_round_trips_and_defaults_to_github_when_absent() {
        let request = drive_request();
        let encoded = serde_json::to_string(&WorkerRequest::Run(request.clone())).unwrap();
        assert!(encoded.contains("\"provider\":\"drive\""));
        let decoded: WorkerRequest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, WorkerRequest::Run(request.clone()));
        assert!(request.validate().is_ok());

        // A slice-8 request has no provider field and must still parse as
        // GitHub, with the Drive fields empty.
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
        assert_eq!(run.provider, Provider::Github);
        assert!(run.drive_store.is_empty());
        assert!(run.refresh_token.is_empty());
        assert_eq!(run.expires_unix_ms, None);
    }

    #[test]
    fn drive_validation_wants_a_store_and_a_token_not_a_repository() {
        let mut request = drive_request();
        assert!(request.validate().is_ok());
        // GitHub's fields are absent on purpose and are not required.
        assert!(request.owner.is_empty() && request.repo.is_empty());
        request.drive_store.clear();
        assert_eq!(
            request.validate().unwrap_err(),
            "the sync request is missing its Drive store name"
        );
        request.drive_store = "Muninn".to_owned();
        request.token.clear();
        assert!(request.validate().unwrap_err().contains("access token"));
        // A refresh token is optional: GIS's token flow may not return one.
        request.token = "ya29.access".to_owned();
        request.refresh_token.clear();
        request.expires_unix_ms = None;
        assert!(request.validate().is_ok());
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

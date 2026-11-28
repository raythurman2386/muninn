//! Manual vault sync for Muninn.
//!
//! The vault's ordinary files stay the source of truth. This crate plans a
//! three-way merge, publishes it through GitHub, an app-managed Google Drive
//! commit graph, or a commit store hosted by one Muninn device on a Tailscale
//! tailnet, and applies the result locally with a crash-safe journal.
//! It does not talk to the UI, start work by itself, or store tokens in the
//! vault or in settings.
//!
//! # What a successful sync guarantees
//!
//! A reported success means one logical snapshot converged: every included
//! file's bytes match the published snapshot, conflicts were resolved
//! explicitly, and the per-file journal is complete. It does not mean a
//! filesystem-wide atomic transaction, real-time collaborative editing, or
//! that an uncooperative program outside Muninn was frozen for the whole run.
//!
//! # Drive is not a Markdown folder
//!
//! Google Drive v3 uploads are not a compare-and-swap. Drive sync stores
//! immutable blobs and immutable commit manifests in the app's hidden
//! `appDataFolder`.
//! Concurrent writers become two heads. Nothing in this crate overwrites a
//! Drive file in place.
//!
//! # Features
//!
//! The default `native` feature carries every transport and credential store
//! in this crate, plus the build-time Google Desktop secret. Without it
//! (`--no-default-features`, the configuration a browser build uses) the crate
//! is nearly empty: nothing links the blocking HTTP client, the OS keyring or
//! the secret. The crate's modules are native-only until the web slices add
//! their own; a wasm build must never enable `native`.

#![forbid(unsafe_code)]
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

// Every module below is a native concern today: blocking HTTP, the OS
// credential store, the loopback sign-in listener, the Tailnet host, the
// filesystem lease and the build-time Google Desktop secret. A web build uses
// `--no-default-features`, which leaves this crate nearly empty on purpose: it
// cannot link a native transport, and `clients.rs` is not even compiled, so
// the wasm artifact has no path to the Desktop secret. Slices 8-9 add the
// web-relevant pieces behind their own gates.
#[cfg(feature = "native")]
mod apply;
#[cfg(feature = "native")]
mod auth;
#[cfg(feature = "native")]
mod clients;
#[cfg(feature = "native")]
mod credentials;
#[cfg(feature = "native")]
mod drive;
#[cfg(feature = "native")]
mod engine;
#[cfg(feature = "native")]
mod error;
#[cfg(feature = "native")]
mod fsutil;
#[cfg(feature = "native")]
mod github;
#[cfg(feature = "native")]
mod http;
#[cfg(feature = "native")]
mod inventory;
#[cfg(feature = "native")]
mod journal;
#[cfg(feature = "native")]
mod lease;
#[cfg(feature = "native")]
mod limits;
#[cfg(feature = "native")]
mod model;
#[cfg(feature = "native")]
mod pathutil;
#[cfg(feature = "native")]
mod plan;
#[cfg(feature = "native")]
mod secret;
// The engine's small host-state IO goes through core's `StateHome`; the native
// twin here keeps the existing per-binding file layout byte-for-byte.
#[cfg(feature = "native")]
mod state_home;
#[cfg(feature = "native")]
mod tailnet;

#[cfg(all(test, feature = "native"))]
mod auth_tests;
#[cfg(all(test, feature = "native"))]
mod provider_tests;
#[cfg(all(test, feature = "native"))]
mod tailnet_tests;
#[cfg(all(test, feature = "native"))]
mod testkit;

#[cfg(feature = "native")]
pub use apply::{ApplyDelete, ApplyOutcome, ApplyRequest, ApplyWrite, apply_plan, resume_apply};
#[cfg(feature = "native")]
pub use auth::{
    BrowserOpener, LoopbackRedirect, PendingAnswer, SignInEvents, SystemBrowser, browser_command,
    github_device_sign_in, google_sign_in,
};
#[cfg(feature = "native")]
pub use clients::{
    BUNDLED_GITHUB_CLIENT_ID, BUNDLED_GOOGLE_CLIENT_ID, GoogleOAuthClient,
    OFFICIAL_GITHUB_CLIENT_ID, OFFICIAL_GOOGLE_CLIENT_ID, bundled_client_id,
    bundled_google_client_secret, client_id_for, google_client_for,
};
#[cfg(feature = "native")]
pub use credentials::{
    CredentialKey, CredentialStore, MemoryCredentialStore, NativeCredentialStore,
};
#[cfg(feature = "native")]
pub use drive::{
    DRIVE_SCOPE, DriveClient, DriveCommitDoc, DriveRemote, DriveScope, OAuthRedirect, PkceRequest,
    merge_heads, new_pkce, parse_oauth_redirect,
};
#[cfg(feature = "native")]
pub use engine::{
    EngineParts, GOOGLE_AUTH_BASE, RunRequest, SyncReport, binding_state_dir, disconnect,
    production_sleeper, run_sync, run_sync_with_state_home,
};
#[cfg(feature = "native")]
pub use error::SyncError;
#[cfg(feature = "native")]
pub use github::{
    DeviceChallenge, DevicePoll, GitHubClient, GitHubRemote, GitHubScope, git_blob_sha,
};
#[cfg(feature = "native")]
pub use http::{
    CancelToken, HttpRequest, HttpResponse, HttpTransport, RealSleeper, RecordingSleeper,
    ReqwestTransport, Sleeper,
};
#[cfg(feature = "native")]
pub use inventory::{Inventory, exclusion_summary, inventory, inventory_default};
#[cfg(feature = "native")]
pub use journal::{Journal, JournalPhase};
// Fuzz-harness seams; see the `fuzzing` feature in Cargo.toml.
#[cfg(all(feature = "native", feature = "fuzzing"))]
pub use drive::parse_commit_for_fuzzing;
#[cfg(all(feature = "native", feature = "fuzzing"))]
pub use journal::parse_journal_for_fuzzing;
#[cfg(feature = "native")]
pub use lease::VaultLease;
#[cfg(feature = "native")]
pub use limits::{DEFAULT_LIMITS, Exclusion, Limits, included_path};
#[cfg(feature = "native")]
pub use model::{
    BindingId, Conflict, ConflictKind, ContentHash, FileKind, OperationId, Plan, ProviderKind,
    Resolution, RunId, Side, Snapshot, Tombstone, VaultIdentity,
};
#[cfg(feature = "native")]
pub use pathutil::RelativePath;
#[cfg(feature = "native")]
pub use secret::{Secret, TokenSet};
#[cfg(feature = "native")]
pub use tailnet::{
    DirStore, HttpStore, TAILNET_API_PREFIX, TAILNET_DEFAULT_PORT, TailnetHost, TailnetHostConfig,
    TailnetRole, TailnetScope, new_shared_key, serve_command, tailnet_store_dir,
    validate_tailnet_host,
};

/// Service name passed to the OS credential store. Not a secret.
pub const CREDENTIAL_SERVICE: &str = "app.muninn.sync";

/// Schema version for local journals, bases, and Drive commit manifests.
pub const SCHEMA_VERSION: u32 = 1;

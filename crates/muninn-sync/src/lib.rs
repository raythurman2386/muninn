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
//! in this crate, plus the build-time Google Desktop secret. The non-default
//! `web` feature compiles the browser's surface for
//! `wasm32-unknown-unknown`: the HTTP boundary and its status mapping, the
//! GitHub and Drive clients, the credential contract with its memory store,
//! the journal's state-home API, the state-key table, and the sync engine
//! itself — inventory, planning, apply and the lease contract — over
//! `muninn_core::VaultStore` and `StateHome`. The browser supplies
//! `muninn-web`'s OPFS store, state home and Web Lock lease to
//! `run_sync_with_parts`. `clients.rs`, the OS keyring, the loopback sign-in
//! listener, the Drive client's secret-bearing sign-in helpers, Tailnet and
//! the filesystem lease implementation stay native. A wasm build must never
//! enable `native`.

#![forbid(unsafe_code)]
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

// The browser's transport surface (`web`) and the desktop build (`native`)
// share the modules below. The engine, inventory and apply are shared too: they
// reach the vault through `muninn_core::VaultStore` and host state through
// `StateHome`, so the same orchestration compiles for wasm32. Native keeps the
// filesystem implementations (the store wrapper, the state home, the lease),
// the loopback sign-in listener, the Tailnet host, Drive, and the build-time
// Google Desktop secret. A wasm build with `--no-default-features` leaves this
// crate nearly empty on purpose: it cannot link a native transport, and
// `clients.rs` is not even compiled, so the wasm artifact has no path to the
// Desktop secret.
#[cfg(any(feature = "native", feature = "web"))]
mod apply;
#[cfg(feature = "native")]
mod auth;
#[cfg(feature = "native")]
mod clients;
#[cfg(any(feature = "native", feature = "web"))]
mod credentials;
// Slice 9: the Drive client is shared. It talks to Google over the same
// `HttpTransport` trait as GitHub, so the browser's worker drives the same
// pagination, head-merge and publish code the desktop does. Only its
// secret-bearing sign-in (loopback PKCE, the Desktop client) stays native.
#[cfg(any(feature = "native", feature = "web"))]
mod drive;
#[cfg(any(feature = "native", feature = "web"))]
mod engine;
#[cfg(any(feature = "native", feature = "web"))]
mod error;
#[cfg(feature = "native")]
mod fsutil;
#[cfg(any(feature = "native", feature = "web"))]
mod github;
#[cfg(any(feature = "native", feature = "web"))]
mod http;
#[cfg(any(feature = "native", feature = "web"))]
mod inventory;
#[cfg(any(feature = "native", feature = "web"))]
mod journal;
#[cfg(any(feature = "native", feature = "web"))]
mod lease;
#[cfg(any(feature = "native", feature = "web"))]
mod limits;
#[cfg(any(feature = "native", feature = "web"))]
mod model;
#[cfg(any(feature = "native", feature = "web"))]
mod pathutil;
#[cfg(any(feature = "native", feature = "web"))]
mod plan;
#[cfg(any(feature = "native", feature = "web"))]
mod secret;
// The engine's small host-state IO goes through core's `StateHome`; the native
// twin here keeps the existing per-binding file layout byte-for-byte. The key
// table is shared with the browser twin, which spells the same keys in OPFS.
#[cfg(any(feature = "native", feature = "web"))]
pub mod state_home;
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

#[cfg(any(feature = "native", feature = "web"))]
pub use apply::{ApplyDelete, ApplyOutcome, ApplyRequest, ApplyWrite};
#[cfg(feature = "native")]
pub use apply::{apply_plan, resume_apply};
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
pub use credentials::NativeCredentialStore;
#[cfg(any(feature = "native", feature = "web"))]
pub use credentials::{CredentialKey, CredentialStore, MemoryCredentialStore};
#[cfg(any(feature = "native", feature = "web"))]
pub use drive::{
    DRIVE_API_BASE, DRIVE_AUTH_BASE, DRIVE_OAUTH_BASE, DRIVE_SCOPE, DRIVE_UPLOAD_BASE, DriveClient,
    DriveCommitDoc, DriveRemote, DriveScope, merge_heads,
};
#[cfg(feature = "native")]
pub use drive::{OAuthRedirect, PkceRequest, new_pkce, parse_oauth_redirect};
#[cfg(any(feature = "native", feature = "web"))]
pub use engine::{EngineParts, RunRequest, SyncReport, run_sync_with_parts};
#[cfg(feature = "native")]
pub use engine::{
    GOOGLE_AUTH_BASE, binding_state_dir, disconnect, production_sleeper, run_sync,
    run_sync_with_state_home,
};
#[cfg(any(feature = "native", feature = "web"))]
pub use error::SyncError;
#[cfg(any(feature = "native", feature = "web"))]
pub use github::{
    DeviceChallenge, DevicePoll, GitHubClient, GitHubRemote, GitHubScope, git_blob_sha,
};
#[cfg(any(feature = "native", feature = "web"))]
pub use http::{
    CancelToken, HttpRequest, HttpResponse, HttpTransport, RecordingSleeper, Sleeper,
    WEB_GITHUB_API_HOST, WEB_GOOGLE_HOSTS, classify_status, ensure_web_allowed,
};
#[cfg(feature = "native")]
pub use http::{RealSleeper, ReqwestTransport};
#[cfg(any(feature = "native", feature = "web"))]
pub use inventory::{Inventory, exclusion_summary, inventory_with, vault_identity};
#[cfg(feature = "native")]
pub use inventory::{inventory, inventory_default};
#[cfg(any(feature = "native", feature = "web"))]
pub use journal::{Journal, JournalPhase};
// Fuzz-harness seams; see the `fuzzing` feature in Cargo.toml.
#[cfg(all(feature = "native", feature = "fuzzing"))]
pub use drive::parse_commit_for_fuzzing;
#[cfg(all(feature = "native", feature = "fuzzing"))]
pub use journal::parse_journal_for_fuzzing;
#[cfg(feature = "native")]
pub use lease::{FsLease, VaultLease, lock_path};
#[cfg(any(feature = "native", feature = "web"))]
pub use lease::{LeaseGuard, SyncLease};
#[cfg(any(feature = "native", feature = "web"))]
pub use limits::{DEFAULT_LIMITS, Exclusion, Limits, included_path};
#[cfg(any(feature = "native", feature = "web"))]
pub use model::{
    BindingId, Conflict, ConflictKind, ContentHash, FileKind, OperationId, Plan, ProviderKind,
    Resolution, RunId, Side, Snapshot, Tombstone, VaultIdentity,
};
#[cfg(any(feature = "native", feature = "web"))]
pub use pathutil::RelativePath;
#[cfg(any(feature = "native", feature = "web"))]
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

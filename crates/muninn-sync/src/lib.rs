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
//! immutable blobs and immutable commit manifests inside the selected folder.
//! Concurrent writers become two heads. Nothing in this crate overwrites a
//! Drive file in place.

#![forbid(unsafe_code)]
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod apply;
mod credentials;
mod drive;
mod engine;
mod error;
mod fsutil;
mod github;
mod http;
mod inventory;
mod journal;
mod lease;
mod limits;
mod model;
mod pathutil;
mod plan;
mod secret;
mod tailnet;

#[cfg(test)]
mod provider_tests;
#[cfg(test)]
mod tailnet_tests;
#[cfg(test)]
mod testkit;

pub use apply::{ApplyDelete, ApplyOutcome, ApplyRequest, ApplyWrite, apply_plan, resume_apply};
pub use credentials::{
    CredentialKey, CredentialStore, MemoryCredentialStore, NativeCredentialStore,
};
pub use drive::{
    DRIVE_SCOPE, DriveClient, DriveCommitDoc, DriveRemote, DriveScope, OAuthRedirect, PkceRequest,
    merge_heads, new_pkce, parse_oauth_redirect,
};
pub use engine::{EngineParts, RunRequest, SyncReport, disconnect, production_sleeper, run_sync};
pub use error::SyncError;
pub use github::{
    DeviceChallenge, DevicePoll, GitHubClient, GitHubRemote, GitHubScope, git_blob_sha,
};
pub use http::{
    CancelToken, HttpRequest, HttpResponse, HttpTransport, RealSleeper, RecordingSleeper,
    ReqwestTransport,
};
pub use inventory::{Inventory, exclusion_summary, inventory, inventory_default};
pub use journal::{Journal, JournalPhase};
pub use lease::VaultLease;
pub use limits::{DEFAULT_LIMITS, Exclusion, Limits, included_path};
pub use model::{
    BindingId, Conflict, ConflictKind, ContentHash, FileKind, OperationId, Plan, ProviderKind,
    Resolution, RunId, Side, Snapshot, Tombstone, VaultIdentity,
};
pub use pathutil::RelativePath;
pub use secret::{Secret, TokenSet};
pub use tailnet::{
    DirStore, HttpStore, TAILNET_API_PREFIX, TAILNET_DEFAULT_PORT, TailnetHost, TailnetHostConfig,
    TailnetRole, TailnetScope, new_shared_key, serve_command, tailnet_store_dir,
    validate_tailnet_host,
};

/// Service name passed to the OS credential store. Not a secret.
pub const CREDENTIAL_SERVICE: &str = "app.muninn.sync";

/// Schema version for local journals, bases, and Drive commit manifests.
pub const SCHEMA_VERSION: u32 = 1;

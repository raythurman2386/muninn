//! Turn the wire request into the pieces the engine takes (#240 slices 8a, 9).
//!
//! This is the worker's first step: it parses the binding and operation ids,
//! validates the provider's scope, seeds the run-scoped credential store, and
//! maps the result onto [`muninn_sync::RunRequest`]. The three host seams the
//! engine takes separately (`VaultStore`, `StateHome`, `SyncLease`) are the
//! worker's OPFS vault store, its state home and the Web Lock lease;
//! everything the request itself carries is built here.
//!
//! # The vault label is the identity
//!
//! `muninn_sync::VaultIdentity` is a hash of the vault root's path bytes; the
//! engine only ever sees the path the [`muninn_core::VaultStore`] reports. A
//! browser has no filesystem path, so the worker passes [`VAULT_LABEL`], the
//! name of the OPFS directory the vault lives in. That is a stable, vault-
//! specific label: origin-private storage is namespaced per origin, so
//! `muninn-vault` is exactly one vault (one origin's copy) and cannot collide
//! with another origin's vault or another folder in this one. A page that
//! later hosts several vaults must give each its own OPFS directory name, and
//! the label follows that name; renaming the directory would change the
//! identity and start a fresh binding, so the label is part of the storage
//! contract, not a display string.
//!
//! # GitHub and Drive
//!
//! The wire request names its provider. GitHub's fields become a
//! [`GitHubScope`]; Drive's become a [`DriveScope`] (the store name inside the
//! app's hidden `appDataFolder`). The credential store is seeded with a
//! [`TokenSet`] either way: GitHub's is one access token with no expiry, Drive
//! carries whatever Google Identity Services returned (access, optional
//! refresh, expiry), which is exactly what the engine's refresh check expects.

use std::collections::BTreeSet;
use std::path::PathBuf;

use muninn_sync::{
    BindingId, DriveScope, GitHubScope, OperationId, ProviderKind, Resolution, RunRequest, Secret,
    SyncError, TokenSet,
};
use muninn_web::sync_protocol::Provider;

use crate::credentials::RunCredentials;

/// The OPFS directory the browser vault lives in, and the vault root label the
/// engine hashes for its identity.
///
/// It matches the page boot's directory (`muninn-web`'s `opfs.rs`); the two
/// must not drift, or the worker would sync a different directory than the one
/// the UI shows.
pub const VAULT_LABEL: &str = "muninn-vault";

/// The provider scope this run syncs.
pub enum PreparedScope {
    /// A GitHub repository and the path inside it.
    Github(GitHubScope),
    /// A store inside the app's hidden Drive `appDataFolder`.
    Drive(DriveScope),
}

/// A validated run request with the credential store built.
///
/// The worker keeps this alive for the length of the run; dropping it wipes
/// the tokens.
pub struct PreparedRun {
    /// The binding whose state the run reads and writes.
    pub binding: BindingId,
    /// The publish idempotency key for this run.
    pub operation: OperationId,
    /// Which provider this run uses.
    pub provider: ProviderKind,
    /// Owner/repo/branch/prefix, or the Drive store name, after validation.
    pub scope: PreparedScope,
    /// Resolve every conflict by keeping both versions.
    pub keep_both: bool,
    /// The run-scoped credential store, holding the access (and any refresh)
    /// token.
    pub credentials: RunCredentials,
}

impl PreparedRun {
    /// Validate `request` and build the run's credential store.
    ///
    /// # Errors
    ///
    /// [`SyncError::Config`] for a missing field, and the engine's own
    /// [`SyncError`] for an id or scope that fails its parser. Tokens are
    /// never reflected in any message.
    pub fn new(request: &muninn_web::sync_protocol::RunRequest) -> Result<PreparedRun, SyncError> {
        request
            .validate()
            .map_err(|detail| SyncError::Config { detail })?;
        let binding = BindingId::parse(&request.binding_id)?;
        let operation = OperationId::parse(&request.operation_id)?;
        let provider = match request.provider {
            Provider::Github => ProviderKind::GitHub,
            Provider::Drive => ProviderKind::GoogleDrive,
        };
        let scope = match provider {
            ProviderKind::GitHub => {
                let scope = GitHubScope {
                    owner: request.owner.clone(),
                    repo: request.repo.clone(),
                    branch: request.branch.clone(),
                    prefix: request.prefix.clone(),
                };
                scope.validate()?;
                PreparedScope::Github(scope)
            }
            ProviderKind::GoogleDrive => {
                let scope = DriveScope {
                    store: request.drive_store.clone(),
                };
                scope.validate()?;
                PreparedScope::Drive(scope)
            }
            ProviderKind::Tailscale => {
                // The browser has no Tailscale door (#240 slice 10).
                return Err(SyncError::Config {
                    detail: "Tailscale sync is not available in the browser".to_owned(),
                });
            }
        };
        // The wire request still holds plain `String` copies of the tokens
        // (serde has no zeroizing string). The page is the only other holder;
        // it drops its copies after posting the message.
        let token = TokenSet {
            access: Secret::new(request.token.clone()),
            refresh: (!request.refresh_token.is_empty())
                .then(|| Secret::new(request.refresh_token.clone())),
            expires_unix_ms: request.expires_unix_ms,
            account: String::new(),
        };
        let credentials = RunCredentials::new(binding.clone(), provider, token);
        Ok(PreparedRun {
            binding,
            operation,
            provider,
            scope,
            keep_both: request.keep_both,
            credentials,
        })
    }

    /// The engine-side request, following the seam map from slice 8a-vault.
    ///
    /// - `root` is [`VAULT_LABEL`], the store's root label, because the engine
    ///   never reads the path — it builds every path from the store's root and
    ///   hashes that label for the vault identity. See the module docs.
    /// - `state_dir` is empty: the state home is a trait object, and the only
    ///   caller that reads the directory is the native wrapper this worker does
    ///   not use.
    /// - `protected` and `resolutions` are empty: the browser sync has no
    ///   open editor buffers to protect and asks the engine for no per-path
    ///   choices; the Sync surface offers no conflict picker yet.
    /// - `default_resolution` is `KeepBoth` when the wire request asks for it
    ///   (the app's default), so a note edited on two devices never stops a
    ///   run. A request that turns it off gets `None`, the engine's "stop with
    ///   conflicts" behaviour.
    /// - `drive` carries the store for a Drive run and is `None` for GitHub;
    ///   `tailnet` is absent from the browser engine surface entirely.
    #[must_use]
    pub fn engine_request(&self) -> RunRequest {
        let (github, drive) = match &self.scope {
            PreparedScope::Github(scope) => (Some(scope.clone()), None),
            PreparedScope::Drive(scope) => (None, Some(scope.clone())),
        };
        RunRequest {
            root: PathBuf::from(VAULT_LABEL),
            state_dir: PathBuf::new(),
            binding: self.binding.clone(),
            provider: self.provider,
            github,
            drive,
            operation: self.operation.clone(),
            protected: BTreeSet::new(),
            resolutions: Vec::new(),
            default_resolution: self.keep_both.then_some(Resolution::KeepBoth),
            // The native engine surface carries the Tailscale provider; the
            // browser build has no field at all. A host test compiles this
            // file against the native crate, a wasm build against `web`.
            #[cfg(not(target_arch = "wasm32"))]
            tailnet: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_sync::{CredentialStore, VaultIdentity};
    use muninn_web::sync_protocol::{Provider, RunRequest as WireRequest};

    fn wire() -> WireRequest {
        WireRequest {
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

    fn drive_wire() -> WireRequest {
        WireRequest {
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
    fn the_engine_request_carries_the_seam_map_key_by_key() {
        let prepared = PreparedRun::new(&wire()).expect("a valid request");
        let engine = prepared.engine_request();
        assert_eq!(engine.root, PathBuf::from(VAULT_LABEL));
        assert_eq!(
            engine.state_dir,
            PathBuf::new(),
            "the browser has no state path"
        );
        assert_eq!(engine.binding, prepared.binding);
        assert_eq!(engine.operation, prepared.operation);
        assert_eq!(engine.provider, ProviderKind::GitHub);
        let scope = engine.github.expect("a GitHub scope");
        assert_eq!(scope.owner, "octocat");
        assert_eq!(scope.repo, "notes");
        assert_eq!(scope.branch, "main");
        assert_eq!(scope.prefix, "vault");
        assert!(engine.drive.is_none());
        assert!(engine.protected.is_empty());
        assert!(engine.resolutions.is_empty());
        assert_eq!(engine.default_resolution, Some(Resolution::KeepBoth));
    }

    /// Slice 9: the Drive mapping, key by key. The GitHub fields are empty on
    /// purpose and the provider is what selects the Drive scope.
    #[test]
    fn a_drive_request_maps_onto_the_engine_request_key_by_key() {
        let prepared = PreparedRun::new(&drive_wire()).expect("a valid Drive request");
        let engine = prepared.engine_request();
        assert_eq!(engine.provider, ProviderKind::GoogleDrive);
        assert_eq!(engine.root, PathBuf::from(VAULT_LABEL));
        assert_eq!(engine.state_dir, PathBuf::new());
        assert_eq!(engine.binding.as_str(), "0123456789abcdef");
        assert_eq!(engine.operation.as_str(), "fedcba9876543210");
        assert!(engine.github.is_none());
        let scope = engine.drive.expect("a Drive scope");
        assert_eq!(scope.store, "Muninn");
        assert_eq!(scope.root_name(), "muninn-sync-Muninn");
        assert!(engine.protected.is_empty());
        assert!(engine.resolutions.is_empty());
        assert_eq!(engine.default_resolution, Some(Resolution::KeepBoth));

        // The credential store answers exactly the Drive key with the three
        // GIS values, and nothing for GitHub.
        let key = muninn_sync::CredentialKey {
            provider: ProviderKind::GoogleDrive,
            binding: prepared.binding.clone(),
        };
        let token = prepared
            .credentials
            .get(&key)
            .expect("a readable store")
            .expect("the Drive credential");
        assert_eq!(token.access.expose(), "ya29.access");
        assert_eq!(
            token.refresh.as_ref().map(Secret::expose),
            Some("1//refresh")
        );
        assert_eq!(token.expires_unix_ms, Some(1_700_000_000_000));
        assert!(
            prepared
                .credentials
                .get(&muninn_sync::CredentialKey {
                    provider: ProviderKind::GitHub,
                    binding: prepared.binding.clone(),
                })
                .expect("a readable store")
                .is_none(),
            "a Drive run holds no GitHub credential"
        );
    }

    #[test]
    fn a_drive_request_without_a_refresh_token_still_maps() {
        let mut request = drive_wire();
        request.refresh_token.clear();
        request.expires_unix_ms = None;
        let prepared = PreparedRun::new(&request).expect("a valid Drive request");
        let key = muninn_sync::CredentialKey {
            provider: ProviderKind::GoogleDrive,
            binding: prepared.binding.clone(),
        };
        let token = prepared
            .credentials
            .get(&key)
            .expect("a readable store")
            .expect("the Drive credential");
        assert!(token.refresh.is_none());
        assert_eq!(token.expires_unix_ms, None);
    }

    #[test]
    fn turning_off_keep_both_asks_the_engine_to_stop_on_conflicts() {
        let mut request = wire();
        request.keep_both = false;
        let prepared = PreparedRun::new(&request).expect("a valid request");
        assert_eq!(prepared.engine_request().default_resolution, None);
    }

    #[test]
    fn the_pages_wire_request_builds_the_engine_request_key_by_key() {
        // Slice 8b: the page builds the wire request from the saved config and
        // the PAT. The test above pins the #326 seam map; this one pins that
        // the page's builder is that same wire request field for field, and
        // that the worker's `PreparedRun` still lands on the map.
        let page = muninn_web::page_request::request_for(muninn_web::page_request::PageRequest {
            binding_id: "0123456789abcdef",
            operation_id: "fedcba9876543210",
            owner: "octocat",
            repo: "notes",
            branch: "main",
            prefix: "vault",
            token: "github_pat_example",
        });
        assert_eq!(page, wire(), "the page builder is the wire request");
        let engine = PreparedRun::new(&page)
            .expect("a valid page request")
            .engine_request();
        assert_eq!(
            engine.root,
            PathBuf::from(VAULT_LABEL),
            "the worker's store root label is the engine's root"
        );
        assert_eq!(
            engine.state_dir,
            PathBuf::new(),
            "the state home is a trait"
        );
        assert_eq!(engine.binding.as_str(), "0123456789abcdef");
        assert_eq!(engine.operation.as_str(), "fedcba9876543210");
        assert_eq!(engine.provider, ProviderKind::GitHub);
        let scope = engine.github.expect("a GitHub scope");
        assert_eq!(
            (
                scope.owner.as_str(),
                scope.repo.as_str(),
                scope.branch.as_str(),
                scope.prefix.as_str(),
            ),
            ("octocat", "notes", "main", "vault")
        );
        assert!(engine.protected.is_empty());
        assert!(engine.resolutions.is_empty());
        assert_eq!(
            engine.default_resolution,
            Some(Resolution::KeepBoth),
            "the page's Sync now keeps both copies"
        );
    }

    #[test]
    fn the_pages_drive_builder_is_the_wire_request_field_for_field() {
        let page = muninn_web::page_request::drive_request_for(
            muninn_web::page_request::DrivePageRequest {
                binding_id: "0123456789abcdef",
                operation_id: "fedcba9876543210",
                store: "Muninn",
                access: "ya29.access",
                refresh: "1//refresh",
                expires_unix_ms: Some(1_700_000_000_000),
            },
        );
        assert_eq!(page, drive_wire(), "the page builder is the wire request");
        let prepared = PreparedRun::new(&page).expect("a valid page request");
        assert_eq!(prepared.provider, ProviderKind::GoogleDrive);
        let engine = prepared.engine_request();
        assert_eq!(engine.provider, ProviderKind::GoogleDrive);
        assert_eq!(
            engine.drive.expect("a Drive scope").store,
            "Muninn",
            "the store survives page → wire → worker → engine"
        );
        assert!(engine.github.is_none());
    }

    #[test]
    fn the_vault_label_is_the_identity_the_engine_hashes() {
        // The engine's identity is `VaultIdentity::of_root` over the store's
        // canonical root. The label is stable across runs and distinct from a
        // different directory name (the multi-vault case the docs describe).
        let label = PathBuf::from(VAULT_LABEL);
        assert_eq!(
            VaultIdentity::of_root(&label),
            VaultIdentity::of_root(&PathBuf::from("muninn-vault"))
        );
        assert_ne!(
            VaultIdentity::of_root(&label),
            VaultIdentity::of_root(&PathBuf::from("muninn-vault-2"))
        );
        assert_eq!(VAULT_LABEL, "muninn-vault");
    }

    #[test]
    fn ids_and_scope_still_go_through_the_engines_validators() {
        let mut request = wire();
        request.binding_id = "zzzzzzzz".to_owned();
        assert!(PreparedRun::new(&request).is_err());
        let mut request = wire();
        request.branch = "bad//ref".to_owned();
        assert!(PreparedRun::new(&request).is_err());
        let mut request = wire();
        request.prefix = "../escape".to_owned();
        assert!(PreparedRun::new(&request).is_err());
        // Drive's store name has the same treatment: the engine's own
        // validator runs before any request.
        let mut request = drive_wire();
        request.drive_store = "a/b".to_owned();
        assert!(PreparedRun::new(&request).is_err());
        let mut request = drive_wire();
        request.drive_store = String::new();
        assert!(PreparedRun::new(&request).is_err());
        // 65 characters is past the store-name limit.
        let mut request = drive_wire();
        request.drive_store = "a".repeat(65);
        assert!(PreparedRun::new(&request).is_err());
    }
}

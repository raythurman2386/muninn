//! Turn the wire request into the pieces the engine takes (#240 slice 8a).
//!
//! This is the worker's first step: it parses the binding and operation ids,
//! validates the git ref and vault prefix, seeds the run-scoped credential
//! store, and maps the result onto [`muninn_sync::RunRequest`]. The three host
//! seams the engine takes separately (`VaultStore`, `StateHome`, `SyncLease`)
//! are the worker's OPFS vault store, its state home and the Web Lock lease;
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

use std::collections::BTreeSet;
use std::path::PathBuf;

use muninn_sync::{
    BindingId, GitHubScope, OperationId, ProviderKind, Resolution, RunRequest, Secret, SyncError,
};

use crate::credentials::RunCredentials;

/// The OPFS directory the browser vault lives in, and the vault root label the
/// engine hashes for its identity.
///
/// It matches the page boot's directory (`muninn-web`'s `opfs.rs`); the two
/// must not drift, or the worker would sync a different directory than the one
/// the UI shows.
pub const VAULT_LABEL: &str = "muninn-vault";

/// A validated run request with the credential store built.
///
/// The worker keeps this alive for the length of the run; dropping it wipes
/// the PAT.
pub struct PreparedRun {
    /// The binding whose state the run reads and writes.
    pub binding: BindingId,
    /// The publish idempotency key for this run.
    pub operation: OperationId,
    /// Owner/repo/branch/prefix after the engine's own validation.
    pub scope: GitHubScope,
    /// Resolve every conflict by keeping both versions.
    pub keep_both: bool,
    /// The run-scoped credential store, holding the PAT.
    pub credentials: RunCredentials,
}

impl PreparedRun {
    /// Validate `request` and build the run's credential store.
    ///
    /// # Errors
    ///
    /// [`SyncError::Config`] for a missing field, and the engine's own
    /// [`SyncError`] for an id or git ref that fails its parser. The token
    /// itself is not reflected in any message.
    pub fn new(request: &muninn_web::sync_protocol::RunRequest) -> Result<PreparedRun, SyncError> {
        request
            .validate()
            .map_err(|detail| SyncError::Config { detail })?;
        let binding = BindingId::parse(&request.binding_id)?;
        let operation = OperationId::parse(&request.operation_id)?;
        let scope = GitHubScope {
            owner: request.owner.clone(),
            repo: request.repo.clone(),
            branch: request.branch.clone(),
            prefix: request.prefix.clone(),
        };
        scope.validate()?;
        // The wire request still holds a plain `String` copy of the token
        // (serde has no zeroizing string). The page is the only other holder;
        // 8b must drop its copy after posting the message.
        let credentials = RunCredentials::new(binding.clone(), Secret::new(request.token.clone()));
        Ok(PreparedRun {
            binding,
            operation,
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
    #[must_use]
    pub fn engine_request(&self) -> RunRequest {
        RunRequest {
            root: PathBuf::from(VAULT_LABEL),
            state_dir: PathBuf::new(),
            binding: self.binding.clone(),
            provider: ProviderKind::GitHub,
            github: Some(self.scope.clone()),
            operation: self.operation.clone(),
            protected: BTreeSet::new(),
            resolutions: Vec::new(),
            default_resolution: self.keep_both.then_some(Resolution::KeepBoth),
            // The native engine surface carries the other two providers; the
            // browser build has neither. A host test compiles this arm against
            // the native crate, a wasm build against `web`.
            #[cfg(not(target_arch = "wasm32"))]
            drive: None,
            #[cfg(not(target_arch = "wasm32"))]
            tailnet: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_sync::VaultIdentity;

    fn wire() -> muninn_web::sync_protocol::RunRequest {
        muninn_web::sync_protocol::RunRequest {
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
        assert!(engine.protected.is_empty());
        assert!(engine.resolutions.is_empty());
        assert_eq!(engine.default_resolution, Some(Resolution::KeepBoth));
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
    }
}

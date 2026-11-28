//! Turn the wire request into the pieces the engine takes (#240 slice 8a).
//!
//! This is the worker's first step: it parses the binding and operation ids,
//! validates the git ref and vault prefix, and seeds the run-scoped credential
//! store. The result is the engine-side request minus the three host seams
//! (`VaultStore`, `StateHome`, `SyncLease`), which the worker entry will pass
//! to `run_sync_with_parts`.

use muninn_sync::{BindingId, GitHubScope, OperationId, Secret, SyncError};

use crate::sync::credentials::RunCredentials;
use crate::sync_protocol::RunRequest;

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
    pub fn new(request: &RunRequest) -> Result<PreparedRun, SyncError> {
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
}

//! The page's run request builder (#240 slice 8b).
//!
//! The browser's Sync surface (`muninn_ui::web_sync`) holds the saved public
//! config and the PAT; `muninn_web::page_sync::spawn` wants the serialized
//! wire request. This module is the one mapping between them, kept free of
//! browser types so `cargo test -p muninn-web` can pin it on the host.
//!
//! # The seam map
//!
//! The wire request is not the engine's request. The worker
//! (`muninn_sync_worker::request::PreparedRun`) parses this type into
//! `muninn_sync::RunRequest`, and *that* is where the #326 seam map applies:
//!
//! - `request.root` is the worker's store root label (`muninn-vault`), not the
//!   page's vault label;
//! - `state_dir` is empty (the worker passes its state home as a trait object);
//! - `binding`, `operation` and `github` come from this request;
//! - `default_resolution` is `Some(KeepBoth)`, because `keep_both` is always
//!   true here.
//!
//! The page therefore carries no path at all: only the binding id (derived
//! stably by the UI), the operation id (fresh per run), the scope fields and
//! the token.

use crate::sync_protocol::{Provider, RunRequest};

/// One page-side view of a GitHub run. Borrowed, so building the owned wire
/// request is the only allocation.
pub struct PageRequest<'a> {
    /// The binding id the UI derived from the vault label and repository.
    pub binding_id: &'a str,
    /// The publish idempotency key for this run.
    pub operation_id: &'a str,
    pub owner: &'a str,
    pub repo: &'a str,
    pub branch: &'a str,
    /// Empty means the repository root.
    pub prefix: &'a str,
    pub token: &'a str,
}

/// One page-side view of a Drive run. The tokens are borrowed from the UI's
/// zeroizing copies and are moved into the wire request (whose `Debug`
/// redacts them) at the end of the call.
pub struct DrivePageRequest<'a> {
    /// The binding id the UI derived from the vault label and store name.
    pub binding_id: &'a str,
    /// The publish idempotency key for this run.
    pub operation_id: &'a str,
    /// The Drive store name inside the app's hidden `appDataFolder`.
    pub store: &'a str,
    /// The GIS access token.
    pub access: &'a str,
    /// The GIS refresh token, when Google returned one.
    pub refresh: &'a str,
    /// Absolute Unix milliseconds when `access` expires.
    pub expires_unix_ms: Option<u64>,
}

/// Build the wire request for one GitHub run.
///
/// `keep_both` is always true: the app's Sync Now resolves every conflict as
/// "keep both", so a note edited on two devices never stops a run. The worker
/// turns that into `default_resolution: Some(KeepBoth)`.
#[must_use]
pub fn request_for(config: PageRequest<'_>) -> RunRequest {
    RunRequest {
        provider: Provider::Github,
        binding_id: config.binding_id.to_owned(),
        operation_id: config.operation_id.to_owned(),
        owner: config.owner.to_owned(),
        repo: config.repo.to_owned(),
        branch: config.branch.to_owned(),
        prefix: config.prefix.to_owned(),
        drive_store: String::new(),
        token: config.token.to_owned(),
        refresh_token: String::new(),
        expires_unix_ms: None,
        keep_both: true,
    }
}

/// Build the wire request for one Drive run.
///
/// The GitHub fields stay empty; the worker chooses the Drive scope by
/// `provider`. `keep_both` is true for the same reason as [`request_for`].
#[must_use]
pub fn drive_request_for(config: DrivePageRequest<'_>) -> RunRequest {
    RunRequest {
        provider: Provider::Drive,
        binding_id: config.binding_id.to_owned(),
        operation_id: config.operation_id.to_owned(),
        owner: String::new(),
        repo: String::new(),
        branch: String::new(),
        prefix: String::new(),
        drive_store: config.store.to_owned(),
        token: config.access.to_owned(),
        refresh_token: config.refresh.to_owned(),
        expires_unix_ms: config.expires_unix_ms,
        keep_both: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> PageRequest<'static> {
        PageRequest {
            binding_id: "0123456789abcdef0123456789abcdef",
            operation_id: "fedcba9876543210fedcba9876543210",
            owner: "octocat",
            repo: "notes",
            branch: "main",
            prefix: "vault",
            token: "github_pat_example",
        }
    }

    fn drive_page() -> DrivePageRequest<'static> {
        DrivePageRequest {
            binding_id: "0123456789abcdef0123456789abcdef",
            operation_id: "fedcba9876543210fedcba9876543210",
            store: "Muninn",
            access: "ya29.access",
            refresh: "1//refresh",
            expires_unix_ms: Some(1_700_000_000_000),
        }
    }

    #[test]
    fn the_wire_request_carries_every_field_and_keeps_both_by_default() {
        let request = request_for(page());
        assert_eq!(request.provider, Provider::Github);
        assert_eq!(request.binding_id, "0123456789abcdef0123456789abcdef");
        assert_eq!(request.operation_id, "fedcba9876543210fedcba9876543210");
        assert_eq!(request.owner, "octocat");
        assert_eq!(request.repo, "notes");
        assert_eq!(request.branch, "main");
        assert_eq!(request.prefix, "vault");
        assert_eq!(request.token, "github_pat_example");
        assert!(
            request.keep_both,
            "the browser's Sync now always keeps both copies"
        );
    }

    #[test]
    fn an_absent_prefix_is_the_repository_root() {
        let mut config = page();
        config.prefix = "";
        assert_eq!(request_for(config).prefix, "");
    }

    #[test]
    fn a_built_request_passes_the_wire_validation() {
        assert!(request_for(page()).validate().is_ok());
        let mut config = page();
        config.token = "";
        assert!(request_for(config).validate().is_err());
    }

    #[test]
    fn a_drive_request_carries_the_store_the_tokens_and_the_expiry() {
        let request = drive_request_for(drive_page());
        assert_eq!(request.provider, Provider::Drive);
        assert_eq!(request.binding_id, "0123456789abcdef0123456789abcdef");
        assert_eq!(request.operation_id, "fedcba9876543210fedcba9876543210");
        assert_eq!(request.drive_store, "Muninn");
        assert_eq!(request.token, "ya29.access");
        assert_eq!(request.refresh_token, "1//refresh");
        assert_eq!(request.expires_unix_ms, Some(1_700_000_000_000));
        // The GitHub fields are empty on purpose: `provider` selects the scope.
        assert!(request.owner.is_empty());
        assert!(request.repo.is_empty());
        assert!(request.branch.is_empty());
        assert!(request.prefix.is_empty());
        assert!(request.keep_both);
        assert!(request.validate().is_ok());
        assert!(!format!("{request:?}").contains("ya29.access"));
    }

    #[test]
    fn a_drive_request_without_a_refresh_token_is_still_valid() {
        let mut config = drive_page();
        config.refresh = "";
        config.expires_unix_ms = None;
        let request = drive_request_for(config);
        assert!(request.refresh_token.is_empty());
        assert_eq!(request.expires_unix_ms, None);
        assert!(
            request.validate().is_ok(),
            "GIS's token flow may grant no refresh token"
        );
    }
}

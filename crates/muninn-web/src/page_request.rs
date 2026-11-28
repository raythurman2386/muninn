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

use crate::sync_protocol::RunRequest;

/// One page-side view of a run request. Borrowed, so building the owned wire
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

/// Build the wire request for one run.
///
/// `keep_both` is always true: the app's Sync Now resolves every conflict as
/// "keep both", so a note edited on two devices never stops a run. The worker
/// turns that into `default_resolution: Some(KeepBoth)`.
#[must_use]
pub fn request_for(config: PageRequest<'_>) -> RunRequest {
    RunRequest {
        binding_id: config.binding_id.to_owned(),
        operation_id: config.operation_id.to_owned(),
        owner: config.owner.to_owned(),
        repo: config.repo.to_owned(),
        branch: config.branch.to_owned(),
        prefix: config.prefix.to_owned(),
        token: config.token.to_owned(),
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

    #[test]
    fn the_wire_request_carries_every_field_and_keeps_both_by_default() {
        let request = request_for(page());
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
}

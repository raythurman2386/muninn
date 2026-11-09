//! Public OAuth client ids that a Muninn build ships with.
//!
//! # INSERTION POINT for the official client ids
//!
//! - GitHub: the official "Muninn Sync" GitHub App's public client id is
//!   [`OFFICIAL_GITHUB_CLIENT_ID`]. A build may replace it by setting
//!   `MUNINN_GITHUB_CLIENT_ID`.
//! - Google: **not set yet.** The official Desktop OAuth client id goes in
//!   [`BUNDLED_GOOGLE_CLIENT_ID`], today only through the build environment:
//!
//! ```text
//! MUNINN_GOOGLE_CLIENT_ID=<Google "Desktop app" OAuth client id> \
//!   cargo build --release -p muninn-app
//! ```
//!
//! Without a Google id, Drive Connect fails closed: it reports that no client
//! id is bundled and sends nothing, unless the person enters their own id
//! under Settings → Sync. A person's own id always wins over a bundled one.
//!
//! Both are *public* client ids, safe to ship in a binary:
//! - GitHub: the client id of a GitHub App with device flow enabled (not an
//!   OAuth App, and never a personal access token).
//! - Google: a "Desktop app" OAuth client used with PKCE and a loopback
//!   redirect. Never ship a Google client secret; this code sends none.
//!   Publish the consent screen to production: an app left in "Testing"
//!   expires refresh tokens after 7 days. `drive.appdata` is a
//!   non-sensitive scope, so publishing does not need a security review.

use crate::error::SyncError;
use crate::model::ProviderKind;

/// The public client id of the official "Muninn Sync" GitHub App
/// (<https://github.com/apps/muninn-sync>, app id 5685382): device flow on,
/// Contents read and write only, expiring user tokens, no webhook. A client
/// id is public; the app has no client secret or private key in this code,
/// and device-flow tokens refresh without one.
pub const OFFICIAL_GITHUB_CLIENT_ID: &str = "Iv23liRtzA7pkpaiFeZK";

/// The GitHub App client id compiled into this build: the official app's id,
/// or `MUNINN_GITHUB_CLIENT_ID` when a build sets it (for a fork's own app).
pub const BUNDLED_GITHUB_CLIENT_ID: Option<&str> = match option_env!("MUNINN_GITHUB_CLIENT_ID") {
    Some(id) => Some(id),
    None => Some(OFFICIAL_GITHUB_CLIENT_ID),
};

/// INSERTION POINT (Google): the Google Desktop OAuth client id compiled
/// into this build, if any. Unset until the official id is ready; Drive
/// Connect fails closed without it or a person's own id.
pub const BUNDLED_GOOGLE_CLIENT_ID: Option<&str> = option_env!("MUNINN_GOOGLE_CLIENT_ID");

/// The bundled client id for `provider`, if this build has one.
#[must_use]
pub fn bundled_client_id(provider: ProviderKind) -> Option<&'static str> {
    let id = match provider {
        ProviderKind::GitHub => BUNDLED_GITHUB_CLIENT_ID,
        ProviderKind::GoogleDrive => BUNDLED_GOOGLE_CLIENT_ID,
        ProviderKind::Tailscale => None,
    };
    id.map(str::trim).filter(|id| !id.is_empty())
}

/// The client id to sign in and refresh with: the person's own id when they
/// entered one, else the bundled id. With neither, a `Config` error that says
/// so; nothing should be sent.
pub fn client_id_for(provider: ProviderKind, user_override: &str) -> Result<String, SyncError> {
    pick_client_id(provider, user_override, bundled_client_id(provider))
}

pub(crate) fn pick_client_id(
    provider: ProviderKind,
    user_override: &str,
    bundled: Option<&str>,
) -> Result<String, SyncError> {
    let user_override = user_override.trim();
    if !user_override.is_empty() {
        return Ok(user_override.to_owned());
    }
    if let Some(id) = bundled.map(str::trim).filter(|id| !id.is_empty()) {
        return Ok(id.to_owned());
    }
    let name = match provider {
        ProviderKind::GitHub => "GitHub",
        ProviderKind::GoogleDrive => "Google",
        ProviderKind::Tailscale => "Tailscale",
    };
    Err(SyncError::Config {
        detail: format!(
            "This build has no bundled {name} client id. Enter your own public client id under Sync to connect. Nothing was saved and no request was sent."
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_override_wins_then_the_bundled_id_then_a_clear_error() {
        let github = ProviderKind::GitHub;
        assert_eq!(
            pick_client_id(github, " mine ", Some("bundled")).unwrap(),
            "mine"
        );
        assert_eq!(
            pick_client_id(github, "", Some(" bundled ")).unwrap(),
            "bundled"
        );
        for bundled in [None, Some(""), Some("  ")] {
            let error = pick_client_id(github, "  ", bundled).unwrap_err();
            assert!(matches!(error, SyncError::Config { .. }), "{error}");
            assert!(
                error.to_string().contains("no bundled GitHub client id"),
                "{error}"
            );
        }
        if option_env!("MUNINN_GITHUB_CLIENT_ID").is_none() {
            assert_eq!(
                bundled_client_id(github),
                Some(OFFICIAL_GITHUB_CLIENT_ID),
                "GitHub ships the official app's client id"
            );
            assert_eq!(
                client_id_for(github, "").unwrap(),
                OFFICIAL_GITHUB_CLIENT_ID
            );
            assert_eq!(client_id_for(github, "fork-app").unwrap(), "fork-app");
        }
        let error = pick_client_id(ProviderKind::GoogleDrive, "", None).unwrap_err();
        assert!(
            error.to_string().contains("no bundled Google client id"),
            "{error}"
        );
    }
}

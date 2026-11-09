//! Public OAuth client ids that a Muninn build ships with.
//!
//! # INSERTION POINT: the official client ids
//!
//! - GitHub: the official "Muninn Sync" GitHub App's public client id is
//!   [`OFFICIAL_GITHUB_CLIENT_ID`]; a build may replace it with
//!   `MUNINN_GITHUB_CLIENT_ID`.
//! - Google: the official Desktop OAuth client's public id is
//!   [`OFFICIAL_GOOGLE_CLIENT_ID`]; a build may replace it with
//!   `MUNINN_GOOGLE_CLIENT_ID`.
//!
//! A person's own id under Settings → Sync always wins over a bundled one.
//! If a build ends up with an empty id and no override, Connect fails closed:
//! it reports that no client id is bundled and sends nothing.
//!
//! Both are *public* client ids, safe to ship in a binary:
//! - GitHub: the client id of a GitHub App with device flow enabled (not an
//!   OAuth App, and never a personal access token).
//! - Google: a "Desktop app" OAuth client used with PKCE and a loopback
//!   redirect. Never ship a Google client secret; this code sends none.
//!   The official app is published to production; its privacy policy is
//!   <https://github.com/raythurman2386/muninn/blob/main/PRIVACY.md>.

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

/// The public client id of the official Google "Desktop app" OAuth client
/// (project Muninn, `muninn-514218`; consent app "Muninn"; scope
/// `drive.appdata` only). Used with PKCE and a loopback redirect; there is
/// no client secret to ship.
/// The app is published to production (External); privacy policy:
/// <https://github.com/raythurman2386/muninn/blob/main/PRIVACY.md>.
pub const OFFICIAL_GOOGLE_CLIENT_ID: &str =
    "39034215624-i6ek77i3vc32cueus258ef5ai1q93ars.apps.googleusercontent.com";

/// The Google Desktop OAuth client id compiled into this build: the official
/// client's id, or `MUNINN_GOOGLE_CLIENT_ID` when a build sets it.
pub const BUNDLED_GOOGLE_CLIENT_ID: Option<&str> = match option_env!("MUNINN_GOOGLE_CLIENT_ID") {
    Some(id) => Some(id),
    None => Some(OFFICIAL_GOOGLE_CLIENT_ID),
};

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
        let google = ProviderKind::GoogleDrive;
        if option_env!("MUNINN_GOOGLE_CLIENT_ID").is_none() {
            assert_eq!(bundled_client_id(google), Some(OFFICIAL_GOOGLE_CLIENT_ID));
            assert_eq!(
                client_id_for(google, "").unwrap(),
                OFFICIAL_GOOGLE_CLIENT_ID
            );
            assert_eq!(client_id_for(google, "mine.apps").unwrap(), "mine.apps");
        }
        let error = pick_client_id(google, "", None).unwrap_err();
        assert!(
            error.to_string().contains("no bundled Google client id"),
            "{error}"
        );
    }
}

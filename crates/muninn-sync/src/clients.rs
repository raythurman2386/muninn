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
//! The ids are *public*, safe to ship in a binary:
//! - GitHub: the client id of a GitHub App with device flow enabled (not an
//!   OAuth App, and never a personal access token).
//! - Google: a "Desktop app" OAuth client used with PKCE and a loopback
//!   redirect. Google's token endpoint also requires the Desktop client's
//!   secret, which a build supplies with `MUNINN_GOOGLE_CLIENT_SECRET`; it
//!   is never committed here (see [`bundled_google_client_secret`]).
//!   The official app is published to production; its privacy policy is
//!   <https://github.com/raythurman2386/muninn/blob/main/PRIVACY.md>.

use crate::error::SyncError;
use crate::model::ProviderKind;
use crate::secret::Secret;

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
/// `drive.appdata` only). Used with PKCE and a loopback redirect, plus the
/// build-time secret from `MUNINN_GOOGLE_CLIENT_SECRET`.
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

/// The official Google Desktop client's secret, compiled in from
/// `MUNINN_GOOGLE_CLIENT_SECRET` at build time. It is never written in this
/// repository. Google's token endpoint refuses a Desktop client's code
/// exchange and refresh without it (`client_secret is missing.`), even with
/// PKCE. Google documents that an installed app's secret is embedded in the
/// app and is not confidential the way a web server's secret is.
///
/// Private on purpose: callers get it through
/// [`bundled_google_client_secret`] as a [`Secret`], whose `Debug` and
/// `Display` never show the value.
const BUNDLED_GOOGLE_CLIENT_SECRET: Option<&str> = option_env!("MUNINN_GOOGLE_CLIENT_SECRET");

/// The Google client secret compiled into this build, if any.
#[must_use]
pub fn bundled_google_client_secret() -> Option<Secret> {
    BUNDLED_GOOGLE_CLIENT_SECRET
        .map(str::trim)
        .filter(|secret| !secret.is_empty())
        .map(Secret::new)
}

/// The Google OAuth client a sign-in or refresh uses: the bundled public id
/// and the secret that belongs to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoogleOAuthClient {
    pub id: String,
    pub secret: Secret,
}

/// The Google client to sign in and refresh with. `bundled_secret` is
/// normally [`bundled_google_client_secret`]; tests pass a fake one.
///
/// Fails closed, before anything is sent, when the person entered a
/// different client id under Settings (the bundled secret belongs to the
/// bundled client only), when this build has no bundled id, or when it has
/// no secret.
pub fn google_client_for(
    user_override: &str,
    bundled_secret: Option<Secret>,
) -> Result<GoogleOAuthClient, SyncError> {
    pick_google_client(
        user_override,
        bundled_client_id(ProviderKind::GoogleDrive),
        bundled_secret,
    )
}

pub(crate) fn pick_google_client(
    user_override: &str,
    bundled_id: Option<&str>,
    bundled_secret: Option<Secret>,
) -> Result<GoogleOAuthClient, SyncError> {
    let user_override = user_override.trim();
    let bundled = bundled_id.map(str::trim).filter(|id| !id.is_empty());
    if !user_override.is_empty() && Some(user_override) != bundled {
        return Err(SyncError::Config {
            detail: "A custom Google client id is set under Sync. Muninn's built-in Google secret belongs to Muninn's own client and is never sent for another one, and Google will not finish sign-in without a secret. Clear the Google client id field to use Muninn's client. Nothing was sent.".to_owned(),
        });
    }
    let id = pick_client_id(ProviderKind::GoogleDrive, "", bundled)?;
    let Some(secret) = bundled_secret.filter(|secret| !secret.expose().trim().is_empty()) else {
        return Err(missing_google_secret());
    };
    Ok(GoogleOAuthClient { id, secret })
}

/// This build has no Google client secret, so Google would refuse the token
/// request.
pub(crate) fn missing_google_secret() -> SyncError {
    SyncError::Config {
        detail: "This build has no Google client secret (it was built without MUNINN_GOOGLE_CLIENT_SECRET), and Google will not finish Desktop sign-in or refresh without one. Use an official Muninn release, or build with it set. Nothing was sent.".to_owned(),
    }
}

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

    #[test]
    fn google_sends_the_bundled_secret_only_with_the_bundled_id() {
        let official = Some(OFFICIAL_GOOGLE_CLIENT_ID);
        let secret = || Some(Secret::new("test-only-google-secret"));

        let client = pick_google_client("", official, secret()).unwrap();
        assert_eq!(client.id, OFFICIAL_GOOGLE_CLIENT_ID);
        assert_eq!(client.secret.expose(), "test-only-google-secret");
        // Typing the official id into Settings is still the official client.
        let client = pick_google_client(
            &format!(" {OFFICIAL_GOOGLE_CLIENT_ID} "),
            official,
            secret(),
        )
        .unwrap();
        assert_eq!(client.secret.expose(), "test-only-google-secret");

        // Someone else's client id must never get the official secret.
        let error =
            pick_google_client("mine.apps.googleusercontent.com", official, secret()).unwrap_err();
        assert!(matches!(error, SyncError::Config { .. }), "{error}");
        let text = error.to_string();
        assert!(text.contains("custom Google client id"), "{text}");
        assert!(text.contains("Nothing was sent"), "{text}");
        assert!(!text.contains("test-only-google-secret"), "{text}");

        // A build without a secret fails closed instead of asking Google
        // for a token it will refuse.
        for missing in [None, Some(Secret::new("")), Some(Secret::new("  "))] {
            let error = pick_google_client("", official, missing).unwrap_err();
            assert!(matches!(error, SyncError::Config { .. }), "{error}");
            let text = error.to_string();
            assert!(text.contains("MUNINN_GOOGLE_CLIENT_SECRET"), "{text}");
            assert!(text.contains("Nothing was sent"), "{text}");
        }

        let error = pick_google_client("", None, secret()).unwrap_err();
        assert!(
            error.to_string().contains("no bundled Google client id"),
            "{error}"
        );
    }

    #[test]
    fn the_google_client_never_prints_its_secret() {
        let client = GoogleOAuthClient {
            id: "id".to_owned(),
            secret: Secret::new("test-only-google-secret"),
        };
        assert!(!format!("{client:?}").contains("test-only-google-secret"));
    }
}

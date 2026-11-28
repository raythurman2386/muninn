//! Google Identity Services configuration and token parsing (#240 slice 9).
//!
//! The browser's Drive sign-in is Google Identity Services' token client: the
//! page loads `https://accounts.google.com/gsi/client`, opens Google's own
//! popup, and receives an access token (plus a refresh token when the client
//! grants one) in the callback. There is no redirect page and no hosted
//! service of ours.
//!
//! # The client id is public, and it is not the Desktop one
//!
//! A Google **Web application** OAuth client id is safe to embed: it identifies
//! the app, it is not a secret, and Google binds it to the deployment origin
//! the person registered. It is compiled in from
//! `MUNINN_GOOGLE_WEB_CLIENT_ID`, exactly as the desktop's public id comes
//! from `MUNINN_GOOGLE_CLIENT_ID`. The desktop's **Desktop** client id and its
//! secret belong to the native app only; §"Google Desktop secret and web
//! builds" in `SECURITY.md` is the boundary, and `xtask secret-canary` proves
//! the secret cannot reach a wasm artifact.
//!
//! # Unconfigured is a designed state
//!
//! A build with no id must not pretend it can sign in.
//! [`crate::google_auth::drive_auth_state`] reports
//! [`crate::google_auth::DriveAuthState::Unconfigured`], and the Sync surface renders a
//! catalogued "needs configuration" status instead of opening a popup that
//! would fail with Google's own error page. The registration checklist lives
//! in `docs/web.md` (slice 9).
//!
//! This module is deliberately free of browser types, so the parsing and the
//! configured/unconfigured decision are tested natively while the wasm glue
//! that calls `google.accounts.oauth2.initTokenClient` stays in `page_drive`.

use serde_json::Value;

/// The scope Drive sync requests: the app's hidden `appDataFolder`, nothing
/// else in the person's Drive.
///
/// It is the same string as `muninn_sync::DRIVE_SCOPE`; the equality is pinned
/// by a test, because this crate does not depend on `muninn-sync` at build
/// time (the worker does, on wasm).
pub const DRIVE_APP_DATA_SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";

/// The Google **Web application** client id compiled into this build, or an
/// empty string when the build has none.
///
/// Set at build time: `MUNINN_GOOGLE_WEB_CLIENT_ID=…apps.googleusercontent.com`.
/// Never the Desktop client id, never a secret.
pub const WEB_GOOGLE_CLIENT_ID: &str = match option_env!("MUNINN_GOOGLE_WEB_CLIENT_ID") {
    Some(id) => id,
    None => "",
};

/// Whether this build can open a Google sign-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveAuthState {
    /// A public Web-application client id is compiled in; Connect may open the
    /// GIS popup.
    Configured,
    /// No id is configured. The Sync surface says so and Connect does nothing
    /// else.
    Unconfigured,
}

/// The client id this build embeds, trimmed, or `None` when it has none.
#[must_use]
pub fn web_client_id() -> Option<&'static str> {
    let id = WEB_GOOGLE_CLIENT_ID.trim();
    // A set-but-blank id is the unconfigured state, not a client called "".
    (!id.is_empty()).then_some(id)
}

/// The configured/unconfigured decision for a candidate id.
///
/// Pure, so the UI's unconfigured state is headlessly testable; the page passes
/// [`web_client_id`].
#[must_use]
pub fn drive_auth_state(client_id: Option<&str>) -> DriveAuthState {
    match client_id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(_) => DriveAuthState::Configured,
        None => DriveAuthState::Unconfigured,
    }
}

/// What GIS returned when a person finished (or abandoned) sign-in.
///
/// Tokens live in `String`s here because the page hands them to the UI, which
/// stores them and builds the run request. The page never logs this value and
/// `Debug` redacts both tokens.
#[derive(Clone, PartialEq, Eq)]
pub struct GisToken {
    /// The access token sent as `Authorization: Bearer …`.
    pub access_token: String,
    /// Google's refresh token, when the client granted one. The token flow
    /// often omits it; a run then needs Connect again after expiry.
    pub refresh_token: Option<String>,
    /// Absolute Unix milliseconds when `access_token` expires, derived from
    /// GIS's `expires_in` seconds and the page clock.
    pub expires_unix_ms: Option<u64>,
    /// The granted scopes, space-separated, as Google reported them.
    pub scope: String,
}

impl std::fmt::Debug for GisToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GisToken")
            .field("access_token", &"Secret([redacted])")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "Secret([redacted])"),
            )
            .field("expires_unix_ms", &self.expires_unix_ms)
            .field("scope", &self.scope)
            .finish()
    }
}

/// Parse the JSON object GIS hands the token callback.
///
/// GIS's success payload is `{access_token, expires_in, scope, token_type}`
/// (and `refresh_token` when one was granted). A payload without an access
/// token, or one that did not grant [`DRIVE_APP_DATA_SCOPE`], is refused before
/// anything is stored or sent: the same check the desktop's
/// `token_from_oauth` makes.
///
/// # Errors
///
/// A short diagnostic for the page's log and the generic translated failure
/// status. It never includes a token value.
pub fn parse_token_response(json: &Value, now_unix_ms: u64) -> Result<GisToken, String> {
    let access = json
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "the Google response has no access token".to_owned())?
        .to_owned();
    let scope = json
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    // An absent scope field is accepted, as the desktop accepts it: Google
    // includes it on the responses this flow produces. A present scope that
    // lacks drive.appdata is a real refusal.
    if !scope.is_empty()
        && !scope
            .split_whitespace()
            .any(|item| item == DRIVE_APP_DATA_SCOPE)
    {
        return Err("Google did not grant the Drive app-data scope".to_owned());
    }
    let refresh = json
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_owned);
    let expires_unix_ms = json
        .get("expires_in")
        .and_then(Value::as_u64)
        .map(|seconds| expires_at(now_unix_ms, seconds));
    Ok(GisToken {
        access_token: access,
        refresh_token: refresh,
        expires_unix_ms,
        scope,
    })
}

/// Absolute expiry from GIS's `expires_in` seconds.
///
/// The same arithmetic as `muninn_sync::secret::expires_at`; duplicated because
/// the page crate does not link `muninn-sync`'s secret module on the host.
#[must_use]
pub fn expires_at(now_unix_ms: u64, seconds: u64) -> u64 {
    now_unix_ms.saturating_add(seconds.saturating_mul(1000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_or_blank_id_is_the_unconfigured_state() {
        assert_eq!(drive_auth_state(None), DriveAuthState::Unconfigured);
        assert_eq!(drive_auth_state(Some("")), DriveAuthState::Unconfigured);
        assert_eq!(
            drive_auth_state(Some("   ")),
            DriveAuthState::Unconfigured,
            "a set-but-blank build is not a configured client"
        );
        assert_eq!(
            drive_auth_state(Some(" 123-muninn.apps.googleusercontent.com ")),
            DriveAuthState::Configured
        );
        assert_eq!(
            drive_auth_state(web_client_id()),
            if web_client_id().is_some() {
                DriveAuthState::Configured
            } else {
                DriveAuthState::Unconfigured
            },
            "the compile-time id and the decision agree"
        );
    }

    /// The web id is public and is not Raymond's Desktop client.
    ///
    /// This is a build-contract check, not a value check: it fails if the web
    /// build was configured with the Desktop client id, which would mean a
    /// Desktop client was reused for a flow that cannot supply its secret.
    #[test]
    fn the_web_client_id_is_never_the_desktop_client_id() {
        if let Some(id) = web_client_id() {
            assert!(
                id.ends_with(".apps.googleusercontent.com"),
                "a Google web client id ends with the client suffix: {id}"
            );
            assert_ne!(
                id,
                muninn_sync::OFFICIAL_GOOGLE_CLIENT_ID,
                "a Web application client is not the Desktop OAuth client"
            );
            assert!(
                !id.starts_with("39034215624-"),
                "the Desktop client's project prefix must not appear in a web build"
            );
        }
    }

    #[test]
    fn the_scope_is_the_engines_drive_scope() {
        assert_eq!(DRIVE_APP_DATA_SCOPE, muninn_sync::DRIVE_SCOPE);
    }

    #[test]
    fn a_gis_token_response_parses_every_field() {
        let json = serde_json::json!({
            "access_token": "ya29.access",
            "refresh_token": "1//refresh",
            "expires_in": 3599,
            "scope": "https://www.googleapis.com/auth/drive.appdata",
            "token_type": "Bearer",
        });
        let token = parse_token_response(&json, 1_000).expect("a token");
        assert_eq!(token.access_token, "ya29.access");
        assert_eq!(token.refresh_token.as_deref(), Some("1//refresh"));
        assert_eq!(token.expires_unix_ms, Some(1_000 + 3_599_000));
        assert_eq!(token.scope, DRIVE_APP_DATA_SCOPE);
        let rendered = format!("{token:?}");
        assert!(!rendered.contains("ya29.access"));
        assert!(!rendered.contains("1//refresh"));
        assert!(rendered.contains("redacted"));
    }

    #[test]
    fn a_token_response_may_omit_the_refresh_token_and_the_expiry() {
        let json = serde_json::json!({
            "access_token": "ya29.access",
            "scope": DRIVE_APP_DATA_SCOPE,
        });
        let token = parse_token_response(&json, 0).expect("a token");
        assert!(token.refresh_token.is_none());
        assert_eq!(token.expires_unix_ms, None);
    }

    #[test]
    fn a_response_without_an_access_token_or_without_the_scope_is_refused() {
        let missing = serde_json::json!({"scope": DRIVE_APP_DATA_SCOPE});
        assert!(parse_token_response(&missing, 0).is_err());
        let empty = serde_json::json!({"access_token": "", "scope": DRIVE_APP_DATA_SCOPE});
        assert!(parse_token_response(&empty, 0).is_err());
        let wrong_scope = serde_json::json!({
            "access_token": "ya29.access",
            "scope": "https://www.googleapis.com/auth/drive",
        });
        let error = parse_token_response(&wrong_scope, 0).unwrap_err();
        assert!(error.contains("app-data scope"), "{error}");
        assert!(!error.contains("ya29.access"), "{error}");
    }

    #[test]
    fn an_absent_scope_field_is_accepted_as_the_desktop_accepts_it() {
        let json = serde_json::json!({"access_token": "ya29.access"});
        assert!(parse_token_response(&json, 0).is_ok());
    }

    #[test]
    fn expiry_is_absolute_and_saturating() {
        assert_eq!(expires_at(10, 2), 2_010);
        assert_eq!(expires_at(u64::MAX, 1), u64::MAX);
    }
}

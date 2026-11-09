//! Token material. `Debug` and `Display` never reveal the bytes.

use std::fmt;

use zeroize::Zeroize;

use crate::error::SyncError;

/// An opaque secret. The bytes live in a `String` so the OAuth code can hand
/// them to an HTTP header, and `Drop` best-effort wipes them.
pub struct Secret {
    value: String,
}

impl Secret {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}

impl Clone for Secret {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
        }
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        // Constant-time would be nicer; these comparisons are tests and
        // binding checks, not a MAC. Avoid `==` on the raw Debug output.
        self.value == other.value
    }
}

impl Eq for Secret {}

/// Access token plus optional refresh token. Stored only in the credential store.
#[derive(Clone)]
pub struct TokenSet {
    pub access: Secret,
    pub refresh: Option<Secret>,
    /// Unix milliseconds when the access token expires, if the provider said.
    pub expires_unix_ms: Option<u64>,
    /// Provider account label (GitHub login). Not a secret.
    pub account: String,
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenSet")
            .field("access", &self.access)
            .field(
                "refresh",
                &self.refresh.as_ref().map(|_| "Secret([redacted])"),
            )
            .field("expires_unix_ms", &self.expires_unix_ms)
            .field("account", &self.account)
            .finish()
    }
}

impl TokenSet {
    /// JSON for the credential store. Callers must not write this string to
    /// settings, the vault, or a log.
    #[must_use]
    pub fn to_store_json(&self) -> String {
        let refresh = self.refresh.as_ref().map(Secret::expose);
        serde_json::json!({
            "access_token": self.access.expose(),
            "refresh_token": refresh,
            "expires_unix_ms": self.expires_unix_ms,
            "account": self.account,
        })
        .to_string()
    }

    pub fn from_store_json(text: &str) -> Result<Self, SyncError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|_| SyncError::Credentials {
                detail: "stored credential is not valid token JSON".to_owned(),
            })?;
        let access = value
            .get("access_token")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| SyncError::Credentials {
                detail: "stored credential has no access token".to_owned(),
            })?;
        let refresh = value
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(Secret::new);
        let expires_unix_ms = value.get("expires_unix_ms").and_then(|v| v.as_u64());
        let account = value
            .get("account")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        Ok(Self {
            access: Secret::new(access),
            refresh,
            expires_unix_ms,
            account,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_the_token() {
        let secret = Secret::new("super-secret-token");
        let rendered = format!("{secret:?}");
        assert!(!rendered.contains("super-secret"));
        assert!(rendered.contains("redacted"));
        let set = TokenSet {
            access: Secret::new("access-value"),
            refresh: Some(Secret::new("refresh-value")),
            expires_unix_ms: Some(10),
            account: "octocat".to_owned(),
        };
        let rendered = format!("{set:?}");
        assert!(!rendered.contains("access-value"));
        assert!(!rendered.contains("refresh-value"));
        assert!(rendered.contains("octocat"));
    }

    #[test]
    fn store_json_round_trips_without_debug_leaking() {
        let set = TokenSet {
            access: Secret::new("a"),
            refresh: Some(Secret::new("r")),
            expires_unix_ms: Some(42),
            account: "me".to_owned(),
        };
        let parsed = TokenSet::from_store_json(&set.to_store_json()).expect("parse");
        assert_eq!(parsed.access.expose(), "a");
        assert_eq!(parsed.refresh.unwrap().expose(), "r");
        assert_eq!(parsed.account, "me");
        assert_eq!(parsed.expires_unix_ms, Some(42));
    }
}

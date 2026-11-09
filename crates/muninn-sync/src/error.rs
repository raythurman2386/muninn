//! Failures a sync run can surface. Messages are safe to show in the UI:
//! they carry relative paths and provider status text, never tokens or note bodies.

use std::fmt;

/// Why a sync run stopped. `PartialEq` ignores nothing on purpose so tests can
/// pin the exact class of failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    /// DNS, connection refused, or a dropped connection before a response.
    Offline {
        detail: String,
    },
    /// The request timed out. `mutation` is true when the request could have
    /// been accepted by the provider before the timeout.
    Timeout {
        mutation: bool,
    },
    Cancelled,
    /// 429, or a 403 that the provider identified as a rate limit.
    RateLimited {
        retry_after_ms: Option<u64>,
    },
    /// 5xx after retries are exhausted.
    Server {
        status: u16,
        detail: String,
    },
    /// 401, invalid_grant, or an expired device flow. The user must reconnect.
    Auth {
        detail: String,
    },
    /// 403 that is not a rate limit, or a 404 that means "not visible".
    /// Never a license to delete local notes.
    Permission {
        detail: String,
    },
    /// The granted OAuth scope is missing or wider than this binding allows.
    Scope {
        detail: String,
    },
    /// A selected repo, branch, or folder does not exist for this token.
    NotFound {
        detail: String,
    },
    /// The remote moved, or a conditional publish was rejected.
    ConflictRemote {
        detail: String,
    },
    /// A mutating call timed out or the connection died after the bytes were sent.
    OutcomeUnknown {
        detail: String,
    },
    /// A listing was truncated, paginated incompletely, or marked incomplete.
    ListingIncomplete {
        detail: String,
    },
    /// Symlink, special file, shortcut, or a Workspace document we refuse to convert.
    Unsupported {
        path: String,
        detail: String,
    },
    /// A relative path failed the portable-path rules.
    Path {
        detail: String,
    },
    /// The local walk could not be trusted (permissions, hardlink, unreadable entry).
    Inventory {
        detail: String,
    },
    /// A file or the vault exceeded the published size, count, or depth contract.
    Limit {
        detail: String,
    },
    /// The OS credential store is missing, locked, or refused the request.
    /// There is no plaintext fallback.
    Credentials {
        detail: String,
    },
    /// Another Muninn process holds the vault lease.
    LeaseBusy,
    /// The open editor buffer is dirty for a path this apply would change.
    DirtyBuffer {
        path: String,
    },
    /// Disk bytes changed after the snapshot this run planned against.
    LocalChanged {
        path: String,
    },
    /// The on-disk journal is missing, corrupt, or belongs to another binding.
    Journal {
        detail: String,
    },
    /// A provider response could not be interpreted, or a fixture broke a contract.
    Protocol {
        detail: String,
    },
    /// Test-only fault injection. Production runs never construct this.
    InjectedFault {
        at: String,
    },
    /// Binding, client id, or scope fields are missing or inconsistent.
    Config {
        detail: String,
    },
    Io {
        detail: String,
    },
}

impl SyncError {
    /// Transient failures a bounded retry may repeat. Auth, permission, and
    /// conflict are never in this set.
    #[must_use]
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            SyncError::Offline { .. }
                | SyncError::Timeout { mutation: false }
                | SyncError::RateLimited { .. }
                | SyncError::Server { .. }
                | SyncError::LeaseBusy
        )
    }

    /// True when retrying the same mutation without looking it up again could
    /// duplicate a commit or a file.
    #[must_use]
    pub fn outcome_unknown(&self) -> bool {
        matches!(
            self,
            SyncError::OutcomeUnknown { .. } | SyncError::Timeout { mutation: true }
        )
    }

    #[must_use]
    pub fn io(error: impl fmt::Display) -> Self {
        SyncError::Io {
            detail: error.to_string(),
        }
    }
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::Offline { detail } => write!(f, "Offline: {detail}"),
            SyncError::Timeout { mutation: true } => {
                write!(
                    f,
                    "Timed out after a change was sent; reconcile before retrying"
                )
            }
            SyncError::Timeout { mutation: false } => {
                write!(f, "Timed out before the provider responded")
            }
            SyncError::Cancelled => write!(f, "Sync cancelled"),
            SyncError::RateLimited { retry_after_ms } => match retry_after_ms {
                Some(ms) => write!(f, "Rate limited; retry after {ms} ms"),
                None => write!(f, "Rate limited"),
            },
            SyncError::Server { status, detail } => write!(f, "Provider error {status}: {detail}"),
            SyncError::Auth { detail } => write!(f, "Authentication needed: {detail}"),
            SyncError::Permission { detail } => write!(f, "Permission denied: {detail}"),
            SyncError::Scope { detail } => write!(f, "OAuth scope rejected: {detail}"),
            SyncError::NotFound { detail } => write!(f, "Remote scope not found: {detail}"),
            SyncError::ConflictRemote { detail } => write!(f, "Remote changed: {detail}"),
            SyncError::OutcomeUnknown { detail } => {
                write!(f, "Outcome unknown, reconcile before retrying: {detail}")
            }
            SyncError::ListingIncomplete { detail } => {
                write!(
                    f,
                    "Remote listing incomplete; refusing to treat that as deletion: {detail}"
                )
            }
            SyncError::Unsupported { path, detail } => write!(f, "Unsupported {path}: {detail}"),
            SyncError::Path { detail } => write!(f, "Unsafe path: {detail}"),
            SyncError::Inventory { detail } => write!(f, "Vault inventory failed: {detail}"),
            SyncError::Limit { detail } => write!(f, "Sync limit exceeded: {detail}"),
            SyncError::Credentials { detail } => write!(f, "Credentials unavailable: {detail}"),
            SyncError::LeaseBusy => {
                write!(f, "Another Muninn instance is writing this vault")
            }
            SyncError::DirtyBuffer { path } => {
                write!(
                    f,
                    "Save or discard unsaved edits in {path} before sync can change it"
                )
            }
            SyncError::LocalChanged { path } => {
                write!(
                    f,
                    "{path} changed during sync; run Sync Now again to replan"
                )
            }
            SyncError::Journal { detail } => write!(f, "Sync journal: {detail}"),
            SyncError::Protocol { detail } => write!(f, "Provider protocol: {detail}"),
            SyncError::InjectedFault { at } => write!(f, "Injected fault at {at}"),
            SyncError::Config { detail } => write!(f, "Sync is not configured: {detail}"),
            SyncError::Io { detail } => write!(f, "File error: {detail}"),
        }
    }
}

impl std::error::Error for SyncError {}

/// Keep a provider error message, dropping anything that looks like a token
/// or an OAuth code. Bodies are capped so a hostile response cannot flood the UI.
#[must_use]
pub fn sanitize_provider_text(text: &str) -> String {
    let mut out = String::new();
    for (index, line) in text.lines().take(8).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let lower = line.to_ascii_lowercase();
        if lower.contains("access_token")
            || lower.contains("refresh_token")
            || lower.contains("client_secret")
            || lower.contains("authorization:")
            || lower.contains("device_code")
        {
            out.push_str("[redacted]");
            continue;
        }
        let trimmed = line.trim();
        if out.len() + trimmed.len() > 280 {
            out.push('…');
            break;
        }
        out.push_str(trimmed);
    }
    if out.is_empty() {
        "empty response".to_owned()
    } else {
        out
    }
}

/// Pull `message` / `error_description` out of a provider JSON body.
#[must_use]
pub fn provider_message(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return sanitize_provider_text(body);
    };
    let message = value
        .get("message")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("error_description").and_then(|v| v.as_str()))
        .or_else(|| value.get("error").and_then(|v| v.as_str()))
        .or_else(|| value.pointer("/error/message").and_then(|v| v.as_str()));
    sanitize_provider_text(message.unwrap_or("unrecognized error"))
}

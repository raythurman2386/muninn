//! HTTP boundary. Production calls go through reqwest to an allowlisted host.
//! Tests point the same client at a loopback server; nothing in this module
//! reads ambient tokens.

use std::time::Duration;

use url::Url;

use crate::error::{SyncError, provider_message};

#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// When true, a timeout is reported as an unknown outcome rather than a
    /// clean retry. GET and token probes are not mutations.
    pub mutation: bool,
    pub timeout: Duration,
}

impl HttpRequest {
    pub fn redacted(&self) -> String {
        format!(
            "{} {} headers={} body_bytes={}",
            self.method,
            redact_url(&self.url),
            self.headers.len(),
            self.body.len()
        )
    }
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then_some(value.as_str()))
    }

    pub fn text(&self) -> Result<&str, SyncError> {
        std::str::from_utf8(&self.body).map_err(|_| SyncError::Protocol {
            detail: "provider response was not utf-8".to_owned(),
        })
    }

    pub fn json(&self) -> Result<serde_json::Value, SyncError> {
        let text = self.text()?;
        serde_json::from_str(text).map_err(|_| SyncError::Protocol {
            detail: "provider response was not json".to_owned(),
        })
    }

    #[must_use]
    pub fn retry_after_ms(&self) -> Option<u64> {
        let value = self.header("retry-after")?;
        if let Ok(seconds) = value.trim().parse::<u64>() {
            return Some(seconds.saturating_mul(1000));
        }
        None
    }
}

pub trait HttpTransport: Send + Sync {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, SyncError>;
}

impl<T: HttpTransport + ?Sized> HttpTransport for &T {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, SyncError> {
        (*self).execute(request)
    }
}

#[cfg(feature = "native")]
pub struct ReqwestTransport {
    client: reqwest::blocking::Client,
    extra_hosts: Vec<String>,
}

#[cfg(feature = "native")]
impl ReqwestTransport {
    pub fn production() -> Result<Self, SyncError> {
        Self::build(&[])
    }

    /// Loopback only. Production hosts stay allowlisted; `127.0.0.1` is added
    /// for tests and is rejected unless the scheme is `http`.
    pub fn for_tests(extra_hosts: &[&str]) -> Result<Self, SyncError> {
        let hosts = extra_hosts
            .iter()
            .map(|host| (*host).to_owned())
            .collect::<Vec<_>>();
        Self::build(&hosts)
    }

    fn build(extra_hosts: &[String]) -> Result<Self, SyncError> {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .user_agent(concat!("muninn-sync/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| SyncError::Io {
                detail: format!("could not build the http client: {error}"),
            })?;
        Ok(Self {
            client,
            extra_hosts: extra_hosts.to_vec(),
        })
    }
}

#[cfg(feature = "native")]
impl HttpTransport for ReqwestTransport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, SyncError> {
        ensure_allowed(&request.url, &self.extra_hosts)?;
        let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|_| {
            SyncError::Protocol {
                detail: format!("bad method {}", request.method),
            }
        })?;
        let mut builder = self
            .client
            .request(method, &request.url)
            .timeout(request.timeout)
            .body(request.body.clone());
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        match builder.send() {
            Ok(response) => {
                let status = response.status().as_u16();
                let headers = response
                    .headers()
                    .iter()
                    .filter_map(|(name, value)| {
                        let value = value.to_str().ok()?.to_owned();
                        if name.as_str().eq_ignore_ascii_case("set-cookie") {
                            return None;
                        }
                        Some((name.to_string(), value))
                    })
                    .collect();
                let body = response
                    .bytes()
                    .map_err(|error| map_reqwest(error, request.mutation))?
                    .to_vec();
                Ok(HttpResponse {
                    status,
                    headers,
                    body,
                })
            }
            Err(error) => Err(map_reqwest(error, request.mutation)),
        }
    }
}

#[cfg(feature = "native")]
fn map_reqwest(error: reqwest::Error, mutation: bool) -> SyncError {
    if error.is_timeout() {
        return if mutation {
            SyncError::Timeout { mutation: true }
        } else {
            SyncError::Timeout { mutation: false }
        };
    }
    if error.is_connect() || error.is_request() {
        return SyncError::Offline {
            detail: "could not reach the provider".to_owned(),
        };
    }
    SyncError::Offline {
        detail: "provider request failed".to_owned(),
    }
}

/// The native allowlist: https, the provider hosts above, and a Tailscale host
/// that passed [`crate::tailnet::validate_tailnet_host`].
///
/// The browser transport uses [`ensure_web_allowed`] instead: it is narrower
/// on purpose, because a browser run reaches the GitHub API and Google's Drive
/// hosts only — never the Tailscale host, and never GitHub's browser-blocked
/// OAuth pages.
#[cfg(feature = "native")]
pub fn ensure_allowed(raw: &str, extra_hosts: &[String]) -> Result<(), SyncError> {
    let url = Url::parse(raw).map_err(|_| SyncError::Protocol {
        detail: "refusing a provider url that did not parse".to_owned(),
    })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(SyncError::Protocol {
            detail: "refusing a provider url with embedded credentials".to_owned(),
        });
    }
    let host = url.host_str().unwrap_or("");
    let loopback = host == "127.0.0.1" || host == "localhost";
    if loopback {
        if url.scheme() != "http" || !extra_hosts.iter().any(|extra| extra == host) {
            return Err(SyncError::Protocol {
                detail: "loopback is only available to the test client".to_owned(),
            });
        }
        return Ok(());
    }
    if url.scheme() != "https" {
        return Err(SyncError::Protocol {
            detail: format!("refusing non-https host {host}"),
        });
    }
    const HOSTS: &[&str] = &[
        "api.github.com",
        "github.com",
        "www.googleapis.com",
        "oauth2.googleapis.com",
        "accounts.google.com",
        "upload.googleapis.com",
    ];
    if HOSTS.contains(&host) || crate::tailnet::validate_tailnet_host(host).is_ok() {
        Ok(())
    } else {
        Err(SyncError::Protocol {
            detail: format!("host {host} is not an allowlisted sync provider"),
        })
    }
}

/// The GitHub host the browser's data plane talks to.
///
/// PAT sync needs `api.github.com` for reads and writes. The page's OAuth
/// endpoints (`github.com/login/...`) are browser-blocked and are not part of
/// a PAT run.
pub const WEB_GITHUB_API_HOST: &str = "api.github.com";

/// The Google hosts the browser's Drive run talks to.
///
/// These are the native allowlist's Google entries, unchanged: the Drive v3
/// API and its resumable-upload endpoint, the token endpoint the engine uses
/// to refresh an expiring access token, and the consent page that Google
/// Identity Services opens in its own popup. The popup is a page, not a
/// transport call, but the host stays listed so the client-id check fails
/// closed against the same set a desktop build uses.
pub const WEB_GOOGLE_HOSTS: &[&str] = &[
    "www.googleapis.com",
    "oauth2.googleapis.com",
    "accounts.google.com",
    "upload.googleapis.com",
];

/// The browser twin of the native allowlist (`ensure_allowed`): https, one of
/// [`WEB_GITHUB_API_HOST`] or [`WEB_GOOGLE_HOSTS`], no credentials in the URL,
/// no explicit port.
///
/// This is not a superset of the native list on purpose. A browser transport
/// must never be able to reach the browser-blocked GitHub OAuth endpoints or a
/// Tailnet host just because a native build may. GitHub's data plane and
/// Google's Drive hosts are the complete set a browser run needs: PAT sync
/// (slice 8) and GIS Drive sync (slice 9).
pub fn ensure_web_allowed(raw: &str) -> Result<(), SyncError> {
    let url = Url::parse(raw).map_err(|_| SyncError::Protocol {
        detail: "refusing a provider url that did not parse".to_owned(),
    })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(SyncError::Protocol {
            detail: "refusing a provider url with embedded credentials".to_owned(),
        });
    }
    if url.scheme() != "https" {
        return Err(SyncError::Protocol {
            detail: format!("the browser transport requires https, got {}", url.scheme()),
        });
    }
    if url.port().is_some() {
        return Err(SyncError::Protocol {
            detail: "refusing a provider url with an explicit port".to_owned(),
        });
    }
    let host = url.host_str().unwrap_or("");
    if host != WEB_GITHUB_API_HOST && !WEB_GOOGLE_HOSTS.contains(&host) {
        return Err(SyncError::Protocol {
            detail: format!(
                "host {host} is not available to the browser sync transport; only {WEB_GITHUB_API_HOST} and the Google Drive hosts are"
            ),
        });
    }
    Ok(())
}

pub fn classify_status(response: &HttpResponse, mutation: bool) -> Result<(), SyncError> {
    if (200..300).contains(&response.status) {
        return Ok(());
    }
    let detail = provider_message(response.text().unwrap_or(""));
    let lower = detail.to_ascii_lowercase();
    match response.status {
        401 => Err(SyncError::Auth { detail }),
        429 => Err(SyncError::RateLimited {
            retry_after_ms: response.retry_after_ms(),
        }),
        403 if rate_limited(&lower, response) => Err(SyncError::RateLimited {
            retry_after_ms: response.retry_after_ms(),
        }),
        403 => Err(SyncError::Permission { detail }),
        404 => Err(SyncError::NotFound { detail }),
        409 | 412 | 422 => Err(SyncError::ConflictRemote { detail }),
        500..=599 => Err(SyncError::Server {
            status: response.status,
            detail,
        }),
        _ if mutation => Err(SyncError::OutcomeUnknown { detail }),
        status => Err(SyncError::Protocol {
            detail: format!("unexpected status {status}: {detail}"),
        }),
    }
}

fn rate_limited(detail: &str, response: &HttpResponse) -> bool {
    if response
        .header("x-ratelimit-remaining")
        .is_some_and(|value| value.trim() == "0")
    {
        return true;
    }
    detail.contains("rate limit") || detail.contains("too many requests")
}

pub fn redact_url(raw: &str) -> String {
    let Ok(mut url) = Url::parse(raw) else {
        return "unparsed-url".to_owned();
    };
    let query: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            let key_owned = key.into_owned();
            let lower = key_owned.to_ascii_lowercase();
            let hidden = lower.contains("token")
                || lower.contains("code")
                || lower.contains("secret")
                || lower == "state";
            (
                key_owned,
                if hidden {
                    "redacted".to_owned()
                } else {
                    value.into_owned()
                },
            )
        })
        .collect();
    if !query.is_empty() {
        url.query_pairs_mut().clear().extend_pairs(&query);
    }
    url.set_password(None).ok();
    url.set_username("").ok();
    url.to_string()
}

/// Shared read retry. Mutations do not use this: a timeout after a write is
/// reconciled by the caller instead of being sent again blindly.
pub fn with_retry<T>(
    cancel: &CancelToken,
    sleeper: &dyn Sleeper,
    mut operation: impl FnMut() -> Result<T, SyncError>,
) -> Result<T, SyncError> {
    let mut attempt = 0u32;
    loop {
        cancel.check()?;
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if error.retryable() && attempt < 4 => {
                let delay = match &error {
                    SyncError::RateLimited {
                        retry_after_ms: Some(ms),
                    } => Duration::from_millis((*ms).min(30_000)),
                    _ => Duration::from_millis(50u64.saturating_mul(1 << attempt)),
                };
                sleeper.sleep(delay, cancel)?;
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CancelToken {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl CancelToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn check(&self) -> Result<(), SyncError> {
        if self.cancelled.load(std::sync::atomic::Ordering::SeqCst) {
            Err(SyncError::Cancelled)
        } else {
            Ok(())
        }
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
}

pub trait Sleeper: Send + Sync {
    fn sleep(&self, delay: Duration, cancel: &CancelToken) -> Result<(), SyncError>;
}

/// The desktop sleeper: real wall-clock delay in 50 ms slices so a cancel is
/// noticed promptly. The browser supplies its own (see `muninn-web`): a
/// thread sleep is not available in a `wasm32-unknown-unknown` worker.
#[cfg(feature = "native")]
pub struct RealSleeper;

#[cfg(feature = "native")]
impl Sleeper for RealSleeper {
    fn sleep(&self, delay: Duration, cancel: &CancelToken) -> Result<(), SyncError> {
        let slice = Duration::from_millis(50);
        let mut left = delay;
        while !left.is_zero() {
            cancel.check()?;
            let step = left.min(slice);
            std::thread::sleep(step);
            left = left.saturating_sub(step);
        }
        cancel.check()
    }
}

/// Records delays and returns immediately. Tests use this so retries do not
/// wait on a wall clock.
#[derive(Default)]
pub struct RecordingSleeper {
    pub delays: std::sync::Mutex<Vec<Duration>>,
}

impl Sleeper for RecordingSleeper {
    fn sleep(&self, delay: Duration, cancel: &CancelToken) -> Result<(), SyncError> {
        cancel.check()?;
        self.delays.lock().expect("sleeper").push(delay);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    /// The web allowlist is deliberately narrower than the native one: the
    /// GitHub data plane and Google's Drive hosts, and nothing else. This test
    /// is the boundary's contract.
    #[test]
    fn web_allowlist_is_the_github_api_and_the_google_drive_hosts() {
        for allowed in [
            "https://api.github.com/repos/o/r/git/ref/heads/main",
            "https://api.github.com/repos/o/r/git/trees/abc?recursive=1",
            // Slice 9: the Drive client's three data-plane hosts plus the
            // consent page the GIS popup loads.
            "https://www.googleapis.com/drive/v3/files?q=appDataFolder",
            "https://www.googleapis.com/upload/drive/v3/files?uploadType=resumable",
            "https://oauth2.googleapis.com/token",
            "https://accounts.google.com/o/oauth2/v2/auth",
            "https://upload.googleapis.com/upload/drive/v3/files",
        ] {
            assert!(ensure_web_allowed(allowed).is_ok(), "{allowed} is allowed");
        }
        for refused in [
            // GitHub's OAuth endpoints are browser-blocked; a PAT run never
            // uses them and the worker must not even try.
            "https://github.com/login/oauth/access_token",
            // A Tailnet host is desktop-only until #240's slice 10 decides.
            "https://minis.tail1234.ts.net:8443/state",
            // A Drive-adjacent host that is not on the list, including a
            // Google-hosted one (the list is exact, not a suffix match).
            "https://www.google.com/drive",
            "https://storage.googleapis.com/bucket",
            "https://www.googleapis.com.evil.example/drive/v3/files",
            // Scheme and URL-shape rules.
            "http://api.github.com/repos/o/r",
            "http://www.googleapis.com/drive/v3/files",
            "https://api.github.com:8443/repos/o/r",
            "https://www.googleapis.com:8443/drive/v3/files",
            "https://user:secret@api.github.com/repos/o/r",
            "https://user@www.googleapis.com/drive/v3/files",
            "not a url",
        ] {
            assert!(
                ensure_web_allowed(refused).is_err(),
                "{refused} must be refused"
            );
        }
        assert_eq!(WEB_GOOGLE_HOSTS.len(), 4, "the exact Google host set");
    }

    /// The status→error mapping is shared by both transports, so pin each
    /// class here rather than in the browser-only code.
    #[test]
    fn status_mapping_matches_the_native_contract() {
        let auth = classify_status(&response(401, r#"{"message":"Bad credentials"}"#), false)
            .expect_err("401");
        assert!(matches!(auth, SyncError::Auth { .. }));

        let permission =
            classify_status(&response(403, r#"{"message":"Forbidden"}"#), false).expect_err("403");
        assert!(matches!(permission, SyncError::Permission { .. }));

        let mut limited = response(403, r#"{"message":"rate limit"}"#);
        limited.headers = vec![("Retry-After".to_owned(), "7".to_owned())];
        let limited = classify_status(&limited, false).expect_err("rate limit");
        assert!(matches!(
            limited,
            SyncError::RateLimited {
                retry_after_ms: Some(7000)
            }
        ));

        let conflict = classify_status(&response(409, "{}"), false).expect_err("409");
        assert!(matches!(conflict, SyncError::ConflictRemote { .. }));

        let server = classify_status(&response(503, "down"), false).expect_err("503");
        assert!(matches!(server, SyncError::Server { status: 503, .. }));

        let unknown = classify_status(&response(418, "teapot"), true).expect_err("mutation");
        assert!(matches!(unknown, SyncError::OutcomeUnknown { .. }));
        let protocol = classify_status(&response(418, "teapot"), false).expect_err("read");
        assert!(matches!(protocol, SyncError::Protocol { .. }));

        assert!(classify_status(&response(204, ""), true).is_ok());
    }
}

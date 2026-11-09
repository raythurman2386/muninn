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

pub struct ReqwestTransport {
    client: reqwest::blocking::Client,
    extra_hosts: Vec<String>,
}

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

pub struct RealSleeper;

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

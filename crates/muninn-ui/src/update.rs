//! The notify-only release check: the model and network layer.
//!
//! Muninn asks GitHub's public releases API whether a newer stable release
//! exists and returns that answer to the caller. This module does no drawing
//! and holds no catalog copy: the settings page and the banner that consume
//! [`AvailableUpdate`] live in the UI layer. It never downloads or replaces the
//! binary.
//!
//! # Calling it
//!
//! [`should_check`] decides whether a check is due from the persisted
//! `Settings` fields; [`ReleasesClient::check`] then performs one blocking
//! request. That call belongs on a worker thread: it can block for the client
//! timeout (five seconds by default) and has no state that the UI thread needs
//! to share. Send the [`CheckOutcome`] back over a channel and apply it on the
//! UI thread, exactly as `sync_ui` does for a sync run.
//!
//! The caller persists, in `settings.json`:
//!
//! - `last_update_check` — the `now` passed to a completed check,
//! - `update_next_check` — [`next_due`] on success (see
//!   [`success_resets`]) or [`backoff_until`] after a failure,
//! - `update_failures` — incremented on [`CheckOutcome::Failed`], reset on
//!   success,
//! - `update_etag` — the `etag` in [`CheckOutcome::Available`] or
//!   [`CheckOutcome::UpToDate`] and sent as `If-None-Match` next time,
//! - `last_seen_version` / `skip_version` — the UI's dedupe: suppress the
//!   banner when the available version equals either one,
//! - `check_for_updates` — when `false`, no request is made at all.
//!
//! [`CheckOutcome::NotModified`] is unit-like on purpose: a 304 returns no new
//! ETag, so the caller keeps the one it already has.
//!
//! # Request shape
//!
//! One HTTPS `GET` to [`LATEST_RELEASE_URL`] with `User-Agent:
//! muninn/<version>`, `Accept: application/vnd.github+json` and
//! `X-GitHub-Api-Version: 2022-11-28`, plus `If-None-Match` when an ETag is
//! known. There is no `Authorization` header, no token, no request body, no
//! query string, no vault path and no settings data. Only `tag_name`, `draft`,
//! `prerelease` and `html_url` are read from the response; the release body,
//! title and notes are never stored or logged.

use std::env;
use std::fs;
use std::io::Read as _;
use std::path::Path;
use std::time::Duration;

use semver::Version;
use serde_json::Value;

/// The GitHub releases API endpoint for the latest release.
pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/raythurman2386/muninn/releases/latest";

/// The human-facing releases page, used when a release's `html_url` is absent
/// or does not point at this repository's releases.
pub const RELEASES_PAGE_URL: &str = "https://github.com/raythurman2386/muninn/releases/latest";

/// Seconds between successful checks: one day.
pub const CHECK_INTERVAL_SECS: u64 = 86_400;

/// Largest jitter added to a successful check's next due time: six hours.
/// The caller supplies the jitter (this module has no RNG), so a fleet of
/// installs does not all ask GitHub at the same moment.
pub const JITTER_MAX_SECS: u64 = 6 * 60 * 60;

/// First retry delay after a failed check: one hour.
pub const BACKOFF_BASE_SECS: u64 = 60 * 60;

/// The backoff cap: one day, the same as a normal successful interval.
pub const MAX_BACKOFF_SECS: u64 = 86_400;

/// Default client timeout. A check must never hold up launch or autosave.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// The `Accept` header GitHub asks API clients to send.
const ACCEPT: &str = "application/vnd.github+json";

/// The pinned GitHub REST API version. Without it the response shape follows
/// whatever the account or repository default is, which can change under us.
pub const GITHUB_API_VERSION: &str = "2022-11-28";

/// The only prefixes an accepted `html_url` may start with.
const RELEASE_URL_PREFIX: &str = "https://github.com/raythurman2386/muninn/releases/";

/// A response larger than this is not the release document we expect; refuse
/// it rather than buffer an unbounded body.
const MAX_BODY_BYTES: usize = 64 * 1024;

/// Pacman's local package database, scanned for an AUR `muninn-bin` install.
const AUR_LOCAL_DIR: &str = "/var/lib/pacman/local";

/// Dpkg's per-package file list for the `.deb` build.
const DEB_LIST: &str = "/var/lib/dpkg/info/muninn.list";

/// How this copy of Muninn was installed. The UI maps each to a manual
/// upgrade hint; nothing here ever downloads or runs an installer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallMethod {
    /// A single self-mounting AppImage (`$APPIMAGE`).
    AppImage,
    /// An AUR package (`muninn-bin`).
    Aur,
    /// A Debian/Ubuntu `.deb`.
    Deb,
    /// `install.sh` or a release tarball, identified by its receipt.
    Tarball,
    /// Nothing recognisable; the UI offers the releases page only.
    Unknown,
}

/// The filesystem/environment facts [`detect`] turns into an [`InstallMethod`].
///
/// Kept as plain data so the precedence rule is testable without touching the
/// real system; [`probe_current`] fills it from this machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InstallProbe {
    /// `$APPIMAGE` is set and non-empty.
    pub appimage: bool,
    /// `/var/lib/dpkg/info/muninn.list` exists.
    pub deb_package: bool,
    /// A `/var/lib/pacman/local/muninn-bin-*` entry exists.
    pub aur_package: bool,
    /// `share/muninn/install-receipt` exists beside the executable's prefix.
    pub installer_receipt: bool,
}

/// Pick the install method from a probe. Precedence: AppImage, AUR, `.deb`,
/// tarball receipt, then unknown. An AppImage is self-contained, so it wins
/// even if another marker is present.
#[must_use]
pub fn detect(probe: InstallProbe) -> InstallMethod {
    if probe.appimage {
        InstallMethod::AppImage
    } else if probe.aur_package {
        InstallMethod::Aur
    } else if probe.deb_package {
        InstallMethod::Deb
    } else if probe.installer_receipt {
        InstallMethod::Tarball
    } else {
        InstallMethod::Unknown
    }
}

/// Probe this machine using environment variables and path checks only.
///
/// Deliberately runs no subprocesses (`pacman -Q`, `dpkg -s`): a check must
/// stay cheap and silent. An unreadable path simply reads as absent.
#[must_use]
pub fn probe_current() -> InstallProbe {
    InstallProbe {
        appimage: env::var_os("APPIMAGE").is_some_and(|value| !value.is_empty()),
        deb_package: Path::new(DEB_LIST).is_file(),
        aur_package: aur_installed(Path::new(AUR_LOCAL_DIR)),
        installer_receipt: installer_receipt_present(),
    }
}

fn aur_installed(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with("muninn-bin-"))
    })
}

/// `install.sh` writes `$PREFIX/share/muninn/install-receipt`; with the binary
/// at `$PREFIX/bin/muninn`, the prefix is the executable's grandparent.
fn installer_receipt_present() -> bool {
    let Ok(exe) = env::current_exe() else {
        return false;
    };
    let Some(prefix) = exe.parent().and_then(Path::parent) else {
        return false;
    };
    prefix.join("share/muninn/install-receipt").is_file()
}

/// A newer stable release the caller may offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableUpdate {
    /// The release version without the tag's `v` prefix, e.g. `0.9.4`.
    pub version: String,
    /// The release page, always an `https://github.com/raythurman2386/muninn/
    /// releases/…` URL.
    pub url: String,
    /// How this copy was installed, for the matching upgrade hint.
    pub method: InstallMethod,
    /// The API's ETag for this response, to persist for `If-None-Match`.
    pub etag: Option<String>,
}

/// The result of one check. Every variant is safe to send across threads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    /// A newer stable release exists.
    Available(AvailableUpdate),
    /// The current version is current or newer. `etag` (when present) is the
    /// new ETag to persist.
    UpToDate {
        /// The API's ETag for this response, if it sent one.
        etag: Option<String>,
    },
    /// The server answered 304: the stored ETag is still valid, so nothing new
    /// arrived and the caller keeps that ETag unchanged.
    NotModified,
    /// Offline, timed out, a non-success status, or an unreadable response.
    /// Intentionally carries no detail: the caller stays silent and no
    /// response data is logged.
    Failed,
}

/// One-shot client for the public releases endpoint.
///
/// [`ReleasesClient::github`] is the production client; [`with_base_url`]
/// points the same request at a local fixture in tests.
///
/// [`with_base_url`]: ReleasesClient::with_base_url
#[derive(Clone, Debug)]
pub struct ReleasesClient {
    base_url: String,
    timeout: Duration,
    probe: InstallProbe,
}

impl ReleasesClient {
    /// The production client for [`LATEST_RELEASE_URL`].
    #[must_use]
    pub fn github(timeout: Duration) -> Self {
        Self {
            base_url: LATEST_RELEASE_URL.to_owned(),
            timeout,
            probe: probe_current(),
        }
    }

    /// A client for `base_url`, used by fixture tests and nothing else.
    #[must_use]
    pub fn with_base_url(base_url: impl Into<String>, timeout: Duration) -> Self {
        Self {
            base_url: base_url.into(),
            timeout,
            probe: probe_current(),
        }
    }

    /// Replace the install probe. Tests use this to make [`detect`]'s input
    /// deterministic; production callers use [`ReleasesClient::github`]'s.
    #[must_use]
    pub fn with_probe(mut self, probe: InstallProbe) -> Self {
        self.probe = probe;
        self
    }

    /// Ask GitHub once. `current` is the running version and `etag` the value
    /// stored from the previous response, if any.
    ///
    /// Never panics and never returns error detail: every failure is
    /// [`CheckOutcome::Failed`].
    #[must_use]
    pub fn check(&self, current: &Version, etag: Option<&str>) -> CheckOutcome {
        let Some(client) = self.build_client() else {
            return CheckOutcome::Failed;
        };
        let mut request = client
            .get(&self.base_url)
            .header(reqwest::header::ACCEPT, ACCEPT)
            .header("x-github-api-version", GITHUB_API_VERSION);
        if let Some(etag) = etag.filter(|value| !value.is_empty()) {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let Ok(response) = request.send() else {
            return CheckOutcome::Failed;
        };
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return CheckOutcome::NotModified;
        }
        if !response.status().is_success() {
            return CheckOutcome::Failed;
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let Some(body) = read_body(response) else {
            return CheckOutcome::Failed;
        };
        self.parse(&body, current, etag)
    }

    fn build_client(&self) -> Option<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            // The API endpoint does not redirect; refusing redirects also
            // keeps the request on the one allowlisted host. A loopback
            // fixture in tests must not be routed through a system proxy.
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .user_agent(concat!("muninn/", env!("CARGO_PKG_VERSION")))
            .build()
            .ok()
    }

    /// Read only the four fields the model needs. The release body, name and
    /// notes are ignored, not extracted.
    fn parse(&self, body: &[u8], current: &Version, etag: Option<String>) -> CheckOutcome {
        let Ok(value) = serde_json::from_slice::<Value>(body) else {
            return CheckOutcome::Failed;
        };
        let draft = value.get("draft").and_then(Value::as_bool).unwrap_or(false);
        let prerelease = value
            .get("prerelease")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if draft || prerelease {
            return CheckOutcome::UpToDate { etag };
        }
        let Some(version) = value
            .get("tag_name")
            .and_then(Value::as_str)
            .and_then(parse_tag)
        else {
            return CheckOutcome::Failed;
        };
        if !version.pre.is_empty() {
            return CheckOutcome::UpToDate { etag };
        }
        let url = value
            .get("html_url")
            .and_then(Value::as_str)
            .filter(|url| is_release_url(url))
            .unwrap_or(RELEASES_PAGE_URL)
            .to_owned();
        if version <= *current {
            return CheckOutcome::UpToDate { etag };
        }
        CheckOutcome::Available(AvailableUpdate {
            version: version.to_string(),
            url,
            method: detect(self.probe),
            etag,
        })
    }
}

/// Accept `v0.9.4` and `0.9.4` and require valid semver. A tag that carries a
/// pre-release suffix parses here but is treated as non-stable by [`parse`].
///
/// [`parse`]: ReleasesClient::parse
fn parse_tag(tag: &str) -> Option<Version> {
    let tag = tag.strip_prefix('v').unwrap_or(tag);
    Version::parse(tag).ok()
}

/// Only this repository's release pages are offered, so a malformed or hostile
/// response cannot point the banner elsewhere.
fn is_release_url(url: &str) -> bool {
    url.starts_with(RELEASE_URL_PREFIX)
}

/// Read at most [`MAX_BODY_BYTES`]; `None` for a read error or an oversized
/// body.
fn read_body(mut response: reqwest::blocking::Response) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    response
        .by_ref()
        .take((MAX_BODY_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .ok()?;
    (body.len() <= MAX_BODY_BYTES).then_some(body)
}

/// When a completed successful check is due next: one day plus jitter, the
/// jitter clamped to [`JITTER_MAX_SECS`].
#[must_use]
pub fn next_due(last_check: u64, jitter: u64) -> u64 {
    last_check
        .saturating_add(CHECK_INTERVAL_SECS)
        .saturating_add(jitter.min(JITTER_MAX_SECS))
}

/// Whether a check is due now.
///
/// `enabled` is `Settings::check_for_updates`; when it is false this is always
/// false, and the caller must not construct a request. `next_check` wins when
/// present (it is already the persisted schedule); otherwise the due time is
/// [`next_due`] of `last_check`; with neither, this is the first run and the
/// check is due.
#[must_use]
pub fn should_check(
    now: u64,
    last_check: Option<u64>,
    next_check: Option<u64>,
    enabled: bool,
    jitter: u64,
) -> bool {
    if !enabled {
        return false;
    }
    match (last_check, next_check) {
        (_, Some(next)) => now >= next,
        (Some(last), None) => now >= next_due(last, jitter),
        (None, None) => true,
    }
}

/// When to retry after `failures` consecutive failed checks: one hour,
/// doubling, capped at [`MAX_BACKOFF_SECS`]. `failures == 0` returns `now`
/// (there is nothing to back off from).
///
/// Saturating, so an absurd clock or count cannot overflow.
#[must_use]
pub fn backoff_until(now: u64, failures: u32) -> u64 {
    if failures == 0 {
        return now;
    }
    // 1 h, 2 h, 4 h, 8 h, 16 h, then the 24 h cap. Five doublings already
    // exceed the cap, so larger counts only need `min`.
    let doublings = failures.saturating_sub(1).min(5);
    let delay = BACKOFF_BASE_SECS
        .checked_shl(doublings)
        .unwrap_or(MAX_BACKOFF_SECS)
        .min(MAX_BACKOFF_SECS);
    now.saturating_add(delay)
}

/// The `(update_failures, update_next_check)` pair to persist after the server
/// answered at all (200 or 304, newer or not): clear the failures and schedule
/// the next routine check.
#[must_use]
pub fn success_resets(now: u64, jitter: u64) -> (u32, Option<u64>) {
    (0, Some(next_due(now, jitter)))
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::net::TcpListener;

    use super::*;

    /// Serve exactly one request on loopback, return the recorded request
    /// bytes and write `status`/`headers`/`body`. The base URL points at
    /// `/repos/raythurman2386/muninn/releases/latest`, the production path.
    fn fixture(
        status: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> (String, std::thread::JoinHandle<(String, usize)>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("addr").port();
        let status = status.to_owned();
        let headers = headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<Vec<_>>();
        let body = body.to_owned();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut request = String::new();
            let mut line = String::new();
            let mut length = 0usize;
            loop {
                line.clear();
                assert!(reader.read_line(&mut line).expect("read line") > 0);
                if line == "\r\n" || line == "\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().expect("content-length");
                }
                request.push_str(&line);
            }
            let mut sent = vec![0; length];
            reader.read_exact(&mut sent).expect("read body");
            request.push_str(&String::from_utf8(sent).expect("utf-8 body"));
            let mut reply = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
            for (name, value) in &headers {
                reply.push_str(&format!("{name}: {value}\r\n"));
            }
            reply.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
            stream
                .try_clone()
                .expect("stream")
                .write_all(reply.as_bytes())
                .expect("write reply");
            (request, length)
        });
        (
            format!("http://127.0.0.1:{port}/repos/raythurman2386/muninn/releases/latest"),
            handle,
        )
    }

    const NEWER: &str = r#"{
        "tag_name": "v0.10.0",
        "draft": false,
        "prerelease": false,
        "html_url": "https://github.com/raythurman2386/muninn/releases/tag/v0.10.0",
        "body": "private notes that must not be stored"
    }"#;

    fn client(base: String) -> ReleasesClient {
        ReleasesClient::with_base_url(base, Duration::from_secs(2))
    }

    const CURRENT: Version = Version::new(0, 9, 3);

    #[test]
    fn a_newer_stable_release_is_available() {
        let (base, server) = fixture("200 OK", &[("ETag", "\"release-1\"")], NEWER);
        let outcome = client(base)
            .with_probe(InstallProbe {
                deb_package: true,
                ..InstallProbe::default()
            })
            .check(&CURRENT, None);
        let request = server.join().expect("server thread");
        assert_eq!(
            outcome,
            CheckOutcome::Available(AvailableUpdate {
                version: "0.10.0".to_owned(),
                url: "https://github.com/raythurman2386/muninn/releases/tag/v0.10.0".to_owned(),
                method: InstallMethod::Deb,
                etag: Some("\"release-1\"".to_owned()),
            })
        );
        assert!(
            !request.0.to_ascii_lowercase().contains("authorization"),
            "{}",
            request.0
        );
        assert_eq!(request.1, 0, "no request body: {}", request.0);
    }

    #[test]
    fn the_same_and_older_versions_are_up_to_date() {
        for body in [
            r#"{"tag_name":"v0.9.3","draft":false,"prerelease":false}"#,
            r#"{"tag_name":"0.9.2","draft":false,"prerelease":false}"#,
        ] {
            let (base, server) = fixture("200 OK", &[("ETag", "abc")], body);
            assert_eq!(
                client(base).check(&CURRENT, None),
                CheckOutcome::UpToDate {
                    etag: Some("abc".to_owned())
                },
                "{body}"
            );
            server.join().expect("server thread");
        }
    }

    #[test]
    fn drafts_and_prereleases_are_ignored() {
        for body in [
            r#"{"tag_name":"v0.10.0","draft":true,"prerelease":false}"#,
            r#"{"tag_name":"v0.10.0","draft":false,"prerelease":true}"#,
            r#"{"tag_name":"v0.10.0-rc.1","draft":false,"prerelease":false}"#,
        ] {
            let (base, server) = fixture("200 OK", &[], body);
            assert_eq!(
                client(base).check(&CURRENT, None),
                CheckOutcome::UpToDate { etag: None },
                "{body}"
            );
            server.join().expect("server thread");
        }
    }

    #[test]
    fn an_invalid_tag_or_payload_fails() {
        for body in [
            r#"{"tag_name":"nightly","draft":false,"prerelease":false}"#,
            r#"{"tag_name":null,"draft":false,"prerelease":false}"#,
            r#"{"draft":false,"prerelease":false}"#,
            "{not json",
        ] {
            let (base, server) = fixture("200 OK", &[], body);
            assert_eq!(
                client(base).check(&CURRENT, None),
                CheckOutcome::Failed,
                "{body}"
            );
            server.join().expect("server thread");
        }
    }

    #[test]
    fn a_304_is_not_modified_and_keeps_the_callers_etag() {
        let (base, server) = fixture("304 Not Modified", &[], "");
        let outcome = client(base).check(&CURRENT, Some("\"release-1\""));
        let (request, _) = server.join().expect("server thread");
        assert_eq!(outcome, CheckOutcome::NotModified);
        assert!(
            request
                .to_ascii_lowercase()
                .contains("if-none-match: \"release-1\""),
            "{request}"
        );
    }

    #[test]
    fn a_200_etag_is_returned_and_sent_back_next_time() {
        let (first_base, first_server) = fixture("200 OK", &[("ETag", "\"v1\"")], NEWER);
        let first = client(first_base).check(&CURRENT, None);
        first_server.join().expect("server thread");
        let CheckOutcome::Available(update) = first else {
            panic!("expected an available update, got {first:?}");
        };
        assert_eq!(update.etag.as_deref(), Some("\"v1\""));

        let (second_base, second_server) = fixture("304 Not Modified", &[], "");
        let second = client(second_base).check(&CURRENT, update.etag.as_deref());
        let (request, _) = second_server.join().expect("server thread");
        assert_eq!(second, CheckOutcome::NotModified);
        assert!(request.contains("\"v1\""), "{request}");
    }

    #[test]
    fn a_hostile_html_url_falls_back_to_the_releases_page() {
        let (base, server) = fixture(
            "200 OK",
            &[],
            r#"{"tag_name":"v0.10.0","draft":false,"prerelease":false,
                "html_url":"https://evil.example/muninn"}"#,
        );
        let outcome = client(base).check(&CURRENT, None);
        server.join().expect("server thread");
        let CheckOutcome::Available(update) = outcome else {
            panic!("expected an available update, got {outcome:?}");
        };
        assert_eq!(update.url, RELEASES_PAGE_URL);
    }

    #[test]
    fn the_request_has_the_documented_headers_and_path() {
        let (base, server) = fixture("200 OK", &[("ETag", "\"e\"")], NEWER);
        let _ = client(base).check(&CURRENT, None);
        let (request, body_len) = server.join().expect("server thread");
        let lower = request.to_ascii_lowercase();
        assert!(
            lower.contains(&format!("user-agent: muninn/{}", env!("CARGO_PKG_VERSION"))),
            "{request}"
        );
        assert!(
            lower.contains("accept: application/vnd.github+json"),
            "{request}"
        );
        assert!(
            lower.contains(&format!("x-github-api-version: {GITHUB_API_VERSION}")),
            "{request}"
        );
        assert!(!lower.contains("authorization"), "{request}");
        assert!(!lower.contains("if-none-match"), "{request}");
        assert_eq!(body_len, 0, "a check has no request body: {request}");
        let request_line = request.lines().next().expect("request line");
        // The production client sends the same path to the API host.
        assert_eq!(
            request_line,
            "GET /repos/raythurman2386/muninn/releases/latest HTTP/1.1"
        );
        assert_eq!(
            LATEST_RELEASE_URL,
            "https://api.github.com/repos/raythurman2386/muninn/releases/latest"
        );
    }

    #[test]
    fn a_server_error_or_connection_failure_is_silent() {
        let (base, server) = fixture("500 Internal Server Error", &[], "{}");
        assert_eq!(client(base).check(&CURRENT, None), CheckOutcome::Failed);
        server.join().expect("server thread");

        // Bind and drop to get a port nothing listens on.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let base = format!("http://127.0.0.1:{port}/repos/raythurman2386/muninn/releases/latest");
        assert_eq!(client(base).check(&CURRENT, None), CheckOutcome::Failed);
    }

    #[test]
    fn scheduling_waits_a_day_and_respects_jitter() {
        assert_eq!(next_due(1_000, 0), 1_000 + CHECK_INTERVAL_SECS);
        assert_eq!(next_due(1_000, 900), 1_000 + CHECK_INTERVAL_SECS + 900);
        // Jitter beyond the cap is clamped to six hours.
        assert_eq!(
            next_due(1_000, u64::MAX),
            1_000 + CHECK_INTERVAL_SECS + JITTER_MAX_SECS
        );
        let due = next_due(1_000, 60);
        assert!(!should_check(due - 1, Some(1_000), None, true, 60));
        assert!(should_check(due, Some(1_000), None, true, 60));
        assert!(should_check(due + 1, Some(1_000), None, true, 60));
        // An explicit schedule wins over last_check.
        assert!(should_check(50, Some(1_000), Some(50), true, 0));
        assert!(!should_check(49, Some(1_000), Some(50), true, 0));
        // A first run is due; disabled never is.
        assert!(should_check(0, None, None, true, 0));
        assert!(!should_check(u64::MAX, None, None, false, 0));
    }

    #[test]
    fn backoff_doubles_hourly_and_caps_at_a_day() {
        assert_eq!(backoff_until(10, 0), 10);
        assert_eq!(backoff_until(10, 1), 10 + 3_600);
        assert_eq!(backoff_until(10, 2), 10 + 7_200);
        assert_eq!(backoff_until(10, 3), 10 + 14_400);
        assert_eq!(backoff_until(10, 4), 10 + 28_800);
        assert_eq!(backoff_until(10, 5), 10 + 57_600);
        assert_eq!(backoff_until(10, 6), 10 + MAX_BACKOFF_SECS);
        assert_eq!(backoff_until(10, u32::MAX), 10 + MAX_BACKOFF_SECS);
        assert_eq!(backoff_until(u64::MAX, 9), u64::MAX);
        assert_eq!(success_resets(10, 0), (0, Some(next_due(10, 0))));
    }

    #[test]
    fn install_detection_has_a_fixed_precedence() {
        let only = |appimage, aur_package, deb_package, installer_receipt| InstallProbe {
            appimage,
            aur_package,
            deb_package,
            installer_receipt,
        };
        assert_eq!(detect(InstallProbe::default()), InstallMethod::Unknown);
        assert_eq!(
            detect(only(false, false, false, true)),
            InstallMethod::Tarball
        );
        assert_eq!(detect(only(false, false, true, true)), InstallMethod::Deb);
        assert_eq!(detect(only(false, true, true, true)), InstallMethod::Aur);
        assert_eq!(
            detect(only(true, true, true, true)),
            InstallMethod::AppImage
        );
    }
}

//! Verified self-update: plan, download, verify, extract and atomically install.
//!
//! This is the engine half of #254. It turns an [`AvailableUpdate`] from the
//! notify-only check into a new installed binary, and it holds no UI: the
//! banner's Update action and its progress display arrive in a stacked change
//! and call [`perform`] on a worker thread, reporting [`Phase`] through a
//! callback.
//!
//! # Policy, shared with `install.sh`
//!
//! - Only tarball (`install.sh`) and AppImage installs update themselves. AUR
//!   and `.deb` installs are refused so their package manager stays in charge.
//! - The release archive comes from `github.com`, and every redirect must land
//!   on `github.com` or a GitHub asset host over HTTPS.
//! - The SHA-256 in `checksums.txt` is mandatory: a missing or mismatched
//!   checksum is fatal.
//! - The Sigstore policy is the installer's: cosign first (bundle on
//!   `checksums.txt`, workflow identity and GitHub's OIDC issuer), then `gh
//!   attestation verify`. A missing or failing verifier is a [`SignatureStatus::Warning`],
//!   never fatal. No user token is passed to either tool.
//! - Files are replaced by writing a staging file beside the target, syncing
//!   it and renaming over the target. Only the binary, the receipt-listed
//!   desktop/icons and one rollback copy are written: never `~/.config/muninn`,
//!   sync state or a vault.
//!
//! The hosts that need network access ([`Downloader`]) and the hosts that need
//! a real installation are kept apart from the pure pieces ([`plan`],
//! [`platform_for`], [`parse_checksums`], [`verify_checksum`], [`extract`],
//! [`layout_for`], [`read_receipt`]) so tests run offline against fixtures.
//!
//! Self-update is implemented on Unix only. Windows compiles and reports
//! [`UpdateError::Unsupported`]; the Windows packaging work comes later.

use std::env;
use std::fs;
use std::io::{self, Read as _, Write as _};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use flate2::read::GzDecoder;
use semver::Version;
use sha2::{Digest as _, Sha256};

use crate::update::{AvailableUpdate, InstallMethod};

/// The GitHub repository releases are published from.
pub const REPO: &str = "raythurman2386/muninn";

/// The release download root, the same one `install.sh` uses.
pub const DOWNLOAD_BASE: &str = "https://github.com/raythurman2386/muninn/releases/download";

/// The workflow identity the release signature must carry. This is the exact
/// expression `install.sh` passes to `cosign verify-blob`.
pub const COSIGN_IDENTITY: &str =
    "^https://github.com/raythurman2386/muninn/.github/workflows/release.yml@refs/tags/";

/// The OIDC issuer GitHub Actions uses, as `install.sh` pins it.
pub const OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";

/// Cap on the release archive: 256 MiB. A larger body is not a Muninn build.
const ARCHIVE_MAX_BYTES: usize = 256 * 1024 * 1024;

/// Cap on one desktop/icon asset: 8 MiB.
const ASSET_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Cap on `checksums.txt`: 64 KiB.
const CHECKSUMS_MAX_BYTES: usize = 64 * 1024;

/// Cap on the Sigstore bundle: 1 MiB. Certificates and a log entry fit well
/// inside it.
const BUNDLE_MAX_BYTES: usize = 1024 * 1024;

/// How long a verifier may run before it is killed: 30 seconds. A stuck
/// `cosign` or `gh` must not hang the update.
const VERIFIER_TIMEOUT: Duration = Duration::from_secs(30);

/// How often a running verifier is polled for exit.
const VERIFIER_POLL: Duration = Duration::from_millis(50);

/// Cap on one verifier stream, so a chatty or hostile tool cannot grow the
/// captured log without bound.
const VERIFIER_LOG_MAX_BYTES: u64 = 64 * 1024;

/// Where the one rollback copy lives below `state_home`.
const PREVIOUS_NAME: &str = "muninn.previous";

/// Anything that stops an update. The old binary is intact for every variant
/// raised before the rename; [`apply`] documents the one case that can follow a
/// successful rename (an optional asset refresh) and reports it as a warning
/// instead.
#[derive(Debug)]
pub enum UpdateError {
    /// The release is not newer than the running version.
    NotNewer {
        /// The offered release version.
        version: String,
        /// The running version.
        current: String,
    },
    /// The release version is not valid semver.
    Version(String),
    /// The install method or platform has no update path.
    Unsupported(String),
    /// A filesystem operation failed.
    Io {
        /// What was attempted, in the present tense, e.g. `replace the binary`.
        action: &'static str,
        /// The path it was attempted on.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// A download failed, returned a non-success status, or came from a host
    /// outside GitHub.
    Http(String),
    /// A body was larger than its cap.
    TooLarge {
        /// What was too large.
        what: &'static str,
        /// The cap in bytes.
        limit: usize,
    },
    /// `checksums.txt` has no line for the asset. Treated like a mismatch.
    ChecksumMissing {
        /// The release asset name.
        asset: String,
    },
    /// The SHA-256 did not match. Always fatal.
    Checksum {
        /// The release asset name.
        asset: String,
        /// The hash `checksums.txt` lists.
        expected: String,
        /// The hash of the bytes we downloaded.
        actual: String,
    },
    /// The archive is not the expected release archive.
    Archive(String),
    /// This installation does not have the layout an update needs.
    Layout(String),
    /// The install receipt is missing, malformed or unsafe.
    Receipt(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotNewer { version, current } => {
                write!(
                    formatter,
                    "muninn {version} is not newer than the installed {current}"
                )
            }
            Self::Version(version) => {
                write!(formatter, "release version '{version}' is not valid semver")
            }
            Self::Unsupported(detail) => {
                write!(formatter, "self-update is not available: {detail}")
            }
            Self::Io {
                action,
                path,
                source,
            } => write!(formatter, "could not {action} {}: {source}", path.display()),
            Self::Http(detail) => write!(formatter, "download failed: {detail}"),
            Self::TooLarge { what, limit } => {
                write!(formatter, "{what} is larger than the {limit}-byte limit")
            }
            Self::ChecksumMissing { asset } => {
                write!(formatter, "checksums.txt does not list {asset}")
            }
            Self::Checksum {
                asset,
                expected,
                actual,
            } => write!(
                formatter,
                "SHA-256 mismatch for {asset}: checksums.txt says {expected}, the download is {actual}"
            ),
            Self::Archive(detail) => {
                write!(formatter, "the release archive is not usable: {detail}")
            }
            Self::Layout(detail) => {
                write!(
                    formatter,
                    "this installation cannot be updated here: {detail}"
                )
            }
            Self::Receipt(detail) => {
                write!(formatter, "the install receipt is not usable: {detail}")
            }
        }
    }
}

impl std::error::Error for UpdateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Map one release to the asset this copy can install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdatePlan {
    /// The release version without the tag's `v`, e.g. `0.10.0`.
    pub version: String,
    /// The release asset file name, e.g. `muninn_v0.10.0_linux_x86_64.tar.gz`.
    pub asset: String,
    /// The release platform tag, e.g. `linux_x86_64`.
    pub platform: String,
    /// How this copy was installed; only [`InstallMethod::Tarball`] and
    /// [`InstallMethod::AppImage`] plan.
    pub method: InstallMethod,
}

impl UpdatePlan {
    /// The release tag, `v` plus the version.
    #[must_use]
    pub fn tag(&self) -> String {
        format!("v{}", self.version)
    }

    /// The release asset URL.
    #[must_use]
    pub fn archive_url(&self) -> String {
        format!("{}/{}/{}", DOWNLOAD_BASE, self.tag(), self.asset)
    }

    /// The checksums URL for this release.
    #[must_use]
    pub fn checksums_url(&self) -> String {
        format!("{}/{}/checksums.txt", DOWNLOAD_BASE, self.tag())
    }

    /// The cosign bundle URL for this release.
    #[must_use]
    pub fn bundle_url(&self) -> String {
        format!(
            "{}/{}/checksums.txt.sigstore.json",
            DOWNLOAD_BASE,
            self.tag()
        )
    }
}

/// Map an OS/arch pair to the release platform tag.
///
/// The shipped targets are Linux x86_64, Linux aarch64 and macOS aarch64;
/// everything else is `None`. This is pure so the matrix is testable without
/// depending on the machine running the test.
#[must_use]
pub fn platform_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("linux_x86_64"),
        ("linux", "aarch64") => Some("linux_aarch64"),
        ("macos", "aarch64") => Some("macos_aarch64"),
        _ => None,
    }
}

fn host_platform() -> Option<&'static str> {
    platform_for(env::consts::OS, env::consts::ARCH)
}

fn unsupported_platform() -> UpdateError {
    UpdateError::Unsupported(format!(
        "{} on {} is not supported (Linux x86_64/aarch64 and macOS aarch64 only)",
        env::consts::ARCH,
        env::consts::OS
    ))
}

/// The package-manager methods have no self-update path; explain where the
/// update does come from instead.
fn unsupported_method(method: InstallMethod) -> UpdateError {
    match method {
        InstallMethod::Deb => UpdateError::Unsupported(
            "a Debian/Ubuntu package install is updated with apt/dpkg, not by muninn".to_owned(),
        ),
        InstallMethod::Aur => UpdateError::Unsupported(
            "an AUR package install is updated with the package manager, not by muninn".to_owned(),
        ),
        InstallMethod::Unknown => UpdateError::Unsupported(
            "this installation is not recognised; download the release from GitHub instead"
                .to_owned(),
        ),
        InstallMethod::Tarball | InstallMethod::AppImage => {
            unreachable!("tarball and AppImage installs update themselves")
        }
    }
}

/// Build the plan for one release, or explain why this copy cannot update.
///
/// Refuses a version that is not strictly newer, a method other than tarball or
/// AppImage, an unsupported platform, and an AppImage on a non-Linux host.
pub fn plan(update: &AvailableUpdate, current: &Version) -> Result<UpdatePlan, UpdateError> {
    let version = Version::parse(&update.version)
        .map_err(|_| UpdateError::Version(update.version.clone()))?;
    if version <= *current {
        return Err(UpdateError::NotNewer {
            version: version.to_string(),
            current: current.to_string(),
        });
    }
    match update.method {
        InstallMethod::Tarball | InstallMethod::AppImage => {}
        other => return Err(unsupported_method(other)),
    }
    let platform = host_platform().ok_or_else(unsupported_platform)?;
    if update.method == InstallMethod::AppImage && !platform.starts_with("linux_") {
        return Err(UpdateError::Unsupported(
            "AppImage updates are only available on Linux".to_owned(),
        ));
    }
    let asset = match update.method {
        InstallMethod::Tarball => {
            format!("muninn_v{version}_{platform}.tar.gz")
        }
        InstallMethod::AppImage => {
            format!("muninn_v{version}_linux_{}.AppImage", arch_of(platform))
        }
        _ => unreachable!("the method was checked above"),
    };
    Ok(UpdatePlan {
        version: version.to_string(),
        asset,
        platform: platform.to_owned(),
        method: update.method,
    })
}

/// The architecture part of a release platform tag (`linux_x86_64` → `x86_64`).
fn arch_of(platform: &str) -> &str {
    platform.rsplit('_').next().unwrap_or(platform)
}

/// Where this installation keeps its files.
///
/// For a tarball install the asset paths are the standard `install.sh`
/// destinations. They stay in the layout even when the receipt does not list
/// them: [`apply`] rewrites only paths the receipt names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The file to replace: `<prefix>/bin/muninn` or the AppImage itself.
    pub binary: PathBuf,
    /// `install.sh`'s receipt. `None` for an AppImage.
    pub receipt: Option<PathBuf>,
    /// `share/applications/muninn.desktop`.
    pub desktop: Option<PathBuf>,
    /// `share/icons/hicolor/512x512/apps/muninn.png`.
    pub png: Option<PathBuf>,
    /// `share/icons/hicolor/scalable/apps/muninn.svg`.
    pub svg: Option<PathBuf>,
    /// The install prefix. `None` for an AppImage.
    pub prefix: Option<PathBuf>,
}

/// Derive the layout from how the copy was installed.
///
/// A tarball install must run from `<prefix>/bin/muninn`; an AppImage must be
/// given its `$APPIMAGE` path and its file name must end with `.AppImage`.
/// The release asset name is not required: users rename AppImages freely.
/// Both paths must be absolute.
pub fn layout_for(
    method: InstallMethod,
    exe: &Path,
    appimage: Option<&Path>,
) -> Result<Layout, UpdateError> {
    match method {
        InstallMethod::Tarball => {
            if !exe.is_absolute() {
                return Err(UpdateError::Layout(
                    "the executable path is not absolute".to_owned(),
                ));
            }
            if exe.file_name().and_then(|name| name.to_str()) != Some("muninn") {
                return Err(UpdateError::Layout(
                    "expected the executable to be named muninn".to_owned(),
                ));
            }
            let bin_dir = exe.parent().ok_or_else(|| {
                UpdateError::Layout("the executable has no parent directory".to_owned())
            })?;
            if bin_dir.file_name().and_then(|name| name.to_str()) != Some("bin") {
                return Err(UpdateError::Layout(
                    "expected the executable at <prefix>/bin/muninn".to_owned(),
                ));
            }
            let prefix = bin_dir.parent().ok_or_else(|| {
                UpdateError::Layout("the executable has no install prefix".to_owned())
            })?;
            Ok(Layout {
                binary: exe.to_path_buf(),
                receipt: Some(prefix.join("share/muninn/install-receipt")),
                desktop: Some(prefix.join("share/applications/muninn.desktop")),
                png: Some(prefix.join("share/icons/hicolor/512x512/apps/muninn.png")),
                svg: Some(prefix.join("share/icons/hicolor/scalable/apps/muninn.svg")),
                prefix: Some(prefix.to_path_buf()),
            })
        }
        InstallMethod::AppImage => {
            let appimage = appimage.ok_or_else(|| {
                UpdateError::Layout("an AppImage update needs the AppImage path".to_owned())
            })?;
            if !appimage.is_absolute() {
                return Err(UpdateError::Layout(
                    "the AppImage path is not absolute".to_owned(),
                ));
            }
            let name = appimage
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    UpdateError::Layout("the AppImage has no usable file name".to_owned())
                })?;
            if !name.ends_with(".AppImage") {
                return Err(UpdateError::Layout(format!(
                    "'{name}' is not an .AppImage file"
                )));
            }
            Ok(Layout {
                binary: appimage.to_path_buf(),
                receipt: None,
                desktop: None,
                png: None,
                svg: None,
                prefix: None,
            })
        }
        other => Err(unsupported_method(other)),
    }
}

/// Read and validate the receipt `install.sh` wrote.
///
/// Returns the normalized absolute paths it lists. Every entry must be inside
/// the prefix, there may be no duplicates, and the running binary must be
/// listed (compared through symlinks when both paths resolve).
pub fn read_receipt(layout: &Layout) -> Result<Vec<PathBuf>, UpdateError> {
    let Some(receipt_path) = layout.receipt.as_deref() else {
        return Err(UpdateError::Receipt(
            "this installation has no install receipt".to_owned(),
        ));
    };
    let prefix = layout
        .prefix
        .as_deref()
        .and_then(normalize)
        .ok_or_else(|| {
            UpdateError::Receipt("the install prefix is not a normal path".to_owned())
        })?;
    let text = match fs::read_to_string(receipt_path) {
        Ok(text) => text,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Err(UpdateError::Receipt(format!(
                "there is no install receipt at {}",
                receipt_path.display()
            )));
        }
        Err(source) => return Err(io_failure("read the install receipt", receipt_path, source)),
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry = Path::new(line);
        let normalized = normalize(entry).ok_or_else(|| {
            UpdateError::Receipt(format!(
                "'{line}' is not a normalized absolute path (relative paths and '..' are refused)"
            ))
        })?;
        if !entry.is_absolute() || normalized == prefix || !normalized.starts_with(&prefix) {
            return Err(UpdateError::Receipt(format!(
                "'{line}' is outside the install prefix {}",
                prefix.display()
            )));
        }
        if paths.contains(&normalized) {
            return Err(UpdateError::Receipt(format!(
                "'{line}' is listed more than once"
            )));
        }
        paths.push(normalized);
    }
    if !paths.iter().any(|path| same_file(path, &layout.binary)) {
        return Err(UpdateError::Receipt(format!(
            "the receipt does not list the running binary {}",
            layout.binary.display()
        )));
    }
    Ok(paths)
}

/// Lexically normalize a path: drop `.`, refuse `..`, keep the root. `None`
/// means the path is not safe to compare or write.
fn normalize(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => return None,
            Component::Normal(part) => normalized.push(part),
        }
    }
    Some(normalized)
}

/// Whether two paths name the same file, resolving symlinks where both exist.
fn same_file(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => normalize(left) == normalize(right),
    }
}

/// The bytes one download produced.
#[derive(Debug)]
pub struct Fetched {
    /// The release archive, or the AppImage.
    pub archive: Vec<u8>,
    /// The `checksums.txt` text.
    pub checksums: String,
    /// The asset name, carried so verification cannot use the wrong line.
    pub asset: String,
}

/// The production downloader.
///
/// It builds its own blocking client per call, requires HTTPS and checks the
/// final host after redirects. Tests never call [`fetch`]; they exercise the
/// pure verification and extraction pieces instead.
///
/// [`fetch`]: Downloader::fetch
#[derive(Clone, Debug)]
pub struct Downloader {
    timeout: Duration,
}

impl Downloader {
    /// A downloader with `timeout` as the whole-request timeout.
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Download the archive and `checksums.txt` for `plan`.
    pub fn fetch(&self, plan: &UpdatePlan) -> Result<Fetched, UpdateError> {
        let client = self.client()?;
        let archive = self.get(
            &client,
            &plan.archive_url(),
            ARCHIVE_MAX_BYTES,
            "the release archive",
        )?;
        let checksums = self.get(
            &client,
            &plan.checksums_url(),
            CHECKSUMS_MAX_BYTES,
            "checksums.txt",
        )?;
        let checksums = String::from_utf8(checksums)
            .map_err(|_| UpdateError::Http("checksums.txt is not UTF-8".to_owned()))?;
        Ok(Fetched {
            archive,
            checksums,
            asset: plan.asset.clone(),
        })
    }

    /// Download the Sigstore bundle. Only called when cosign was selected.
    fn fetch_bundle(&self, plan: &UpdatePlan) -> Result<Vec<u8>, UpdateError> {
        let client = self.client()?;
        self.get(
            &client,
            &plan.bundle_url(),
            BUNDLE_MAX_BYTES,
            "the Sigstore bundle",
        )
    }

    fn client(&self) -> Result<reqwest::blocking::Client, UpdateError> {
        reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            // Release downloads redirect to an asset host; the final host is
            // checked in `get`.
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent(concat!("muninn/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| UpdateError::Http(format!("could not build the HTTP client: {error}")))
    }

    fn get(
        &self,
        client: &reqwest::blocking::Client,
        url: &str,
        limit: usize,
        what: &'static str,
    ) -> Result<Vec<u8>, UpdateError> {
        if !url.starts_with("https://") {
            return Err(UpdateError::Http(format!("{url} is not HTTPS")));
        }
        let response = client
            .get(url)
            .send()
            .map_err(|error| UpdateError::Http(format!("GET {url}: {error}")))?;
        let final_url = response.url();
        let host_ok = final_url.host_str().is_some_and(host_allowed);
        if final_url.scheme() != "https" || !host_ok {
            return Err(UpdateError::Http(format!(
                "GET {url} ended at {final_url}, which is not a GitHub host"
            )));
        }
        if !response.status().is_success() {
            return Err(UpdateError::Http(format!(
                "GET {url} returned HTTP {}",
                response.status().as_u16()
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(UpdateError::TooLarge { what, limit });
        }
        let mut body = Vec::new();
        let mut response = response;
        response
            .by_ref()
            .take(limit as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|error| UpdateError::Http(format!("reading {what} failed: {error}")))?;
        if body.len() > limit {
            return Err(UpdateError::TooLarge { what, limit });
        }
        Ok(body)
    }
}

/// Only GitHub serves a release: the repository host and its asset hosts
/// (`objects.githubusercontent.com`, `release-assets.githubusercontent.com`).
fn host_allowed(host: &str) -> bool {
    const SUFFIXES: [&str; 2] = ["github.com", "githubusercontent.com"];
    SUFFIXES.iter().any(|suffix| {
        host == *suffix
            || host
                .strip_suffix(suffix)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

/// Find the hash `checksums.txt` lists for exactly `asset`.
///
/// Accepts `sha256sum` and BSD-style spacing and the `*` binary marker. Returns
/// `None` for a missing line or a line whose hash is not a SHA-256.
#[must_use]
pub fn parse_checksums(text: &str, asset: &str) -> Option<String> {
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(hash), Some(name)) = (fields.next(), fields.next()) else {
            continue;
        };
        let name = name.strip_prefix('*').unwrap_or(name);
        if name == asset && is_sha256_hex(hash) {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

/// Verify `bytes` against `asset`'s line in `checksums`.
///
/// A missing line is [`UpdateError::ChecksumMissing`]; a mismatch is
/// [`UpdateError::Checksum`]. Both are fatal.
pub fn verify_checksum(checksums: &str, asset: &str, bytes: &[u8]) -> Result<(), UpdateError> {
    let expected =
        parse_checksums(checksums, asset).ok_or_else(|| UpdateError::ChecksumMissing {
            asset: asset.to_owned(),
        })?;
    let actual = sha256_hex(bytes);
    if actual.eq_ignore_ascii_case(&expected) {
        Ok(())
    } else {
        Err(UpdateError::Checksum {
            asset: asset.to_owned(),
            expected,
            actual,
        })
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The signature result. A bad checksum is always fatal; this only softens a
/// missing or failing verifier into a warning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignatureStatus {
    /// A verifier checked the release.
    Verified,
    /// No verifier was found, or it failed. The checksum still passed.
    Warning(String),
}

/// The verifier executables found on `PATH`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Verifiers {
    /// `cosign`, preferred because it verifies the signed checksums bundle.
    pub cosign: Option<PathBuf>,
    /// `gh`, used for the GitHub attestation when cosign is absent.
    pub gh: Option<PathBuf>,
}

/// Find `cosign` and `gh` on `PATH`.
#[must_use]
pub fn detect_verifiers() -> Verifiers {
    Verifiers {
        cosign: find_on_path("cosign"),
        gh: find_on_path("gh"),
    }
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Verify the release signature with the same policy as `install.sh`.
///
/// cosign is preferred and needs the bundle. `gh attestation verify` is the
/// fallback. No verifier, or a failing one, is a [`SignatureStatus::Warning`]:
/// only the checksum is mandatory.
///
/// `checksums_path` and `archive_path` must be real files; `bundle_path` is
/// `Some` only when the caller downloaded it. Neither tool is passed a token:
/// `GH_TOKEN`, `GITHUB_TOKEN` and `GH_ENTERPRISE_TOKEN` are removed from the
/// child environment.
#[must_use]
pub fn verify_signature(
    verifiers: &Verifiers,
    checksums_path: &Path,
    archive_path: &Path,
    bundle_path: Option<&Path>,
) -> SignatureStatus {
    verify_signature_with_timeout(
        verifiers,
        checksums_path,
        archive_path,
        bundle_path,
        VERIFIER_TIMEOUT,
    )
}

fn verify_signature_with_timeout(
    verifiers: &Verifiers,
    checksums_path: &Path,
    archive_path: &Path,
    bundle_path: Option<&Path>,
    timeout: Duration,
) -> SignatureStatus {
    if let Some(cosign) = verifiers.cosign.as_deref() {
        let Some(bundle) = bundle_path else {
            return SignatureStatus::Warning(
                "cosign is installed but the Sigstore bundle was not downloaded, so the signature was not checked"
                    .to_owned(),
            );
        };
        let checksums = checksums_path.to_string_lossy().into_owned();
        let bundle = bundle.to_string_lossy().into_owned();
        let (ok, log) = run_verifier_with_timeout(
            cosign,
            &[
                "verify-blob",
                "--bundle",
                &bundle,
                "--certificate-identity-regexp",
                COSIGN_IDENTITY,
                "--certificate-oidc-issuer",
                OIDC_ISSUER,
                &checksums,
            ],
            timeout,
        );
        if ok {
            return SignatureStatus::Verified;
        }
        return SignatureStatus::Warning(format!(
            "cosign could not verify the release signature; the SHA-256 checksum matched: {log}"
        ));
    }
    if let Some(gh) = verifiers.gh.as_deref() {
        let archive = archive_path.to_string_lossy().into_owned();
        let (ok, log) = run_verifier_with_timeout(
            gh,
            &["attestation", "verify", &archive, "--repo", REPO],
            timeout,
        );
        if ok {
            return SignatureStatus::Verified;
        }
        return SignatureStatus::Warning(format!(
            "gh could not verify the release attestation; the SHA-256 checksum matched: {log}"
        ));
    }
    SignatureStatus::Warning(
        "no cosign or gh was found, so the release signature was not checked; the SHA-256 checksum matched"
            .to_owned(),
    )
}

/// Run one verifier without letting it hang the update.
///
/// The child's pipes are drained on reader threads so a chatty verifier cannot
/// fill a pipe and deadlock, and a verifier still running at `timeout` is
/// killed and reported on the warning path. No token is inherited.
fn run_verifier_with_timeout(program: &Path, args: &[&str], timeout: Duration) -> (bool, String) {
    let spawned = Command::new(program)
        .args(args)
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_ENTERPRISE_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => return (false, error.to_string()),
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || read_verifier_pipe(stdout));
    let stderr_reader = std::thread::spawn(move || read_verifier_pipe(stderr));

    let deadline = Instant::now() + timeout;
    let mut exit = None;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit = Some(status);
                break;
            }
            Ok(None) if Instant::now() >= deadline => {
                timed_out = true;
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            Ok(None) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                std::thread::sleep(VERIFIER_POLL.min(remaining));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                // As on timeout, do not wait on a pipe a leftover child may
                // still hold; the error is the report that matters.
                return (false, error.to_string());
            }
        }
    }

    if timed_out {
        // Drop the reader handles instead of joining: a verifier that left a
        // grandchild holding stdout/stderr must not defeat the timeout. The
        // threads end when the OS closes the pipes.
        return (false, format!("verifier timed out after {timeout:?}"));
    }
    // A normal exit closed the pipes, so these joins cannot block.
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    let succeeded = exit.is_some_and(|status| status.success());
    let stderr = String::from_utf8_lossy(&stderr);
    let log = if stderr.trim().is_empty() {
        String::from_utf8_lossy(&stdout).into_owned()
    } else {
        stderr.into_owned()
    };
    (succeeded, tail(&log, 400))
}

fn read_verifier_pipe<R: io::Read>(pipe: Option<R>) -> Vec<u8> {
    let Some(mut pipe) = pipe else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    let _ = pipe
        .by_ref()
        .take(VERIFIER_LOG_MAX_BYTES)
        .read_to_end(&mut bytes);
    bytes
}

/// Keep the last `limit` characters, so a verifier's log cannot dominate a
/// banner or a settings page.
fn tail(text: &str, limit: usize) -> String {
    let text = text.trim();
    let count = text.chars().count();
    if count <= limit {
        return text.to_owned();
    }
    text.chars().skip(count - limit).collect()
}

/// The files one archive contributes to an installation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchivePayload {
    /// The new `muninn` executable (or the AppImage bytes).
    pub binary: Vec<u8>,
    /// `assets/muninn.desktop`, when the archive has one.
    pub desktop: Option<Vec<u8>>,
    /// `assets/logo.png`, when the archive has one.
    pub png: Option<Vec<u8>>,
    /// `assets/logo.svg`, when the archive has one.
    pub svg: Option<Vec<u8>>,
    /// `LICENSE`, when the archive has one. Not installed; kept for callers
    /// that want to show it.
    pub license: Option<Vec<u8>>,
}

/// Unpack the release payload.
///
/// A tarball must have the release root `muninn_v<version>_<platform>/` and may
/// contain only the known entries; other regular files (README, CHANGELOG, the
/// sample vault) are skipped without buffering. Absolute paths, `..`, symlinks,
/// hardlinks and entries outside the root are refused. An AppImage is its own
/// payload.
pub fn extract(bytes: &[u8], plan: &UpdatePlan) -> Result<ArchivePayload, UpdateError> {
    match plan.method {
        InstallMethod::AppImage => {
            if bytes.len() > ARCHIVE_MAX_BYTES {
                return Err(UpdateError::TooLarge {
                    what: "the AppImage",
                    limit: ARCHIVE_MAX_BYTES,
                });
            }
            Ok(ArchivePayload {
                binary: bytes.to_vec(),
                ..ArchivePayload::default()
            })
        }
        InstallMethod::Tarball => extract_tarball(bytes, plan),
        other => Err(unsupported_method(other)),
    }
}

fn extract_tarball(bytes: &[u8], plan: &UpdatePlan) -> Result<ArchivePayload, UpdateError> {
    let root = format!("muninn_v{}_{}", plan.version, plan.platform);
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let entries = archive
        .entries()
        .map_err(|error| UpdateError::Archive(format!("cannot read the tar stream: {error}")))?;
    let mut payload = ArchivePayload::default();
    let mut binary_entries = 0usize;

    for entry in entries {
        let mut entry = entry
            .map_err(|error| UpdateError::Archive(format!("cannot read a tar entry: {error}")))?;
        let kind = entry.header().entry_type();
        let path = entry
            .path()
            .map_err(|error| UpdateError::Archive(format!("cannot read an entry path: {error}")))?
            .into_owned();
        let parts = archive_parts(&path).ok_or_else(|| {
            UpdateError::Archive(format!("unsafe entry path '{}'", path.display()))
        })?;
        if parts.is_empty() {
            continue;
        }
        if parts[0] != root {
            return Err(UpdateError::Archive(format!(
                "entry '{}' is outside {root}/",
                path.display()
            )));
        }
        if parts.len() == 1 {
            if kind.is_dir() {
                continue;
            }
            return Err(UpdateError::Archive(format!(
                "unexpected file '{root}' at the archive root"
            )));
        }
        let name = parts[1..].join("/");
        if kind.is_dir() {
            continue;
        }
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(UpdateError::Archive(format!(
                "link entry '{name}' is not accepted"
            )));
        }
        if !kind.is_file() {
            return Err(UpdateError::Archive(format!(
                "unsupported entry type for '{name}'"
            )));
        }
        match name.as_str() {
            "muninn" => {
                binary_entries += 1;
                if binary_entries > 1 {
                    return Err(UpdateError::Archive(
                        "the archive contains more than one muninn binary".to_owned(),
                    ));
                }
                payload.binary = read_entry(entry, ARCHIVE_MAX_BYTES, "the muninn binary")?;
            }
            "assets/muninn.desktop" => {
                set_once(
                    &mut payload.desktop,
                    read_entry(entry, ASSET_MAX_BYTES, "the desktop entry")?,
                    &name,
                )?;
            }
            "assets/logo.png" => {
                set_once(
                    &mut payload.png,
                    read_entry(entry, ASSET_MAX_BYTES, "the PNG icon")?,
                    &name,
                )?;
            }
            "assets/logo.svg" => {
                set_once(
                    &mut payload.svg,
                    read_entry(entry, ASSET_MAX_BYTES, "the SVG icon")?,
                    &name,
                )?;
            }
            "LICENSE" => {
                set_once(
                    &mut payload.license,
                    read_entry(entry, ASSET_MAX_BYTES, "LICENSE")?,
                    &name,
                )?;
            }
            _ => {
                // README, CHANGELOG and the sample vault are not installed.
                // Drain the entry so the tar stream stays positioned, but never
                // keep its bytes.
                io::copy(&mut entry, &mut io::sink()).map_err(|error| {
                    UpdateError::Archive(format!("cannot skip '{name}': {error}"))
                })?;
            }
        }
    }

    if binary_entries == 0 || payload.binary.is_empty() {
        return Err(UpdateError::Archive(
            "the archive does not contain a muninn binary".to_owned(),
        ));
    }
    Ok(payload)
}

fn archive_parts(path: &Path) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_str()?.to_owned()),
            _ => return None,
        }
    }
    Some(parts)
}

fn set_once(slot: &mut Option<Vec<u8>>, bytes: Vec<u8>, name: &str) -> Result<(), UpdateError> {
    if slot.replace(bytes).is_some() {
        return Err(UpdateError::Archive(format!("duplicate entry '{name}'")));
    }
    Ok(())
}

fn read_entry<R: io::Read>(
    entry: tar::Entry<'_, R>,
    limit: usize,
    what: &'static str,
) -> Result<Vec<u8>, UpdateError> {
    if entry.size() > limit as u64 {
        return Err(UpdateError::TooLarge { what, limit });
    }
    let mut bytes = Vec::new();
    entry
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| UpdateError::Archive(format!("cannot read {what}: {error}")))?;
    if bytes.len() > limit {
        return Err(UpdateError::TooLarge { what, limit });
    }
    Ok(bytes)
}

/// One message from the self-update worker to the UI thread.
///
/// A worker reports each [`Phase`] before starting it and then exactly one
/// [`Finished`](SelfUpdateMessage::Finished). The UI applies these on its own
/// thread ([`crate::State::apply_self_update_message`]); the worker never
/// touches `State`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelfUpdateMessage {
    /// The worker is entering this phase.
    Phase(Phase),
    /// The update finished, with the new version or a readable failure.
    Finished(Result<Applied, String>),
}

/// What one successful install did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    /// The installed version.
    pub version: String,
    /// Something optional failed after the new binary was in place.
    pub warning: Option<String>,
    /// The rollback copy, `<state_home>/update/muninn.previous`.
    pub previous: Option<PathBuf>,
    /// Always true: the running process still maps the old binary.
    pub restart_required: bool,
}

/// Replace the installed binary and, for a tarball, refresh receipt-listed
/// desktop/icons.
///
/// The new binary is staged in its own directory, synced and renamed over the
/// target; the old binary is copied to `<state_home>/update/muninn.previous`
/// first. Nothing is written outside the binary's directory, the receipt-listed
/// destinations and that one rollback copy. The receipt itself is rewritten
/// only if the set of installed paths changed, which this update never does.
///
/// `receipt` is [`read_receipt`]'s output. It is validated again here so a
/// hand-built list cannot write outside the prefix. For an AppImage the receipt
/// is empty and no asset is written; the backup keeps the AppImage bytes under
/// the fixed name `muninn.previous`.
pub fn apply(
    layout: &Layout,
    receipt: &[PathBuf],
    payload: &ArchivePayload,
    plan: &UpdatePlan,
    state_home: &Path,
) -> Result<Applied, UpdateError> {
    if !cfg!(unix) {
        return Err(UpdateError::Unsupported(
            "self-update is not implemented on this platform yet".to_owned(),
        ));
    }
    match plan.method {
        InstallMethod::Tarball | InstallMethod::AppImage => {}
        other => return Err(unsupported_method(other)),
    }
    if payload.binary.is_empty() {
        return Err(UpdateError::Archive(
            "the downloaded binary is empty".to_owned(),
        ));
    }
    if plan.method == InstallMethod::Tarball {
        let prefix = layout
            .prefix
            .as_deref()
            .ok_or_else(|| UpdateError::Layout("a tarball install has no prefix".to_owned()))?;
        validate_receipt(receipt, prefix, &layout.binary)?;
    }

    // Stage beside the target so the final rename is a same-filesystem,
    // atomic replacement. A failure here leaves the old binary untouched.
    let staged = stage_binary(&layout.binary, &payload.binary)?;

    // One rollback copy. A failure here aborts before anything is replaced.
    let previous = match backup_binary(&layout.binary, state_home) {
        Ok(previous) => previous,
        Err(error) => {
            let _ = fs::remove_file(&staged);
            return Err(error);
        }
    };

    if let Err(source) = fs::rename(&staged, &layout.binary) {
        let _ = fs::remove_file(&staged);
        return Err(io_failure("replace the binary", &layout.binary, source));
    }
    if let Some(directory) = layout.binary.parent() {
        sync_directory(directory);
    }

    let mut warning = None;
    if plan.method == InstallMethod::Tarball {
        for (destination, bytes, label) in [
            (
                layout.desktop.as_deref(),
                payload.desktop.as_deref(),
                "the desktop entry",
            ),
            (
                layout.png.as_deref(),
                payload.png.as_deref(),
                "the PNG icon",
            ),
            (
                layout.svg.as_deref(),
                payload.svg.as_deref(),
                "the SVG icon",
            ),
        ] {
            let (Some(destination), Some(bytes)) = (destination, bytes) else {
                continue;
            };
            if !receipt.iter().any(|entry| entry == destination) {
                continue;
            }
            if fs::read(destination).is_ok_and(|current| current == bytes) {
                continue;
            }
            if let Err(source) = write_atomic(destination, bytes, 0o644) {
                warning = Some(format!(
                    "muninn {} was installed, but {label} could not be refreshed: {source}",
                    plan.version
                ));
            }
        }
        if let Err(error) = sync_receipt(layout, receipt) {
            // The binary is already in place; report rather than fail.
            let message = format!(
                "muninn {} was installed, but the receipt could not be updated: {error}",
                plan.version
            );
            warning = Some(match warning {
                Some(existing) => format!("{existing}; {message}"),
                None => message,
            });
        }
    }

    Ok(Applied {
        version: plan.version.clone(),
        warning,
        previous: Some(previous),
        restart_required: true,
    })
}

fn validate_receipt(receipt: &[PathBuf], prefix: &Path, binary: &Path) -> Result<(), UpdateError> {
    let prefix = normalize(prefix).ok_or_else(|| {
        UpdateError::Receipt("the install prefix is not a normal path".to_owned())
    })?;
    if receipt.is_empty() {
        return Err(UpdateError::Receipt(
            "the receipt lists no installed files".to_owned(),
        ));
    }
    let mut seen: Vec<PathBuf> = Vec::new();
    for entry in receipt {
        let normalized = normalize(entry).ok_or_else(|| {
            UpdateError::Receipt(format!(
                "'{}' is not a normal absolute path",
                entry.display()
            ))
        })?;
        if !entry.is_absolute() || normalized == prefix || !normalized.starts_with(&prefix) {
            return Err(UpdateError::Receipt(format!(
                "'{}' is outside the install prefix {}",
                entry.display(),
                prefix.display()
            )));
        }
        if seen.contains(&normalized) {
            return Err(UpdateError::Receipt(format!(
                "'{}' is listed more than once",
                entry.display()
            )));
        }
        seen.push(normalized);
    }
    if !receipt.iter().any(|entry| same_file(entry, binary)) {
        return Err(UpdateError::Receipt(format!(
            "the receipt does not list the running binary {}",
            binary.display()
        )));
    }
    Ok(())
}

/// Write the new binary to a staging file beside the target and sync it.
fn stage_binary(target: &Path, bytes: &[u8]) -> Result<PathBuf, UpdateError> {
    let directory = target
        .parent()
        .ok_or_else(|| UpdateError::Layout("the binary has no parent directory".to_owned()))?;
    let staged = staging_path(directory);
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&staged)?;
        file.write_all(bytes)?;
        file.flush()?;
        set_mode(&staged, 0o755)?;
        file.sync_all()
    })();
    if let Err(source) = result {
        let _ = fs::remove_file(&staged);
        return Err(io_failure("stage the update", &staged, source));
    }
    Ok(staged)
}

/// Copy the current binary to `<state_home>/update/muninn.previous`.
fn backup_binary(binary: &Path, state_home: &Path) -> Result<PathBuf, UpdateError> {
    let directory = state_home.join("update");
    fs::create_dir_all(&directory)
        .map_err(|source| io_failure("create the update state directory", &directory, source))?;
    let previous = directory.join(PREVIOUS_NAME);
    copy_atomic(binary, &previous, 0o755)
        .map_err(|source| io_failure("back up the current binary", &previous, source))?;
    Ok(previous)
}

/// Rewrite the receipt only when the installed set differs from what is on
/// disk. A normal update keeps it byte-for-byte.
fn sync_receipt(layout: &Layout, receipt: &[PathBuf]) -> Result<(), UpdateError> {
    let Some(path) = layout.receipt.as_deref() else {
        return Ok(());
    };
    let current = fs::read_to_string(path)
        .map_err(|source| io_failure("read the install receipt", path, source))?;
    let mut on_disk: Vec<PathBuf> = current
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| normalize(Path::new(line)))
        .collect();
    let mut wanted = receipt.to_vec();
    on_disk.sort();
    on_disk.dedup();
    wanted.sort();
    wanted.dedup();
    if on_disk == wanted {
        return Ok(());
    }
    let mut text = String::new();
    for entry in receipt {
        text.push_str(&entry.to_string_lossy());
        text.push('\n');
    }
    write_atomic(path, text.as_bytes(), 0o644)
        .map_err(|source| io_failure("rewrite the install receipt", path, source))
}

/// Write `bytes` to a staging file in `path`'s directory and rename it over
/// `path`.
fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let directory = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "path has no parent directory")
    })?;
    let staged = staging_path(directory);
    let result = (|| {
        let mut file = fs::File::create(&staged)?;
        file.write_all(bytes)?;
        file.flush()?;
        set_mode(&staged, mode)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&staged, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
        return result;
    }
    sync_directory(directory);
    Ok(())
}

/// Copy `source` to `destination` through a temp file in the destination
/// directory, with `mode`.
fn copy_atomic(source: &Path, destination: &Path, mode: u32) -> io::Result<()> {
    let directory = destination.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "path has no parent directory")
    })?;
    let staged = staging_path(directory);
    let result = (|| {
        let mut from = fs::File::open(source)?;
        let mut to = fs::File::create(&staged)?;
        io::copy(&mut from, &mut to)?;
        to.flush()?;
        set_mode(&staged, mode)?;
        to.sync_all()?;
        drop(to);
        fs::rename(&staged, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
        return result;
    }
    sync_directory(directory);
    Ok(())
}

fn staging_path(directory: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    directory.join(format!(".muninn-update-{}-{id}", std::process::id()))
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

/// Create a directory only this user can enter. On Unix the mode goes to
/// `mkdir` itself, so there is no window with looser permissions; other
/// platforms use the `set_mode` fallback.
#[cfg(unix)]
fn create_private_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt as _;
    fs::DirBuilder::new().mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir(path)?;
    set_mode(path, 0o700)
}

/// fsync a directory after a rename, where the platform supports it. Failure is
/// ignored: the rename already happened, and not every filesystem allows
/// opening a directory.
fn sync_directory(directory: &Path) {
    if let Ok(handle) = fs::File::open(directory) {
        let _ = handle.sync_all();
    }
}

fn io_failure(action: &'static str, path: &Path, source: io::Error) -> UpdateError {
    UpdateError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

/// The stage a [`perform`] call is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Fetching the archive and `checksums.txt`.
    Downloading,
    /// Checking the SHA-256 and, when available, the signature.
    Verifying,
    /// Unpacking the archive.
    Extracting,
    /// Replacing the installed files.
    Installing,
}

/// Plan, download, verify, extract and install in one call.
///
/// This is what the UI's Update action will run on a worker thread; it reports
/// each [`Phase`] before starting it. The pure pieces stay public so tests (and
/// any future caller) can work offline. A failed signature becomes
/// [`Applied::warning`], never an error.
#[allow(clippy::too_many_arguments)]
pub fn perform(
    update: &AvailableUpdate,
    current: &Version,
    exe: &Path,
    appimage: Option<&Path>,
    state_home: &Path,
    downloader: &Downloader,
    verifiers: &Verifiers,
    progress: &mut dyn FnMut(Phase),
) -> Result<Applied, UpdateError> {
    let plan = plan(update, current)?;
    let layout = layout_for(plan.method, exe, appimage)?;
    let receipt = if plan.method == InstallMethod::Tarball {
        read_receipt(&layout)?
    } else {
        Vec::new()
    };

    progress(Phase::Downloading);
    let fetched = downloader.fetch(&plan)?;

    progress(Phase::Verifying);
    verify_checksum(&fetched.checksums, &fetched.asset, &fetched.archive)?;
    let work = WorkDir::create()?;
    let checksums_path = work.write("checksums.txt", fetched.checksums.as_bytes())?;
    let archive_path = work.write(&fetched.asset, &fetched.archive)?;
    // Same policy as `install.sh`: a bundle that cannot be fetched is a
    // signature warning, not a failed update. The checksum already passed.
    let bundle_path = if verifiers.cosign.is_some() {
        match downloader.fetch_bundle(&plan) {
            Ok(bundle) => Some(work.write("checksums.txt.sigstore.json", &bundle)?),
            Err(_) => None,
        }
    } else {
        None
    };
    let signature = verify_signature(
        verifiers,
        &checksums_path,
        &archive_path,
        bundle_path.as_deref(),
    );

    progress(Phase::Extracting);
    let payload = extract(&fetched.archive, &plan)?;

    progress(Phase::Installing);
    let mut applied = apply(&layout, &receipt, &payload, &plan, state_home)?;
    if let SignatureStatus::Warning(message) = signature {
        applied.warning = Some(match applied.warning {
            Some(existing) => format!("{existing}; {message}"),
            None => message,
        });
    }
    Ok(applied)
}

/// A scratch directory for the files the verifier needs. Removed on drop.
struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    fn create() -> Result<Self, UpdateError> {
        let base = env::temp_dir();
        for _ in 0..8 {
            let path = staging_path(&base);
            match create_private_directory(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(io_failure(
                        "create the update temp directory",
                        &path,
                        source,
                    ));
                }
            }
        }
        Err(UpdateError::Io {
            action: "create a unique update temp directory",
            path: base,
            source: io::Error::new(io::ErrorKind::AlreadyExists, "every candidate name exists"),
        })
    }

    fn write(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, UpdateError> {
        let path = self.path.join(name);
        fs::write(&path, bytes)
            .map_err(|source| io_failure("write an update temp file", &path, source))?;
        Ok(path)
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "muninn-self-update-{}-{id}-{label}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        #[cfg(unix)]
        fn path(&self) -> &Path {
            &self.path
        }

        fn join(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn available(version: &str, method: InstallMethod) -> AvailableUpdate {
        AvailableUpdate {
            version: version.to_owned(),
            url: format!("https://github.com/raythurman2386/muninn/releases/tag/v{version}"),
            method,
            etag: None,
        }
    }

    fn tarball_plan(platform: &str) -> UpdatePlan {
        UpdatePlan {
            version: "0.10.0".to_owned(),
            asset: format!("muninn_v0.10.0_{platform}.tar.gz"),
            platform: platform.to_owned(),
            method: InstallMethod::Tarball,
        }
    }

    fn appimage_plan() -> UpdatePlan {
        UpdatePlan {
            version: "0.10.0".to_owned(),
            asset: "muninn_v0.10.0_linux_x86_64.AppImage".to_owned(),
            platform: "linux_x86_64".to_owned(),
            method: InstallMethod::AppImage,
        }
    }

    #[test]
    fn platform_mapping_accepts_only_the_shipped_targets() {
        assert_eq!(platform_for("linux", "x86_64"), Some("linux_x86_64"));
        assert_eq!(platform_for("linux", "aarch64"), Some("linux_aarch64"));
        assert_eq!(platform_for("macos", "aarch64"), Some("macos_aarch64"));
        for (os, arch) in [
            ("macos", "x86_64"),
            ("linux", "arm"),
            ("linux", "amd64"),
            ("linux", ""),
            ("windows", "x86_64"),
            ("freebsd", "x86_64"),
        ] {
            assert_eq!(platform_for(os, arch), None, "{os}/{arch}");
        }
    }

    #[test]
    fn a_release_is_planned_only_when_it_is_newer() {
        let current = Version::new(0, 10, 0);
        for version in ["0.10.0", "0.9.9"] {
            assert!(
                matches!(
                    plan(&available(version, InstallMethod::Tarball), &current),
                    Err(UpdateError::NotNewer { .. })
                ),
                "{version}"
            );
        }
        assert!(matches!(
            plan(&available("nightly", InstallMethod::Tarball), &current),
            Err(UpdateError::Version(_))
        ));
    }

    #[test]
    fn package_manager_installs_are_refused() {
        let current = Version::new(0, 1, 0);
        for method in [
            InstallMethod::Deb,
            InstallMethod::Aur,
            InstallMethod::Unknown,
        ] {
            assert!(
                matches!(
                    plan(&available("0.10.0", method), &current),
                    Err(UpdateError::Unsupported(_))
                ),
                "{method:?}"
            );
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn a_tarball_plan_names_the_asset_and_urls() {
        let host = platform_for(env::consts::OS, env::consts::ARCH).expect("supported host");
        let plan = plan(
            &available("0.11.0", InstallMethod::Tarball),
            &Version::new(0, 10, 0),
        )
        .expect("plan");
        assert_eq!(plan.platform, host);
        assert_eq!(plan.asset, format!("muninn_v0.11.0_{host}.tar.gz"));
        assert_eq!(
            plan.archive_url(),
            format!(
                "https://github.com/raythurman2386/muninn/releases/download/v0.11.0/muninn_v0.11.0_{host}.tar.gz"
            )
        );
        assert_eq!(
            plan.checksums_url(),
            "https://github.com/raythurman2386/muninn/releases/download/v0.11.0/checksums.txt"
        );
        assert_eq!(
            plan.bundle_url(),
            "https://github.com/raythurman2386/muninn/releases/download/v0.11.0/checksums.txt.sigstore.json"
        );
        assert_eq!(plan.method, InstallMethod::Tarball);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_appimage_plan_names_the_linux_asset() {
        let host = platform_for(env::consts::OS, env::consts::ARCH).expect("supported host");
        let arch = arch_of(host);
        let plan = plan(
            &available("0.11.0", InstallMethod::AppImage),
            &Version::new(0, 10, 0),
        )
        .expect("plan");
        assert_eq!(plan.asset, format!("muninn_v0.11.0_linux_{arch}.AppImage"));
        assert_eq!(plan.platform, host);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn an_appimage_is_refused_on_macos() {
        assert!(matches!(
            plan(
                &available("0.11.0", InstallMethod::AppImage),
                &Version::new(0, 10, 0)
            ),
            Err(UpdateError::Unsupported(_))
        ));
    }

    fn tarball_install(tmp: &TempDir) -> (Layout, PathBuf, PathBuf) {
        let prefix = tmp.join("prefix");
        let binary = prefix.join("bin/muninn");
        fs::create_dir_all(binary.parent().expect("bin dir")).expect("create bin");
        fs::write(&binary, b"old binary").expect("write binary");
        set_mode(&binary, 0o755).expect("chmod binary");
        let layout = layout_for(InstallMethod::Tarball, &binary, None).expect("layout");
        (layout, prefix, binary)
    }

    fn write_receipt(layout: &Layout, paths: &[&Path]) {
        let path = layout.receipt.as_deref().expect("receipt path");
        fs::create_dir_all(path.parent().expect("receipt dir")).expect("create receipt dir");
        let mut text = String::new();
        for entry in paths {
            text.push_str(&entry.to_string_lossy());
            text.push('\n');
        }
        fs::write(path, text).expect("write receipt");
    }

    #[test]
    fn layout_finds_the_standard_tarball_paths() {
        let tmp = TempDir::new("layout");
        let (layout, prefix, binary) = tarball_install(&tmp);
        assert_eq!(layout.binary, binary);
        assert_eq!(layout.prefix.as_deref(), Some(prefix.as_path()));
        assert_eq!(
            layout.receipt.as_deref(),
            Some(prefix.join("share/muninn/install-receipt").as_path())
        );
        assert_eq!(
            layout.desktop.as_deref(),
            Some(prefix.join("share/applications/muninn.desktop").as_path())
        );
        assert_eq!(
            layout.png.as_deref(),
            Some(
                prefix
                    .join("share/icons/hicolor/512x512/apps/muninn.png")
                    .as_path()
            )
        );
        assert_eq!(
            layout.svg.as_deref(),
            Some(
                prefix
                    .join("share/icons/hicolor/scalable/apps/muninn.svg")
                    .as_path()
            )
        );
    }

    #[test]
    fn layout_refuses_a_binary_that_is_not_below_bin() {
        let tmp = TempDir::new("layout-bad");
        let prefix = tmp.join("prefix");
        fs::create_dir_all(&prefix).expect("prefix");
        let loose = prefix.join("muninn");
        fs::write(&loose, b"binary").expect("write");
        assert!(matches!(
            layout_for(InstallMethod::Tarball, &loose, None),
            Err(UpdateError::Layout(_))
        ));
        assert!(matches!(
            layout_for(InstallMethod::Tarball, Path::new("bin/muninn"), None),
            Err(UpdateError::Layout(_))
        ));
    }

    #[test]
    fn layout_accepts_a_renamed_appimage_and_refuses_other_files() {
        let tmp = TempDir::new("layout-appimage");
        // `$APPIMAGE` may be the release name or a name the user chose: the
        // update targets the file the runtime already reports.
        let released = tmp.join("muninn_v0.10.0_linux_x86_64.AppImage");
        let renamed = tmp.join("Muninn.AppImage");
        fs::write(&released, b"appimage").expect("write");
        fs::write(&renamed, b"appimage").expect("write");
        for appimage in [&released, &renamed] {
            let layout = layout_for(
                InstallMethod::AppImage,
                &tmp.join("ignored"),
                Some(appimage),
            )
            .expect("layout");
            assert_eq!(layout.binary, *appimage);
            assert!(layout.receipt.is_none());
            assert!(layout.desktop.is_none());
            assert!(layout.prefix.is_none());
        }

        for name in [
            "Muninn.appimage",
            "muninn_v0.10.0_linux_x86_64.tar.gz",
            "AppImage",
            "Muninn.AppImage.bak",
        ] {
            let path = tmp.join(name);
            fs::write(&path, b"x").expect("write");
            assert!(
                matches!(
                    layout_for(InstallMethod::AppImage, &path, Some(&path)),
                    Err(UpdateError::Layout(_))
                ),
                "{name}"
            );
        }
        assert!(matches!(
            layout_for(InstallMethod::AppImage, &released, None),
            Err(UpdateError::Layout(_))
        ));
        for method in [
            InstallMethod::Deb,
            InstallMethod::Aur,
            InstallMethod::Unknown,
        ] {
            assert!(matches!(
                layout_for(method, &released, Some(&released)),
                Err(UpdateError::Unsupported(_))
            ));
        }
    }

    #[test]
    fn a_valid_receipt_is_read_back_normalized() {
        let tmp = TempDir::new("receipt");
        let (layout, prefix, binary) = tarball_install(&tmp);
        let desktop = prefix.join("share/applications/muninn.desktop");
        write_receipt(&layout, &[&binary, &desktop]);
        let receipt = read_receipt(&layout).expect("receipt");
        assert_eq!(receipt, vec![binary, desktop]);
    }

    #[test]
    fn a_receipt_must_list_the_running_binary() {
        let tmp = TempDir::new("receipt-binary");
        let (layout, prefix, _binary) = tarball_install(&tmp);
        let desktop = prefix.join("share/applications/muninn.desktop");
        write_receipt(&layout, &[&desktop]);
        assert!(matches!(
            read_receipt(&layout),
            Err(UpdateError::Receipt(_))
        ));
    }

    #[test]
    fn a_receipt_entry_outside_the_prefix_is_refused() {
        let tmp = TempDir::new("receipt-outside");
        let (layout, _prefix, binary) = tarball_install(&tmp);
        let outside = tmp.join("outside");
        fs::write(&outside, b"x").expect("write outside");
        write_receipt(&layout, &[&binary, &outside]);
        assert!(matches!(
            read_receipt(&layout),
            Err(UpdateError::Receipt(_))
        ));

        let traversal = format!("{}/../outside", binary.parent().unwrap().display());
        write_receipt(&layout, &[&binary, Path::new(&traversal)]);
        assert!(matches!(
            read_receipt(&layout),
            Err(UpdateError::Receipt(_))
        ));
    }

    #[test]
    fn a_missing_receipt_and_duplicate_entries_are_refused() {
        let tmp = TempDir::new("receipt-dup");
        let (layout, _prefix, binary) = tarball_install(&tmp);
        assert!(matches!(
            read_receipt(&layout),
            Err(UpdateError::Receipt(_))
        ));
        write_receipt(&layout, &[&binary, &binary]);
        assert!(matches!(
            read_receipt(&layout),
            Err(UpdateError::Receipt(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_receipt_matches_the_binary_through_a_symlink() {
        use std::os::unix::fs::symlink;
        let tmp = TempDir::new("receipt-symlink");
        let (layout, _prefix, binary) = tarball_install(&tmp);
        // `install.sh` may have installed the real file and left the usual
        // path as a link; the receipt names the real file and must still match
        // the running binary.
        let real = binary.with_file_name("muninn-real");
        fs::rename(&binary, &real).expect("rename binary");
        symlink(&real, &binary).expect("symlink");
        write_receipt(&layout, &[&real]);
        let receipt = read_receipt(&layout).expect("receipt");
        assert_eq!(receipt, vec![real]);
    }

    const CHECKSUMS: &str = "\
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  muninn_v0.10.0_linux_x86_64.tar.gz
fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210 *muninn_v0.10.0_linux_aarch64.tar.gz
";

    #[test]
    fn checksums_match_exactly_one_asset() {
        assert_eq!(
            parse_checksums(CHECKSUMS, "muninn_v0.10.0_linux_x86_64.tar.gz").as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert_eq!(
            parse_checksums(CHECKSUMS, "muninn_v0.10.0_linux_aarch64.tar.gz").as_deref(),
            Some("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210")
        );
        assert_eq!(
            parse_checksums(CHECKSUMS, "muninn_v0.10.0_macos_aarch64.tar.gz"),
            None
        );
        assert_eq!(
            parse_checksums(CHECKSUMS, "muninn_v0.10.0_linux_x86_64.tar.gz.sig"),
            None
        );
        assert_eq!(parse_checksums("not a checksum line\n", "muninn"), None);
        assert_eq!(
            parse_checksums(
                "  0123456789ABCDEF0123456789abcdef0123456789ABCDEF0123456789abcdef\tmuninn  \n",
                "muninn"
            )
            .as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
    }

    #[test]
    fn a_bad_checksum_is_fatal() {
        let asset = "muninn_v0.10.0_linux_x86_64.tar.gz";
        let bytes = b"the archive";
        let actual = sha256_hex(bytes);
        let good = format!("{actual}  {asset}\n");
        assert!(verify_checksum(&good, asset, bytes).is_ok());

        assert!(matches!(
            verify_checksum(&good, "muninn_v0.10.0_linux_aarch64.tar.gz", bytes),
            Err(UpdateError::ChecksumMissing { .. })
        ));
        let bad = format!("{}  {asset}\n", "0".repeat(64));
        assert!(matches!(
            verify_checksum(&bad, asset, bytes),
            Err(UpdateError::Checksum { .. })
        ));
    }

    struct Fixture {
        path: String,
        data: Vec<u8>,
        kind: tar::EntryType,
        link: Option<String>,
        declared_size: Option<u64>,
    }

    impl Fixture {
        fn file(path: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
            Self {
                path: path.into(),
                data: data.into(),
                kind: tar::EntryType::Regular,
                link: None,
                declared_size: None,
            }
        }

        fn dir(path: impl Into<String>) -> Self {
            Self {
                path: path.into(),
                data: Vec::new(),
                kind: tar::EntryType::Directory,
                link: None,
                declared_size: None,
            }
        }

        fn symlink(path: impl Into<String>, target: impl Into<String>) -> Self {
            Self {
                path: path.into(),
                data: Vec::new(),
                kind: tar::EntryType::Symlink,
                link: Some(target.into()),
                declared_size: None,
            }
        }

        fn oversize(path: impl Into<String>, size: u64) -> Self {
            Self {
                path: path.into(),
                data: Vec::new(),
                kind: tar::EntryType::Regular,
                link: None,
                declared_size: Some(size),
            }
        }
    }

    /// Build a gzip tar with exact header paths. The name field is written
    /// directly so the fixture can carry unsafe paths (`..`, absolute) that a
    /// well-behaved builder would refuse.
    fn build_archive(entries: &[Fixture]) -> Vec<u8> {
        let mut tar = Vec::new();
        for entry in entries {
            let size = entry.declared_size.unwrap_or(entry.data.len() as u64);
            let mut header = tar::Header::new_gnu();
            header.set_path("placeholder").expect("placeholder path");
            header.set_size(size);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_entry_type(entry.kind);
            let name = entry.path.as_bytes();
            assert!(name.len() <= 100, "fixture path too long");
            {
                let bytes = header.as_mut_bytes();
                bytes[..100].fill(0);
                bytes[..name.len()].copy_from_slice(name);
                if let Some(link) = &entry.link {
                    let link = link.as_bytes();
                    assert!(link.len() <= 100, "fixture link too long");
                    bytes[157..257].fill(0);
                    bytes[157..157 + link.len()].copy_from_slice(link);
                }
            }
            header.set_cksum();
            tar.extend_from_slice(header.as_bytes());
            if entry.declared_size.is_none() {
                tar.extend_from_slice(&entry.data);
                let padding = (512 - entry.data.len() % 512) % 512;
                tar.extend(std::iter::repeat_n(0u8, padding));
            }
        }
        tar.extend(std::iter::repeat_n(0u8, 1024));
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&tar).expect("gzip fixture");
        encoder.finish().expect("gzip fixture")
    }

    fn release_archive(platform: &str, binary: &[u8]) -> Vec<u8> {
        let root = format!("muninn_v0.10.0_{platform}");
        build_archive(&[
            Fixture::dir(root.clone()),
            Fixture::file(format!("{root}/muninn"), binary),
            Fixture::file(
                format!("{root}/assets/muninn.desktop"),
                "[Desktop Entry]\nName=Muninn\n",
            ),
            Fixture::file(format!("{root}/assets/logo.png"), b"\x89PNG fixture"),
            Fixture::file(format!("{root}/assets/logo.svg"), "<svg/>"),
            Fixture::file(format!("{root}/LICENSE"), "MIT"),
            Fixture::file(format!("{root}/README.md"), "ignored"),
            Fixture::file(format!("{root}/CHANGELOG.md"), "ignored"),
            Fixture::dir(format!("{root}/vault")),
        ])
    }

    #[test]
    fn a_release_archive_yields_the_expected_entries() {
        let plan = tarball_plan("linux_x86_64");
        let payload =
            extract(&release_archive("linux_x86_64", b"new binary"), &plan).expect("extract");
        assert_eq!(payload.binary, b"new binary");
        assert_eq!(
            payload.desktop.as_deref(),
            Some(b"[Desktop Entry]\nName=Muninn\n".as_slice())
        );
        assert_eq!(payload.png.as_deref(), Some(b"\x89PNG fixture".as_slice()));
        assert_eq!(payload.svg.as_deref(), Some(b"<svg/>".as_slice()));
        assert_eq!(payload.license.as_deref(), Some(b"MIT".as_slice()));
    }

    #[test]
    fn a_different_archive_root_is_refused() {
        let plan = tarball_plan("linux_x86_64");
        let archive = release_archive("linux_aarch64", b"binary");
        assert!(matches!(
            extract(&archive, &plan),
            Err(UpdateError::Archive(_))
        ));
    }

    #[test]
    fn traversal_and_absolute_entries_are_refused() {
        let plan = tarball_plan("linux_x86_64");
        let traversal = build_archive(&[
            Fixture::file("muninn_v0.10.0_linux_x86_64/../evil", "x"),
            Fixture::file("muninn_v0.10.0_linux_x86_64/muninn", "binary"),
        ]);
        assert!(matches!(
            extract(&traversal, &plan),
            Err(UpdateError::Archive(_))
        ));

        let absolute = build_archive(&[
            Fixture::file("/etc/passwd", "x"),
            Fixture::file("muninn_v0.10.0_linux_x86_64/muninn", "binary"),
        ]);
        assert!(matches!(
            extract(&absolute, &plan),
            Err(UpdateError::Archive(_))
        ));
    }

    #[test]
    fn links_are_refused() {
        let plan = tarball_plan("linux_x86_64");
        let archive = build_archive(&[Fixture::symlink(
            "muninn_v0.10.0_linux_x86_64/muninn",
            "/bin/sh",
        )]);
        assert!(matches!(
            extract(&archive, &plan),
            Err(UpdateError::Archive(_))
        ));
    }

    #[test]
    fn a_missing_binary_is_refused() {
        let plan = tarball_plan("linux_x86_64");
        let archive = build_archive(&[Fixture::file("muninn_v0.10.0_linux_x86_64/LICENSE", "MIT")]);
        assert!(matches!(
            extract(&archive, &plan),
            Err(UpdateError::Archive(_))
        ));
    }

    #[test]
    fn an_oversized_binary_is_refused_from_the_header() {
        let plan = tarball_plan("linux_x86_64");
        let archive = build_archive(&[Fixture::oversize(
            "muninn_v0.10.0_linux_x86_64/muninn",
            ARCHIVE_MAX_BYTES as u64 + 1,
        )]);
        assert!(matches!(
            extract(&archive, &plan),
            Err(UpdateError::TooLarge { .. })
        ));
    }

    #[test]
    fn a_truncated_gzip_is_an_error_not_a_panic() {
        let plan = tarball_plan("linux_x86_64");
        let mut archive = release_archive("linux_x86_64", b"binary");
        archive.truncate(archive.len() / 2);
        assert!(matches!(
            extract(&archive, &plan),
            Err(UpdateError::Archive(_))
        ));
    }

    #[test]
    fn an_appimage_is_its_own_payload() {
        let payload = extract(b"appimage bytes", &appimage_plan()).expect("extract");
        assert_eq!(payload.binary, b"appimage bytes");
        assert!(payload.desktop.is_none());
        assert!(payload.license.is_none());
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt as _;
        fs::metadata(path).expect("metadata").permissions().mode() & 0o777
    }

    #[cfg(unix)]
    fn no_staging_files(directory: &Path) -> bool {
        fs::read_dir(directory)
            .expect("read dir")
            .filter_map(Result::ok)
            .all(|entry| {
                !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".muninn-update-")
            })
    }

    #[cfg(unix)]
    #[test]
    fn apply_replaces_the_binary_and_keeps_a_backup() {
        let tmp = TempDir::new("apply");
        let (layout, _prefix, binary) = tarball_install(&tmp);
        write_receipt(&layout, &[&binary]);
        let receipt = read_receipt(&layout).expect("receipt");
        let receipt_path = layout.receipt.clone().expect("receipt path");
        let before = fs::read(&receipt_path).expect("read receipt");
        let state = tmp.join("state");
        let payload = ArchivePayload {
            binary: b"new binary".to_vec(),
            ..ArchivePayload::default()
        };

        let applied = apply(
            &layout,
            &receipt,
            &payload,
            &tarball_plan("linux_x86_64"),
            &state,
        )
        .expect("apply");
        assert_eq!(fs::read(&binary).expect("binary"), b"new binary");
        assert_eq!(mode_of(&binary), 0o755);
        let previous = state.join("update/muninn.previous");
        assert_eq!(fs::read(&previous).expect("previous"), b"old binary");
        assert_eq!(mode_of(&previous), 0o755);
        assert_eq!(applied.version, "0.10.0");
        assert!(applied.warning.is_none());
        assert_eq!(applied.previous.as_deref(), Some(previous.as_path()));
        assert!(applied.restart_required);
        assert_eq!(fs::read(&receipt_path).expect("receipt"), before);
        assert!(no_staging_files(binary.parent().expect("bin dir")));
        assert!(no_staging_files(&state.join("update")));
    }

    #[cfg(unix)]
    #[test]
    fn apply_refreshes_only_receipt_listed_assets() {
        let tmp = TempDir::new("apply-assets");
        let (layout, prefix, binary) = tarball_install(&tmp);
        let desktop = prefix.join("share/applications/muninn.desktop");
        let svg = prefix.join("share/icons/hicolor/scalable/apps/muninn.svg");
        for path in [&desktop, &svg] {
            fs::create_dir_all(path.parent().expect("asset dir")).expect("create asset dir");
            fs::write(path, b"old asset").expect("write asset");
        }
        write_receipt(&layout, &[&binary, &desktop]);
        let receipt = read_receipt(&layout).expect("receipt");
        let state = tmp.join("state");
        let payload = ArchivePayload {
            binary: b"new binary".to_vec(),
            desktop: Some(b"new desktop".to_vec()),
            png: Some(b"new png".to_vec()),
            svg: Some(b"new svg".to_vec()),
            license: None,
        };

        let applied = apply(
            &layout,
            &receipt,
            &payload,
            &tarball_plan("linux_x86_64"),
            &state,
        )
        .expect("apply");
        assert!(applied.warning.is_none());
        assert_eq!(fs::read(&desktop).expect("desktop"), b"new desktop");
        assert_eq!(fs::read(&svg).expect("svg"), b"old asset");
        assert!(
            !prefix
                .join("share/icons/hicolor/512x512/apps/muninn.png")
                .exists()
        );
    }

    #[cfg(unix)]
    #[test]
    fn apply_never_writes_receipt_entries_outside_the_prefix() {
        let tmp = TempDir::new("apply-outside");
        let (layout, _prefix, binary) = tarball_install(&tmp);
        write_receipt(&layout, &[&binary]);
        let receipt = read_receipt(&layout).expect("receipt");
        let outside = tmp.join("outside");
        let mut tampered = receipt.clone();
        tampered.push(outside.clone());
        let state = tmp.join("state");
        let payload = ArchivePayload {
            binary: b"new binary".to_vec(),
            ..ArchivePayload::default()
        };

        let result = apply(
            &layout,
            &tampered,
            &payload,
            &tarball_plan("linux_x86_64"),
            &state,
        );
        assert!(matches!(result, Err(UpdateError::Receipt(_))));
        assert!(!outside.exists());
        assert_eq!(fs::read(&binary).expect("binary"), b"old binary");
        assert!(!state.join("update/muninn.previous").exists());
    }

    #[cfg(unix)]
    #[test]
    fn apply_refuses_a_duplicated_receipt() {
        let tmp = TempDir::new("apply-duplicate");
        let (layout, _prefix, binary) = tarball_install(&tmp);
        write_receipt(&layout, &[&binary]);
        let mut duplicated = read_receipt(&layout).expect("receipt");
        duplicated.push(binary.clone());
        let state = tmp.join("state");
        let payload = ArchivePayload {
            binary: b"new binary".to_vec(),
            ..ArchivePayload::default()
        };

        let result = apply(
            &layout,
            &duplicated,
            &payload,
            &tarball_plan("linux_x86_64"),
            &state,
        );
        assert!(matches!(result, Err(UpdateError::Receipt(_))));
        assert_eq!(fs::read(&binary).expect("binary"), b"old binary");
        assert!(!state.join("update/muninn.previous").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_binary_directory_leaves_the_old_binary_intact() {
        use std::os::unix::fs::PermissionsExt as _;

        let tmp = TempDir::new("apply-readonly");
        let (layout, _prefix, binary) = tarball_install(&tmp);
        write_receipt(&layout, &[&binary]);
        let receipt = read_receipt(&layout).expect("receipt");
        let bin_dir = binary.parent().expect("bin dir");
        let mut permissions = fs::metadata(bin_dir).expect("metadata").permissions();
        permissions.set_mode(0o555);
        fs::set_permissions(bin_dir, permissions).expect("chmod read-only");

        let probe = bin_dir.join(".probe");
        if fs::write(&probe, b"x").is_ok() {
            let _ = fs::remove_file(&probe);
            let mut permissions = fs::metadata(bin_dir).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(bin_dir, permissions).expect("chmod writable");
            eprintln!("skipping: this process can write to a read-only directory");
            return;
        }

        let state = tmp.join("state");
        let payload = ArchivePayload {
            binary: b"new binary".to_vec(),
            ..ArchivePayload::default()
        };
        let result = apply(
            &layout,
            &receipt,
            &payload,
            &tarball_plan("linux_x86_64"),
            &state,
        );

        let mut permissions = fs::metadata(bin_dir).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(bin_dir, permissions).expect("chmod writable");

        assert!(result.is_err(), "the update must fail");
        assert_eq!(fs::read(&binary).expect("binary"), b"old binary");
        assert!(no_staging_files(bin_dir));
    }

    #[cfg(unix)]
    #[test]
    fn apply_replaces_an_appimage_and_backs_it_up() {
        let tmp = TempDir::new("apply-appimage");
        let appimage = tmp.join("muninn_v0.10.0_linux_x86_64.AppImage");
        fs::write(&appimage, b"old appimage").expect("write appimage");
        set_mode(&appimage, 0o755).expect("chmod appimage");
        let layout =
            layout_for(InstallMethod::AppImage, &appimage, Some(&appimage)).expect("layout");
        let state = tmp.join("state");
        let payload = ArchivePayload {
            binary: b"new appimage".to_vec(),
            ..ArchivePayload::default()
        };

        let applied = apply(&layout, &[], &payload, &appimage_plan(), &state).expect("apply");
        assert_eq!(fs::read(&appimage).expect("appimage"), b"new appimage");
        assert_eq!(mode_of(&appimage), 0o755);
        let previous = state.join("update/muninn.previous");
        assert_eq!(fs::read(&previous).expect("previous"), b"old appimage");
        assert_eq!(mode_of(&previous), 0o755);
        assert_eq!(applied.version, "0.10.0");
        assert!(applied.previous.is_some());
        assert!(applied.restart_required);
    }

    #[cfg(unix)]
    #[test]
    fn apply_touches_no_product_state() {
        let tmp = TempDir::new("apply-state");
        let (layout, _prefix, binary) = tarball_install(&tmp);
        write_receipt(&layout, &[&binary]);
        let receipt = read_receipt(&layout).expect("receipt");

        // Sentinels a product-state write would disturb.
        let config = tmp.join("home/.config/muninn");
        let vault = tmp.join("home/vault");
        fs::create_dir_all(&config).expect("config dir");
        fs::create_dir_all(&vault).expect("vault dir");
        let settings = config.join("settings.json");
        let note = vault.join("note.md");
        fs::write(&settings, b"{\"theme\":\"dark\"}").expect("settings");
        fs::write(&note, b"# note\n").expect("note");

        let state = tmp.join("home/.local/state");
        let payload = ArchivePayload {
            binary: b"new binary".to_vec(),
            ..ArchivePayload::default()
        };
        apply(
            &layout,
            &receipt,
            &payload,
            &tarball_plan("linux_x86_64"),
            &state,
        )
        .expect("apply");

        assert_eq!(
            fs::read(&settings).expect("settings"),
            b"{\"theme\":\"dark\"}"
        );
        assert_eq!(fs::read(&note).expect("note"), b"# note\n");
        assert_eq!(
            fs::read_dir(&config).expect("config entries").count(),
            1,
            "only settings.json lives in the config dir"
        );
        assert_eq!(
            fs::read_dir(&vault).expect("vault entries").count(),
            1,
            "only the note lives in the vault"
        );
        assert_eq!(
            fs::read_dir(&state).expect("state entries").count(),
            1,
            "the only state write is the update directory"
        );
    }

    #[cfg(unix)]
    fn fake_verifier(directory: &Path, name: &str, script: &str) -> PathBuf {
        let path = directory.join(name);
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("write fake verifier");
        set_mode(&path, 0o755).expect("chmod fake verifier");
        path
    }

    #[cfg(unix)]
    struct SignatureFixture {
        directory: TempDir,
        checksums: PathBuf,
        archive: PathBuf,
        bundle: PathBuf,
    }

    #[cfg(unix)]
    impl SignatureFixture {
        fn new() -> Self {
            let directory = TempDir::new("signature");
            let checksums = directory.join("checksums.txt");
            let archive = directory.join("muninn_v0.10.0_linux_x86_64.tar.gz");
            let bundle = directory.join("checksums.txt.sigstore.json");
            fs::write(&checksums, b"hash  muninn_v0.10.0_linux_x86_64.tar.gz\n")
                .expect("write checksums");
            fs::write(&archive, b"archive").expect("write archive");
            fs::write(&bundle, b"{}").expect("write bundle");
            Self {
                directory,
                checksums,
                archive,
                bundle,
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn no_verifier_is_a_warning() {
        let fixture = SignatureFixture::new();
        let status = verify_signature(
            &Verifiers::default(),
            &fixture.checksums,
            &fixture.archive,
            Some(&fixture.bundle),
        );
        assert!(matches!(status, SignatureStatus::Warning(_)), "{status:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_fake_cosign_success_is_verified_with_the_installer_policy() {
        let fixture = SignatureFixture::new();
        let marker = fixture.directory.join("cosign-args");
        let script = format!("printf '%s\\n' \"$@\" > '{}'\nexit 0", marker.display());
        let cosign = fake_verifier(fixture.directory.path(), "cosign", &script);
        let status = verify_signature(
            &Verifiers {
                cosign: Some(cosign),
                gh: None,
            },
            &fixture.checksums,
            &fixture.archive,
            Some(&fixture.bundle),
        );
        assert_eq!(status, SignatureStatus::Verified);
        let arguments = fs::read_to_string(&marker).expect("cosign arguments");
        assert!(arguments.contains("verify-blob"), "{arguments}");
        assert!(arguments.contains("--bundle"), "{arguments}");
        assert!(arguments.contains(COSIGN_IDENTITY), "{arguments}");
        assert!(
            arguments.contains("--certificate-identity-regexp"),
            "{arguments}"
        );
        assert!(arguments.contains(OIDC_ISSUER), "{arguments}");
        assert!(
            arguments.contains("--certificate-oidc-issuer"),
            "{arguments}"
        );
        assert!(!arguments.contains("login/oauth"), "{arguments}");
        assert!(arguments.contains("checksums.txt"), "{arguments}");
        assert!(!arguments.contains("archive"), "{arguments}");
    }

    #[cfg(unix)]
    #[test]
    fn a_fake_cosign_failure_is_only_a_warning() {
        let fixture = SignatureFixture::new();
        let cosign = fake_verifier(
            fixture.directory.path(),
            "cosign",
            "echo 'bad signature' >&2\nexit 1",
        );
        let status = verify_signature(
            &Verifiers {
                cosign: Some(cosign),
                gh: None,
            },
            &fixture.checksums,
            &fixture.archive,
            Some(&fixture.bundle),
        );
        assert!(
            matches!(status, SignatureStatus::Warning(ref message) if message.contains("bad signature")),
            "{status:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_stuck_verifier_times_out_as_a_warning() {
        let fixture = SignatureFixture::new();
        // `exec` makes the killed process the sleeper itself, so the test
        // leaves nothing behind.
        let cosign = fake_verifier(fixture.directory.path(), "cosign", "exec sleep 60");
        let started = Instant::now();
        let status = verify_signature_with_timeout(
            &Verifiers {
                cosign: Some(cosign),
                gh: None,
            },
            &fixture.checksums,
            &fixture.archive,
            Some(&fixture.bundle),
            Duration::from_millis(200),
        );
        assert!(
            matches!(status, SignatureStatus::Warning(ref message) if message.contains("timed out")),
            "{status:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "a stuck verifier must be bounded"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_verifier_grandchild_cannot_defeat_the_timeout() {
        let fixture = SignatureFixture::new();
        // The background sleeper inherits stdout/stderr, so killing only the
        // shell leaves the pipes open. The update must still return promptly.
        let cosign = fake_verifier(fixture.directory.path(), "cosign", "sleep 5 & wait");
        let started = Instant::now();
        let status = verify_signature_with_timeout(
            &Verifiers {
                cosign: Some(cosign),
                gh: None,
            },
            &fixture.checksums,
            &fixture.archive,
            Some(&fixture.bundle),
            Duration::from_millis(200),
        );
        assert!(
            matches!(status, SignatureStatus::Warning(ref message) if message.contains("timed out")),
            "{status:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "a grandchild holding the pipes must not defeat the timeout"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cosign_without_a_bundle_warns_and_does_not_run() {
        let fixture = SignatureFixture::new();
        let marker = fixture.directory.join("cosign-ran");
        let script = format!("touch '{}'\nexit 0", marker.display());
        let cosign = fake_verifier(fixture.directory.path(), "cosign", &script);
        let status = verify_signature(
            &Verifiers {
                cosign: Some(cosign),
                gh: None,
            },
            &fixture.checksums,
            &fixture.archive,
            None,
        );
        assert!(matches!(status, SignatureStatus::Warning(_)), "{status:?}");
        assert!(!marker.exists(), "cosign must not run without a bundle");
    }

    #[cfg(unix)]
    #[test]
    fn a_fake_gh_attestation_success_is_verified() {
        let fixture = SignatureFixture::new();
        let marker = fixture.directory.join("gh-args");
        let script = format!("printf '%s\\n' \"$@\" > '{}'\nexit 0", marker.display());
        let gh = fake_verifier(fixture.directory.path(), "gh", &script);
        let status = verify_signature(
            &Verifiers {
                cosign: None,
                gh: Some(gh),
            },
            &fixture.checksums,
            &fixture.archive,
            None,
        );
        assert_eq!(status, SignatureStatus::Verified);
        let arguments = fs::read_to_string(&marker).expect("gh arguments");
        assert!(arguments.contains("attestation"), "{arguments}");
        assert!(arguments.contains("verify"), "{arguments}");
        assert!(arguments.contains("--repo"), "{arguments}");
        assert!(arguments.contains(REPO), "{arguments}");
    }

    #[cfg(unix)]
    #[test]
    fn a_fake_gh_failure_is_only_a_warning() {
        let fixture = SignatureFixture::new();
        let gh = fake_verifier(
            fixture.directory.path(),
            "gh",
            "echo 'no attestation' >&2\nexit 1",
        );
        let status = verify_signature(
            &Verifiers {
                cosign: None,
                gh: Some(gh),
            },
            &fixture.checksums,
            &fixture.archive,
            None,
        );
        assert!(
            matches!(status, SignatureStatus::Warning(ref message) if message.contains("no attestation")),
            "{status:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_bad_checksum_is_fatal_even_with_a_verifier() {
        let fixture = SignatureFixture::new();
        let cosign = fake_verifier(fixture.directory.path(), "cosign", "exit 0");
        let verifiers = Verifiers {
            cosign: Some(cosign),
            gh: None,
        };
        assert_eq!(
            verify_signature(
                &verifiers,
                &fixture.checksums,
                &fixture.archive,
                Some(&fixture.bundle)
            ),
            SignatureStatus::Verified
        );
        assert!(matches!(
            verify_checksum(
                "0000000000000000000000000000000000000000000000000000000000000000  muninn_v0.10.0_linux_x86_64.tar.gz\n",
                "muninn_v0.10.0_linux_x86_64.tar.gz",
                b"archive"
            ),
            Err(UpdateError::Checksum { .. })
        ));
    }

    #[test]
    fn release_hosts_are_limited_to_github() {
        for host in [
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ] {
            assert!(host_allowed(host), "{host}");
        }
        for host in [
            "evilgithub.com",
            "github.com.evil.example",
            "raw.githubusercontent.com.evil.example",
            "example.com",
            "api.github.com.evil.example",
        ] {
            assert!(!host_allowed(host), "{host}");
        }
    }

    #[test]
    fn the_identity_policy_uses_the_actions_issuer() {
        assert_eq!(OIDC_ISSUER, "https://token.actions.githubusercontent.com");
        assert_eq!(
            COSIGN_IDENTITY,
            "^https://github.com/raythurman2386/muninn/.github/workflows/release.yml@refs/tags/"
        );
        assert!(!COSIGN_IDENTITY.contains("login/oauth"));
    }

    #[test]
    fn an_archive_for_another_method_is_refused_by_extract() {
        let plan = UpdatePlan {
            version: "0.10.0".to_owned(),
            asset: "muninn_0.10.0_amd64.deb".to_owned(),
            platform: "linux_x86_64".to_owned(),
            method: InstallMethod::Deb,
        };
        assert!(matches!(
            extract(b"deb", &plan),
            Err(UpdateError::Unsupported(_))
        ));
    }
}

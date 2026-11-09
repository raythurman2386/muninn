//! Tailscale transport.
//!
//! One Muninn device hosts an immutable commit store. Peers reach it over
//! HTTPS through `tailscale serve`, which terminates TLS with the tailnet's
//! `*.ts.net` certificate and proxies to a server bound to `127.0.0.1` only.
//! The store uses the same commit manifests as the Drive adapter. Unlike
//! Drive, the host is a single serialized writer, so it accepts a commit only
//! when its parents are exactly the current heads. A peer that read an older
//! head is told the host moved and replans, like a rejected non-force GitHub
//! ref update. Nothing here overwrites or deletes a stored blob or commit.
//!
//! The host serves the commit store, not the vault. Its own vault syncs into
//! the store through [`DirStore`] with no HTTP in between.
//!
//! Auth is a shared key from the OS credential store, sent as a bearer token.
//! The host can also require that `Tailscale-User-Login`, which Serve sets from
//! its own WhoIs lookup, is on an allow-list.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::SCHEMA_VERSION;
use crate::drive::{self, DriveCommitDoc, DriveRemote};
use crate::error::SyncError;
use crate::http::{CancelToken, HttpRequest, HttpTransport, Sleeper, classify_status, with_retry};
use crate::limits::DEFAULT_LIMITS;
use crate::model::{BindingId, ContentHash, Snapshot};
use crate::pathutil::RelativePath;
use crate::secret::Secret;

/// Loopback port the host listens on unless the user picks another.
pub const TAILNET_DEFAULT_PORT: u16 = 47821;
/// Every store path lives under this prefix.
pub const TAILNET_API_PREFIX: &str = "/muninn-sync/v1/";
const MAX_COMMIT_BYTES: usize = 8 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const LOGIN_HEADER: &str = "tailscale-user-login";

static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Where this device's side of a Tailscale binding lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TailnetRole {
    /// This device owns the store directory and serves it to peers.
    Host { store_dir: PathBuf },
    /// This device reaches a host by its MagicDNS name over HTTPS.
    Peer { host: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TailnetScope {
    pub role: TailnetRole,
}

impl TailnetScope {
    pub fn validate(&self) -> Result<(), SyncError> {
        match &self.role {
            TailnetRole::Host { store_dir } if store_dir.as_os_str().is_empty() => {
                Err(SyncError::Config {
                    detail: "Tailscale host store folder is missing".to_owned(),
                })
            }
            TailnetRole::Host { .. } => Ok(()),
            TailnetRole::Peer { host } => validate_tailnet_host(host),
        }
    }
}

/// Accepts a full MagicDNS name such as `minis.tail1234.ts.net`. A short
/// MagicDNS name has no TLS certificate, so it is refused.
pub fn validate_tailnet_host(host: &str) -> Result<(), SyncError> {
    let bad = |detail: &str| SyncError::Config {
        detail: format!("Tailscale host {host:?}: {detail}"),
    };
    if host.is_empty() || host.len() > 253 {
        return Err(bad(
            "enter the full MagicDNS name, for example minis.tail1234.ts.net",
        ));
    }
    if host != host.to_ascii_lowercase() {
        return Err(bad("use the lowercase MagicDNS name"));
    }
    if !host.ends_with(".ts.net") {
        return Err(bad("the name must end in .ts.net"));
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 4 {
        return Err(bad(
            "enter the device and tailnet name, not only the device",
        ));
    }
    for label in labels {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        {
            return Err(bad("the name is not a valid DNS name"));
        }
    }
    Ok(())
}

/// Store directory for a host binding, inside the sync state directory and
/// never inside the vault.
#[must_use]
pub fn tailnet_store_dir(state_dir: &Path, binding: &BindingId) -> PathBuf {
    state_dir.join("tailnet-store").join(binding.as_str())
}

/// Fresh 32-byte shared key, base64url. Store it only in the credential store.
#[must_use]
pub fn new_shared_key() -> Secret {
    // 256 bits straight from the OS CSPRNG (UUIDv4 spends 6 bits per 16
    // bytes on version and variant).
    Secret::new(crate::drive::random_token(32))
}

/// What a store holds. Commits are complete; there is no pagination.
pub(crate) struct StoreState {
    pub commits: BTreeMap<String, DriveCommitDoc>,
    pub blobs: BTreeSet<String>,
}

/// Immutable, create-only commit store. Both the host's directory and the
/// peer's HTTP client implement it.
pub(crate) trait CommitStore {
    fn state(&self) -> Result<StoreState, SyncError>;
    fn blob(&self, sha256: &str) -> Result<Vec<u8>, SyncError>;
    fn put_blob(&self, sha256: &str, bytes: &[u8]) -> Result<(), SyncError>;
    fn put_commit(&self, id: &str, bytes: &[u8]) -> Result<(), SyncError>;
}

fn valid_sha(text: &str) -> bool {
    text.len() == 64
        && text
            .chars()
            .all(|ch| ch.is_ascii_digit() || ('a'..='f').contains(&ch))
}

fn valid_commit_id(text: &str) -> bool {
    (8..=64).contains(&text.len())
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

/// The host's on-disk store: `blobs/<sha256>` and `commits/<id>.json`.
pub struct DirStore {
    root: PathBuf,
}

impl DirStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn blobs_dir(&self) -> PathBuf {
        self.root.join("blobs")
    }

    fn commits_dir(&self) -> PathBuf {
        self.root.join("commits")
    }

    /// Writes `bytes` to `target` only if `target` does not exist. A temp file
    /// is linked into place, so a reader never sees a partial file and an
    /// existing file is never replaced.
    fn create_new(&self, dir: &Path, name: &str, bytes: &[u8]) -> Result<bool, SyncError> {
        std::fs::create_dir_all(dir).map_err(SyncError::io)?;
        let target = dir.join(name);
        if target.exists() {
            return Ok(false);
        }
        let temp = dir.join(format!(".tmp-{}", uuid::Uuid::new_v4().simple()));
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(SyncError::io)?;
            file.write_all(bytes).map_err(SyncError::io)?;
            file.sync_all().map_err(SyncError::io)?;
        }
        let linked = std::fs::hard_link(&temp, &target);
        let _ = std::fs::remove_file(&temp);
        match linked {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(error) => Err(SyncError::io(error)),
        }
    }
}

impl CommitStore for DirStore {
    fn state(&self) -> Result<StoreState, SyncError> {
        let mut blobs = BTreeSet::new();
        if let Ok(entries) = std::fs::read_dir(self.blobs_dir()) {
            for entry in entries {
                let entry = entry.map_err(SyncError::io)?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if valid_sha(&name) {
                    blobs.insert(name);
                }
            }
        }
        let mut commits = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(self.commits_dir()) {
            for entry in entries {
                let entry = entry.map_err(SyncError::io)?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let Some(id) = name.strip_suffix(".json") else {
                    continue;
                };
                if !valid_commit_id(id) {
                    continue;
                }
                let bytes = std::fs::read(entry.path()).map_err(SyncError::io)?;
                let doc = drive::parse_commit(&bytes)?;
                if doc.id != id {
                    return Err(SyncError::Protocol {
                        detail: format!("commit file {name} does not match id {}", doc.id),
                    });
                }
                commits.insert(doc.id.clone(), doc);
            }
        }
        Ok(StoreState { commits, blobs })
    }

    fn blob(&self, sha256: &str) -> Result<Vec<u8>, SyncError> {
        if !valid_sha(sha256) {
            return Err(SyncError::Protocol {
                detail: "blob name is not a sha256".to_owned(),
            });
        }
        let path = self.blobs_dir().join(sha256);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(SyncError::NotFound {
                    detail: format!("blob {sha256} is not in the store"),
                });
            }
            Err(error) => return Err(SyncError::io(error)),
        };
        if ContentHash::of(&bytes).to_hex() != sha256 {
            return Err(SyncError::Protocol {
                detail: format!("stored blob {sha256} does not match its bytes"),
            });
        }
        Ok(bytes)
    }

    fn put_blob(&self, sha256: &str, bytes: &[u8]) -> Result<(), SyncError> {
        if !valid_sha(sha256) {
            return Err(SyncError::Protocol {
                detail: "blob name is not a sha256".to_owned(),
            });
        }
        if bytes.len() as u64 > DEFAULT_LIMITS.max_file_bytes {
            return Err(SyncError::Limit {
                detail: format!("blob {sha256} is over the per-file limit"),
            });
        }
        if ContentHash::of(bytes).to_hex() != sha256 {
            return Err(SyncError::Protocol {
                detail: format!("blob {sha256} does not match its bytes"),
            });
        }
        self.create_new(&self.blobs_dir(), sha256, bytes)?;
        Ok(())
    }

    fn put_commit(&self, id: &str, bytes: &[u8]) -> Result<(), SyncError> {
        if !valid_commit_id(id) {
            return Err(SyncError::Protocol {
                detail: "commit id is not valid".to_owned(),
            });
        }
        if bytes.len() > MAX_COMMIT_BYTES {
            return Err(SyncError::Limit {
                detail: "commit manifest is too large".to_owned(),
            });
        }
        let doc = drive::parse_commit(bytes)?;
        if doc.id != id {
            return Err(SyncError::Protocol {
                detail: "commit id does not match its manifest".to_owned(),
            });
        }
        doc.snapshot()?;
        let name = format!("{id}.json");
        let existing = self.commits_dir().join(&name);
        if let Ok(previous) = std::fs::read(&existing) {
            return if previous == bytes {
                Ok(())
            } else {
                Err(SyncError::ConflictRemote {
                    detail: format!("commit {id} already exists with other contents"),
                })
            };
        }
        // One writer at a time inside this process: the server thread and the
        // host's own sync both come through here. The head check and the
        // create are one step, so the store never forks.
        let _guard = STORE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Ok(previous) = std::fs::read(&existing) {
            return if previous == bytes {
                Ok(())
            } else {
                Err(SyncError::ConflictRemote {
                    detail: format!("commit {id} already exists with other contents"),
                })
            };
        }
        let state = self.state()?;
        let mut heads = drive::heads_of(&state.commits);
        heads.sort();
        let mut parents = doc.parents.clone();
        parents.sort();
        if parents != heads {
            return Err(SyncError::ConflictRemote {
                detail: "the Tailscale host moved since this sync read it; Sync Now again"
                    .to_owned(),
            });
        }
        for parent in &doc.parents {
            if !state.commits.contains_key(parent) {
                return Err(SyncError::Protocol {
                    detail: format!("commit {id} names parent {parent}, which is not stored"),
                });
            }
        }
        for entry in &doc.entries {
            if !state.blobs.contains(&entry.sha256) {
                return Err(SyncError::Protocol {
                    detail: format!(
                        "commit {id} needs blob {}, which is not stored",
                        entry.sha256
                    ),
                });
            }
        }
        if !self.create_new(&self.commits_dir(), &name, bytes)? {
            let previous = std::fs::read(&existing).map_err(SyncError::io)?;
            if previous != bytes {
                return Err(SyncError::ConflictRemote {
                    detail: format!("commit {id} already exists with other contents"),
                });
            }
        }
        Ok(())
    }
}

/// Peer side: the same store reached over HTTPS.
pub struct HttpStore<'a, T> {
    /// `https://<host>.ts.net` in production; a loopback base in tests.
    pub base: String,
    pub transport: T,
    pub key: &'a Secret,
    pub cancel: CancelToken,
    pub sleeper: &'a dyn Sleeper,
    pub request_timeout: Duration,
}

impl<T: HttpTransport> HttpStore<'_, T> {
    fn url(&self, rest: &str) -> String {
        format!(
            "{}{}{}",
            self.base.trim_end_matches('/'),
            TAILNET_API_PREFIX,
            rest
        )
    }

    fn request(&self, method: &str, rest: &str, body: &[u8], kind: &str) -> HttpRequest {
        HttpRequest {
            method: method.to_owned(),
            url: self.url(rest),
            headers: vec![
                (
                    "authorization".to_owned(),
                    format!("Bearer {}", self.key.expose()),
                ),
                ("content-type".to_owned(), kind.to_owned()),
            ],
            body: body.to_vec(),
            mutation: method != "GET",
            timeout: self.request_timeout,
        }
    }

    fn get(&self, rest: &str) -> Result<Vec<u8>, SyncError> {
        with_retry(&self.cancel, self.sleeper, || {
            let response =
                self.transport
                    .execute(&self.request("GET", rest, &[], "application/json"))?;
            host_status(&response, false)?;
            Ok(response.body)
        })
    }

    /// PUTs are create-only and idempotent on the server, so a retry after a
    /// dropped response cannot duplicate or replace anything.
    fn put(&self, rest: &str, body: &[u8], kind: &str) -> Result<(), SyncError> {
        let mut attempt = 0u32;
        loop {
            self.cancel.check()?;
            let result = self
                .transport
                .execute(&self.request("PUT", rest, body, kind))
                .and_then(|response| host_status(&response, true));
            match result {
                Ok(()) => return Ok(()),
                Err(error) if (error.retryable() || error.outcome_unknown()) && attempt < 3 => {
                    self.sleeper
                        .sleep(Duration::from_millis(50u64 << attempt), &self.cancel)?;
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }
}

/// The host answers 400 or 422 when it refused bytes it will never accept
/// (bad hash, malformed manifest, missing blob). That is a protocol error,
/// not a remote conflict and not an unknown outcome.
fn host_status(response: &crate::http::HttpResponse, mutation: bool) -> Result<(), SyncError> {
    if matches!(response.status, 400 | 422) {
        return Err(SyncError::Protocol {
            detail: format!(
                "Tailscale host refused the request: {}",
                crate::error::provider_message(response.text().unwrap_or(""))
            ),
        });
    }
    classify_status(response, mutation)
}

impl<T: HttpTransport> CommitStore for HttpStore<'_, T> {
    fn state(&self) -> Result<StoreState, SyncError> {
        let body = self.get("state")?;
        let json: serde_json::Value =
            serde_json::from_slice(&body).map_err(|_| SyncError::Protocol {
                detail: "Tailscale host state is not json".to_owned(),
            })?;
        if json.get("kind").and_then(|v| v.as_str()) != Some("muninn-sync-store")
            || json.get("schema").and_then(|v| v.as_u64()) != Some(u64::from(SCHEMA_VERSION))
        {
            return Err(SyncError::Protocol {
                detail: "Tailscale host is not a supported Muninn store".to_owned(),
            });
        }
        if json.get("complete").and_then(|v| v.as_bool()) != Some(true) {
            return Err(SyncError::ListingIncomplete {
                detail: "Tailscale host listing was not complete".to_owned(),
            });
        }
        let mut commits = BTreeMap::new();
        for value in json
            .get("commits")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let doc = drive::parse_commit(value.to_string().as_bytes())?;
            commits.insert(doc.id.clone(), doc);
        }
        let mut blobs = BTreeSet::new();
        for value in json
            .get("blobs")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let sha = value.as_str().unwrap_or_default();
            if !valid_sha(sha) {
                return Err(SyncError::Protocol {
                    detail: "Tailscale host listed a bad blob name".to_owned(),
                });
            }
            blobs.insert(sha.to_owned());
        }
        Ok(StoreState { commits, blobs })
    }

    fn blob(&self, sha256: &str) -> Result<Vec<u8>, SyncError> {
        self.get(&format!("blobs/{sha256}"))
    }

    fn put_blob(&self, sha256: &str, bytes: &[u8]) -> Result<(), SyncError> {
        self.put(
            &format!("blobs/{sha256}"),
            bytes,
            "application/octet-stream",
        )
    }

    fn put_commit(&self, id: &str, bytes: &[u8]) -> Result<(), SyncError> {
        self.put(&format!("commits/{id}"), bytes, "application/json")
    }
}

/// Commits and heads only, with no blob bytes. Publish uses this to see
/// whether the host moved since fetch.
pub(crate) fn listing(store: &dyn CommitStore) -> Result<DriveRemote, SyncError> {
    let state = store.state()?;
    for commit in state.commits.values() {
        for parent in &commit.parents {
            if !state.commits.contains_key(parent) {
                return Err(SyncError::ListingIncomplete {
                    detail: format!(
                        "commit {} names parent {parent}, which the host did not list",
                        commit.id
                    ),
                });
            }
        }
        for entry in &commit.entries {
            if !state.blobs.contains(&entry.sha256) {
                return Err(SyncError::Protocol {
                    detail: format!(
                        "commit {} needs blob {}, which the host did not list",
                        commit.id, entry.sha256
                    ),
                });
            }
        }
    }
    let heads = drive::heads_of(&state.commits);
    let blob_ids = state
        .blobs
        .iter()
        .map(|sha| (sha.clone(), sha.clone()))
        .collect();
    Ok(DriveRemote {
        commits: state.commits,
        heads,
        blobs: BTreeMap::new(),
        blob_ids,
    })
}

/// Full remote view: the listing plus the bytes every head needs, each
/// checked against its hash.
pub(crate) fn fetch(store: &dyn CommitStore) -> Result<DriveRemote, SyncError> {
    let mut remote = listing(store)?;
    let mut needed = BTreeSet::new();
    for head in &remote.heads {
        if let Some(commit) = remote.commits.get(head) {
            for entry in &commit.entries {
                needed.insert(entry.sha256.clone());
            }
        }
    }
    for sha in needed {
        let bytes = store.blob(&sha)?;
        if ContentHash::of(&bytes).to_hex() != sha {
            return Err(SyncError::Protocol {
                detail: format!("Tailscale blob {sha} does not match its bytes"),
            });
        }
        remote.blobs.insert(ContentHash::parse(&sha)?, bytes);
    }
    Ok(remote)
}

/// Writes missing blobs, then one immutable commit. Returns the commit id.
/// A commit that already carries `operation_id` is reused, so a retry after an
/// unknown outcome does not publish twice.
pub(crate) fn publish(
    store: &dyn CommitStore,
    remote: &DriveRemote,
    desired: &Snapshot,
    parents: &[String],
    tombstones: &[(RelativePath, ContentHash)],
    local_bytes: &BTreeMap<RelativePath, Vec<u8>>,
    operation_id: &str,
) -> Result<String, SyncError> {
    if let Some(existing) = remote
        .commits
        .values()
        .find(|commit| commit.operation_id == operation_id)
    {
        return Ok(existing.id.clone());
    }
    let mut stored: BTreeSet<String> = remote.blob_ids.keys().cloned().collect();
    for (path, record) in &desired.files {
        let hex_hash = record.hash.to_hex();
        if stored.contains(&hex_hash) {
            continue;
        }
        let bytes = remote
            .blobs
            .get(&record.hash)
            .map(Vec::as_slice)
            .or_else(|| local_bytes.get(path).map(Vec::as_slice))
            .ok_or_else(|| SyncError::Protocol {
                detail: format!("missing bytes for {path}"),
            })?;
        if ContentHash::of(bytes) != record.hash {
            return Err(SyncError::Protocol {
                detail: format!("bytes for {path} do not match the plan"),
            });
        }
        store.put_blob(&hex_hash, bytes)?;
        stored.insert(hex_hash);
    }
    let commit_id = uuid::Uuid::new_v4().simple().to_string();
    let entries: Vec<_> = desired
        .files
        .iter()
        .map(|(path, record)| {
            let hex_hash = record.hash.to_hex();
            serde_json::json!({
                "path": path.as_str(),
                "sha256": hex_hash,
                "size": record.size,
                "blob_id": hex_hash,
            })
        })
        .collect();
    let tombstones: Vec<_> = tombstones
        .iter()
        .map(|(path, hash)| {
            serde_json::json!({
                "path": path.as_str(),
                "base_sha256": hash.to_hex(),
            })
        })
        .collect();
    let manifest = serde_json::json!({
        "schema": SCHEMA_VERSION,
        "kind": "muninn-sync-commit",
        "id": commit_id,
        "operation_id": operation_id,
        "parents": parents,
        "entries": entries,
        "tombstones": tombstones,
    });
    store.put_commit(&commit_id, manifest.to_string().as_bytes())?;
    Ok(commit_id)
}

/// Host-side settings for [`TailnetHost::start`].
pub struct TailnetHostConfig {
    /// Loopback port. `0` picks a free port (tests).
    pub port: u16,
    pub store_dir: PathBuf,
    pub key: Secret,
    /// Optional `Tailscale-User-Login` allow-list. Empty means the shared key
    /// alone decides.
    pub allowed_logins: Vec<String>,
}

/// The host's commit-store server. It binds `127.0.0.1` only; `tailscale
/// serve` is the only way in from the tailnet. Dropping it stops the server.
pub struct TailnetHost {
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TailnetHost {
    pub fn start(config: TailnetHostConfig) -> Result<Self, SyncError> {
        if config.key.expose().len() < 32 {
            return Err(SyncError::Credentials {
                detail: "the Tailscale shared key is too short".to_owned(),
            });
        }
        let listener =
            TcpListener::bind(("127.0.0.1", config.port)).map_err(|error| SyncError::Io {
                detail: format!("could not listen on 127.0.0.1:{}: {error}", config.port),
            })?;
        listener.set_nonblocking(true).map_err(SyncError::io)?;
        let addr = listener.local_addr().map_err(SyncError::io)?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&shutdown);
        let server = Server {
            store: DirStore::new(config.store_dir),
            key_digest: Sha256::digest(config.key.expose().as_bytes()).into(),
            allowed: config
                .allowed_logins
                .iter()
                .map(|login| login.trim().to_ascii_lowercase())
                .filter(|login| !login.is_empty())
                .collect(),
        };
        let thread = thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        server.handle(stream);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => thread::sleep(Duration::from_millis(100)),
                }
            }
        });
        Ok(Self {
            addr,
            shutdown,
            thread: Some(thread),
        })
    }

    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The command the user runs once on the host.
    #[must_use]
    pub fn serve_command(&self) -> String {
        serve_command(self.addr.port())
    }
}

/// `tailscale serve` command that publishes a loopback port on the tailnet.
#[must_use]
pub fn serve_command(port: u16) -> String {
    format!("tailscale serve --bg --https=443 http://127.0.0.1:{port}")
}

impl Drop for TailnetHost {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Server {
    store: DirStore,
    key_digest: [u8; 32],
    allowed: Vec<String>,
}

struct Incoming {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Incoming {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then_some(value.as_str()))
    }
}

impl Server {
    fn handle(&self, mut stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(20)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(20)));
        let (status, kind, body) = match read_request(&mut stream) {
            Ok(request) => self.route(&request),
            Err(status) => (status, "text/plain", b"bad request".to_vec()),
        };
        let reason = match status {
            200 => "OK",
            201 => "Created",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            411 => "Length Required",
            413 => "Payload Too Large",
            422 => "Unprocessable Entity",
            _ => "Server Error",
        };
        let head = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: {kind}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(&body);
        let _ = stream.flush();
    }

    fn authorized(&self, request: &Incoming) -> Result<(), u16> {
        let presented = request
            .header("authorization")
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or(401u16)?;
        let digest: [u8; 32] = Sha256::digest(presented.trim().as_bytes()).into();
        let mut diff = 0u8;
        for (left, right) in digest.iter().zip(self.key_digest.iter()) {
            diff |= left ^ right;
        }
        if diff != 0 {
            return Err(401);
        }
        if !self.allowed.is_empty() {
            let login = request
                .header(LOGIN_HEADER)
                .map(|value| value.trim().to_ascii_lowercase())
                .ok_or(403u16)?;
            if !self.allowed.contains(&login) {
                return Err(403);
            }
        }
        Ok(())
    }

    fn route(&self, request: &Incoming) -> (u16, &'static str, Vec<u8>) {
        if let Err(status) = self.authorized(request) {
            return (status, "text/plain", b"not authorized".to_vec());
        }
        let path = request.path.split('?').next().unwrap_or("");
        let Some(rest) = path.strip_prefix(TAILNET_API_PREFIX) else {
            return (404, "text/plain", b"not found".to_vec());
        };
        let result = match (request.method.as_str(), rest) {
            ("GET", "state") => self
                .state_json()
                .map(|body| (200, "application/json", body)),
            ("GET", blob) if blob.starts_with("blobs/") => self
                .store
                .blob(&blob["blobs/".len()..])
                .map(|body| (200, "application/octet-stream", body)),
            ("PUT", blob) if blob.starts_with("blobs/") => self
                .store
                .put_blob(&blob["blobs/".len()..], &request.body)
                .map(|()| (201, "application/json", b"{}".to_vec())),
            ("PUT", commit) if commit.starts_with("commits/") => self
                .store
                .put_commit(&commit["commits/".len()..], &request.body)
                .map(|()| (201, "application/json", b"{}".to_vec())),
            _ => return (405, "text/plain", b"method not allowed".to_vec()),
        };
        match result {
            Ok(reply) => reply,
            Err(error) => {
                let status = match &error {
                    SyncError::NotFound { .. } => 404,
                    SyncError::ConflictRemote { .. } => 409,
                    SyncError::Limit { .. } => 413,
                    SyncError::Protocol { .. } | SyncError::Path { .. } => 422,
                    _ => 500,
                };
                let body = serde_json::json!({ "message": error.to_string() })
                    .to_string()
                    .into_bytes();
                (status, "application/json", body)
            }
        }
    }

    fn state_json(&self) -> Result<Vec<u8>, SyncError> {
        let commits_dir = self.store.commits_dir();
        let state = self.store.state()?;
        let mut commits = Vec::new();
        for id in state.commits.keys() {
            let bytes =
                std::fs::read(commits_dir.join(format!("{id}.json"))).map_err(SyncError::io)?;
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| SyncError::Protocol {
                    detail: "stored commit is not json".to_owned(),
                })?;
            commits.push(value);
        }
        Ok(serde_json::json!({
            "schema": SCHEMA_VERSION,
            "kind": "muninn-sync-store",
            "complete": true,
            "commits": commits,
            "blobs": state.blobs.iter().collect::<Vec<_>>(),
        })
        .to_string()
        .into_bytes())
    }
}

fn read_request(stream: &mut TcpStream) -> Result<Incoming, u16> {
    let mut buf = Vec::new();
    let mut temp = [0u8; 8192];
    let header_end = loop {
        let read = stream.read(&mut temp).map_err(|_| 400u16)?;
        if read == 0 {
            return Err(400);
        }
        buf.extend_from_slice(&temp[..read]);
        if let Some(pos) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Err(413);
        }
    };
    let header_text = std::str::from_utf8(&buf[..header_end]).map_err(|_| 400u16)?;
    let mut lines = header_text.split("\r\n");
    let mut parts = lines.next().ok_or(400u16)?.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let path = parts.next().unwrap_or("").to_owned();
    let mut headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }
    if headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("transfer-encoding"))
    {
        return Err(411);
    }
    let length = match headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
    {
        Some((_, value)) => value.parse::<usize>().map_err(|_| 400u16)?,
        None => 0,
    };
    let max_body = (DEFAULT_LIMITS.max_file_bytes as usize).max(MAX_COMMIT_BYTES);
    if length > max_body {
        return Err(413);
    }
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < length {
        let read = stream.read(&mut temp).map_err(|_| 400u16)?;
        if read == 0 {
            return Err(400);
        }
        body.extend_from_slice(&temp[..read]);
    }
    body.truncate(length);
    Ok(Incoming {
        method,
        path,
        headers,
        body,
    })
}

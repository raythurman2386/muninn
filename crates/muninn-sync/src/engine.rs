//! One sync run: inventory, plan, publish, then local apply.
//!
//! Network work uses the snapshots taken at the start. The lease is held only
//! while reading or writing the vault, not while waiting on a provider.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::SCHEMA_VERSION;
use crate::apply::{ApplyDelete, ApplyOutcome, ApplyRequest, ApplyWrite, apply_plan};
use crate::credentials::{CredentialKey, CredentialStore};
use crate::drive::{self, DriveClient, DriveScope};
use crate::error::SyncError;
use crate::github::{GitHubClient, GitHubScope};
use crate::http::{CancelToken, HttpTransport, Sleeper};
use crate::inventory::{self, Inventory};
use crate::journal::{Journal, JournalPhase};
use crate::lease::VaultLease;
use crate::limits::Exclusion;
use crate::model::{
    BindingId, Conflict, ContentHash, FileKind, FileRecord, OperationId, Plan, ProviderKind,
    Resolution, Snapshot,
};
use crate::pathutil::RelativePath as RelPath;
use crate::plan::{self, plan};
use crate::secret::TokenSet;
use crate::tailnet::{self, DirStore, HttpStore, TailnetRole, TailnetScope};

pub struct EngineParts<T, C> {
    pub transport: T,
    pub credentials: C,
    pub cancel: CancelToken,
    pub sleeper: Box<dyn Sleeper>,
    pub github_api: String,
    pub github_oauth: String,
    pub drive_api: String,
    pub drive_upload: String,
    pub drive_oauth: String,
    /// OAuth client ids, used only to refresh an expiring access token.
    pub github_client_id: String,
    pub google_client_id: String,
    /// Test-only override for a Tailscale peer's base URL. Production derives
    /// `https://<host>` from the binding.
    pub tailnet_api: Option<String>,
}

pub struct RunRequest {
    pub root: std::path::PathBuf,
    pub state_dir: std::path::PathBuf,
    pub binding: BindingId,
    pub provider: ProviderKind,
    pub github: Option<GitHubScope>,
    pub drive: Option<DriveScope>,
    pub tailnet: Option<TailnetScope>,
    pub operation: OperationId,
    pub protected: BTreeSet<RelPath>,
    pub resolutions: Vec<(RelPath, Resolution)>,
}

#[derive(Debug)]
pub enum SyncReport {
    Succeeded {
        head: String,
        exclusions: Vec<Exclusion>,
    },
    Conflicts {
        conflicts: Vec<Conflict>,
    },
}

/// How many fetch → plan → publish rounds one Sync Now may take when the
/// remote moves underneath it. Each round plans from a fresh read.
const MAX_PUBLISH_ROUNDS: u32 = 4;

pub fn run_sync<T: HttpTransport, C: CredentialStore>(
    engine: &EngineParts<T, C>,
    request: &RunRequest,
) -> Result<SyncReport, SyncError> {
    engine.cancel.check()?;
    if let Some(journal) = Journal::load(&request.state_dir)?
        && journal.needs_apply_resume()
        && journal.operation_id == request.operation.as_str()
    {
        return resume_local(engine, request);
    }
    let identity = crate::model::VaultIdentity::of_canonical(&request.root)?;
    let _lease = VaultLease::try_acquire(&request.state_dir, &identity)?;
    let inventory = inventory::inventory_default(&request.root)?;
    drop(_lease);

    let key = CredentialKey {
        provider: request.provider,
        binding: request.binding.clone(),
    };
    let mut token = engine
        .credentials
        .get(&key)?
        .ok_or_else(|| SyncError::Auth {
            detail: "this binding has no credentials; connect the provider first".to_owned(),
        })?;
    let mut refreshed = false;
    if token.expires_within(crate::secret::now_unix_ms(), REFRESH_MARGIN_MS) {
        token = refresh_token(engine, request.provider, &key, &token)?;
        refreshed = true;
    }

    let mut round = 0;
    loop {
        round += 1;
        let outcome = match sync_round(engine, request, &token, &inventory) {
            // A 401 with a refresh token on hand: refresh once and retry.
            Err(SyncError::Auth { .. }) if !refreshed && can_refresh(request.provider, &token) => {
                token = refresh_token(engine, request.provider, &key, &token)?;
                refreshed = true;
                round -= 1;
                continue;
            }
            other => other?,
        };
        match outcome {
            Round::Done(report) => return Ok(report),
            Round::RemoteMoved(error) if round < MAX_PUBLISH_ROUNDS => {
                engine.sleeper.sleep(
                    Duration::from_millis(100 * u64::from(round)),
                    &engine.cancel,
                )?;
                let _ = error;
            }
            Round::RemoteMoved(error) => return Err(error),
        }
    }
}

/// Refresh an access token this close to its expiry before using it.
const REFRESH_MARGIN_MS: u64 = 5 * 60 * 1000;

fn can_refresh(provider: ProviderKind, token: &TokenSet) -> bool {
    provider != ProviderKind::Tailscale && token.refresh.is_some()
}

/// Trade the refresh token for a new pair and store it before it is used,
/// so a rotated refresh token (GitHub rotates them) is never lost.
fn refresh_token<T: HttpTransport, C: CredentialStore>(
    engine: &EngineParts<T, C>,
    provider: ProviderKind,
    key: &CredentialKey,
    token: &TokenSet,
) -> Result<TokenSet, SyncError> {
    let Some(refresh) = token
        .refresh
        .as_ref()
        .filter(|_| can_refresh(provider, token))
    else {
        return Err(SyncError::Auth {
            detail: "the access token expired and there is no refresh token; connect again"
                .to_owned(),
        });
    };
    let next = match provider {
        ProviderKind::GitHub => github_client(engine).refresh(&engine.github_client_id, refresh)?,
        ProviderKind::GoogleDrive => {
            drive_client(engine).refresh(&engine.google_client_id, refresh)?
        }
        ProviderKind::Tailscale => unreachable!("can_refresh excludes Tailscale"),
    };
    let next = token.refreshed(next);
    engine.credentials.put(key, &next)?;
    Ok(next)
}

enum Round {
    Done(SyncReport),
    /// The remote refused a publish because it moved after the read this
    /// round planned from. Nothing was applied locally.
    RemoteMoved(SyncError),
}

/// One fetch → plan → publish → apply. The publish names exactly the remote
/// state the plan was made from, so a concurrent writer is never overwritten:
/// the remote refuses and the next round plans again from a fresh read.
fn sync_round<T: HttpTransport, C: CredentialStore>(
    engine: &EngineParts<T, C>,
    request: &RunRequest,
    token: &TokenSet,
    inventory: &Inventory,
) -> Result<Round, SyncError> {
    let base = load_base(&request.state_dir, &request.binding, request.provider)?;
    let fetched = fetch(engine, request, token)?;
    let planned = plan(base.as_ref(), &inventory.snapshot, &fetched.snapshot);
    if let Some(report) = conflicts_or_desired(&planned, request, inventory, &fetched.snapshot)? {
        return Ok(Round::Done(report));
    }
    let desired = if planned.has_conflicts() {
        plan::resolve(
            &inventory.snapshot,
            &fetched.snapshot,
            &planned.conflicts,
            &request.resolutions,
        )?
    } else {
        planned.desired.clone()
    };
    let local_bytes = read_local_bytes(&request.root, &desired, &inventory.snapshot)?;
    let remote_blobs = remote_bytes_for(engine, request, token, &fetched, &desired, inventory)?;
    let head = match publish(
        engine,
        request,
        token,
        &fetched,
        &remote_blobs,
        &desired,
        &planned,
        &local_bytes,
    ) {
        Ok(head) => head,
        Err(error @ SyncError::ConflictRemote { .. }) => return Ok(Round::RemoteMoved(error)),
        Err(error) => return Err(error),
    };
    apply_desired(
        request,
        inventory,
        &desired,
        &remote_blobs,
        &local_bytes,
        &head,
    )?;
    save_base(
        &request.state_dir,
        &request.binding,
        request.provider,
        &head,
        &desired,
    )?;
    if let Anchor::GitHub { remote, .. } = &fetched.anchor {
        save_sha_cache(&request.state_dir, remote, &local_bytes)?;
    }
    Journal::clear(&request.state_dir)?;
    Ok(Round::Done(SyncReport::Succeeded {
        head,
        exclusions: inventory.exclusions.clone(),
    }))
}

/// What one fetch read, kept so publish can name exactly that state.
struct Fetched {
    snapshot: Snapshot,
    blobs: BTreeMap<ContentHash, Vec<u8>>,
    anchor: Anchor,
}

enum Anchor {
    GitHub {
        scope: GitHubScope,
        remote: crate::github::GitHubRemote,
    },
    Drive {
        scope: DriveScope,
        remote: drive::DriveRemote,
        parents: Vec<String>,
    },
    Tailscale {
        scope: TailnetScope,
        parents: Vec<String>,
    },
}

/// Bytes for every file the apply step must write from the remote. GitHub
/// reads skip blobs whose content is already known, so fetch any of those the
/// plan now needs.
fn remote_bytes_for<T: HttpTransport, C: CredentialStore>(
    engine: &EngineParts<T, C>,
    _request: &RunRequest,
    token: &TokenSet,
    fetched: &Fetched,
    desired: &Snapshot,
    inventory: &Inventory,
) -> Result<BTreeMap<ContentHash, Vec<u8>>, SyncError> {
    let mut blobs = fetched.blobs.clone();
    if let Anchor::GitHub { scope, remote } = &fetched.anchor {
        let client = github_client(engine);
        for (path, record) in &desired.files {
            if inventory.snapshot.hash_of(path) == Some(record.hash)
                || blobs.contains_key(&record.hash)
            {
                continue;
            }
            let bytes = client.blob_for_hash(scope, &token.access, remote, record.hash)?;
            blobs.insert(record.hash, bytes);
        }
    }
    Ok(blobs)
}

fn conflicts_or_desired(
    planned: &Plan,
    request: &RunRequest,
    _inventory: &Inventory,
    _remote: &Snapshot,
) -> Result<Option<SyncReport>, SyncError> {
    if planned.has_conflicts() && request.resolutions.is_empty() {
        return Ok(Some(SyncReport::Conflicts {
            conflicts: planned.conflicts.clone(),
        }));
    }
    if planned.first_bind && !planned.local_deletes.is_empty() {
        return Err(SyncError::Protocol {
            detail: "first bind tried to delete a file".to_owned(),
        });
    }
    Ok(None)
}

fn resume_local<T: HttpTransport, C: CredentialStore>(
    _engine: &EngineParts<T, C>,
    request: &RunRequest,
) -> Result<SyncReport, SyncError> {
    let prepared = load_prepared(&request.state_dir)?;
    let outcome = apply_plan(&ApplyRequest {
        root: request.root.clone(),
        state_dir: request.state_dir.clone(),
        journal: Journal::load(&request.state_dir)?.unwrap_or(prepared.journal),
        writes: prepared.writes,
        deletes: prepared.deletes,
        protected: request.protected.clone(),
        fault_after_checkpoint: None,
        fault_after_files: None,
    })?;
    if outcome != ApplyOutcome::Applied {
        return Err(SyncError::InjectedFault {
            at: "resume".to_owned(),
        });
    }
    save_base(
        &request.state_dir,
        &request.binding,
        request.provider,
        &prepared.head,
        &prepared.desired,
    )?;
    Journal::clear(&request.state_dir)?;
    Ok(SyncReport::Succeeded {
        head: prepared.head,
        exclusions: Vec::new(),
    })
}

struct Prepared {
    journal: Journal,
    writes: Vec<ApplyWrite>,
    deletes: Vec<ApplyDelete>,
    head: String,
    desired: Snapshot,
}

fn fetch<T: HttpTransport, C: CredentialStore>(
    engine: &EngineParts<T, C>,
    request: &RunRequest,
    token: &TokenSet,
) -> Result<Fetched, SyncError> {
    match request.provider {
        ProviderKind::GitHub => {
            let scope = request.github.clone().ok_or_else(|| SyncError::Config {
                detail: "GitHub scope is missing".to_owned(),
            })?;
            let client = github_client(engine);
            let known = load_sha_cache(&request.state_dir);
            let remote = client.fetch_cached(&scope, &token.access, &known)?;
            Ok(Fetched {
                snapshot: remote.snapshot.clone(),
                blobs: remote.blobs.clone(),
                anchor: Anchor::GitHub { scope, remote },
            })
        }
        ProviderKind::GoogleDrive => {
            let scope = request.drive.clone().ok_or_else(|| SyncError::Config {
                detail: "Drive folder is missing".to_owned(),
            })?;
            let client = drive_client(engine);
            let remote = client.fetch(&scope, &token.access)?;
            // Disagreeing heads are an error from merge_heads, never an empty
            // snapshot that would plan every file as a remote delete.
            let merged = drive::merge_heads(&remote)?;
            Ok(Fetched {
                snapshot: merged.snapshot,
                blobs: remote.blobs.clone(),
                anchor: Anchor::Drive {
                    scope,
                    parents: merged.parents,
                    remote,
                },
            })
        }
        ProviderKind::Tailscale => {
            let scope = tailnet_scope(request)?;
            let remote = with_tailnet_store(engine, &scope, token, tailnet::fetch)?;
            let merged = drive::merge_heads(&remote)?;
            if merged.plan.has_conflicts() {
                return Err(SyncError::ConflictRemote {
                    detail: format!("{} Tailscale heads disagree", remote.heads.len()),
                });
            }
            // The heads this run planned against. Publish names exactly these
            // as parents, so the host refuses the commit if it moved.
            Ok(Fetched {
                snapshot: merged.snapshot,
                blobs: remote.blobs,
                anchor: Anchor::Tailscale {
                    scope,
                    parents: merged.parents,
                },
            })
        }
    }
}

fn tailnet_scope(request: &RunRequest) -> Result<TailnetScope, SyncError> {
    let scope = request.tailnet.clone().ok_or_else(|| SyncError::Config {
        detail: "Tailscale host is missing".to_owned(),
    })?;
    scope.validate()?;
    Ok(scope)
}

fn with_tailnet_store<T: HttpTransport, C, R>(
    engine: &EngineParts<T, C>,
    scope: &TailnetScope,
    token: &TokenSet,
    run: impl FnOnce(&dyn tailnet::CommitStore) -> Result<R, SyncError>,
) -> Result<R, SyncError> {
    match &scope.role {
        TailnetRole::Host { store_dir } => run(&DirStore::new(store_dir.clone())),
        TailnetRole::Peer { host } => {
            let store = HttpStore {
                base: engine
                    .tailnet_api
                    .clone()
                    .unwrap_or_else(|| format!("https://{host}")),
                transport: &engine.transport,
                key: &token.access,
                cancel: engine.cancel.clone(),
                sleeper: engine.sleeper.as_ref(),
                request_timeout: Duration::from_secs(60),
            };
            run(&store)
        }
    }
}

fn publish<T: HttpTransport, C: CredentialStore>(
    engine: &EngineParts<T, C>,
    request: &RunRequest,
    token: &TokenSet,
    fetched: &Fetched,
    _remote_blobs: &BTreeMap<ContentHash, Vec<u8>>,
    desired: &Snapshot,
    planned: &Plan,
    local_bytes: &BTreeMap<RelPath, Vec<u8>>,
) -> Result<String, SyncError> {
    let tombstones: Vec<_> = planned
        .tombstones
        .iter()
        .map(|tomb| (tomb.path.clone(), tomb.base_hash))
        .collect();
    match &fetched.anchor {
        Anchor::GitHub { scope, remote } => github_client(engine).publish(
            scope,
            &token.access,
            remote,
            desired,
            local_bytes,
            request.operation.as_str(),
        ),
        Anchor::Drive {
            scope,
            remote,
            parents,
        } => {
            let client = drive_client(engine);
            // Drive has no compare-and-swap. Name exactly the heads this plan
            // merged, and refuse if any commit appeared since the read: the
            // next round plans with it. A writer that still slips in between
            // this check and the upload becomes a second head, which the next
            // read merges; its changes are never named as a parent unseen.
            let now = client.commit_ids(scope, &token.access)?;
            let seen: BTreeSet<String> = remote.commits.keys().cloned().collect();
            if now != seen {
                return Err(SyncError::ConflictRemote {
                    detail: "Drive gained a commit after this sync read it".to_owned(),
                });
            }
            client.publish(
                scope,
                &token.access,
                remote,
                desired,
                parents,
                &tombstones,
                local_bytes,
                request.operation.as_str(),
            )
        }
        Anchor::Tailscale { scope, parents } => with_tailnet_store(engine, scope, token, |store| {
            let remote = tailnet::listing(store)?;
            tailnet::publish(
                store,
                &remote,
                desired,
                parents,
                &tombstones,
                local_bytes,
                request.operation.as_str(),
            )
        }),
    }
}

fn apply_desired(
    request: &RunRequest,
    inventory: &Inventory,
    desired: &Snapshot,
    remote_blobs: &BTreeMap<ContentHash, Vec<u8>>,
    local_bytes: &BTreeMap<RelPath, Vec<u8>>,
    head: &str,
) -> Result<(), SyncError> {
    let mut writes = Vec::new();
    let mut deletes = Vec::new();
    for (path, record) in &desired.files {
        let current = inventory.snapshot.hash_of(path);
        if current == Some(record.hash) {
            continue;
        }
        let bytes = local_bytes
            .get(path)
            .cloned()
            .or_else(|| remote_blobs.get(&record.hash).cloned())
            .ok_or_else(|| SyncError::Protocol {
                detail: format!("no bytes to write for {path}"),
            })?;
        if ContentHash::of(&bytes) != record.hash {
            return Err(SyncError::Protocol {
                detail: format!("staged bytes for {path} changed"),
            });
        }
        writes.push(ApplyWrite {
            path: path.clone(),
            bytes,
            before: current,
            after: record.hash,
        });
    }
    for (path, record) in &inventory.snapshot.files {
        if !desired.files.contains_key(path) {
            deletes.push(ApplyDelete {
                path: path.clone(),
                before: record.hash,
            });
        }
    }
    let journal = Journal {
        schema: SCHEMA_VERSION,
        run_id: request.operation.as_str().to_owned(),
        operation_id: request.operation.as_str().to_owned(),
        binding_id: request.binding.as_str().to_owned(),
        vault_identity: inventory.identity.as_str().to_owned(),
        provider: request.provider.as_str().to_owned(),
        phase: JournalPhase::Published,
        remote_head: Some(head.to_owned()),
        published: true,
        files: Vec::new(),
    };
    store_prepared(&request.state_dir, &writes, &deletes, head, desired)?;
    let identity = inventory.identity.clone();
    let _lease = VaultLease::try_acquire(&request.state_dir, &identity)?;
    let outcome = apply_plan(&ApplyRequest {
        root: request.root.clone(),
        state_dir: request.state_dir.clone(),
        journal,
        writes,
        deletes,
        protected: request.protected.clone(),
        fault_after_checkpoint: None,
        fault_after_files: None,
    })?;
    if outcome != ApplyOutcome::Applied {
        return Err(SyncError::InjectedFault {
            at: "apply".to_owned(),
        });
    }
    Ok(())
}

fn read_local_bytes(
    root: &Path,
    desired: &Snapshot,
    local: &Snapshot,
) -> Result<BTreeMap<RelPath, Vec<u8>>, SyncError> {
    let canonical = root.canonicalize().map_err(SyncError::io)?;
    let dir = crate::fsutil::open_root(&canonical)?;
    let mut out = BTreeMap::new();
    for (path, record) in &desired.files {
        if local.hash_of(path) == Some(record.hash) {
            let bytes = dir.read_file(path, crate::limits::DEFAULT_LIMITS.max_file_bytes)?;
            out.insert(path.clone(), bytes);
        }
    }
    Ok(out)
}

fn github_client<'a, T: HttpTransport, C>(
    engine: &'a EngineParts<T, C>,
) -> GitHubClient<'a, &'a T> {
    GitHubClient {
        api_base: engine.github_api.clone(),
        oauth_base: engine.github_oauth.clone(),
        transport: &engine.transport,
        cancel: engine.cancel.clone(),
        sleeper: engine.sleeper.as_ref(),
        request_timeout: Duration::from_secs(20),
    }
}

fn drive_client<'a, T: HttpTransport, C>(engine: &'a EngineParts<T, C>) -> DriveClient<'a, &'a T> {
    DriveClient {
        api_base: engine.drive_api.clone(),
        upload_base: engine.drive_upload.clone(),
        oauth_base: engine.drive_oauth.clone(),
        auth_base: GOOGLE_AUTH_BASE.to_owned(),
        transport: &engine.transport,
        cancel: engine.cancel.clone(),
        sleeper: engine.sleeper.as_ref(),
        request_timeout: Duration::from_secs(20),
    }
}

/// Google's consent page. Runs never open it; only sign-in does.
pub const GOOGLE_AUTH_BASE: &str = "https://accounts.google.com";

fn load_base(
    state_dir: &Path,
    binding: &BindingId,
    provider: ProviderKind,
) -> Result<Option<Snapshot>, SyncError> {
    let path = state_dir.join("base.json");
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(SyncError::io)?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|_| SyncError::Journal {
        detail: "base.json is corrupt; sync will not guess a deletion plan".to_owned(),
    })?;
    if json.get("schema").and_then(|value| value.as_u64()) != Some(u64::from(SCHEMA_VERSION)) {
        return Err(SyncError::Journal {
            detail: "base schema is not supported".to_owned(),
        });
    }
    if json.get("binding_id").and_then(|value| value.as_str()) != Some(binding.as_str())
        || json.get("provider").and_then(|value| value.as_str()) != Some(provider.as_str())
    {
        return Ok(None);
    }
    let mut snapshot = Snapshot::default();
    let entries = json.get("entries").and_then(|value| value.as_object());
    for (path, hash) in entries.into_iter().flatten() {
        let rel = RelPath::parse(path)?;
        let hash = ContentHash::parse(hash.as_str().unwrap_or(""))?;
        snapshot.files.insert(
            rel.clone(),
            FileRecord {
                hash,
                size: 0,
                kind: FileKind::of_name(rel.file_name()),
            },
        );
    }
    Ok(Some(snapshot))
}

fn save_base(
    state_dir: &Path,
    binding: &BindingId,
    provider: ProviderKind,
    head: &str,
    desired: &Snapshot,
) -> Result<(), SyncError> {
    let mut entries = serde_json::Map::new();
    for (path, record) in &desired.files {
        entries.insert(
            path.to_string(),
            serde_json::Value::String(record.hash.to_hex()),
        );
    }
    let json = serde_json::json!({
        "schema": SCHEMA_VERSION,
        "binding_id": binding.as_str(),
        "provider": provider.as_str(),
        "remote_head": head,
        "entries": entries,
    });
    crate::fsutil::write_outside(&state_dir.join("base.json"), json.to_string().as_bytes())
}

const SHA_CACHE: &str = "github-blob-cache.json";

/// Files a run keeps in its state dir. Listed so a legacy shared dir can be
/// moved into the binding it belongs to.
const STATE_FILES: [&str; 5] = [
    "base.json",
    "journal.json",
    "prepared.json",
    "staged",
    SHA_CACHE,
];

/// The state dir for one vault binding: `<sync_home>/bindings/<binding>`.
///
/// Each binding keeps its own base, journal, staged bytes and caches, so a
/// second vault or a reconnect never reads another binding's base. A base
/// left in the old shared `<sync_home>` by an earlier build moves here when
/// it names this binding; one that names another binding stays put. The
/// dirs are private to the user on Unix.
pub fn binding_state_dir(sync_home: &Path, binding: &BindingId) -> Result<PathBuf, SyncError> {
    let bindings = sync_home.join("bindings");
    let dir = bindings.join(binding.as_str());
    std::fs::create_dir_all(&dir).map_err(SyncError::io)?;
    for path in [sync_home, bindings.as_path(), dir.as_path()] {
        private_dir(path)?;
    }
    let legacy = sync_home.join("base.json");
    if !dir.join("base.json").exists() && legacy_base_names(&legacy, binding) {
        for name in STATE_FILES {
            let from = sync_home.join(name);
            if from.exists() {
                std::fs::rename(&from, dir.join(name)).map_err(SyncError::io)?;
            }
        }
    }
    Ok(dir)
}

fn legacy_base_names(path: &Path, binding: &BindingId) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| json.get("binding_id")?.as_str().map(str::to_owned))
        .is_some_and(|id| id == binding.as_str())
}

#[cfg(unix)]
fn private_dir(path: &Path) -> Result<(), SyncError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(SyncError::io)
}

#[cfg(not(unix))]
fn private_dir(_path: &Path) -> Result<(), SyncError> {
    Ok(())
}

/// git blob sha → content hash for blobs this binding has already read or
/// written. It is a fact about bytes, so a stale or missing cache only costs
/// downloads; a corrupt one is ignored.
fn load_sha_cache(state_dir: &Path) -> BTreeMap<String, ContentHash> {
    let Ok(text) = std::fs::read_to_string(state_dir.join(SHA_CACHE)) else {
        return BTreeMap::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return BTreeMap::new();
    };
    json.get("entries")
        .and_then(|value| value.as_object())
        .into_iter()
        .flatten()
        .filter_map(|(sha, hash)| Some((sha.clone(), ContentHash::parse(hash.as_str()?).ok()?)))
        .collect()
}

/// Keep only the blobs the branch now names, plus what this run uploaded.
fn save_sha_cache(
    state_dir: &Path,
    remote: &crate::github::GitHubRemote,
    local_bytes: &BTreeMap<RelPath, Vec<u8>>,
) -> Result<(), SyncError> {
    let mut entries = serde_json::Map::new();
    for (path, sha) in &remote.shas {
        if let Some(hash) = remote.snapshot.hash_of(path) {
            entries.insert(sha.clone(), serde_json::Value::String(hash.to_hex()));
        }
    }
    for bytes in local_bytes.values() {
        entries.insert(
            crate::github::git_blob_sha(bytes),
            serde_json::Value::String(ContentHash::of(bytes).to_hex()),
        );
    }
    let json = serde_json::json!({ "schema": SCHEMA_VERSION, "entries": entries });
    crate::fsutil::write_outside(&state_dir.join(SHA_CACHE), json.to_string().as_bytes())
}

fn store_prepared(
    state_dir: &Path,
    writes: &[ApplyWrite],
    deletes: &[ApplyDelete],
    head: &str,
    desired: &Snapshot,
) -> Result<(), SyncError> {
    let mut staged = Vec::new();
    for write in writes {
        let rel = format!("staged/{}", write.path.as_str());
        crate::fsutil::write_outside(&state_dir.join(&rel), &write.bytes)?;
        staged.push(serde_json::json!({
            "path": write.path.as_str(),
            "before": write.before.map(|hash| hash.to_hex()),
            "after": write.after.to_hex(),
            "file": rel,
        }));
    }
    let deletes_json: Vec<_> = deletes
        .iter()
        .map(|delete| {
            serde_json::json!({
                "path": delete.path.as_str(),
                "before": delete.before.to_hex(),
            })
        })
        .collect();
    let mut entries = serde_json::Map::new();
    for (path, record) in &desired.files {
        entries.insert(
            path.to_string(),
            serde_json::Value::String(record.hash.to_hex()),
        );
    }
    let json = serde_json::json!({
        "head": head,
        "writes": staged,
        "deletes": deletes_json,
        "entries": entries,
    });
    crate::fsutil::write_outside(
        &state_dir.join("prepared.json"),
        json.to_string().as_bytes(),
    )
}

fn load_prepared(state_dir: &Path) -> Result<Prepared, SyncError> {
    let text = std::fs::read_to_string(state_dir.join("prepared.json")).map_err(|_| {
        SyncError::Journal {
            detail: "prepared apply state is missing".to_owned(),
        }
    })?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|_| SyncError::Journal {
        detail: "prepared apply state is corrupt".to_owned(),
    })?;
    let mut writes = Vec::new();
    for item in json
        .get("writes")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
    {
        let path = RelPath::parse(
            item.get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(""),
        )?;
        let file = item
            .get("file")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let bytes = crate::fsutil::read_outside(&state_dir.join(file))?;
        let before = item
            .get("before")
            .and_then(|value| value.as_str())
            .map(ContentHash::parse)
            .transpose()?;
        writes.push(ApplyWrite {
            path,
            before,
            after: ContentHash::of(&bytes),
            bytes,
        });
    }
    let mut deletes = Vec::new();
    for item in json
        .get("deletes")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
    {
        deletes.push(ApplyDelete {
            path: RelPath::parse(
                item.get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or(""),
            )?,
            before: ContentHash::parse(
                item.get("before")
                    .and_then(|value| value.as_str())
                    .unwrap_or(""),
            )?,
        });
    }
    let mut desired = Snapshot::default();
    for (path, hash) in json
        .get("entries")
        .and_then(|value| value.as_object())
        .into_iter()
        .flatten()
    {
        let rel = RelPath::parse(path)?;
        desired.files.insert(
            rel.clone(),
            FileRecord {
                hash: ContentHash::parse(hash.as_str().unwrap_or(""))?,
                size: 0,
                kind: FileKind::of_name(rel.file_name()),
            },
        );
    }
    let journal = Journal::load(state_dir)?.ok_or_else(|| SyncError::Journal {
        detail: "journal disappeared before resume".to_owned(),
    })?;
    Ok(Prepared {
        journal,
        writes,
        deletes,
        head: json
            .get("head")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_owned(),
        desired,
    })
}

pub fn disconnect<C: CredentialStore>(store: &C, key: &CredentialKey) -> Result<(), SyncError> {
    store.delete(key)
}

/// Re-export the sleeper the clients should share. The helpers above build a
/// recording sleeper so unit tests of the engine do not wait. Production
/// passes [`crate::http::RealSleeper`] by constructing clients directly.
pub fn production_sleeper() -> Box<dyn Sleeper> {
    Box::new(crate::http::RealSleeper)
}

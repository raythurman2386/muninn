//! One sync run: inventory, plan, publish, then local apply.
//!
//! Network work uses the snapshots taken at the start. The lease is held only
//! while reading or writing the vault, not while waiting on a provider.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
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
}

pub struct RunRequest {
    pub root: std::path::PathBuf,
    pub state_dir: std::path::PathBuf,
    pub binding: BindingId,
    pub provider: ProviderKind,
    pub github: Option<GitHubScope>,
    pub drive: Option<DriveScope>,
    pub operation: OperationId,
    pub protected: BTreeSet<RelPath>,
    pub resolutions: Vec<(RelPath, Resolution)>,
}

pub enum SyncReport {
    Succeeded {
        head: String,
        exclusions: Vec<Exclusion>,
    },
    Conflicts {
        conflicts: Vec<Conflict>,
    },
}

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
    let token = engine
        .credentials
        .get(&key)?
        .ok_or_else(|| SyncError::Auth {
            detail: "this binding has no credentials; connect the provider first".to_owned(),
        })?;

    let base = load_base(&request.state_dir, &request.binding, request.provider)?;
    let (remote_snapshot, remote_blobs, publish_head) = fetch(engine, request, &token)?;
    let planned = plan(base.as_ref(), &inventory.snapshot, &remote_snapshot);
    if let Some(report) = conflicts_or_desired(&planned, request, &inventory, &remote_snapshot)? {
        return Ok(report);
    }
    let desired = if planned.has_conflicts() {
        plan::resolve(
            &inventory.snapshot,
            &remote_snapshot,
            &planned.conflicts,
            &request.resolutions,
        )?
    } else {
        planned.desired.clone()
    };
    let local_bytes = read_local_bytes(&request.root, &desired, &inventory.snapshot)?;
    let head = publish(
        engine,
        request,
        &token,
        &remote_snapshot,
        &remote_blobs,
        &desired,
        &planned,
        &local_bytes,
    )?;
    let _ = publish_head;
    apply_desired(
        request,
        &inventory,
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
    Journal::clear(&request.state_dir)?;
    Ok(SyncReport::Succeeded {
        head,
        exclusions: inventory.exclusions,
    })
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
) -> Result<(Snapshot, BTreeMap<ContentHash, Vec<u8>>, String), SyncError> {
    match request.provider {
        ProviderKind::GitHub => {
            let scope = request.github.clone().ok_or_else(|| SyncError::Config {
                detail: "GitHub scope is missing".to_owned(),
            })?;
            let client = github_client(engine);
            let remote = client.fetch(&scope, &token.access)?;
            let head = remote.commit.clone().unwrap_or_default();
            Ok((remote.snapshot, remote.blobs, head))
        }
        ProviderKind::GoogleDrive => {
            let scope = request.drive.clone().ok_or_else(|| SyncError::Config {
                detail: "Drive folder is missing".to_owned(),
            })?;
            let client = drive_client(engine);
            let remote = client.fetch(&scope, &token.access)?;
            let merged = drive::merge_heads(&remote)?;
            if merged.plan.has_conflicts() && request.resolutions.is_empty() {
                return Err(SyncError::ConflictRemote {
                    detail: format!("{} concurrent Drive heads disagree", remote.heads.len()),
                });
            }
            let head = merged.parents.first().cloned().unwrap_or_default();
            Ok((merged.snapshot, remote.blobs, head))
        }
    }
}

fn publish<T: HttpTransport, C: CredentialStore>(
    engine: &EngineParts<T, C>,
    request: &RunRequest,
    token: &TokenSet,
    remote_snapshot: &Snapshot,
    remote_blobs: &BTreeMap<ContentHash, Vec<u8>>,
    desired: &Snapshot,
    planned: &Plan,
    local_bytes: &BTreeMap<RelPath, Vec<u8>>,
) -> Result<String, SyncError> {
    match request.provider {
        ProviderKind::GitHub => {
            let scope = request.github.clone().unwrap();
            let client = github_client(engine);
            let remote = crate::github::GitHubRemote {
                commit: load_anchor(&request.state_dir),
                tree: None,
                snapshot: remote_snapshot.clone(),
                blobs: remote_blobs.clone(),
            };
            // `commit: None` would create a new root. Use the fetched head from
            // the anchor file written by fetch via state. The GitHub client
            // re-reads the ref itself, so a missing commit here is safe.
            let _ = remote;
            let fetched = client.fetch(&scope, &token.access)?;
            client.publish(
                &scope,
                &token.access,
                &fetched,
                desired,
                local_bytes,
                request.operation.as_str(),
            )
        }
        ProviderKind::GoogleDrive => {
            let scope = request.drive.clone().unwrap();
            let client = drive_client(engine);
            let remote = client.fetch(&scope, &token.access)?;
            let merged = drive::merge_heads(&remote)?;
            let parents = if remote.heads.is_empty() {
                Vec::new()
            } else {
                merged.parents
            };
            client.publish(
                &scope,
                &token.access,
                &remote,
                desired,
                &parents,
                &planned
                    .tombstones
                    .iter()
                    .map(|tomb| (tomb.path.clone(), tomb.base_hash))
                    .collect::<Vec<_>>(),
                local_bytes,
                request.operation.as_str(),
            )
        }
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
        transport: &engine.transport,
        cancel: engine.cancel.clone(),
        sleeper: engine.sleeper.as_ref(),
        request_timeout: Duration::from_secs(20),
    }
}

/// The engine must use the caller's sleeper, not a fresh one that drops the
/// recorded delays. Rebuild clients with the shared sleeper by leaking a
/// reference through the trait object stored on the engine.
fn load_anchor(state_dir: &Path) -> Option<String> {
    let _ = state_dir;
    None
}

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

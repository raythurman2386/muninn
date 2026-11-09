//! Google Drive transport.
//!
//! Drive v3 `files.update` is not a compare-and-swap. This adapter never
//! overwrites a file. It writes immutable blobs and immutable commit manifests
//! into the app's hidden `appDataFolder`, one store folder per store name.
//! Two writers produce two heads. A later sync merges those heads, or stops on
//! a conflict. This is not a mirror of an editable Markdown folder.
//!
//! Why `appDataFolder` with `drive.appdata`: the store is app-private data,
//! not files a person browses, so no Picker or folder grant is needed, every
//! device signed in to the same Google account and OAuth client sees the
//! same store, and the scope reaches nothing else in the user's Drive. The
//! cost: the store is invisible in the Drive UI and counts against the user's
//! quota; removing the app's access in Google Account settings deletes it.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::SCHEMA_VERSION;
use crate::error::SyncError;
use crate::http::{
    CancelToken, HttpRequest, HttpTransport, Sleeper, classify_status, ensure_allowed, with_retry,
};
use crate::limits::DEFAULT_LIMITS;
use crate::model::{ContentHash, FileKind, FileRecord, Plan, Snapshot};
use crate::pathutil::RelativePath;
use crate::plan;
use crate::secret::{Secret, TokenSet};

pub const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";
/// The alias Drive v3 uses for the app's hidden data folder.
pub const APP_DATA: &str = "appDataFolder";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const SHORTCUT_MIME: &str = "application/vnd.google-apps.shortcut";

/// Which store in the app's data folder a vault syncs with. Devices that
/// should share a vault use the same store name; another vault on the same
/// Google account uses another name.
#[derive(Clone, Debug)]
pub struct DriveScope {
    pub store: String,
}

impl DriveScope {
    pub fn validate(&self) -> Result<(), SyncError> {
        if self.store.is_empty()
            || self.store.len() > 64
            || !self
                .store
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ' ' | '.'))
            || self.store.trim() != self.store
        {
            return Err(SyncError::Config {
                detail: "Drive store name must be 1-64 letters, digits, spaces, '.', '-' or '_'"
                    .to_owned(),
            });
        }
        Ok(())
    }

    /// Name of this store's root folder inside `appDataFolder`.
    #[must_use]
    pub fn root_name(&self) -> String {
        format!("muninn-sync-{}", self.store)
    }
}

/// The folders holding one store. Two devices binding at the same moment
/// can each create a folder; every listing reads all of them, and writers
/// pick the lowest id, so the store never splits.
struct StoreFolders {
    blobs: Vec<String>,
    commits: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct DriveEntryDoc {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub blob_id: String,
}

#[derive(Clone, Debug)]
pub struct DriveTombstoneDoc {
    pub path: String,
    pub base_sha256: String,
}

#[derive(Clone, Debug)]
pub struct DriveCommitDoc {
    pub id: String,
    pub parents: Vec<String>,
    pub operation_id: String,
    pub entries: Vec<DriveEntryDoc>,
    pub tombstones: Vec<DriveTombstoneDoc>,
}

impl DriveCommitDoc {
    pub fn snapshot(&self) -> Result<Snapshot, SyncError> {
        let mut snapshot = Snapshot::default();
        for entry in &self.entries {
            let path = RelativePath::parse(&entry.path)?;
            let hash = ContentHash::parse(&entry.sha256)?;
            if snapshot
                .files
                .insert(
                    path.clone(),
                    FileRecord {
                        hash,
                        size: entry.size,
                        kind: FileKind::of_name(path.file_name()),
                    },
                )
                .is_some()
            {
                return Err(SyncError::Protocol {
                    detail: format!("drive commit {} lists {} twice", self.id, path),
                });
            }
        }
        for tomb in &self.tombstones {
            let path = RelativePath::parse(&tomb.path)?;
            if snapshot.files.contains_key(&path) {
                return Err(SyncError::Protocol {
                    detail: format!(
                        "drive commit {} tombstones a path it still contains",
                        self.id
                    ),
                });
            }
        }
        Ok(snapshot)
    }
}

#[derive(Clone, Debug)]
pub struct DriveRemote {
    pub commits: BTreeMap<String, DriveCommitDoc>,
    pub heads: Vec<String>,
    pub blobs: BTreeMap<ContentHash, Vec<u8>>,
    pub blob_ids: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct DriveMerge {
    pub snapshot: Snapshot,
    pub parents: Vec<String>,
    pub plan: Plan,
}

#[derive(Clone, Debug)]
pub struct PkceRequest {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
    pub redirect_uri: String,
}

pub struct DriveClient<'a, T> {
    pub api_base: String,
    pub upload_base: String,
    /// Token endpoint host: `https://oauth2.googleapis.com`.
    pub oauth_base: String,
    /// Consent page host: `https://accounts.google.com`.
    pub auth_base: String,
    pub transport: T,
    pub cancel: CancelToken,
    pub sleeper: &'a dyn Sleeper,
    pub request_timeout: Duration,
}

impl<'a, T: HttpTransport> DriveClient<'a, T> {
    pub fn fetch(&self, scope: &DriveScope, token: &Secret) -> Result<DriveRemote, SyncError> {
        scope.validate()?;
        let Some(folders) = self.store_folders(scope, token)? else {
            return Ok(DriveRemote {
                commits: BTreeMap::new(),
                heads: Vec::new(),
                blobs: BTreeMap::new(),
                blob_ids: BTreeMap::new(),
            });
        };
        // Commits first: a writer uploads blobs before the manifest naming
        // them, so every commit seen here has its blobs in the later listing.
        let mut commit_files = Vec::new();
        for folder in &folders.commits {
            commit_files.extend(self.list_children(folder, token)?);
        }
        let mut blob_files = Vec::new();
        for folder in &folders.blobs {
            blob_files.extend(self.list_children(folder, token)?);
        }
        let mut blob_ids = BTreeMap::new();
        for file in &blob_files {
            if file.mime == SHORTCUT_MIME || file.mime.starts_with("application/vnd.google-apps.") {
                return Err(SyncError::Unsupported {
                    path: file.name.clone(),
                    detail: "Drive shortcut or Workspace file inside the sync folder".to_owned(),
                });
            }
            blob_ids.insert(file.name.clone(), file.id.clone());
        }
        let mut commits = BTreeMap::new();
        for file in &commit_files {
            if file.mime == SHORTCUT_MIME {
                return Err(SyncError::Unsupported {
                    path: file.name.clone(),
                    detail: "Drive shortcut inside the commit folder".to_owned(),
                });
            }
            let bytes = self.download(&file.id, token)?;
            let doc = parse_commit(&bytes)?;
            if format!("{}.json", doc.id) != file.name && doc.id != file.name {
                return Err(SyncError::Protocol {
                    detail: format!("commit file {} does not match id {}", file.name, doc.id),
                });
            }
            commits.insert(doc.id.clone(), doc);
        }
        for commit in commits.values() {
            for parent in &commit.parents {
                if !commits.contains_key(parent) {
                    return Err(SyncError::ListingIncomplete {
                        detail: format!(
                            "commit {} names parent {parent}, which was not in the folder listing",
                            commit.id
                        ),
                    });
                }
            }
            for entry in &commit.entries {
                if !blob_ids.contains_key(&entry.sha256) {
                    return Err(SyncError::Protocol {
                        detail: format!(
                            "commit {} needs blob {}, which is not in the folder",
                            commit.id, entry.sha256
                        ),
                    });
                }
            }
        }
        let heads = heads_of(&commits);
        let mut blobs = BTreeMap::new();
        let mut needed = BTreeSet::new();
        for head in &heads {
            if let Some(commit) = commits.get(head) {
                for entry in &commit.entries {
                    needed.insert(entry.sha256.clone());
                }
            }
        }
        for sha in needed {
            let id = &blob_ids[&sha];
            let bytes = self.download(id, token)?;
            let actual = ContentHash::of(&bytes).to_hex();
            if actual != sha {
                return Err(SyncError::Protocol {
                    detail: format!("drive blob {sha} does not match its bytes"),
                });
            }
            blobs.insert(ContentHash::parse(&sha)?, bytes);
        }
        Ok(DriveRemote {
            commits,
            heads,
            blobs,
            blob_ids,
        })
    }

    /// Ids of every commit manifest in the store, without downloading any.
    pub fn commit_ids(
        &self,
        scope: &DriveScope,
        token: &Secret,
    ) -> Result<BTreeSet<String>, SyncError> {
        scope.validate()?;
        let Some(folders) = self.store_folders(scope, token)? else {
            return Ok(BTreeSet::new());
        };
        let mut ids = BTreeSet::new();
        for folder in &folders.commits {
            for file in self.list_children(folder, token)? {
                ids.insert(
                    file.name
                        .strip_suffix(".json")
                        .unwrap_or(&file.name)
                        .to_owned(),
                );
            }
        }
        Ok(ids)
    }

    /// Every root, blobs and commits folder of this store, or `None` before
    /// the first publish.
    fn store_folders(
        &self,
        scope: &DriveScope,
        token: &Secret,
    ) -> Result<Option<StoreFolders>, SyncError> {
        let roots = self.roots(scope, token)?;
        if roots.is_empty() {
            return Ok(None);
        }
        let mut folders = StoreFolders {
            blobs: Vec::new(),
            commits: Vec::new(),
        };
        for root in &roots {
            let children = self.list_children(root, token)?;
            folders.blobs.extend(role_folders(&children, "blobs"));
            folders.commits.extend(role_folders(&children, "commits"));
        }
        if folders.blobs.is_empty() || folders.commits.is_empty() {
            return Err(SyncError::ListingIncomplete {
                detail: "the Drive store is missing its blobs or commits folder".to_owned(),
            });
        }
        Ok(Some(folders))
    }

    fn roots(&self, scope: &DriveScope, token: &Secret) -> Result<Vec<String>, SyncError> {
        let name = scope.root_name();
        let mut roots: Vec<String> = self
            .list_children(APP_DATA, token)?
            .into_iter()
            .filter(|file| {
                file.role.as_deref() == Some("root")
                    && file.mime == FOLDER_MIME
                    && file.name == name
            })
            .map(|file| file.id)
            .collect();
        roots.sort();
        Ok(roots)
    }

    pub fn publish(
        &self,
        scope: &DriveScope,
        token: &Secret,
        remote: &DriveRemote,
        desired: &Snapshot,
        parents: &[String],
        tombstones: &[(RelativePath, ContentHash)],
        local_bytes: &BTreeMap<RelativePath, Vec<u8>>,
        operation_id: &str,
    ) -> Result<String, SyncError> {
        scope.validate()?;
        if remote
            .commits
            .values()
            .any(|commit| commit.operation_id == operation_id)
        {
            let existing = remote
                .commits
                .values()
                .find(|commit| commit.operation_id == operation_id)
                .expect("just found");
            return Ok(existing.id.clone());
        }
        let commit_id = self.generate_id(token)?;
        let root_children = self.list_children(APP_DATA, token)?;
        let root_name = scope.root_name();
        let named: Vec<DriveFile> = root_children
            .into_iter()
            .filter(|file| file.name == root_name)
            .collect();
        let sync_root = self.ensure_folder(scope, token, APP_DATA, &named, "root", &root_name)?;
        let inner = self.list_children(&sync_root, token)?;
        let blobs_folder =
            self.ensure_folder(scope, token, &sync_root, &inner, "blobs", "blobs")?;
        let commits_folder =
            self.ensure_folder(scope, token, &sync_root, &inner, "commits", "commits")?;
        let mut blob_ids = remote.blob_ids.clone();
        for (path, record) in &desired.files {
            let hex_hash = record.hash.to_hex();
            if blob_ids.contains_key(&hex_hash) {
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
            let id = self.generate_id(token)?;
            self.upload_bytes(
                token,
                &id,
                &hex_hash,
                &blobs_folder,
                "application/octet-stream",
                &[("muninnRole", "blob"), ("muninnSchema", "1")],
                bytes,
            )?;
            blob_ids.insert(hex_hash, id);
        }
        let mut entries = Vec::new();
        for (path, record) in &desired.files {
            let hex_hash = record.hash.to_hex();
            entries.push(serde_json::json!({
                "path": path.as_str(),
                "sha256": hex_hash,
                "size": record.size,
                "blob_id": blob_ids.get(&hex_hash).cloned().unwrap_or_default(),
            }));
        }
        let tombstones_json: Vec<_> = tombstones
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
            "tombstones": tombstones_json,
        });
        let body = manifest.to_string().into_bytes();
        self.upload_bytes(
            token,
            &commit_id,
            &format!("{commit_id}.json"),
            &commits_folder,
            "application/json",
            &[("muninnRole", "commit"), ("muninnSchema", "1")],
            &body,
        )?;
        Ok(commit_id.to_owned())
    }

    pub fn authorization_url(
        &self,
        client_id: &str,
        pkce: &PkceRequest,
    ) -> Result<String, SyncError> {
        require_client_id(client_id)?;
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", &pkce.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", DRIVE_SCOPE)
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &pkce.state)
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .finish();
        Ok(format!(
            "{}/o/oauth2/v2/auth?{query}",
            self.auth_base.trim_end_matches('/')
        ))
    }

    pub fn exchange_code(
        &self,
        client_id: &str,
        redirect_uri: &str,
        code: &str,
        verifier: &str,
    ) -> Result<TokenSet, SyncError> {
        require_client_id(client_id)?;
        let body = form(&[
            ("client_id", client_id),
            ("code", code),
            ("code_verifier", verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
        ]);
        let response = self.transport.execute(&HttpRequest {
            method: "POST".to_owned(),
            url: format!("{}/token", self.oauth_base.trim_end_matches('/')),
            headers: vec![
                (
                    "Content-Type".to_owned(),
                    "application/x-www-form-urlencoded".to_owned(),
                ),
                ("Accept".to_owned(), "application/json".to_owned()),
            ],
            body: body.into_bytes(),
            mutation: false,
            timeout: self.request_timeout,
        })?;
        if response.status != 200 {
            let detail = crate::error::provider_message(response.text().unwrap_or(""));
            if response.status == 400 || response.status == 401 {
                return Err(SyncError::Auth { detail });
            }
            classify_status(&response, false)?;
            return Err(SyncError::Auth { detail });
        }
        token_from_oauth(&response.json()?)
    }

    /// Trade the refresh token for a new access token. Google usually omits
    /// a new refresh token; the caller keeps the old one (see
    /// [`TokenSet::refreshed`]). A revoked or expired grant is `Auth`.
    pub fn refresh(&self, client_id: &str, refresh: &Secret) -> Result<TokenSet, SyncError> {
        require_client_id(client_id)?;
        let body = form(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.expose()),
        ]);
        let response = self.transport.execute(&HttpRequest {
            method: "POST".to_owned(),
            url: format!("{}/token", self.oauth_base.trim_end_matches('/')),
            headers: vec![
                (
                    "Content-Type".to_owned(),
                    "application/x-www-form-urlencoded".to_owned(),
                ),
                ("Accept".to_owned(), "application/json".to_owned()),
            ],
            body: body.into_bytes(),
            mutation: false,
            timeout: self.request_timeout,
        })?;
        if response.status == 400 || response.status == 401 {
            let detail = crate::error::provider_message(response.text().unwrap_or(""));
            return Err(SyncError::Auth {
                detail: format!("Google refused the refresh token ({detail}); connect again"),
            });
        }
        classify_status(&response, false)?;
        token_from_oauth(&response.json()?)
    }

    fn list_children(&self, parent: &str, token: &Secret) -> Result<Vec<DriveFile>, SyncError> {
        let mut page_token = String::new();
        let mut seen_tokens = BTreeSet::new();
        let mut files = Vec::new();
        loop {
            self.cancel.check()?;
            let query = format!("'{parent}' in parents and trashed = false");
            let mut serializer = form_urlencoded::Serializer::new(String::new());
            serializer.append_pair("q", &query);
            serializer.append_pair("pageSize", "100");
            serializer.append_pair(
                "fields",
                "nextPageToken,incompleteSearch,files(id,name,mimeType,size,appProperties,shortcutDetails)",
            );
            serializer.append_pair("spaces", APP_DATA);
            if !page_token.is_empty() {
                serializer.append_pair("pageToken", &page_token);
            }
            let response = self.send(self.api(
                "GET",
                &format!("/drive/v3/files?{}", serializer.finish()),
                token,
                None,
                false,
            ))?;
            let json = response.json()?;
            if json
                .get("incompleteSearch")
                .and_then(|value| value.as_bool())
                == Some(true)
            {
                return Err(SyncError::ListingIncomplete {
                    detail: "Drive reported an incomplete search; refusing to delete from a partial listing".to_owned(),
                });
            }
            for file in json
                .get("files")
                .and_then(|value| value.as_array())
                .into_iter()
                .flatten()
            {
                files.push(DriveFile {
                    id: json_str(file, "id")?,
                    name: json_str(file, "name")?,
                    mime: json_str(file, "mimeType")?,
                    role: file
                        .pointer("/appProperties/muninnRole")
                        .and_then(|value| value.as_str())
                        .map(str::to_owned),
                });
            }
            let next = json
                .get("nextPageToken")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned();
            if next.is_empty() {
                break;
            }
            if !seen_tokens.insert(next.clone()) {
                return Err(SyncError::ListingIncomplete {
                    detail: "Drive repeated a page token".to_owned(),
                });
            }
            page_token = next;
            if seen_tokens.len() > 200 {
                return Err(SyncError::Limit {
                    detail: "Drive listing exceeded 200 pages".to_owned(),
                });
            }
        }
        Ok(files)
    }

    fn ensure_folder(
        &self,
        _scope: &DriveScope,
        token: &Secret,
        parent: &str,
        children: &[DriveFile],
        role: &str,
        name: &str,
    ) -> Result<String, SyncError> {
        // Two devices may each have created one; readers union them all and
        // writers agree on the lowest id.
        if let Some(existing) = role_folders(children, role).into_iter().min() {
            return Ok(existing);
        }
        let id = self.generate_id(token)?;
        let body = serde_json::json!({
            "id": id,
            "name": name,
            "mimeType": FOLDER_MIME,
            "parents": [parent],
            "appProperties": { "muninnRole": role, "muninnSchema": "1" },
        });
        let response = self.send_create(self.api(
            "POST",
            "/drive/v3/files?fields=id",
            token,
            Some(body.to_string().into_bytes()),
            true,
        ))?;
        if response.status == 409 {
            return Ok(id);
        }
        let created = json_str(&response.json()?, "id")?;
        if created != id {
            return Err(SyncError::Protocol {
                detail: "Drive created a folder with a different id".to_owned(),
            });
        }
        Ok(id)
    }

    fn generate_id(&self, token: &Secret) -> Result<String, SyncError> {
        let response = self.send(self.api(
            "GET",
            "/drive/v3/files/generateIds?count=1&space=appDataFolder",
            token,
            None,
            false,
        ))?;
        let json = response.json()?;
        json.get("ids")
            .and_then(|value| value.as_array())
            .and_then(|ids| ids.first())
            .and_then(|id| id.as_str())
            .map(str::to_owned)
            .ok_or_else(|| SyncError::Protocol {
                detail: "Drive generateIds returned no id".to_owned(),
            })
    }

    fn upload_bytes(
        &self,
        token: &Secret,
        id: &str,
        name: &str,
        parent: &str,
        mime: &str,
        props: &[(&str, &str)],
        bytes: &[u8],
    ) -> Result<(), SyncError> {
        let mut app_properties = serde_json::Map::new();
        for (key, value) in props {
            app_properties.insert(
                (*key).to_owned(),
                serde_json::Value::String((*value).to_owned()),
            );
        }
        let metadata = serde_json::json!({
            "id": id,
            "name": name,
            "parents": [parent],
            "mimeType": mime,
            "appProperties": app_properties,
        });
        let start = HttpRequest {
            method: "POST".to_owned(),
            url: format!(
                "{}/drive/v3/files?uploadType=resumable&fields=id",
                self.upload_base.trim_end_matches('/')
            ),
            headers: vec![
                (
                    "Authorization".to_owned(),
                    format!("Bearer {}", token.expose()),
                ),
                (
                    "Content-Type".to_owned(),
                    "application/json; charset=UTF-8".to_owned(),
                ),
                ("X-Upload-Content-Type".to_owned(), mime.to_owned()),
                (
                    "X-Upload-Content-Length".to_owned(),
                    bytes.len().to_string(),
                ),
            ],
            body: metadata.to_string().into_bytes(),
            mutation: true,
            timeout: self.request_timeout,
        };
        let started = match self.transport.execute(&start) {
            Ok(response) if response.status == 409 => {
                return self.confirm_existing(token, id, bytes);
            }
            Ok(response) => {
                classify_status(&response, true)?;
                response
            }
            Err(error) if error.outcome_unknown() => {
                return self.confirm_existing(token, id, bytes);
            }
            Err(error) => return Err(error),
        };
        let location = started
            .header("location")
            .ok_or_else(|| SyncError::Protocol {
                detail: "resumable upload did not return a Location".to_owned(),
            })?
            .to_owned();
        let extras =
            if self.upload_base.contains("127.0.0.1") || self.upload_base.contains("localhost") {
                vec!["127.0.0.1".to_owned(), "localhost".to_owned()]
            } else {
                Vec::new()
            };
        ensure_allowed(&location, &extras)?;
        let put = HttpRequest {
            method: "PUT".to_owned(),
            url: location,
            headers: vec![
                ("Content-Type".to_owned(), mime.to_owned()),
                (
                    "Content-Range".to_owned(),
                    if bytes.is_empty() {
                        "bytes */0".to_owned()
                    } else {
                        format!("bytes 0-{}/{}", bytes.len() - 1, bytes.len())
                    },
                ),
            ],
            body: bytes.to_vec(),
            mutation: true,
            timeout: self.request_timeout,
        };
        match self.transport.execute(&put) {
            Ok(response) if response.status == 409 => self.confirm_existing(token, id, bytes),
            Ok(response) => {
                classify_status(&response, true)?;
                Ok(())
            }
            Err(error) if error.outcome_unknown() => self.confirm_existing(token, id, bytes),
            Err(error) => Err(error),
        }
    }

    fn confirm_existing(&self, token: &Secret, id: &str, bytes: &[u8]) -> Result<(), SyncError> {
        let response = match self.transport.execute(&self.api(
            "GET",
            &format!("/drive/v3/files/{id}?alt=media",),
            token,
            None,
            false,
        )) {
            Ok(response) if response.status == 404 => {
                return Err(SyncError::OutcomeUnknown {
                    detail: format!("drive id {id} was not created"),
                });
            }
            Ok(response) => response,
            Err(error) => return Err(error),
        };
        classify_status(&response, false)?;
        if response.body != bytes {
            return Err(SyncError::Protocol {
                detail: format!("drive id {id} already exists with different bytes"),
            });
        }
        Ok(())
    }

    fn download(&self, id: &str, token: &Secret) -> Result<Vec<u8>, SyncError> {
        let response = self.send(self.api(
            "GET",
            &format!("/drive/v3/files/{id}?alt=media"),
            token,
            None,
            false,
        ))?;
        if response.body.len() as u64 > DEFAULT_LIMITS.max_file_bytes {
            return Err(SyncError::Limit {
                detail: format!("drive file {id} exceeds the per-file limit"),
            });
        }
        Ok(response.body)
    }

    fn api(
        &self,
        method: &str,
        path_and_query: &str,
        token: &Secret,
        body: Option<Vec<u8>>,
        mutation: bool,
    ) -> HttpRequest {
        HttpRequest {
            method: method.to_owned(),
            url: format!("{}{path_and_query}", self.api_base.trim_end_matches('/')),
            headers: vec![
                (
                    "Authorization".to_owned(),
                    format!("Bearer {}", token.expose()),
                ),
                ("Accept".to_owned(), "application/json".to_owned()),
                ("Content-Type".to_owned(), "application/json".to_owned()),
            ],
            body: body.unwrap_or_default(),
            mutation,
            timeout: self.request_timeout,
        }
    }

    fn send(&self, request: HttpRequest) -> Result<crate::http::HttpResponse, SyncError> {
        with_retry(&self.cancel, self.sleeper, || {
            let response = self.transport.execute(&request)?;
            classify_status(&response, request.mutation)?;
            Ok(response)
        })
    }

    fn send_create(&self, request: HttpRequest) -> Result<crate::http::HttpResponse, SyncError> {
        self.cancel.check()?;
        match self.transport.execute(&request) {
            Ok(response) if response.status == 409 => Ok(response),
            Ok(response) => {
                classify_status(&response, true)?;
                Ok(response)
            }
            Err(error) if error.outcome_unknown() => Err(error),
            Err(error) => Err(error),
        }
    }
}

#[derive(Clone, Debug)]
struct DriveFile {
    id: String,
    name: String,
    mime: String,
    role: Option<String>,
}

pub fn merge_heads(remote: &DriveRemote) -> Result<DriveMerge, SyncError> {
    if remote.heads.is_empty() {
        return Ok(DriveMerge {
            snapshot: Snapshot::default(),
            parents: Vec::new(),
            plan: plan::plan(None, &Snapshot::default(), &Snapshot::default()),
        });
    }
    if remote.heads.len() == 1 {
        let commit = remote
            .commits
            .get(&remote.heads[0])
            .ok_or_else(|| SyncError::Protocol {
                detail: "head commit is missing".to_owned(),
            })?;
        let snapshot = commit.snapshot()?;
        return Ok(DriveMerge {
            snapshot: snapshot.clone(),
            parents: vec![commit.id.clone()],
            plan: plan::plan(Some(&snapshot), &snapshot, &snapshot),
        });
    }
    let mut acc_id = remote.heads[0].clone();
    let mut acc = remote.commits[&acc_id].snapshot()?;
    let mut parents = vec![acc_id.clone()];
    for head in remote.heads.iter().skip(1) {
        let other = remote.commits[head].snapshot()?;
        let lca = lowest_common_ancestor(&remote.commits, &acc_id, head)?;
        let base = match &lca {
            Some(id) => Some(remote.commits[id].snapshot()?),
            None => None,
        };
        let planned = plan::plan(base.as_ref(), &acc, &other);
        if planned.has_conflicts() {
            // Never return a placeholder snapshot here: planning against an
            // empty remote would turn every base file into a delete.
            return Err(SyncError::ConflictRemote {
                detail: format!(
                    "{} concurrent remote heads edit the same {} path(s); resolve on one device and sync again",
                    remote.heads.len(),
                    planned.conflicts.len()
                ),
            });
        }
        acc = planned.desired.clone();
        parents.push(head.clone());
        acc_id = head.clone();
    }
    Ok(DriveMerge {
        snapshot: acc,
        parents,
        plan: plan::plan(None, &Snapshot::default(), &Snapshot::default()),
    })
}

fn lowest_common_ancestor(
    commits: &BTreeMap<String, DriveCommitDoc>,
    left: &str,
    right: &str,
) -> Result<Option<String>, SyncError> {
    let left_ancestors = ancestors(commits, left);
    let right_ancestors = ancestors(commits, right);
    let common: BTreeSet<_> = left_ancestors
        .intersection(&right_ancestors)
        .cloned()
        .collect();
    if common.is_empty() {
        return Ok(None);
    }
    let deepest: Vec<_> = common
        .iter()
        .filter(|id| {
            !common
                .iter()
                .any(|other| other != *id && ancestors(commits, other).contains(*id))
        })
        .cloned()
        .collect();
    if deepest.len() != 1 {
        return Err(SyncError::ConflictRemote {
            detail: "drive history has more than one common ancestor; resolve the heads explicitly"
                .to_owned(),
        });
    }
    Ok(deepest.into_iter().next())
}

fn ancestors(commits: &BTreeMap<String, DriveCommitDoc>, id: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![id.to_owned()];
    while let Some(current) = stack.pop() {
        if !out.insert(current.clone()) {
            continue;
        }
        if let Some(commit) = commits.get(&current) {
            stack.extend(commit.parents.iter().cloned());
        }
    }
    out
}

pub(crate) fn heads_of(commits: &BTreeMap<String, DriveCommitDoc>) -> Vec<String> {
    let mut referenced = BTreeSet::new();
    for commit in commits.values() {
        for parent in &commit.parents {
            referenced.insert(parent.clone());
        }
    }
    let mut heads: Vec<_> = commits
        .keys()
        .filter(|id| !referenced.contains(*id))
        .cloned()
        .collect();
    heads.sort();
    heads
}

fn role_folders(files: &[DriveFile], role: &str) -> Vec<String> {
    files
        .iter()
        .filter(|file| file.role.as_deref() == Some(role) && file.mime == FOLDER_MIME)
        .map(|file| file.id.clone())
        .collect()
}

pub(crate) fn parse_commit(bytes: &[u8]) -> Result<DriveCommitDoc, SyncError> {
    let json: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| SyncError::Protocol {
            detail: "drive commit is not json".to_owned(),
        })?;
    if json.get("kind").and_then(|value| value.as_str()) != Some("muninn-sync-commit") {
        return Err(SyncError::Protocol {
            detail: "drive file is not a muninn commit".to_owned(),
        });
    }
    if json.get("schema").and_then(|value| value.as_u64()) != Some(u64::from(SCHEMA_VERSION)) {
        return Err(SyncError::Protocol {
            detail: "drive commit schema is not supported".to_owned(),
        });
    }
    let mut entries = Vec::new();
    for entry in json
        .get("entries")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
    {
        entries.push(DriveEntryDoc {
            path: {
                let path = json_str(entry, "path")?;
                RelativePath::parse(&path)?;
                path
            },
            sha256: json_str(entry, "sha256")?,
            size: entry
                .get("size")
                .and_then(|value| value.as_u64())
                .unwrap_or(0),
            blob_id: json_str(entry, "blob_id")?,
        });
    }
    let mut tombstones = Vec::new();
    for tomb in json
        .get("tombstones")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
    {
        tombstones.push(DriveTombstoneDoc {
            path: {
                let path = json_str(tomb, "path")?;
                RelativePath::parse(&path)?;
                path
            },
            base_sha256: json_str(tomb, "base_sha256")?,
        });
    }
    Ok(DriveCommitDoc {
        id: json_str(&json, "id")?,
        parents: json
            .get("parents")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect(),
        operation_id: json_str(&json, "operation_id")?,
        entries,
        tombstones,
    })
}

pub fn new_pkce(redirect_uri: &str) -> PkceRequest {
    let verifier = random_token(32);
    let digest = Sha256::digest(verifier.as_bytes());
    PkceRequest {
        challenge: URL_SAFE_NO_PAD.encode(digest),
        state: random_token(16),
        verifier,
        redirect_uri: redirect_uri.to_owned(),
    }
}

pub fn parse_oauth_redirect(query: &str, expected_state: &str) -> Result<OAuthRedirect, SyncError> {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    let value = |name: &str| {
        pairs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    if let Some(error) = value("error") {
        return Err(SyncError::Auth {
            detail: format!("google authorization returned {error}"),
        });
    }
    let state = value("state").unwrap_or_default();
    if state != expected_state {
        return Err(SyncError::Auth {
            detail: "oauth state did not match the request".to_owned(),
        });
    }
    let code = value("code").ok_or_else(|| SyncError::Auth {
        detail: "oauth callback did not include a code".to_owned(),
    })?;
    let scope = value("scope").unwrap_or_default();
    if !scope.is_empty() && !scope.split_whitespace().any(|item| item == DRIVE_SCOPE) {
        return Err(SyncError::Scope {
            detail: "google did not grant drive.appdata".to_owned(),
        });
    }
    let picked = value("picked_file_ids")
        .unwrap_or_default()
        .split(',')
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(OAuthRedirect {
        code,
        picked_file_ids: picked,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OAuthRedirect {
    pub code: String,
    pub picked_file_ids: Vec<String>,
}

/// `bytes` from the OS CSPRNG, base64url. Used for PKCE verifiers and state.
pub(crate) fn random_token(bytes: usize) -> String {
    let mut raw = vec![0u8; bytes];
    getrandom::fill(&mut raw).expect("the OS random source is unavailable");
    URL_SAFE_NO_PAD.encode(raw)
}

fn form(pairs: &[(&str, &str)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

fn json_str(value: &serde_json::Value, key: &str) -> Result<String, SyncError> {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .map(str::to_owned)
        .ok_or_else(|| SyncError::Protocol {
            detail: format!("drive response is missing {key}"),
        })
}

fn token_from_oauth(json: &serde_json::Value) -> Result<TokenSet, SyncError> {
    let scope = json
        .get("scope")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if !scope.is_empty() && !scope.split_whitespace().any(|item| item == DRIVE_SCOPE) {
        return Err(SyncError::Scope {
            detail: "token is missing the drive.appdata scope".to_owned(),
        });
    }
    let access = json_str(json, "access_token")?;
    let refresh = json
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(Secret::new);
    let expires = json.get("expires_in").and_then(|value| value.as_u64());
    Ok(TokenSet {
        access: Secret::new(access),
        refresh,
        expires_unix_ms: expires
            .map(|seconds| crate::secret::expires_at(crate::secret::now_unix_ms(), seconds)),
        account: String::new(),
    })
}

fn require_client_id(client_id: &str) -> Result<(), SyncError> {
    if client_id.is_empty()
        || client_id
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err(SyncError::Config {
            detail: "Google OAuth client id is missing".to_owned(),
        });
    }
    Ok(())
}

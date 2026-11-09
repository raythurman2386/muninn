//! GitHub transport.
//!
//! Content is published as Git blobs, one tree, and one commit whose parent is
//! the commit we just read. The branch ref is updated with `force: false`.
//! A lost response is reconciled by looking for this run's operation id in the
//! branch tip before any second commit is created.

use std::collections::BTreeMap;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha1::{Digest, Sha1};

use crate::error::SyncError;
use crate::http::{
    CancelToken, HttpRequest, HttpResponse, HttpTransport, Sleeper, classify_status, with_retry,
};
use crate::limits::DEFAULT_LIMITS;
use crate::model::{ContentHash, FileKind, FileRecord, Snapshot};
use crate::pathutil::RelativePath;
use crate::secret::{Secret, TokenSet};

pub const COMMIT_PREFIX: &str = "Muninn sync ";

#[derive(Clone, Debug)]
pub struct GitHubScope {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    /// Vault root inside the repository. Empty means the repository root.
    /// Paths outside the prefix are preserved and are not part of the vault.
    pub prefix: String,
}

impl GitHubScope {
    pub fn validate(&self) -> Result<(), SyncError> {
        github_component(&self.owner, "repository owner")?;
        github_component(&self.repo, "repository name")?;
        if self.branch.is_empty()
            || self.branch.starts_with('/')
            || self.branch.ends_with('/')
            || self.branch.contains("//")
            || self
                .branch
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(SyncError::Config {
                detail: "branch name is not a portable git ref".to_owned(),
            });
        }
        if !self.prefix.is_empty() {
            RelativePath::parse(&self.prefix)?;
        }
        Ok(())
    }
}

/// One read of the branch. `blobs` holds only the bytes this read had to
/// download; `shas` names the git blob for every path in `snapshot`.
#[derive(Clone, Debug, Default)]
pub struct GitHubRemote {
    pub commit: Option<String>,
    pub tree: Option<String>,
    pub snapshot: Snapshot,
    pub blobs: BTreeMap<ContentHash, Vec<u8>>,
    pub shas: BTreeMap<RelativePath, String>,
}

#[derive(Clone, Debug)]
pub struct DeviceChallenge {
    pub device_code: Secret,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Clone, Debug)]
pub enum DevicePoll {
    Pending { interval: u64 },
    SlowDown { interval: u64 },
    Approved(TokenSet),
}

pub struct GitHubClient<'a, T> {
    pub api_base: String,
    pub oauth_base: String,
    pub transport: T,
    pub cancel: CancelToken,
    pub sleeper: &'a dyn Sleeper,
    pub request_timeout: Duration,
}

impl<'a, T: HttpTransport> GitHubClient<'a, T> {
    /// Read the branch once with an empty sha cache. Every blob is downloaded.
    pub fn fetch(&self, scope: &GitHubScope, token: &Secret) -> Result<GitHubRemote, SyncError> {
        self.fetch_cached(scope, token, &BTreeMap::new())
    }

    /// Read the branch tip, its recursive tree, and the blobs whose content
    /// hash is not already known. `known` maps git blob sha to content hash;
    /// it is a pure fact about bytes, so any earlier run may have filled it.
    /// Blobs found through the cache are not downloaded and are absent from
    /// `blobs`; [`GitHubClient::blob_for_hash`] fetches one on demand.
    pub fn fetch_cached(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        known: &BTreeMap<String, ContentHash>,
    ) -> Result<GitHubRemote, SyncError> {
        scope.validate()?;
        let Some((commit, tree)) = self.get_ref(scope, token)? else {
            return Ok(GitHubRemote::default());
        };
        let mut snapshot = Snapshot::default();
        let mut blobs = BTreeMap::new();
        let mut shas = BTreeMap::new();
        let mut total = 0u64;
        for (path, sha, size) in self.recursive_tree(scope, token, &tree)? {
            let rel = strip_prefix(scope, &path)?;
            total = total.saturating_add(size);
            if total > DEFAULT_LIMITS.max_total_bytes {
                return Err(SyncError::Limit {
                    detail: "remote vault exceeds the sync byte limit".to_owned(),
                });
            }
            if snapshot.files.len() >= DEFAULT_LIMITS.max_files {
                return Err(SyncError::Limit {
                    detail: "remote vault exceeds the sync file limit".to_owned(),
                });
            }
            let hash = match known.get(&sha) {
                Some(hash) => *hash,
                None => {
                    let bytes = self.read_blob(scope, token, &sha)?;
                    if git_blob_sha(&bytes) != sha {
                        return Err(SyncError::Protocol {
                            detail: format!("blob {sha} did not match its contents"),
                        });
                    }
                    let hash = ContentHash::of(&bytes);
                    blobs.insert(hash, bytes);
                    hash
                }
            };
            snapshot.files.insert(
                rel.clone(),
                FileRecord {
                    hash,
                    size,
                    kind: FileKind::of_name(path.rsplit('/').next().unwrap_or(&path)),
                },
            );
            shas.insert(rel, sha);
        }
        Ok(GitHubRemote {
            commit: Some(commit),
            tree: Some(tree),
            snapshot,
            blobs,
            shas,
        })
    }

    /// Bytes for a content hash that `remote` lists but did not download.
    pub fn blob_for_hash(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        remote: &GitHubRemote,
        hash: ContentHash,
    ) -> Result<Vec<u8>, SyncError> {
        if let Some(bytes) = remote.blobs.get(&hash) {
            return Ok(bytes.clone());
        }
        let sha = remote
            .snapshot
            .files
            .iter()
            .find(|(_, record)| record.hash == hash)
            .and_then(|(path, _)| remote.shas.get(path))
            .ok_or_else(|| SyncError::Protocol {
                detail: format!("remote does not list content {hash}"),
            })?;
        let bytes = self.read_blob(scope, token, sha)?;
        if git_blob_sha(&bytes) != *sha || ContentHash::of(&bytes) != hash {
            return Err(SyncError::Protocol {
                detail: format!("blob {sha} did not match its contents"),
            });
        }
        Ok(bytes)
    }

    /// Publish `desired` as one commit whose parent is exactly the commit the
    /// plan was made from (`planned.commit`) and whose base tree is that
    /// commit's tree. The ref moves with `force: false`. If the branch moved
    /// since `planned` was read, GitHub refuses the update (422) and this
    /// returns [`SyncError::ConflictRemote`]; the caller must fetch and plan
    /// again. `desired` is never re-applied over a tip it was not planned on.
    pub fn publish(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        planned: &GitHubRemote,
        desired: &Snapshot,
        local_bytes: &BTreeMap<RelativePath, Vec<u8>>,
        operation_id: &str,
    ) -> Result<String, SyncError> {
        scope.validate()?;
        if let Some(commit) = &planned.commit {
            if same_content(&planned.snapshot, desired) {
                return Ok(commit.clone());
            }
            if self.commit_is_ours(scope, token, commit, operation_id)? {
                return Ok(commit.clone());
            }
        }
        let mut blob_shas: BTreeMap<ContentHash, String> = BTreeMap::new();
        for (path, record) in &planned.snapshot.files {
            if let Some(sha) = planned.shas.get(path) {
                blob_shas.insert(record.hash, sha.clone());
            }
        }
        for (path, record) in &desired.files {
            if blob_shas.contains_key(&record.hash) {
                continue;
            }
            let bytes = local_bytes
                .get(path)
                .or_else(|| planned.blobs.get(&record.hash))
                .ok_or_else(|| SyncError::Protocol {
                    detail: format!("missing bytes for {path}"),
                })?;
            if ContentHash::of(bytes) != record.hash {
                return Err(SyncError::Protocol {
                    detail: format!("bytes for {path} do not match the plan"),
                });
            }
            let sha = self.create_blob(scope, token, bytes)?;
            blob_shas.insert(record.hash, sha);
        }
        let tree_entries = desired_tree(scope, &planned.snapshot, desired, &blob_shas)?;
        let tree = self.create_tree(scope, token, planned.tree.as_deref(), &tree_entries)?;
        let parents = planned.commit.clone().into_iter().collect::<Vec<_>>();
        let message = format!("{COMMIT_PREFIX}{operation_id}\n\nLogical vault sync.\n");
        let commit = self.create_commit(scope, token, &message, &tree, &parents)?;
        match self.update_ref(scope, token, &commit, planned.commit.as_deref()) {
            Ok(()) => Ok(commit),
            Err(error) if error.outcome_unknown() => {
                // The ref may have moved to our commit before the response
                // was lost. Only an exact match counts as success.
                match self.get_ref(scope, token)? {
                    Some((tip, _)) if tip == commit => Ok(commit),
                    _ => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    pub fn device_start(&self, client_id: &str) -> Result<DeviceChallenge, SyncError> {
        require_client_id(client_id)?;
        let body = form(&[("client_id", client_id), ("scope", "")]);
        let response = self.send(
            HttpRequest {
                method: "POST".to_owned(),
                url: format!(
                    "{}/login/device/code",
                    self.oauth_base.trim_end_matches('/')
                ),
                headers: vec![
                    ("Accept".to_owned(), "application/json".to_owned()),
                    (
                        "Content-Type".to_owned(),
                        "application/x-www-form-urlencoded".to_owned(),
                    ),
                ],
                body: body.into_bytes(),
                mutation: false,
                timeout: self.request_timeout,
            },
            false,
        )?;
        let json = response.json()?;
        let device_code = json_str(&json, "device_code")?;
        Ok(DeviceChallenge {
            device_code: Secret::new(device_code),
            user_code: json_str(&json, "user_code")?,
            verification_uri: json_str(&json, "verification_uri")?,
            expires_in: json_u64(&json, "expires_in")?,
            interval: json_u64(&json, "interval")?.max(5),
        })
    }

    pub fn device_poll(
        &self,
        client_id: &str,
        device_code: &Secret,
    ) -> Result<DevicePoll, SyncError> {
        require_client_id(client_id)?;
        let body = form(&[
            ("client_id", client_id),
            ("device_code", device_code.expose()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ]);
        let response = self.transport.execute(&HttpRequest {
            method: "POST".to_owned(),
            url: format!(
                "{}/login/oauth/access_token",
                self.oauth_base.trim_end_matches('/')
            ),
            headers: vec![
                ("Accept".to_owned(), "application/json".to_owned()),
                (
                    "Content-Type".to_owned(),
                    "application/x-www-form-urlencoded".to_owned(),
                ),
            ],
            body: body.into_bytes(),
            mutation: false,
            timeout: self.request_timeout,
        })?;
        if response.status == 400 || response.status == 401 || response.status == 200 {
            let json = response.json().unwrap_or(serde_json::Value::Null);
            let error = json
                .get("error")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            return match error {
                "" if response.status == 200 => Ok(DevicePoll::Approved(token_from_json(&json)?)),
                "authorization_pending" => Ok(DevicePoll::Pending {
                    interval: json.get("interval").and_then(|v| v.as_u64()).unwrap_or(5),
                }),
                // RFC 8628 and GitHub: slow_down adds 5 seconds; GitHub also
                // returns the new interval, which wins when present.
                "slow_down" => Ok(DevicePoll::SlowDown {
                    interval: json.get("interval").and_then(|v| v.as_u64()).unwrap_or(10),
                }),
                "access_denied" => Err(SyncError::Auth {
                    detail: "the device authorization was denied".to_owned(),
                }),
                "expired_token" => Err(SyncError::Auth {
                    detail: "the device code expired".to_owned(),
                }),
                other => Err(SyncError::Auth {
                    detail: format!("device authorization failed ({other})"),
                }),
            };
        }
        classify_status(&response, false)?;
        Err(SyncError::Protocol {
            detail: "device poll returned an unexpected response".to_owned(),
        })
    }

    pub fn refresh(&self, client_id: &str, refresh: &Secret) -> Result<TokenSet, SyncError> {
        require_client_id(client_id)?;
        let body = form(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.expose()),
        ]);
        let response = self.send(
            HttpRequest {
                method: "POST".to_owned(),
                url: format!(
                    "{}/login/oauth/access_token",
                    self.oauth_base.trim_end_matches('/')
                ),
                headers: vec![
                    ("Accept".to_owned(), "application/json".to_owned()),
                    (
                        "Content-Type".to_owned(),
                        "application/x-www-form-urlencoded".to_owned(),
                    ),
                ],
                body: body.into_bytes(),
                mutation: true,
                timeout: self.request_timeout,
            },
            true,
        )?;
        let json = response.json()?;
        // GitHub answers a bad or expired refresh token with 200 and an
        // `error` field, not a 4xx.
        if let Some(error) = json.get("error").and_then(|value| value.as_str()) {
            return Err(SyncError::Auth {
                detail: format!("GitHub refused the refresh token ({error}); connect again"),
            });
        }
        token_from_json(&json)
    }

    pub fn current_login(&self, token: &Secret) -> Result<String, SyncError> {
        let response = self.send(self.api("GET", "/user", token, None, false), false)?;
        json_str(&response.json()?, "login")
    }

    fn get_ref(
        &self,
        scope: &GitHubScope,
        token: &Secret,
    ) -> Result<Option<(String, String)>, SyncError> {
        let path = format!(
            "/repos/{}/{}/git/ref/heads/{}",
            encode(&scope.owner),
            encode(&scope.repo),
            encode_branch(&scope.branch)
        );
        let response = match self
            .transport
            .execute(&self.api("GET", &path, token, None, false))
        {
            Ok(response) if response.status == 404 => return Ok(None),
            Ok(response) => response,
            Err(error) => return Err(error),
        };
        classify_status(&response, false)?;
        let json = response.json()?;
        let sha = json
            .pointer("/object/sha")
            .and_then(|value| value.as_str())
            .ok_or_else(|| SyncError::Protocol {
                detail: "branch ref did not include an object sha".to_owned(),
            })?
            .to_owned();
        let commit = self.send(
            self.api(
                "GET",
                &format!(
                    "/repos/{}/{}/git/commits/{sha}",
                    encode(&scope.owner),
                    encode(&scope.repo)
                ),
                token,
                None,
                false,
            ),
            false,
        )?;
        let tree = commit
            .json()?
            .pointer("/tree/sha")
            .and_then(|value| value.as_str())
            .ok_or_else(|| SyncError::Protocol {
                detail: "commit did not include a tree".to_owned(),
            })?
            .to_owned();
        Ok(Some((sha, tree)))
    }

    /// One `git/trees/<sha>?recursive=1` call. Returns `(path, blob sha,
    /// size)` for every regular file inside the prefix. A truncated listing
    /// is refused instead of being treated as deletions.
    fn recursive_tree(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        tree: &str,
    ) -> Result<Vec<(String, String, u64)>, SyncError> {
        let path = format!(
            "/repos/{}/{}/git/trees/{tree}?recursive=1",
            encode(&scope.owner),
            encode(&scope.repo)
        );
        let response = self.send(self.api("GET", &path, token, None, false), false)?;
        let json = response.json()?;
        if json.get("truncated").and_then(|value| value.as_bool()) != Some(false) {
            return Err(SyncError::ListingIncomplete {
                detail: format!(
                    "git tree {tree} was truncated; refusing to treat missing entries as deletions"
                ),
            });
        }
        let entries = json
            .get("tree")
            .and_then(|value| value.as_array())
            .ok_or_else(|| SyncError::Protocol {
                detail: "git tree response has no entries".to_owned(),
            })?;
        let mut out = Vec::new();
        for entry in entries {
            let full = entry
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned();
            let mode = entry
                .get("mode")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let sha = entry
                .get("sha")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if !inside_prefix(scope, &full) {
                continue;
            }
            match mode {
                "040000" => {}
                "120000" => {
                    return Err(SyncError::Unsupported {
                        path: full,
                        detail: "git symlink".to_owned(),
                    });
                }
                "160000" => {
                    return Err(SyncError::Unsupported {
                        path: full,
                        detail: "git submodule".to_owned(),
                    });
                }
                "100644" | "100755" => {
                    let size = entry
                        .get("size")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0);
                    if size > DEFAULT_LIMITS.max_file_bytes {
                        return Err(SyncError::Limit {
                            detail: format!("{full} is {size} bytes"),
                        });
                    }
                    if sha.len() != 40 || !sha.chars().all(|ch| ch.is_ascii_hexdigit()) {
                        return Err(SyncError::Protocol {
                            detail: format!("{full} has a malformed blob sha"),
                        });
                    }
                    out.push((full, sha.to_owned(), size));
                }
                other => {
                    return Err(SyncError::Unsupported {
                        path: full,
                        detail: format!("git mode {other}"),
                    });
                }
            }
        }
        Ok(out)
    }

    fn read_blob(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        sha: &str,
    ) -> Result<Vec<u8>, SyncError> {
        let response = self.send(
            self.api(
                "GET",
                &format!(
                    "/repos/{}/{}/git/blobs/{sha}",
                    encode(&scope.owner),
                    encode(&scope.repo)
                ),
                token,
                None,
                false,
            ),
            false,
        )?;
        let json = response.json()?;
        let encoding = json
            .get("encoding")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if encoding != "base64" {
            return Err(SyncError::Protocol {
                detail: "git blob was not base64".to_owned(),
            });
        }
        let content = json_str(&json, "content")?.replace(['\n', '\r'], "");
        STANDARD.decode(content).map_err(|_| SyncError::Protocol {
            detail: "git blob base64 did not decode".to_owned(),
        })
    }

    fn create_blob(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        bytes: &[u8],
    ) -> Result<String, SyncError> {
        let body = serde_json::json!({
            "content": STANDARD.encode(bytes),
            "encoding": "base64",
        });
        let response = self.send(
            self.api(
                "POST",
                &format!(
                    "/repos/{}/{}/git/blobs",
                    encode(&scope.owner),
                    encode(&scope.repo)
                ),
                token,
                Some(body.to_string().into_bytes()),
                true,
            ),
            true,
        )?;
        let sha = json_str(&response.json()?, "sha")?;
        if sha != git_blob_sha(bytes) {
            return Err(SyncError::Protocol {
                detail: "created blob sha did not match the bytes".to_owned(),
            });
        }
        Ok(sha)
    }

    fn create_tree(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        base_tree: Option<&str>,
        entries: &[serde_json::Value],
    ) -> Result<String, SyncError> {
        let mut body = serde_json::json!({ "tree": entries });
        if let Some(base) = base_tree {
            body["base_tree"] = serde_json::Value::String(base.to_owned());
        }
        let response = self.send(
            self.api(
                "POST",
                &format!(
                    "/repos/{}/{}/git/trees",
                    encode(&scope.owner),
                    encode(&scope.repo)
                ),
                token,
                Some(body.to_string().into_bytes()),
                true,
            ),
            true,
        )?;
        json_str(&response.json()?, "sha")
    }

    fn create_commit(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        message: &str,
        tree: &str,
        parents: &[String],
    ) -> Result<String, SyncError> {
        let body = serde_json::json!({
            "message": message,
            "tree": tree,
            "parents": parents,
        });
        let response = self.send_mutation_unknown_ok(self.api(
            "POST",
            &format!(
                "/repos/{}/{}/git/commits",
                encode(&scope.owner),
                encode(&scope.repo)
            ),
            token,
            Some(body.to_string().into_bytes()),
            true,
        ))?;
        json_str(&response.json()?, "sha")
    }

    fn update_ref(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        sha: &str,
        previous: Option<&str>,
    ) -> Result<(), SyncError> {
        let body = serde_json::json!({ "sha": sha, "force": false });
        let encoded = body.to_string();
        if encoded.contains("\"force\":true") || !encoded.contains("\"force\":false") {
            return Err(SyncError::Protocol {
                detail: "ref update lost force:false".to_owned(),
            });
        }
        let path = format!(
            "/repos/{}/{}/git/refs/heads/{}",
            encode(&scope.owner),
            encode(&scope.repo),
            encode_branch(&scope.branch)
        );
        let request = if previous.is_none() {
            let create = serde_json::json!({
                "ref": format!("refs/heads/{}", scope.branch),
                "sha": sha,
            });
            self.api(
                "POST",
                &format!(
                    "/repos/{}/{}/git/refs",
                    encode(&scope.owner),
                    encode(&scope.repo)
                ),
                token,
                Some(create.to_string().into_bytes()),
                true,
            )
        } else {
            self.api("PATCH", &path, token, Some(encoded.into_bytes()), true)
        };
        self.send_mutation_unknown_ok(request).map(|_| ())
    }

    fn commit_is_ours(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        sha: &str,
        operation_id: &str,
    ) -> Result<bool, SyncError> {
        let response = self.send(
            self.api(
                "GET",
                &format!(
                    "/repos/{}/{}/git/commits/{sha}",
                    encode(&scope.owner),
                    encode(&scope.repo)
                ),
                token,
                None,
                false,
            ),
            false,
        )?;
        let message = response
            .json()?
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_owned();
        Ok(message.contains(operation_id))
    }

    fn api(
        &self,
        method: &str,
        path: &str,
        token: &Secret,
        body: Option<Vec<u8>>,
        mutation: bool,
    ) -> HttpRequest {
        HttpRequest {
            method: method.to_owned(),
            url: format!("{}{path}", self.api_base.trim_end_matches('/')),
            headers: vec![
                (
                    "Authorization".to_owned(),
                    format!("Bearer {}", token.expose()),
                ),
                (
                    "Accept".to_owned(),
                    "application/vnd.github+json".to_owned(),
                ),
                ("X-GitHub-Api-Version".to_owned(), "2022-11-28".to_owned()),
                ("Content-Type".to_owned(), "application/json".to_owned()),
            ],
            body: body.unwrap_or_default(),
            mutation,
            timeout: self.request_timeout,
        }
    }

    fn send(&self, request: HttpRequest, mutation: bool) -> Result<HttpResponse, SyncError> {
        with_retry(&self.cancel, self.sleeper, || {
            let response = self.transport.execute(&request)?;
            classify_status(&response, mutation)?;
            Ok(response)
        })
    }

    fn send_mutation_unknown_ok(&self, request: HttpRequest) -> Result<HttpResponse, SyncError> {
        self.cancel.check()?;
        match self.transport.execute(&request) {
            Ok(response) => {
                classify_status(&response, true)?;
                Ok(response)
            }
            Err(error) if error.outcome_unknown() => Err(error),
            Err(SyncError::Offline { .. }) => Err(SyncError::OutcomeUnknown {
                detail: "the connection dropped during a git write".to_owned(),
            }),
            Err(error) => Err(error),
        }
    }
}

fn same_content(left: &Snapshot, right: &Snapshot) -> bool {
    left.files.len() == right.files.len()
        && left
            .files
            .iter()
            .all(|(path, record)| right.hash_of(path) == Some(record.hash))
}

fn desired_tree(
    scope: &GitHubScope,
    previous: &Snapshot,
    desired: &Snapshot,
    blob_shas: &BTreeMap<ContentHash, String>,
) -> Result<Vec<serde_json::Value>, SyncError> {
    let mut entries = Vec::new();
    for (path, record) in &desired.files {
        let remote_path = prefixed(scope, path);
        let sha = blob_shas
            .get(&record.hash)
            .ok_or_else(|| SyncError::Protocol {
                detail: format!("no blob sha for {path}"),
            })?;
        entries.push(serde_json::json!({
            "path": remote_path,
            "mode": "100644",
            "type": "blob",
            "sha": sha,
        }));
    }
    for path in previous.files.keys() {
        if desired.files.contains_key(path) {
            continue;
        }
        entries.push(serde_json::json!({
            "path": prefixed(scope, path),
            "mode": "100644",
            "type": "blob",
            "sha": null,
        }));
    }
    Ok(entries)
}

fn prefixed(scope: &GitHubScope, path: &RelativePath) -> String {
    if scope.prefix.is_empty() {
        path.as_str().to_owned()
    } else {
        format!("{}/{}", scope.prefix, path.as_str())
    }
}

fn strip_prefix(scope: &GitHubScope, path: &str) -> Result<RelativePath, SyncError> {
    let relative = if scope.prefix.is_empty() {
        path.to_owned()
    } else {
        path.strip_prefix(&format!("{}/", scope.prefix))
            .ok_or_else(|| SyncError::Protocol {
                detail: format!("{path} is outside the sync prefix"),
            })?
            .to_owned()
    };
    RelativePath::parse(&relative).map_err(|_| SyncError::Path {
        detail: format!("remote path {path} is not portable"),
    })
}

fn inside_prefix(scope: &GitHubScope, path: &str) -> bool {
    if scope.prefix.is_empty() {
        return !path.split('/').any(|part| part.starts_with('.'));
    }
    let prefix = format!("{}/", scope.prefix);
    path.starts_with(&prefix)
        && !path[prefix.len()..]
            .split('/')
            .any(|part| part.starts_with('.'))
}

pub fn git_blob_sha(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn encode_branch(branch: &str) -> String {
    branch.split('/').map(encode).collect::<Vec<_>>().join("/")
}

fn github_component(text: &str, label: &str) -> Result<(), SyncError> {
    if text.is_empty()
        || text.len() > 100
        || !text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
    {
        return Err(SyncError::Config {
            detail: format!("{label} contains characters this sync client will not send"),
        });
    }
    Ok(())
}

fn require_client_id(client_id: &str) -> Result<(), SyncError> {
    if client_id.is_empty()
        || client_id.len() > 120
        || client_id.chars().any(|ch| ch.is_whitespace())
    {
        return Err(SyncError::Config {
            detail: "GitHub OAuth client id is missing".to_owned(),
        });
    }
    Ok(())
}

fn form(pairs: &[(&str, &str)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        if !value.is_empty() {
            serializer.append_pair(key, value);
        }
    }
    serializer.finish()
}

fn json_str(value: &serde_json::Value, key: &str) -> Result<String, SyncError> {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .map(str::to_owned)
        .ok_or_else(|| SyncError::Protocol {
            detail: format!("response is missing {key}"),
        })
}

fn json_u64(value: &serde_json::Value, key: &str) -> Result<u64, SyncError> {
    value
        .get(key)
        .and_then(|item| item.as_u64())
        .ok_or_else(|| SyncError::Protocol {
            detail: format!("response is missing {key}"),
        })
}

fn token_from_json(json: &serde_json::Value) -> Result<TokenSet, SyncError> {
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

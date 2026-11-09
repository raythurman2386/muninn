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

#[derive(Clone, Debug)]
pub struct GitHubRemote {
    pub commit: Option<String>,
    pub tree: Option<String>,
    pub snapshot: Snapshot,
    pub blobs: BTreeMap<ContentHash, Vec<u8>>,
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
    pub fn fetch(&self, scope: &GitHubScope, token: &Secret) -> Result<GitHubRemote, SyncError> {
        scope.validate()?;
        let reference = self.get_ref(scope, token)?;
        let Some((commit, tree)) = reference else {
            return Ok(GitHubRemote {
                commit: None,
                tree: None,
                snapshot: Snapshot::default(),
                blobs: BTreeMap::new(),
            });
        };
        let mut files = Vec::new();
        self.walk_tree(scope, token, &tree, "", &mut files)?;
        let mut snapshot = Snapshot::default();
        let mut blobs = BTreeMap::new();
        let mut total = 0u64;
        for (path, sha, bytes) in files {
            let rel = strip_prefix(scope, &path)?;
            let hash = ContentHash::of(&bytes);
            total = total.saturating_add(bytes.len() as u64);
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
            snapshot.files.insert(
                rel,
                FileRecord {
                    hash,
                    size: bytes.len() as u64,
                    kind: FileKind::of_name(path.rsplit('/').next().unwrap_or(&path)),
                },
            );
            blobs.insert(hash, bytes);
            let _ = sha;
        }
        Ok(GitHubRemote {
            commit: Some(commit),
            tree: Some(tree),
            snapshot,
            blobs,
        })
    }

    /// Publish `desired`. `local_bytes` supplies the bytes for every path whose
    /// content is not already in `remote.blobs`. Returns the commit that the
    /// branch ref now names.
    pub fn publish(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        remote: &GitHubRemote,
        desired: &Snapshot,
        local_bytes: &BTreeMap<RelativePath, Vec<u8>>,
        operation_id: &str,
    ) -> Result<String, SyncError> {
        scope.validate()?;
        for _attempt in 0..3 {
            match self.publish_once(scope, token, remote, desired, local_bytes, operation_id) {
                Err(SyncError::ConflictRemote { .. }) => continue,
                other => return other,
            }
        }
        Err(SyncError::ConflictRemote {
            detail: "the branch moved on every attempt".to_owned(),
        })
    }

    fn publish_once(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        remote: &GitHubRemote,
        desired: &Snapshot,
        local_bytes: &BTreeMap<RelativePath, Vec<u8>>,
        operation_id: &str,
    ) -> Result<String, SyncError> {
        if let Some(commit) = &remote.commit
            && self.commit_is_ours(scope, token, commit, operation_id)?
        {
            return Ok(commit.clone());
        }
        let fresh = self.fetch(scope, token)?;
        if let Some(commit) = &fresh.commit
            && self.commit_is_ours(scope, token, commit, operation_id)?
        {
            return Ok(commit.clone());
        }
        let mut blob_shas: BTreeMap<ContentHash, String> = BTreeMap::new();
        for (path, record) in &desired.files {
            if let Some(bytes) = fresh.blobs.get(&record.hash) {
                blob_shas.insert(record.hash, git_blob_sha(bytes));
                continue;
            }
            let bytes = local_bytes.get(path).ok_or_else(|| SyncError::Protocol {
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
        let tree_entries = desired_tree(scope, &fresh.snapshot, desired, &blob_shas)?;
        let tree = self.create_tree(scope, token, fresh.tree.as_deref(), &tree_entries)?;
        let parents = fresh.commit.clone().into_iter().collect::<Vec<_>>();
        let message = format!("{COMMIT_PREFIX}{operation_id}\n\nLogical vault sync.\n");
        let commit = self.create_commit(scope, token, &message, &tree, &parents)?;
        self.update_ref(scope, token, &commit, fresh.commit.as_deref())?;
        Ok(commit)
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
                "authorization_pending" => Ok(DevicePoll::Pending { interval: 5 }),
                "slow_down" => Ok(DevicePoll::SlowDown { interval: 5 }),
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
        token_from_json(&response.json()?)
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

    fn walk_tree(
        &self,
        scope: &GitHubScope,
        token: &Secret,
        tree: &str,
        prefix: &str,
        out: &mut Vec<(String, String, Vec<u8>)>,
    ) -> Result<(), SyncError> {
        let path = format!(
            "/repos/{}/{}/git/trees/{tree}",
            encode(&scope.owner),
            encode(&scope.repo)
        );
        let response = self.send(self.api("GET", &path, token, None, false), false)?;
        let json = response.json()?;
        if json.get("truncated").and_then(|value| value.as_bool()) == Some(true) {
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
        for entry in entries {
            let name = entry
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let mode = entry
                .get("mode")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let sha = entry
                .get("sha")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let full = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            let inside = inside_prefix(scope, &full);
            match mode {
                "040000" => self.walk_tree(scope, token, sha, &full, out)?,
                "120000" if inside => {
                    return Err(SyncError::Unsupported {
                        path: full,
                        detail: "git symlink".to_owned(),
                    });
                }
                "160000" if inside => {
                    return Err(SyncError::Unsupported {
                        path: full,
                        detail: "git submodule".to_owned(),
                    });
                }
                "100644" | "100755" if inside => {
                    let size = entry
                        .get("size")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0);
                    if size > DEFAULT_LIMITS.max_file_bytes {
                        return Err(SyncError::Limit {
                            detail: format!("{full} is {size} bytes"),
                        });
                    }
                    let bytes = self.read_blob(scope, token, sha)?;
                    if git_blob_sha(&bytes) != sha {
                        return Err(SyncError::Protocol {
                            detail: format!("blob {sha} did not match its contents"),
                        });
                    }
                    out.push((full, sha.to_owned(), bytes));
                }
                "120000" | "160000" | "100644" | "100755" => {}
                other if inside => {
                    return Err(SyncError::Unsupported {
                        path: full,
                        detail: format!("git mode {other}"),
                    });
                }
                _ => {}
            }
        }
        Ok(())
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
        expires_unix_ms: expires.map(|seconds| seconds.saturating_mul(1000)),
        account: String::new(),
    })
}

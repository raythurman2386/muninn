//! Loopback tests for the production GitHub and Drive adapters.
//! The HTTP client is reqwest. Only the provider is scripted.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::credentials::{CredentialKey, CredentialStore, MemoryCredentialStore};
use crate::drive::{self, DriveClient, DriveCommitDoc, DriveScope};
use crate::engine::{EngineParts, RunRequest, SyncReport, run_sync};
use crate::error::SyncError;
use crate::github::{DevicePoll, GitHubClient, GitHubRemote, GitHubScope, git_blob_sha};
use crate::http::{CancelToken, RecordingSleeper, ReqwestTransport};
use crate::model::{
    BindingId, ContentHash, FileKind, FileRecord, OperationId, ProviderKind, Resolution, Snapshot,
};
use crate::pathutil::RelativePath;
use crate::secret::{Secret, TokenSet};
use crate::testkit::{ScriptedReply, ScriptedServer, json_reply};

struct GitStore {
    blobs: BTreeMap<String, Vec<u8>>,
    trees: BTreeMap<String, Vec<Value>>,
    commits: BTreeMap<String, Value>,
    refs: BTreeMap<String, String>,
    patch_races: u32,
    protected: bool,
    tree_reads: u32,
    blob_reads: u32,
    /// When set, API calls with any other bearer token get a 401.
    require_token: Option<String>,
    /// Body for a refresh-token grant on /login/oauth/access_token.
    refresh_reply: Value,
    refreshes: u32,
}

/// What GitHub's `?recursive=1` returns: every entry with its full path.
/// Trees posted by the client already hold full paths; hand-built ones nest.
fn flatten_tree(
    trees: &BTreeMap<String, Vec<Value>>,
    prefix: &str,
    entries: &[Value],
    out: &mut Vec<Value>,
    truncated: &mut bool,
) {
    for entry in entries {
        let name = entry.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let full = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        let mut flat = entry.clone();
        flat["path"] = Value::String(full.clone());
        out.push(flat);
        if entry.get("type").and_then(|v| v.as_str()) == Some("tree") {
            let sha = entry.get("sha").and_then(|v| v.as_str()).unwrap_or("");
            if sha.ends_with("truncated") {
                *truncated = true;
            } else if let Some(children) = trees.get(sha) {
                flatten_tree(trees, &full, children, out, truncated);
            }
        }
    }
}

fn git_store() -> GitStore {
    GitStore {
        blobs: BTreeMap::new(),
        trees: BTreeMap::new(),
        commits: BTreeMap::new(),
        refs: BTreeMap::new(),
        patch_races: 0,
        protected: false,
        tree_reads: 0,
        blob_reads: 0,
        require_token: None,
        refresh_reply: Value::Null,
        refreshes: 0,
    }
}

fn git_server(store: Arc<Mutex<GitStore>>) -> ScriptedServer {
    ScriptedServer::spawn(move |request| {
        let path = request.path.split('?').next().unwrap_or(&request.path).to_owned();
        let mut git = store.lock().expect("git");
        if request.method == "POST" && path.ends_with("/login/oauth/access_token") {
            git.refreshes += 1;
            return json_reply(200, git.refresh_reply.clone());
        }
        if let Some(required) = &git.require_token
            && request.header("authorization") != Some(format!("Bearer {required}").as_str())
        {
            return json_reply(401, json!({"message": "Bad credentials"}));
        }
        if request.method == "POST" && path.ends_with("/git/blobs") {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                content.replace(['\n', '\r'], ""),
            )
            .unwrap_or_default();
            let sha = git_blob_sha(&bytes);
            git.blobs.insert(sha.clone(), bytes);
            return json_reply(201, json!({"sha": sha}));
        }
        if request.method == "POST" && path.ends_with("/git/trees") {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            if body.get("base_tree").is_none() && !git.trees.is_empty() && path.contains("preserve") {
                return json_reply(422, json!({"message": "missing base_tree"}));
            }
            let base = body
                .get("base_tree")
                .and_then(|v| v.as_str())
                .and_then(|sha| git.trees.get(sha).cloned())
                .unwrap_or_default();
            let mut entries = base;
            for change in body.get("tree").and_then(|v| v.as_array()).into_iter().flatten() {
                let change_path = change.get("path").and_then(|v| v.as_str()).unwrap_or("");
                entries.retain(|entry| entry.get("path").and_then(|v| v.as_str()) != Some(change_path));
                if !change.get("sha").is_none() && !change.get("sha").unwrap().is_null() {
                    entries.push(change.clone());
                }
            }
            let sha = format!("tree{}", git.trees.len() + 1);
            git.trees.insert(sha.clone(), entries);
            return json_reply(201, json!({"sha": sha}));
        }
        if request.method == "POST" && path.ends_with("/git/commits") {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let sha = format!("commit{}", git.commits.len() + 1);
            let message = body.get("message").cloned().unwrap_or(Value::Null);
            git.commits.insert(sha.clone(), body);
            return json_reply(201, json!({"sha": sha, "message": message}));
        }
        if request.method == "POST" && path.ends_with("/git/refs") {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let reference = body.get("ref").and_then(|v| v.as_str()).unwrap_or("");
            let sha = body.get("sha").and_then(|v| v.as_str()).unwrap_or("").to_owned();
            let branch = reference.trim_start_matches("refs/heads/");
            if git.refs.contains_key(branch) {
                return json_reply(422, json!({"message": "reference already exists"}));
            }
            git.refs.insert(branch.to_owned(), sha);
            return json_reply(201, json!({"ref": reference}));
        }
        if request.method == "PATCH" && path.contains("/git/refs/heads/") {
            let body = request.body_str();
            if body.contains("\"force\":true") || !body.contains("\"force\":false") {
                return json_reply(400, json!({"message": "force must be false"}));
            }
            if git.protected {
                return json_reply(403, json!({"message": "protected branch"}));
            }
            let branch = path.rsplit('/').next().unwrap_or("").to_owned();
            let parsed: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let sha = parsed.get("sha").and_then(|v| v.as_str()).unwrap_or("").to_owned();
            let parent = git
                .commits
                .get(&sha)
                .and_then(|commit| commit.get("parents"))
                .and_then(|v| v.as_array())
                .and_then(|parents| parents.first())
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            if let Some(current) = git.refs.get(&branch)
                && current != &parent
                && current != &sha
            {
                return json_reply(422, json!({"message": "update is not a fast-forward"}));
            }
            if git.patch_races > 0 {
                // Another device lands a real change on the tip first, so a
                // publish that ignores it would erase vault/racer.md.
                git.patch_races -= 1;
                let current = git.refs.get(&branch).cloned().unwrap_or_default();
                let tree = git
                    .commits
                    .get(&current)
                    .and_then(|commit| commit.get("tree"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_owned();
                let racer = b"from racer".to_vec();
                let racer_sha = git_blob_sha(&racer);
                git.blobs.insert(racer_sha.clone(), racer.clone());
                let mut entries = git.trees.get(&tree).cloned().unwrap_or_default();
                entries.retain(|entry| entry.get("path").and_then(|v| v.as_str()) != Some("vault/racer.md"));
                entries.push(json!({"path": "vault/racer.md", "mode": "100644", "type": "blob", "sha": racer_sha, "size": racer.len()}));
                let racer_tree = format!("tree{}", git.trees.len() + 1);
                git.trees.insert(racer_tree.clone(), entries);
                let other = format!("race{}", git.commits.len() + 1);
                git.commits.insert(
                    other.clone(),
                    json!({"message": "other writer", "tree": racer_tree, "parents": [current]}),
                );
                git.refs.insert(branch, other);
                return json_reply(422, json!({"message": "Update is not a fast forward"}));
            }
            git.refs.insert(branch, sha);
            return json_reply(200, json!({"object": {"sha": git.refs.values().next().cloned().unwrap_or_default()}}));
        }
        if request.method == "GET" && path.contains("/git/ref/heads/") {
            let branch = path.rsplit('/').next().unwrap_or("");
            let Some(sha) = git.refs.get(branch).cloned() else {
                return json_reply(404, json!({"message": "not found"}));
            };
            return json_reply(200, json!({"object": {"sha": sha, "type": "commit"}}));
        }
        if request.method == "GET" && path.contains("/git/commits/") {
            let sha = path.rsplit('/').next().unwrap_or("");
            let Some(commit) = git.commits.get(sha).cloned() else {
                return json_reply(404, json!({"message": "not found"}));
            };
            let tree = commit.get("tree").and_then(|v| v.as_str()).unwrap_or("");
            return json_reply(
                200,
                json!({"sha": sha, "message": commit.get("message").and_then(|v| v.as_str()).unwrap_or(""), "tree": {"sha": tree}}),
            );
        }
        if request.method == "GET" && path.contains("/git/trees/") {
            let sha = path.rsplit('/').next().unwrap_or("");
            git.tree_reads += 1;
            if sha.ends_with("truncated") {
                return json_reply(200, json!({"sha": sha, "truncated": true, "tree": []}));
            }
            let Some(entries) = git.trees.get(sha).cloned() else {
                return json_reply(404, json!({"message": "not found"}));
            };
            if !request.path.contains("recursive=1") {
                return json_reply(200, json!({"sha": sha, "truncated": false, "tree": entries}));
            }
            let mut flat = Vec::new();
            let mut truncated = false;
            flatten_tree(&git.trees, "", &entries, &mut flat, &mut truncated);
            return json_reply(200, json!({"sha": sha, "truncated": truncated, "tree": flat}));
        }
        if request.method == "GET" && path.contains("/git/blobs/") {
            git.blob_reads += 1;
            let sha = path.rsplit('/').next().unwrap_or("");
            let Some(bytes) = git.blobs.get(sha).cloned() else {
                return json_reply(404, json!({"message": "not found"}));
            };
            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
            return json_reply(200, json!({"sha": sha, "encoding": "base64", "content": encoded, "size": bytes.len()}));
        }
        json_reply(404, json!({"message": "unhandled", "path": path}))
    })
    .expect("server")
}

fn github_client<'a>(
    base: &str,
    sleeper: &'a RecordingSleeper,
    cancel: CancelToken,
    timeout: Duration,
) -> GitHubClient<'a, ReqwestTransport> {
    GitHubClient {
        api_base: base.to_owned(),
        oauth_base: base.to_owned(),
        transport: ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap(),
        cancel,
        sleeper,
        request_timeout: timeout,
    }
}

fn scope() -> GitHubScope {
    GitHubScope {
        owner: "octocat".to_owned(),
        repo: "notes".to_owned(),
        branch: "muninn-sync".to_owned(),
        prefix: "vault".to_owned(),
    }
}

fn token() -> Secret {
    Secret::new("test-token")
}

fn snap_text(pairs: &[(&str, &str)]) -> (Snapshot, BTreeMap<RelativePath, Vec<u8>>) {
    let mut snapshot = Snapshot::default();
    let mut bytes = BTreeMap::new();
    for (path, text) in pairs {
        let rel = RelativePath::parse(path).unwrap();
        let data = text.as_bytes().to_vec();
        snapshot.files.insert(
            rel.clone(),
            FileRecord {
                hash: ContentHash::of(&data),
                size: data.len() as u64,
                kind: FileKind::Markdown,
            },
        );
        bytes.insert(rel, data);
    }
    (snapshot, bytes)
}

#[test]
fn github_publish_is_force_false_and_round_trips_rename_delete() {
    let store = Arc::new(Mutex::new(GitStore {
        blobs: BTreeMap::new(),
        trees: BTreeMap::new(),
        commits: BTreeMap::new(),
        refs: BTreeMap::new(),
        patch_races: 0,
        protected: false,
        tree_reads: 0,
        blob_reads: 0,
        require_token: None,
        refresh_reply: Value::Null,
        refreshes: 0,
    }));
    let server = git_server(Arc::clone(&store));
    let sleeper = RecordingSleeper::default();
    let client = github_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let scope = scope();
    let (first, bytes) = snap_text(&[("a.md", "alpha"), ("keep.md", "stay")]);
    let empty = GitHubRemote {
        commit: None,
        tree: None,
        snapshot: Snapshot::default(),
        blobs: BTreeMap::new(),
        shas: BTreeMap::new(),
    };
    let head = client
        .publish(&scope, &token(), &empty, &first, &bytes, "op-1")
        .unwrap();
    let fetched = client.fetch(&scope, &token()).unwrap();
    assert_eq!(fetched.commit.as_deref(), Some(head.as_str()));
    assert!(
        fetched
            .snapshot
            .get(&RelativePath::parse("a.md").unwrap())
            .is_some()
    );
    let (renamed, bytes) = snap_text(&[("b.md", "alpha"), ("keep.md", "stay")]);
    client
        .publish(&scope, &token(), &fetched, &renamed, &bytes, "op-2")
        .unwrap();
    let again = client.fetch(&scope, &token()).unwrap();
    assert!(
        again
            .snapshot
            .get(&RelativePath::parse("a.md").unwrap())
            .is_none()
    );
    assert_eq!(
        again
            .snapshot
            .hash_of(&RelativePath::parse("b.md").unwrap()),
        Some(ContentHash::of(b"alpha"))
    );
    let bodies = server
        .recorded()
        .into_iter()
        .filter(|request| request.method == "PATCH")
        .map(|request| request.body_str())
        .collect::<Vec<_>>();
    assert!(!bodies.is_empty());
    assert!(bodies.iter().all(|body| body.contains("\"force\":false")));
    assert!(bodies.iter().all(|body| !body.contains("\"force\":true")));
}

#[test]
fn github_publish_refuses_a_tip_that_moved_after_the_plan() {
    let store = Arc::new(Mutex::new(git_store()));
    let server = git_server(Arc::clone(&store));
    let sleeper = RecordingSleeper::default();
    let client = github_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let (snapshot, bytes) = snap_text(&[("a.md", "alpha")]);
    client
        .publish(
            &scope(),
            &token(),
            &GitHubRemote::default(),
            &snapshot,
            &bytes,
            "op-base",
        )
        .unwrap();
    let planned = client.fetch(&scope(), &token()).unwrap();
    store.lock().unwrap().patch_races = 1;
    let (changed, bytes) = snap_text(&[("a.md", "beta")]);
    let error = client
        .publish(&scope(), &token(), &planned, &changed, &bytes, "op-race")
        .unwrap_err();
    assert!(matches!(error, SyncError::ConflictRemote { .. }), "{error}");

    // The other device's file is intact and ours was not applied over it.
    let after = client.fetch(&scope(), &token()).unwrap();
    assert_eq!(
        after
            .snapshot
            .hash_of(&RelativePath::parse("racer.md").unwrap()),
        Some(ContentHash::of(b"from racer"))
    );
    assert_eq!(
        after
            .snapshot
            .hash_of(&RelativePath::parse("a.md").unwrap()),
        Some(ContentHash::of(b"alpha"))
    );
    // Exactly one ref update was tried: the refusal is not retried with a
    // tree that was built against the old tip.
    let patches = server
        .recorded()
        .into_iter()
        .filter(|request| request.method == "PATCH")
        .count();
    assert_eq!(patches, 1);
}

#[test]
fn github_rejects_symlink_truncation_and_protected_branch() {
    let store = Arc::new(Mutex::new(GitStore {
        blobs: BTreeMap::new(),
        trees: {
            let mut trees = BTreeMap::new();
            trees.insert(
                "treesym".to_owned(),
                vec![json!({"path": "vault", "mode": "040000", "type": "tree", "sha": "treesym2"})],
            );
            trees.insert(
                "treesym2".to_owned(),
                vec![json!({"path": "link.md", "mode": "120000", "type": "blob", "sha": "abc", "size": 1})],
            );
            trees.insert(
                "treetrunc".to_owned(),
                vec![
                    json!({"path": "vault", "mode": "040000", "type": "tree", "sha": "truncated"}),
                ],
            );
            trees
        },
        commits: {
            let mut commits = BTreeMap::new();
            commits.insert(
                "csym".to_owned(),
                json!({"message": "s", "tree": "treesym", "parents": []}),
            );
            commits.insert(
                "ctrunc".to_owned(),
                json!({"message": "t", "tree": "treetrunc", "parents": []}),
            );
            commits
        },
        refs: BTreeMap::from([("muninn-sync".to_owned(), "csym".to_owned())]),
        patch_races: 0,
        protected: false,
        tree_reads: 0,
        blob_reads: 0,
        require_token: None,
        refresh_reply: Value::Null,
        refreshes: 0,
    }));
    let server = git_server(Arc::clone(&store));
    let sleeper = RecordingSleeper::default();
    let client = github_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let error = client.fetch(&scope(), &token()).unwrap_err();
    assert!(matches!(error, SyncError::Unsupported { .. }), "{error}");

    store
        .lock()
        .unwrap()
        .refs
        .insert("muninn-sync".to_owned(), "ctrunc".to_owned());
    let error = client.fetch(&scope(), &token()).unwrap_err();
    assert!(
        matches!(error, SyncError::ListingIncomplete { .. }),
        "{error}"
    );

    store.lock().unwrap().protected = true;
    store.lock().unwrap().commits.insert(
        "cplain".to_owned(),
        json!({"message": "plain", "tree": "treeplain", "parents": []}),
    );
    store.lock().unwrap().trees.insert(
        "treeplain".to_owned(),
        vec![json!({"path": "vault", "mode": "040000", "type": "tree", "sha": "treeplain2"})],
    );
    store
        .lock()
        .unwrap()
        .trees
        .insert("treeplain2".to_owned(), Vec::new());
    store
        .lock()
        .unwrap()
        .refs
        .insert("muninn-sync".to_owned(), "cplain".to_owned());
    let (snapshot, bytes) = snap_text(&[("a.md", "alpha")]);
    let error = client
        .publish(
            &scope(),
            &token(),
            &GitHubRemote {
                commit: Some("cplain".to_owned()),
                tree: Some("treeplain".to_owned()),
                snapshot: Snapshot::default(),
                blobs: BTreeMap::new(),
                shas: BTreeMap::new(),
            },
            &snapshot,
            &bytes,
            "op-protected",
        )
        .unwrap_err();
    assert!(matches!(error, SyncError::Permission { .. }), "{error}");
}

#[test]
fn github_device_flow_honors_pending_slow_down_and_denial() {
    let step = Arc::new(Mutex::new(0u32));
    let step_handler = Arc::clone(&step);
    let server = ScriptedServer::spawn(move |request| {
        if request.path.contains("/login/device/code") {
            return json_reply(
                200,
                json!({
                    "device_code": "device-secret",
                    "user_code": "ABCD-EFGH",
                    "verification_uri": "https://github.com/login/device",
                    "expires_in": 600,
                    "interval": 5
                }),
            );
        }
        let mut step = step_handler.lock().unwrap();
        *step += 1;
        match *step {
            1 => json_reply(200, json!({"error": "authorization_pending"})),
            2 => json_reply(200, json!({"error": "slow_down"})),
            _ => json_reply(200, json!({"error": "access_denied"})),
        }
    })
    .unwrap();
    let sleeper = RecordingSleeper::default();
    let client = github_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let challenge = client.device_start("Iv1.client").unwrap();
    assert_eq!(challenge.user_code, "ABCD-EFGH");
    assert!(matches!(
        client
            .device_poll("Iv1.client", &challenge.device_code)
            .unwrap(),
        DevicePoll::Pending { .. }
    ));
    assert!(matches!(
        client
            .device_poll("Iv1.client", &challenge.device_code)
            .unwrap(),
        DevicePoll::SlowDown { .. }
    ));
    let denied = client
        .device_poll("Iv1.client", &challenge.device_code)
        .unwrap_err();
    assert!(matches!(denied, SyncError::Auth { .. }));
    let rendered = format!("{denied:?}");
    assert!(!rendered.contains("device-secret"));
}

#[test]
fn drive_pagination_concurrent_heads_and_traversal() {
    let drive = Arc::new(Mutex::new(DriveStore::default()));
    let server = drive_server(Arc::clone(&drive));
    let sleeper = RecordingSleeper::default();
    let client = drive_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let scope = drive_scope();
    let (left, bytes) = snap_text(&[("a.md", "from-a")]);
    let remote = client.fetch(&scope, &token()).unwrap();
    assert!(remote.heads.is_empty());
    client
        .publish(&scope, &token(), &remote, &left, &[], &[], &bytes, "op-a")
        .unwrap();
    let (right, bytes) = snap_text(&[("b.md", "from-b")]);
    let remote = client.fetch(&scope, &token()).unwrap();
    // Publish a sibling head by pretending the parent set is the same first head's parent (none)
    // plus we pass the current head so this one builds on it. A second client that did not
    // see the first head passes an empty parent list.
    let empty = crate::drive::DriveRemote {
        commits: BTreeMap::new(),
        heads: Vec::new(),
        blobs: remote.blobs.clone(),
        blob_ids: remote.blob_ids.clone(),
    };
    client
        .publish(&scope, &token(), &empty, &right, &[], &[], &bytes, "op-b")
        .unwrap();
    let remote = client.fetch(&scope, &token()).unwrap();
    assert_eq!(remote.heads.len(), 2, "concurrent commits must both remain");
    let merged = drive::merge_heads(&remote).unwrap();
    assert!(!merged.plan.has_conflicts(), "{:?}", merged.plan.conflicts);
    assert!(
        merged
            .snapshot
            .get(&RelativePath::parse("a.md").unwrap())
            .is_some()
    );
    assert!(
        merged
            .snapshot
            .get(&RelativePath::parse("b.md").unwrap())
            .is_some()
    );

    drive.lock().unwrap().incomplete = true;
    let error = client.fetch(&scope, &token()).unwrap_err();
    assert!(
        matches!(error, SyncError::ListingIncomplete { .. }),
        "{error}"
    );
    drive.lock().unwrap().incomplete = false;

    let bad = DriveCommitDoc {
        id: "bad".to_owned(),
        parents: Vec::new(),
        operation_id: "bad".to_owned(),
        entries: vec![crate::drive::DriveEntryDoc {
            path: "../outside.md".to_owned(),
            sha256: ContentHash::of(b"x").to_hex(),
            size: 1,
            blob_id: "blob".to_owned(),
        }],
        tombstones: Vec::new(),
    };
    let error = bad.snapshot().unwrap_err();
    assert!(matches!(error, SyncError::Path { .. }), "{error}");
}

fn drive_engine<'a>(
    server: &ScriptedServer,
    credentials: &'a MemoryCredentialStore,
) -> EngineParts<ReqwestTransport, &'a MemoryCredentialStore> {
    github_engine(server, credentials)
}

/// Write `pairs` as another device would: a real commit on top of the
/// current head, held back so the test decides when it lands.
fn stage_drive_racer(
    drive: &Arc<Mutex<DriveStore>>,
    server: &ScriptedServer,
    pairs: &[(&str, &str)],
) {
    let before: std::collections::BTreeSet<String> = drive
        .lock()
        .unwrap()
        .files
        .iter()
        .map(|file| file.id.clone())
        .collect();
    let sleeper = RecordingSleeper::default();
    let client = drive_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    let merged = drive::merge_heads(&remote).unwrap();
    let mut next = merged.snapshot.clone();
    let (extra, mut bytes) = snap_text(pairs);
    next.files.extend(extra.files);
    for (path, record) in &next.files {
        if !bytes.contains_key(path) {
            bytes.insert(path.clone(), remote.blobs[&record.hash].clone());
        }
    }
    client
        .publish(
            &drive_scope(),
            &token(),
            &remote,
            &next,
            &merged.parents,
            &[],
            &bytes,
            "op-racer",
        )
        .unwrap();
    let mut store = drive.lock().unwrap();
    let (landed, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut store.files)
        .into_iter()
        .partition(|file| !before.contains(&file.id));
    store.files = kept;
    store.pending = landed;
    store.commit_lists = 0;
}

#[test]
fn drive_two_device_race_keeps_both_changes() {
    let drive = Arc::new(Mutex::new(DriveStore::default()));
    let server = drive_server(Arc::clone(&drive));
    let (credentials, binding) = connected(ProviderKind::GoogleDrive);
    let engine = drive_engine(&server, &credentials);
    let device = TestVault::new("drive-race-a");
    std::fs::write(device.root.join("a.md"), "hello").unwrap();
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000041"),
        )
        .unwrap(),
    );

    stage_drive_racer(&drive, &server, &[("racer.md", "from racer")]);
    std::fs::write(device.root.join("a.md"), "edited").unwrap();
    // The racer lands after this sync has read the commits folder once.
    drive.lock().unwrap().race_at_commit_list = Some(2);
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000042"),
        )
        .unwrap(),
    );
    assert!(
        drive.lock().unwrap().pending.is_empty(),
        "the race must have fired"
    );

    let sleeper = RecordingSleeper::default();
    let client = drive_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    assert_eq!(remote.heads.len(), 1, "{:?}", remote.heads);
    let merged = drive::merge_heads(&remote).unwrap();
    assert_eq!(
        merged
            .snapshot
            .hash_of(&RelativePath::parse("racer.md").unwrap()),
        Some(ContentHash::of(b"from racer")),
        "the other device's file must survive"
    );
    assert_eq!(
        merged
            .snapshot
            .hash_of(&RelativePath::parse("a.md").unwrap()),
        Some(ContentHash::of(b"edited"))
    );
    assert_eq!(device.read("racer.md").as_deref(), Some("from racer"));
    assert_eq!(device.read("a.md").as_deref(), Some("edited"));
}

/// Two heads that both edited `a.md`, as an older build (or a writer that
/// slipped past the last check) leaves a store.
fn stuck_drive_store(client: &DriveClient<'_, ReqwestTransport>) -> String {
    let (base, bytes) = snap_text(&[("a.md", "base"), ("keep.md", "keep"), ("gone.md", "x")]);
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    let root = client
        .publish(
            &drive_scope(),
            &token(),
            &remote,
            &base,
            &[],
            &[],
            &bytes,
            "op-base",
        )
        .unwrap();
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    for (op, text) in [("op-left", "left"), ("op-right", "right")] {
        // The left head also deletes gone.md; that must merge cleanly.
        let pairs: Vec<(&str, &str)> = if op == "op-left" {
            vec![("a.md", text), ("keep.md", "keep")]
        } else {
            vec![("a.md", text), ("keep.md", "keep"), ("gone.md", "x")]
        };
        let (next, bytes) = snap_text(&pairs);
        client
            .publish(
                &drive_scope(),
                &token(),
                &remote,
                &next,
                std::slice::from_ref(&root),
                &[],
                &bytes,
                op,
            )
            .unwrap();
    }
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    assert_eq!(remote.heads.len(), 2);
    remote.heads[1].clone()
}

fn path(text: &str) -> RelativePath {
    RelativePath::parse(text).unwrap()
}

#[test]
fn drive_conflicting_heads_keep_both_versions_and_unstick_the_store() {
    let drive = Arc::new(Mutex::new(DriveStore::default()));
    let server = drive_server(Arc::clone(&drive));
    let sleeper = RecordingSleeper::default();
    let client = drive_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let second = stuck_drive_store(&client);
    let copy = path(&format!("a (conflict {}).md", &second[second.len() - 8..]));

    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    let merged = drive::merge_heads(&remote).expect("conflicting heads still merge");
    assert_eq!(merged.parents, remote.heads, "the merge names every head");
    assert_eq!(merged.conflict_copies, vec![copy.clone()]);
    assert_eq!(
        merged.snapshot.hash_of(&path("a.md")),
        Some(ContentHash::of(b"left"))
    );
    assert_eq!(
        merged.snapshot.hash_of(&copy),
        Some(ContentHash::of(b"right"))
    );
    assert_eq!(
        merged.snapshot.hash_of(&path("gone.md")),
        None,
        "a clean delete merges"
    );
    assert_eq!(
        merged.snapshot.hash_of(&path("keep.md")),
        Some(ContentHash::of(b"keep"))
    );

    // The next sync of any device merges, keeps both, and publishes one head.
    let (credentials, binding) = connected(ProviderKind::GoogleDrive);
    let engine = drive_engine(&server, &credentials);
    let device = TestVault::new("drive-heads");
    std::fs::write(device.root.join("keep.md"), "keep").unwrap();
    let report = run_sync(
        &engine,
        &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000051"),
    )
    .unwrap();
    match &report {
        SyncReport::Succeeded {
            conflict_copies, ..
        } => {
            assert_eq!(conflict_copies, &vec![copy.clone()]);
        }
        other => panic!("expected success, got {other:?}"),
    }
    assert_eq!(device.read("a.md").as_deref(), Some("left"));
    assert_eq!(device.read(copy.as_str()).as_deref(), Some("right"));
    assert_eq!(device.read("keep.md").as_deref(), Some("keep"));

    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    assert_eq!(
        remote.heads.len(),
        1,
        "the store is unstuck: {:?}",
        remote.heads
    );
    let merged = drive::merge_heads(&remote).unwrap();
    assert!(merged.conflict_copies.is_empty());
    assert_eq!(
        merged.snapshot.hash_of(&copy),
        Some(ContentHash::of(b"right"))
    );

    // A second sync is clean and makes no new copies.
    let report = run_sync(
        &engine,
        &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000052"),
    )
    .unwrap();
    assert!(
        matches!(&report, SyncReport::Succeeded { conflict_copies, .. } if conflict_copies.is_empty()),
        "{report:?}"
    );
}

#[test]
fn drive_devices_that_merge_a_stuck_store_at_once_converge() {
    let drive = Arc::new(Mutex::new(DriveStore::default()));
    let server = drive_server(Arc::clone(&drive));
    let sleeper = RecordingSleeper::default();
    let client = drive_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    stuck_drive_store(&client);
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    let merged = drive::merge_heads(&remote).unwrap();
    // Two devices publish the same keep-both merge without seeing each other.
    for op in ["op-merge-a", "op-merge-b"] {
        client
            .publish(
                &drive_scope(),
                &token(),
                &remote,
                &merged.snapshot,
                &merged.parents,
                &[],
                &BTreeMap::new(),
                op,
            )
            .unwrap();
    }
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    assert_eq!(remote.heads.len(), 2);
    let again = drive::merge_heads(&remote).unwrap();
    assert!(
        again.conflict_copies.is_empty(),
        "{:?}",
        again.conflict_copies
    );
    assert_eq!(again.snapshot, merged.snapshot);

    // Had they merged to different trees (one also edited keep.md), the
    // criss-cross still merges, keeping both versions of what differs.
    let mut edited = merged.snapshot.clone();
    let (extra, bytes) = snap_text(&[("keep.md", "kept differently")]);
    edited.files.extend(extra.files);
    client
        .publish(
            &drive_scope(),
            &token(),
            &remote,
            &edited,
            &remote.heads[..1],
            &[],
            &bytes,
            "op-merge-c",
        )
        .unwrap();
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    let third = drive::merge_heads(&remote).expect("a criss-cross must not wedge");
    assert_eq!(third.parents.len(), remote.heads.len());
    let kept: std::collections::BTreeSet<_> = third
        .snapshot
        .files
        .iter()
        .filter(|(path, _)| path.as_str().starts_with("keep"))
        .map(|(_, record)| record.hash)
        .collect();
    assert!(kept.contains(&ContentHash::of(b"keep")), "{kept:?}");
    assert!(
        kept.contains(&ContentHash::of(b"kept differently")),
        "{kept:?}"
    );
}

#[test]
fn drive_writer_that_lands_during_blob_upload_is_merged_not_forked() {
    let drive = Arc::new(Mutex::new(DriveStore::default()));
    let server = drive_server(Arc::clone(&drive));
    let (credentials, binding) = connected(ProviderKind::GoogleDrive);
    let engine = drive_engine(&server, &credentials);
    let device = TestVault::new("drive-blob-race");
    std::fs::write(device.root.join("a.md"), "hello").unwrap();
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000061"),
        )
        .unwrap(),
    );
    stage_drive_racer(&drive, &server, &[("racer.md", "from racer")]);
    std::fs::write(device.root.join("a.md"), "edited").unwrap();
    // The racer lands while this sync uploads its first blob: after any
    // check made before the blobs, before the manifest.
    drive.lock().unwrap().race_at_blob_upload = true;
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000062"),
        )
        .unwrap(),
    );
    assert!(
        drive.lock().unwrap().pending.is_empty(),
        "the race must have fired"
    );
    let sleeper = RecordingSleeper::default();
    let client = drive_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    assert_eq!(
        remote.heads.len(),
        1,
        "publish must re-check before the manifest: {:?}",
        remote.heads
    );
    let merged = drive::merge_heads(&remote).unwrap();
    assert_eq!(
        merged.snapshot.hash_of(&path("racer.md")),
        Some(ContentHash::of(b"from racer"))
    );
    assert_eq!(
        merged.snapshot.hash_of(&path("a.md")),
        Some(ContentHash::of(b"edited"))
    );
    assert_eq!(device.read("racer.md").as_deref(), Some("from racer"));
}

#[test]
fn drive_oauth_state_and_scope_are_checked() {
    let pkce = drive::new_pkce("http://127.0.0.1:9/callback");
    let ok = drive::parse_oauth_redirect(
        &format!("code=abc&state={}&scope={}", pkce.state, drive::DRIVE_SCOPE),
        &pkce.state,
    )
    .unwrap();
    assert_eq!(ok.code, "abc");
    let mismatch = drive::parse_oauth_redirect("code=abc&state=nope", &pkce.state).unwrap_err();
    assert!(matches!(mismatch, SyncError::Auth { .. }));
    let narrow = drive::parse_oauth_redirect(
        &format!("code=abc&state={}&scope=email", pkce.state),
        &pkce.state,
    )
    .unwrap_err();
    assert!(matches!(narrow, SyncError::Scope { .. }));
}

#[test]
fn cancel_and_timeout_and_rate_limit() {
    let hits = Arc::new(Mutex::new(0u32));
    let hits_handler = Arc::clone(&hits);
    let server = ScriptedServer::spawn(move |request| {
        let mut hits = hits_handler.lock().unwrap();
        *hits += 1;
        if request.path.contains("slow") {
            return ScriptedReply::Delay {
                delay: Duration::from_millis(400),
                response: Box::new(json_reply(200, json!({"ok": true}))),
            };
        }
        if *hits == 1 {
            return ScriptedReply::Response {
                status: 429,
                headers: vec![("retry-after".to_owned(), "0".to_owned())],
                body: br#"{"message":"rate limit"}"#.to_vec(),
            };
        }
        json_reply(200, json!({"login": "octocat"}))
    })
    .unwrap();
    let sleeper = RecordingSleeper::default();
    let client = github_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_millis(80),
    );
    let login = client.current_login(&token()).unwrap();
    assert_eq!(login, "octocat");
    assert!(!sleeper.delays.lock().unwrap().is_empty());

    let cancel = CancelToken::new();
    cancel.cancel();
    let client = github_client(&server.base(), &sleeper, cancel, Duration::from_secs(2));
    let error = client.current_login(&token()).unwrap_err();
    assert!(matches!(error, SyncError::Cancelled));
}

#[test]
fn engine_pushes_and_pulls_between_two_synthetic_vaults() {
    let store = Arc::new(Mutex::new(GitStore {
        blobs: BTreeMap::new(),
        trees: BTreeMap::new(),
        commits: BTreeMap::new(),
        refs: BTreeMap::new(),
        patch_races: 0,
        protected: false,
        tree_reads: 0,
        blob_reads: 0,
        require_token: None,
        refresh_reply: Value::Null,
        refreshes: 0,
    }));
    let server = git_server(Arc::clone(&store));
    let credentials = MemoryCredentialStore::default();
    let binding = BindingId::fresh();
    let key = CredentialKey {
        provider: ProviderKind::GitHub,
        binding: binding.clone(),
    };
    credentials
        .put(
            &key,
            &TokenSet {
                access: Secret::new("test-token"),
                refresh: None,
                expires_unix_ms: None,
                account: "octocat".to_owned(),
            },
        )
        .unwrap();
    let transport = ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap();
    let sleeper = RecordingSleeper::default();
    let engine = EngineParts {
        transport,
        credentials: &credentials,
        cancel: CancelToken::new(),
        sleeper: Box::new(sleeper),
        github_api: server.base(),
        github_oauth: server.base(),
        drive_api: server.base(),
        drive_upload: server.base(),
        drive_oauth: server.base(),
        github_client_id: "Iv1.testclient".to_owned(),
        google_client: Some(crate::clients::GoogleOAuthClient {
            id: "Iv1.testclient".to_owned(),
            secret: Secret::new("test-only-google-secret"),
        }),
        tailnet_api: None,
    };
    let root = std::env::temp_dir().join(format!("muninn-engine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(root.join("a.md"), "hello").unwrap();
    std::fs::write(root.join(".secret"), "nope").unwrap();
    let state = root.with_extension("state");
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).unwrap();
    let request = RunRequest {
        root: root.clone(),
        state_dir: state.clone(),
        binding: binding.clone(),
        provider: ProviderKind::GitHub,
        github: Some(scope()),
        drive: None,
        tailnet: None,
        operation: OperationId::parse("abcde00000000001").unwrap(),
        protected: Default::default(),
        resolutions: Vec::new(),
        default_resolution: None,
    };
    let report = run_sync(&engine, &request).unwrap();
    assert!(matches!(
        report,
        crate::engine::SyncReport::Succeeded { .. }
    ));
    assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), "hello");
    assert_eq!(
        std::fs::read_to_string(root.join(".secret")).unwrap(),
        "nope"
    );

    let other = root.with_extension("other");
    let other_state = root.with_extension("other-state");
    let _ = std::fs::remove_dir_all(&other);
    let _ = std::fs::remove_dir_all(&other_state);
    std::fs::create_dir_all(&other).unwrap();
    std::fs::create_dir_all(&other_state).unwrap();
    let pulled = RunRequest {
        root: other.clone(),
        state_dir: other_state,
        binding,
        provider: ProviderKind::GitHub,
        github: Some(scope()),
        drive: None,
        tailnet: None,
        operation: OperationId::parse("abcde00000000002").unwrap(),
        protected: Default::default(),
        resolutions: Vec::new(),
        default_resolution: None,
    };
    run_sync(&engine, &pulled).unwrap();
    assert_eq!(
        std::fs::read_to_string(other.join("a.md")).unwrap(),
        "hello"
    );
    crate::engine::disconnect(&credentials, &key).unwrap();
    assert!(credentials.get(&key).unwrap().is_none());
    assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), "hello");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&state);
    let _ = std::fs::remove_dir_all(&other);
}

fn github_engine<'a>(
    server: &ScriptedServer,
    credentials: &'a MemoryCredentialStore,
) -> EngineParts<ReqwestTransport, &'a MemoryCredentialStore> {
    EngineParts {
        transport: ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap(),
        credentials,
        cancel: CancelToken::new(),
        sleeper: Box::new(RecordingSleeper::default()),
        github_api: server.base(),
        github_oauth: server.base(),
        drive_api: server.base(),
        drive_upload: server.base(),
        drive_oauth: server.base(),
        github_client_id: "Iv1.testclient".to_owned(),
        google_client: Some(crate::clients::GoogleOAuthClient {
            id: "Iv1.testclient".to_owned(),
            secret: Secret::new("test-only-google-secret"),
        }),
        tailnet_api: None,
    }
}

fn connected(provider: ProviderKind) -> (MemoryCredentialStore, BindingId) {
    let credentials = MemoryCredentialStore::default();
    let binding = BindingId::fresh();
    credentials
        .put(
            &CredentialKey {
                provider,
                binding: binding.clone(),
            },
            &TokenSet {
                access: Secret::new("test-token"),
                refresh: None,
                expires_unix_ms: None,
                account: "octocat".to_owned(),
            },
        )
        .unwrap();
    (credentials, binding)
}

/// A vault directory and its sync state directory, removed on drop.
struct TestVault {
    root: std::path::PathBuf,
    state: std::path::PathBuf,
}

impl TestVault {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("muninn-{name}-{}", std::process::id()));
        let state = root.with_extension("state");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&state);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        Self { root, state }
    }

    fn request(&self, provider: ProviderKind, binding: &BindingId, op: &str) -> RunRequest {
        RunRequest {
            root: self.root.clone(),
            state_dir: self.state.clone(),
            binding: binding.clone(),
            provider,
            github: (provider == ProviderKind::GitHub).then(scope),
            drive: (provider == ProviderKind::GoogleDrive).then(drive_scope),
            tailnet: None,
            operation: OperationId::parse(op).unwrap(),
            protected: Default::default(),
            resolutions: Vec::new(),
            default_resolution: None,
        }
    }

    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(path)).ok()
    }
}

impl Drop for TestVault {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        let _ = std::fs::remove_dir_all(&self.state);
    }
}

fn succeeded(report: crate::engine::SyncReport) {
    assert!(
        matches!(report, crate::engine::SyncReport::Succeeded { .. }),
        "{report:?}"
    );
}

#[test]
fn github_two_device_race_keeps_both_changes() {
    let store = Arc::new(Mutex::new(git_store()));
    let server = git_server(Arc::clone(&store));
    let (credentials, binding) = connected(ProviderKind::GitHub);
    let engine = github_engine(&server, &credentials);
    let device = TestVault::new("gh-race-a");
    std::fs::write(device.root.join("a.md"), "hello").unwrap();
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GitHub, &binding, "abcde00000000011"),
        )
        .unwrap(),
    );

    // This device edits a.md; another device lands racer.md on the branch
    // between our read and our ref update.
    std::fs::write(device.root.join("a.md"), "edited").unwrap();
    store.lock().unwrap().patch_races = 1;
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GitHub, &binding, "abcde00000000012"),
        )
        .unwrap(),
    );
    assert_eq!(
        store.lock().unwrap().patch_races,
        0,
        "the race must have fired"
    );

    let sleeper = RecordingSleeper::default();
    let client = github_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let remote = client.fetch(&scope(), &token()).unwrap();
    assert_eq!(
        remote
            .snapshot
            .hash_of(&RelativePath::parse("racer.md").unwrap()),
        Some(ContentHash::of(b"from racer")),
        "the other device's file must survive on the remote"
    );
    assert_eq!(
        remote
            .snapshot
            .hash_of(&RelativePath::parse("a.md").unwrap()),
        Some(ContentHash::of(b"edited"))
    );
    assert_eq!(device.read("racer.md").as_deref(), Some("from racer"));
    assert_eq!(device.read("a.md").as_deref(), Some("edited"));
}

#[test]
fn github_race_that_never_settles_fails_without_touching_the_vault() {
    let store = Arc::new(Mutex::new(git_store()));
    let server = git_server(Arc::clone(&store));
    let (credentials, binding) = connected(ProviderKind::GitHub);
    let engine = github_engine(&server, &credentials);
    let device = TestVault::new("gh-race-busy");
    std::fs::write(device.root.join("a.md"), "hello").unwrap();
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GitHub, &binding, "abcde00000000021"),
        )
        .unwrap(),
    );
    std::fs::write(device.root.join("a.md"), "edited").unwrap();
    store.lock().unwrap().patch_races = 100;
    let error = run_sync(
        &engine,
        &device.request(ProviderKind::GitHub, &binding, "abcde00000000022"),
    )
    .unwrap_err();
    assert!(matches!(error, SyncError::ConflictRemote { .. }), "{error}");
    assert_eq!(device.read("a.md").as_deref(), Some("edited"));
    assert_eq!(device.read("racer.md"), None);
}

#[test]
fn github_sync_reads_one_recursive_tree_and_reuses_known_blobs() {
    let store = Arc::new(Mutex::new(git_store()));
    let server = git_server(Arc::clone(&store));
    let (credentials, binding) = connected(ProviderKind::GitHub);
    let engine = github_engine(&server, &credentials);
    let device = TestVault::new("gh-cache");
    std::fs::create_dir_all(device.root.join("notes/deep")).unwrap();
    std::fs::write(device.root.join("a.md"), "hello").unwrap();
    std::fs::write(device.root.join("notes/deep/b.md"), "nested").unwrap();
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GitHub, &binding, "abcde00000000031"),
        )
        .unwrap(),
    );

    {
        let mut git = store.lock().unwrap();
        git.tree_reads = 0;
        git.blob_reads = 0;
    }
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GitHub, &binding, "abcde00000000032"),
        )
        .unwrap(),
    );
    {
        let git = store.lock().unwrap();
        assert_eq!(git.tree_reads, 1, "one recursive tree read per sync");
        assert_eq!(
            git.blob_reads, 0,
            "blobs this device wrote are known by sha"
        );
    }

    // A second device pulls once, then knows every blob too.
    let other = TestVault::new("gh-cache-b");
    succeeded(
        run_sync(
            &engine,
            &other.request(ProviderKind::GitHub, &binding, "abcde00000000033"),
        )
        .unwrap(),
    );
    assert_eq!(other.read("notes/deep/b.md").as_deref(), Some("nested"));
    store.lock().unwrap().blob_reads = 0;
    succeeded(
        run_sync(
            &engine,
            &other.request(ProviderKind::GitHub, &binding, "abcde00000000034"),
        )
        .unwrap(),
    );
    assert_eq!(store.lock().unwrap().blob_reads, 0);
    let recursive = server
        .recorded()
        .into_iter()
        .filter(|request| request.path.contains("/git/trees/"))
        .all(|request| request.path.contains("recursive=1"));
    assert!(recursive, "tree reads must be recursive");
}

#[derive(Default)]
struct DriveStore {
    files: Vec<DriveFile>,
    next_id: u64,
    incomplete: bool,
    sessions: BTreeMap<String, DriveFile>,
    /// Files another device wrote, landed just before the commits folder is
    /// listed for the `race_at_commit_list`-th time.
    pending: Vec<DriveFile>,
    race_at_commit_list: Option<u32>,
    /// Land `pending` while the next blob upload completes.
    race_at_blob_upload: bool,
    commit_lists: u32,
    /// Status and body for a POST /token refresh.
    token_reply: Option<(u16, Value)>,
    refreshes: u32,
    /// Form bodies of POST /token requests.
    token_bodies: Vec<String>,
}

#[derive(Clone)]
struct DriveFile {
    id: String,
    name: String,
    mime: String,
    parent: String,
    role: Option<String>,
    bytes: Vec<u8>,
}

fn drive_server(store: Arc<Mutex<DriveStore>>) -> ScriptedServer {
    let port = Arc::new(std::sync::atomic::AtomicU16::new(0));
    let port_for_handler = Arc::clone(&port);
    let server = ScriptedServer::spawn(move |request| {
        let mut drive = store.lock().expect("drive");
        let bound = port_for_handler.load(std::sync::atomic::Ordering::SeqCst);
        if request.method == "POST" && request.path == "/token" {
            drive.refreshes += 1;
            drive.token_bodies.push(request.body_str());
            let (status, body) = drive.token_reply.clone().unwrap_or((500, Value::Null));
            return json_reply(status, body);
        }
        if drive.incomplete && request.path.contains("/drive/v3/files?") {
            return json_reply(200, json!({"incompleteSearch": true, "files": []}));
        }
        // Everything lives in the app's hidden data folder.
        if request.path.contains("/drive/v3/files")
            && request.method == "GET"
            && !request.path.contains("alt=media")
            && !request.path.contains("spaces=appDataFolder")
            && !request.path.contains("space=appDataFolder")
        {
            return json_reply(400, json!({"error": {"message": "expected the appDataFolder space"}}));
        }
        if request.path.starts_with("/drive/v3/files/generateIds") {
            drive.next_id += 1;
            let id = format!("generated{:08}", drive.next_id);
            return json_reply(200, json!({"ids": [id], "space": "appDataFolder"}));
        }
        if request.method == "GET" && request.path.contains("alt=media") {
            let id = request.path.split('/').nth(4).unwrap_or("").split('?').next().unwrap_or("");
            let Some(file) = drive.files.iter().find(|file| file.id == id) else {
                return json_reply(404, json!({"error": {"message": "not found"}}));
            };
            return ScriptedReply::Response {
                status: 200,
                headers: vec![("content-type".to_owned(), "application/octet-stream".to_owned())],
                body: file.bytes.clone(),
            };
        }
        if request.method == "GET" && request.path.contains("/drive/v3/files?") {
            let query = request.path.split_once('?').map(|(_, query)| query).unwrap_or("");
            let parent = form_urlencoded::parse(query.as_bytes())
                .find(|(key, _)| key == "q")
                .and_then(|(_, value)| {
                    let text = value.into_owned();
                    let start = text.find('\'')? + 1;
                    let end = text[start..].find('\'')? + start;
                    Some(text[start..end].to_owned())
                })
                .unwrap_or_default();
            let is_commits = drive
                .files
                .iter()
                .any(|file| file.id == parent && file.role.as_deref() == Some("commits"));
            if is_commits {
                drive.commit_lists += 1;
                if drive.race_at_commit_list == Some(drive.commit_lists) {
                    let landed = std::mem::take(&mut drive.pending);
                    drive.files.extend(landed);
                }
            }
            let files: Vec<_> = drive
                .files
                .iter()
                .filter(|file| file.parent == parent)
                .map(|file| {
                    json!({
                        "id": file.id,
                        "name": file.name,
                        "mimeType": file.mime,
                        "size": file.bytes.len().to_string(),
                        "appProperties": file.role.as_ref().map(|role| json!({"muninnRole": role})).unwrap_or(json!({})),
                    })
                })
                .collect();
            return json_reply(200, json!({"files": files, "incompleteSearch": false}));
        }
        if request.method == "POST" && request.path.contains("uploadType=resumable") {
            let metadata: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let id = metadata.get("id").and_then(|v| v.as_str()).unwrap_or("noid").to_owned();
            if drive.files.iter().any(|file| file.id == id) {
                return json_reply(409, json!({"error": {"message": "conflict"}}));
            }
            let file = DriveFile {
                id: id.clone(),
                name: metadata.get("name").and_then(|v| v.as_str()).unwrap_or("").to_owned(),
                mime: metadata.get("mimeType").and_then(|v| v.as_str()).unwrap_or("").to_owned(),
                parent: metadata
                    .get("parents")
                    .and_then(|v| v.as_array())
                    .and_then(|v| v.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned(),
                role: metadata
                    .pointer("/appProperties/muninnRole")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                bytes: Vec::new(),
            };
            let session = format!("/upload-session/{id}");
            drive.sessions.insert(session.clone(), file);
            return ScriptedReply::Response {
                status: 200,
                headers: vec![(
                    "location".to_owned(),
                    format!("http://127.0.0.1:{bound}{session}"),
                )],
                body: Vec::new(),
            };
        }
        if request.method == "PUT" && request.path.contains("/upload-session/") {
            let id = request
                .path
                .trim_start_matches("/upload-session/")
                .split('?')
                .next()
                .unwrap_or("");
            let Some(mut file) = drive.sessions.remove(&format!("/upload-session/{id}")) else {
                return json_reply(404, json!({"error": {"message": "no session"}}));
            };
            file.bytes = request.body.clone();
            let created = file.id.clone();
            if drive.race_at_blob_upload && file.role.as_deref() == Some("blob") {
                drive.race_at_blob_upload = false;
                let landed = std::mem::take(&mut drive.pending);
                drive.files.extend(landed);
            }
            drive.files.push(file);
            return json_reply(200, json!({"id": created}));
        }
        if request.method == "POST" && request.path.contains("/drive/v3/files") {
            let metadata: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let id = metadata.get("id").and_then(|v| v.as_str()).unwrap_or("").to_owned();
            drive.files.push(DriveFile {
                id: id.clone(),
                name: metadata.get("name").and_then(|v| v.as_str()).unwrap_or("").to_owned(),
                mime: metadata.get("mimeType").and_then(|v| v.as_str()).unwrap_or("").to_owned(),
                parent: metadata
                    .get("parents")
                    .and_then(|v| v.as_array())
                    .and_then(|parents| parents.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned(),
                role: metadata
                    .pointer("/appProperties/muninnRole")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                bytes: Vec::new(),
            });
            return json_reply(200, json!({"id": id}));
        }
        json_reply(404, json!({"error": {"message": "unhandled drive", "path": request.path}}))
    })
    .unwrap();
    port.store(server.addr.port(), std::sync::atomic::Ordering::SeqCst);
    server
}

fn drive_scope() -> DriveScope {
    DriveScope {
        store: "Notes".to_owned(),
    }
}

fn drive_client<'a>(
    base: &str,
    sleeper: &'a RecordingSleeper,
    cancel: CancelToken,
    timeout: Duration,
) -> DriveClient<'a, ReqwestTransport> {
    DriveClient {
        api_base: base.to_owned(),
        upload_base: base.to_owned(),
        oauth_base: base.to_owned(),
        auth_base: "https://accounts.google.com".to_owned(),
        transport: ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap(),
        cancel,
        sleeper,
        request_timeout: timeout,
    }
}

#[test]
fn each_binding_gets_its_own_private_state_dir_and_legacy_base_moves_once() {
    let home = TestVault::new("state-home");
    let sync_home = home.state.join("sync");
    std::fs::create_dir_all(&sync_home).unwrap();
    let first = BindingId::fresh();
    let second = BindingId::fresh();
    // An earlier build kept one shared base for whichever binding ran last.
    std::fs::write(
        sync_home.join("base.json"),
        json!({"schema": crate::SCHEMA_VERSION, "binding_id": first.as_str(), "provider": "github", "head": "c1", "entries": {}})
            .to_string(),
    )
    .unwrap();
    std::fs::write(sync_home.join("github-blob-cache.json"), "{}").unwrap();

    let other = crate::engine::binding_state_dir(&sync_home, &second).unwrap();
    assert!(
        sync_home.join("base.json").exists(),
        "another binding's base must not move"
    );
    assert!(!other.join("base.json").exists());

    let mine = crate::engine::binding_state_dir(&sync_home, &first).unwrap();
    assert_ne!(mine, other);
    assert!(mine.join("base.json").exists());
    assert!(mine.join("github-blob-cache.json").exists());
    assert!(!sync_home.join("base.json").exists());
    assert!(mine.starts_with(sync_home.join("bindings")));

    // Idempotent: a second call neither moves nor loses anything.
    assert_eq!(
        crate::engine::binding_state_dir(&sync_home, &first).unwrap(),
        mine
    );
    assert!(mine.join("base.json").exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for dir in [&sync_home, &mine, &other] {
            let mode = std::fs::metadata(dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", dir.display());
        }
    }
}

#[test]
fn two_bindings_sync_without_sharing_a_base() {
    let store = Arc::new(Mutex::new(git_store()));
    let server = git_server(Arc::clone(&store));
    let (credentials, first) = connected(ProviderKind::GitHub);
    let second = BindingId::fresh();
    credentials
        .put(
            &CredentialKey {
                provider: ProviderKind::GitHub,
                binding: second.clone(),
            },
            &credentials
                .get(&CredentialKey {
                    provider: ProviderKind::GitHub,
                    binding: first.clone(),
                })
                .unwrap()
                .unwrap(),
        )
        .unwrap();
    let engine = github_engine(&server, &credentials);
    let home = TestVault::new("two-bindings");
    let sync_home = home.state.join("sync");
    let one = TestVault::new("two-bindings-one");
    let two = TestVault::new("two-bindings-two");
    std::fs::write(one.root.join("one.md"), "1").unwrap();
    std::fs::write(two.root.join("two.md"), "2").unwrap();
    let mut a = one.request(ProviderKind::GitHub, &first, "abcde00000000061");
    a.state_dir = crate::engine::binding_state_dir(&sync_home, &first).unwrap();
    let mut b = two.request(ProviderKind::GitHub, &second, "abcde00000000062");
    b.github = Some(GitHubScope {
        branch: "other-vault".to_owned(),
        ..scope()
    });
    b.state_dir = crate::engine::binding_state_dir(&sync_home, &second).unwrap();
    succeeded(run_sync(&engine, &a).unwrap());
    succeeded(run_sync(&engine, &b).unwrap());
    let base_a = std::fs::read_to_string(a.state_dir.join("base.json")).unwrap();
    let base_b = std::fs::read_to_string(b.state_dir.join("base.json")).unwrap();
    assert!(base_a.contains(first.as_str()) && base_a.contains("one.md"));
    assert!(base_b.contains(second.as_str()) && base_b.contains("two.md"));
}

fn expiring_token(
    provider: ProviderKind,
    expires_unix_ms: u64,
) -> (MemoryCredentialStore, BindingId, CredentialKey) {
    let (credentials, binding) = connected(provider);
    let key = CredentialKey {
        provider,
        binding: binding.clone(),
    };
    credentials
        .put(
            &key,
            &TokenSet {
                access: Secret::new("old-access"),
                refresh: Some(Secret::new("refresh-1")),
                expires_unix_ms: Some(expires_unix_ms),
                account: "octocat".to_owned(),
            },
        )
        .unwrap();
    (credentials, binding, key)
}

#[test]
fn github_token_near_expiry_is_refreshed_and_the_rotated_pair_is_stored() {
    let mut git = git_store();
    git.require_token = Some("new-access".to_owned());
    git.refresh_reply = json!({"access_token": "new-access", "refresh_token": "refresh-2", "expires_in": 28800, "refresh_token_expires_in": 15_897_600});
    let store = Arc::new(Mutex::new(git));
    let server = git_server(Arc::clone(&store));
    let now = crate::secret::now_unix_ms();
    // Expires in one minute: inside the five-minute margin.
    let (credentials, binding, key) = expiring_token(ProviderKind::GitHub, now + 60_000);
    let engine = github_engine(&server, &credentials);
    let device = TestVault::new("gh-refresh");
    std::fs::write(device.root.join("a.md"), "hello").unwrap();
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GitHub, &binding, "abcde00000000071"),
        )
        .unwrap(),
    );
    assert_eq!(store.lock().unwrap().refreshes, 1);
    let stored = credentials.get(&key).unwrap().unwrap();
    assert_eq!(stored.access.expose(), "new-access");
    assert_eq!(
        stored.refresh.as_ref().map(Secret::expose),
        Some("refresh-2")
    );
    assert_eq!(stored.account, "octocat");
    let at = stored.expires_unix_ms.unwrap();
    assert!(
        at >= now + 28_800_000 && at < now + 28_800_000 + 60_000,
        "expiry must be absolute: {at} vs now {now}"
    );
    let body = server
        .recorded()
        .into_iter()
        .find(|request| request.path.ends_with("/login/oauth/access_token"))
        .unwrap()
        .body_str();
    assert!(body.contains("grant_type=refresh_token") && body.contains("refresh_token=refresh-1"));
    assert!(body.contains("client_id=Iv1.testclient"));

    // A fresh token is used as is.
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GitHub, &binding, "abcde00000000072"),
        )
        .unwrap(),
    );
    assert_eq!(store.lock().unwrap().refreshes, 1);
}

#[test]
fn github_401_refreshes_once_then_retries() {
    let mut git = git_store();
    git.require_token = Some("new-access".to_owned());
    git.refresh_reply =
        json!({"access_token": "new-access", "refresh_token": "refresh-2", "expires_in": 28800});
    let store = Arc::new(Mutex::new(git));
    let server = git_server(Arc::clone(&store));
    // The stored expiry says hours left, but the server revoked the token.
    let (credentials, binding, key) = expiring_token(
        ProviderKind::GitHub,
        crate::secret::now_unix_ms() + 3_600_000,
    );
    let engine = github_engine(&server, &credentials);
    let device = TestVault::new("gh-401");
    std::fs::write(device.root.join("a.md"), "hello").unwrap();
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GitHub, &binding, "abcde00000000073"),
        )
        .unwrap(),
    );
    assert_eq!(store.lock().unwrap().refreshes, 1);
    assert_eq!(
        credentials.get(&key).unwrap().unwrap().access.expose(),
        "new-access"
    );

    // A refresh that does not help is reported, not looped.
    store.lock().unwrap().require_token = Some("never".to_owned());
    let error = run_sync(
        &engine,
        &device.request(ProviderKind::GitHub, &binding, "abcde00000000074"),
    )
    .unwrap_err();
    assert!(matches!(error, SyncError::Auth { .. }), "{error}");
    assert_eq!(store.lock().unwrap().refreshes, 2);
}

#[test]
fn github_bad_refresh_token_is_auth_and_keeps_the_stored_pair() {
    let mut git = git_store();
    git.refresh_reply = json!({"error": "bad_refresh_token", "error_description": "The refresh token passed is incorrect or expired."});
    let store = Arc::new(Mutex::new(git));
    let server = git_server(Arc::clone(&store));
    let (credentials, binding, key) = expiring_token(ProviderKind::GitHub, 1);
    let engine = github_engine(&server, &credentials);
    let device = TestVault::new("gh-bad-refresh");
    let error = run_sync(
        &engine,
        &device.request(ProviderKind::GitHub, &binding, "abcde00000000075"),
    )
    .unwrap_err();
    assert!(matches!(error, SyncError::Auth { .. }), "{error}");
    let stored = credentials.get(&key).unwrap().unwrap();
    assert_eq!(
        stored.refresh.as_ref().map(Secret::expose),
        Some("refresh-1")
    );
}

#[test]
fn drive_refresh_keeps_the_refresh_token_google_did_not_resend() {
    let drive = Arc::new(Mutex::new(DriveStore::default()));
    drive.lock().unwrap().token_reply = Some((
        200,
        json!({"access_token": "new-access", "expires_in": 3599, "scope": drive::DRIVE_SCOPE, "token_type": "Bearer"}),
    ));
    let server = drive_server(Arc::clone(&drive));
    let (credentials, binding, key) = expiring_token(ProviderKind::GoogleDrive, 1);
    let engine = drive_engine(&server, &credentials);
    let device = TestVault::new("drive-refresh");
    std::fs::write(device.root.join("a.md"), "hello").unwrap();
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000076"),
        )
        .unwrap(),
    );
    let stored = credentials.get(&key).unwrap().unwrap();
    assert_eq!(stored.access.expose(), "new-access");
    let bodies = drive.lock().unwrap().token_bodies.clone();
    assert_eq!(bodies.len(), 1);
    let pairs: BTreeMap<String, String> = form_urlencoded::parse(bodies[0].as_bytes())
        .into_owned()
        .collect();
    assert_eq!(pairs["grant_type"], "refresh_token");
    assert_eq!(
        pairs.get("client_secret").map(String::as_str),
        Some("test-only-google-secret"),
        "Google refuses a Desktop refresh without the client secret"
    );
    assert_eq!(
        stored.refresh.as_ref().map(Secret::expose),
        Some("refresh-1")
    );
    assert!(stored.expires_unix_ms.unwrap() > crate::secret::now_unix_ms() + 3_000_000);

    drive.lock().unwrap().token_reply = Some((400, json!({"error": "invalid_grant"})));
    credentials
        .put(
            &key,
            &TokenSet {
                expires_unix_ms: Some(1),
                ..stored
            },
        )
        .unwrap();
    let error = run_sync(
        &engine,
        &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000077"),
    )
    .unwrap_err();
    assert!(matches!(error, SyncError::Auth { .. }), "{error}");
}

#[test]
fn drive_store_lives_in_app_data_and_store_names_do_not_mix() {
    let drive = Arc::new(Mutex::new(DriveStore::default()));
    let server = drive_server(Arc::clone(&drive));
    let sleeper = RecordingSleeper::default();
    let client = drive_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let notes = drive_scope();
    let work = DriveScope {
        store: "Work".to_owned(),
    };
    for (scope, text) in [(&notes, "notes"), (&work, "work")] {
        let (snapshot, bytes) = snap_text(&[("a.md", text)]);
        let remote = client.fetch(scope, &token()).unwrap();
        client
            .publish(scope, &token(), &remote, &snapshot, &[], &[], &bytes, text)
            .unwrap();
    }
    for (scope, text) in [(&notes, "notes"), (&work, "work")] {
        let remote = client.fetch(scope, &token()).unwrap();
        let merged = drive::merge_heads(&remote).unwrap();
        assert_eq!(merged.snapshot.files.len(), 1);
        assert_eq!(
            merged
                .snapshot
                .hash_of(&RelativePath::parse("a.md").unwrap()),
            Some(ContentHash::of(text.as_bytes()))
        );
    }
    let store = drive.lock().unwrap();
    let roots: Vec<_> = store
        .files
        .iter()
        .filter(|file| file.role.as_deref() == Some("root"))
        .collect();
    assert_eq!(roots.len(), 2);
    assert!(roots.iter().all(|root| root.parent == drive::APP_DATA));
    assert!(roots.iter().any(|root| root.name == "muninn-sync-Notes"));
    drop(store);

    let pkce = drive::new_pkce("http://127.0.0.1:9/callback");
    let url = client
        .authorization_url("client.apps.googleusercontent.com", &pkce)
        .unwrap();
    assert!(url.contains("drive.appdata"), "{url}");
    assert!(!url.contains("drive.file"), "{url}");
    assert!(
        DriveScope {
            store: "../x".to_owned()
        }
        .validate()
        .is_err()
    );
    assert!(
        DriveScope {
            store: String::new()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn drive_two_devices_creating_the_store_at_once_do_not_split_it() {
    let drive = Arc::new(Mutex::new(DriveStore::default()));
    let server = drive_server(Arc::clone(&drive));
    // The other device creates the store and its first commit; it lands
    // after this device has also created its own root.
    stage_drive_racer(&drive, &server, &[("racer.md", "from racer")]);
    let (credentials, binding) = connected(ProviderKind::GoogleDrive);
    let engine = drive_engine(&server, &credentials);
    let device = TestVault::new("drive-split");
    std::fs::write(device.root.join("a.md"), "hello").unwrap();
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000081"),
        )
        .unwrap(),
    );
    {
        let mut store = drive.lock().unwrap();
        let landed = std::mem::take(&mut store.pending);
        store.files.extend(landed);
        let roots = store
            .files
            .iter()
            .filter(|file| file.role.as_deref() == Some("root"))
            .count();
        assert_eq!(roots, 2, "both devices created a root");
    }
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000082"),
        )
        .unwrap(),
    );
    assert_eq!(device.read("racer.md").as_deref(), Some("from racer"));
    assert_eq!(device.read("a.md").as_deref(), Some("hello"));
    let sleeper = RecordingSleeper::default();
    let client = drive_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    assert_eq!(
        remote.heads.len(),
        1,
        "the merge commit joins both roots' heads"
    );
}

/// C1: the same note edited on two devices between syncs. Without a choice
/// the run stops on the conflict and writes nothing; with the app's default
/// ("keep both") it continues, keeps this device's text at the path, writes
/// the other device's text as `<stem> (remote)<ext>`, names the copy, and the
/// next syncs on both devices are clean.
fn same_note_edit_on_two_devices_keeps_both(
    engine: &EngineParts<ReqwestTransport, &MemoryCredentialStore>,
    provider: ProviderKind,
    binding: &BindingId,
    tag: &str,
    ops: [&str; 8],
) {
    let one = TestVault::new(&format!("{tag}-c1-one"));
    let two = TestVault::new(&format!("{tag}-c1-two"));
    let keep_both = |vault: &TestVault, op: &str| {
        let mut request = vault.request(provider, binding, op);
        request.default_resolution = Some(Resolution::KeepBoth);
        request
    };
    std::fs::write(one.root.join("note.md"), "base").unwrap();
    std::fs::write(one.root.join("other.md"), "untouched").unwrap();
    succeeded(run_sync(engine, &keep_both(&one, ops[0])).unwrap());
    succeeded(run_sync(engine, &keep_both(&two, ops[1])).unwrap());
    assert_eq!(two.read("note.md").as_deref(), Some("base"));

    // Device one edits and syncs; device two edits the same note offline.
    std::fs::write(one.root.join("note.md"), "edit from one").unwrap();
    succeeded(run_sync(engine, &keep_both(&one, ops[2])).unwrap());
    std::fs::write(two.root.join("note.md"), "edit from two").unwrap();

    // With no choice at all the run still stops and touches nothing.
    let report = run_sync(engine, &two.request(provider, binding, ops[3])).unwrap();
    let SyncReport::Conflicts { conflicts } = report else {
        panic!("expected a conflict, got {report:?}");
    };
    assert_eq!(conflicts.len(), 1);
    assert_eq!(two.read("note.md").as_deref(), Some("edit from two"));
    assert_eq!(two.read("note (remote).md"), None);

    // The app's request: the empty choice list no longer leaves it stuck.
    let request = keep_both(&two, ops[4]);
    assert!(request.resolutions.is_empty());
    let report = run_sync(engine, &request).unwrap();
    let SyncReport::Succeeded {
        conflict_copies, ..
    } = &report
    else {
        panic!("expected success, got {report:?}");
    };
    assert_eq!(
        conflict_copies,
        &vec![RelativePath::parse("note (remote).md").unwrap()]
    );
    assert_eq!(two.read("note.md").as_deref(), Some("edit from two"));
    assert_eq!(
        two.read("note (remote).md").as_deref(),
        Some("edit from one")
    );
    assert_eq!(two.read("other.md").as_deref(), Some("untouched"));

    // Device one pulls both versions; then both devices are clean.
    succeeded(run_sync(engine, &keep_both(&one, ops[5])).unwrap());
    assert_eq!(one.read("note.md").as_deref(), Some("edit from two"));
    assert_eq!(
        one.read("note (remote).md").as_deref(),
        Some("edit from one")
    );
    for (vault, op) in [(&two, ops[6]), (&one, ops[7])] {
        let report = run_sync(engine, &keep_both(vault, op)).unwrap();
        assert!(
            matches!(&report, SyncReport::Succeeded { conflict_copies, .. } if conflict_copies.is_empty()),
            "{report:?}"
        );
    }
}

#[test]
fn github_same_note_edit_on_two_devices_keeps_both_and_does_not_stall() {
    let store = Arc::new(Mutex::new(git_store()));
    let server = git_server(Arc::clone(&store));
    let (credentials, binding) = connected(ProviderKind::GitHub);
    let engine = github_engine(&server, &credentials);
    same_note_edit_on_two_devices_keeps_both(
        &engine,
        ProviderKind::GitHub,
        &binding,
        "gh",
        [
            "abcde000000000c1",
            "abcde000000000c2",
            "abcde000000000c3",
            "abcde000000000c4",
            "abcde000000000c5",
            "abcde000000000c6",
            "abcde000000000c7",
            "abcde000000000c8",
        ],
    );
}

#[test]
fn drive_same_note_edit_on_two_devices_keeps_both_and_does_not_stall() {
    let drive = Arc::new(Mutex::new(DriveStore::default()));
    let server = drive_server(Arc::clone(&drive));
    let (credentials, binding) = connected(ProviderKind::GoogleDrive);
    let engine = drive_engine(&server, &credentials);
    same_note_edit_on_two_devices_keeps_both(
        &engine,
        ProviderKind::GoogleDrive,
        &binding,
        "drive",
        [
            "abcde000000000d1",
            "abcde000000000d2",
            "abcde000000000d3",
            "abcde000000000d4",
            "abcde000000000d5",
            "abcde000000000d6",
            "abcde000000000d7",
            "abcde000000000d8",
        ],
    );
}

/// The texts of every note in a vault, by file name.
fn notes(vault: &TestVault) -> BTreeMap<String, String> {
    std::fs::read_dir(&vault.root)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().unwrap().is_file())
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read_to_string(entry.path()).unwrap(),
            )
        })
        .collect()
}

/// R1: device one renames c.md -> c2.md while device two creates its own new
/// c2.md. Whichever syncs first, neither note may be overwritten: both
/// devices end with `c2.md` and `c2 (remote).md` holding both texts, the
/// status names the copy, and the next syncs are clean.
fn rename_onto_a_new_note_keeps_both(
    engine: &EngineParts<ReqwestTransport, &MemoryCredentialStore>,
    provider: ProviderKind,
    binding: &BindingId,
    tag: &str,
    renamer_syncs_first: bool,
    ops: [&str; 7],
) {
    let one = TestVault::new(&format!("{tag}-r1-one"));
    let two = TestVault::new(&format!("{tag}-r1-two"));
    let run = |vault: &TestVault, op: &str| {
        let mut request = vault.request(provider, binding, op);
        request.default_resolution = Some(Resolution::KeepBoth);
        run_sync(engine, &request).unwrap()
    };
    std::fs::write(one.root.join("c.md"), "renamed note").unwrap();
    succeeded(run(&one, ops[0]));
    succeeded(run(&two, ops[1]));
    std::fs::rename(one.root.join("c.md"), one.root.join("c2.md")).unwrap();
    std::fs::write(two.root.join("c2.md"), "new note on two").unwrap();

    let (first, second) = if renamer_syncs_first {
        (&one, &two)
    } else {
        (&two, &one)
    };
    succeeded(run(first, ops[2]));
    let report = run(second, ops[3]);
    let SyncReport::Succeeded {
        conflict_copies, ..
    } = &report
    else {
        panic!("expected success, got {report:?}");
    };
    assert_eq!(
        conflict_copies,
        &vec![RelativePath::parse("c2 (remote).md").unwrap()],
        "the copy must be named in the status"
    );
    // The device that synced second keeps its own text at c2.md.
    let own = if renamer_syncs_first {
        "new note on two"
    } else {
        "renamed note"
    };
    assert_eq!(second.read("c2.md").as_deref(), Some(own));
    succeeded(run(first, ops[4]));
    let expect: BTreeSet<&str> = ["renamed note", "new note on two"].into();
    for vault in [&one, &two] {
        let found = notes(vault);
        assert_eq!(
            found.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            ["c2 (remote).md", "c2.md"].into(),
            "{found:?}"
        );
        assert_eq!(
            found.values().map(String::as_str).collect::<BTreeSet<_>>(),
            expect,
            "{found:?}"
        );
    }
    assert_eq!(notes(&one), notes(&two));
    for (vault, op) in [(&one, ops[5]), (&two, ops[6])] {
        let report = run(vault, op);
        assert!(
            matches!(&report, SyncReport::Succeeded { conflict_copies, .. } if conflict_copies.is_empty()),
            "{report:?}"
        );
    }
}

#[test]
fn github_rename_onto_a_note_the_other_device_created_keeps_both() {
    for (renamer_first, ops) in [
        (
            false,
            [
                "abcde000000000e1",
                "abcde000000000e2",
                "abcde000000000e3",
                "abcde000000000e4",
                "abcde000000000e5",
                "abcde000000000e6",
                "abcde000000000e7",
            ],
        ),
        (
            true,
            [
                "abcde000000000f1",
                "abcde000000000f2",
                "abcde000000000f3",
                "abcde000000000f4",
                "abcde000000000f5",
                "abcde000000000f6",
                "abcde000000000f7",
            ],
        ),
    ] {
        let store = Arc::new(Mutex::new(git_store()));
        let server = git_server(Arc::clone(&store));
        let (credentials, binding) = connected(ProviderKind::GitHub);
        let engine = github_engine(&server, &credentials);
        rename_onto_a_new_note_keeps_both(
            &engine,
            ProviderKind::GitHub,
            &binding,
            if renamer_first { "gh-a" } else { "gh-b" },
            renamer_first,
            ops,
        );
    }
}

#[test]
fn drive_rename_onto_a_note_the_other_device_created_keeps_both() {
    for (renamer_first, ops) in [
        (
            false,
            [
                "abcde000000001e1",
                "abcde000000001e2",
                "abcde000000001e3",
                "abcde000000001e4",
                "abcde000000001e5",
                "abcde000000001e6",
                "abcde000000001e7",
            ],
        ),
        (
            true,
            [
                "abcde000000001f1",
                "abcde000000001f2",
                "abcde000000001f3",
                "abcde000000001f4",
                "abcde000000001f5",
                "abcde000000001f6",
                "abcde000000001f7",
            ],
        ),
    ] {
        let drive = Arc::new(Mutex::new(DriveStore::default()));
        let server = drive_server(Arc::clone(&drive));
        let (credentials, binding) = connected(ProviderKind::GoogleDrive);
        let engine = drive_engine(&server, &credentials);
        rename_onto_a_new_note_keeps_both(
            &engine,
            ProviderKind::GoogleDrive,
            &binding,
            if renamer_first { "drive-a" } else { "drive-b" },
            renamer_first,
            ops,
        );
    }
}

/// After an S1 merge, the device whose head lost the path (its edit became
/// the `(conflict <id>)` copy) syncs with its own base. Both edits must
/// survive on its disk and on the remote, and the store ends on one head.
#[test]
fn drive_losing_head_device_keeps_both_edits_after_a_head_merge() {
    let drive = Arc::new(Mutex::new(DriveStore::default()));
    let server = drive_server(Arc::clone(&drive));
    let (credentials, binding) = connected(ProviderKind::GoogleDrive);
    let engine = drive_engine(&server, &credentials);
    let device = TestVault::new("drive-loser");
    std::fs::write(device.root.join("a.md"), "base").unwrap();
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000a01"),
        )
        .unwrap(),
    );
    // Another device's edit is written first (so its head id sorts first and
    // wins the path) but held back until this device has published its own.
    stage_drive_racer(&drive, &server, &[("a.md", "from other")]);
    std::fs::write(device.root.join("a.md"), "from this device").unwrap();
    succeeded(
        run_sync(
            &engine,
            &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000a02"),
        )
        .unwrap(),
    );
    {
        let mut store = drive.lock().unwrap();
        let landed = std::mem::take(&mut store.pending);
        store.files.extend(landed);
    }
    let sleeper = RecordingSleeper::default();
    let client = drive_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    assert_eq!(remote.heads.len(), 2, "{:?}", remote.heads);
    let merged = drive::merge_heads(&remote).unwrap();
    assert_eq!(
        merged.snapshot.hash_of(&path("a.md")),
        Some(ContentHash::of(b"from other")),
        "this device's head must be the one that loses the path"
    );

    // This device still has base {a.md: from this device}.
    let report = run_sync(
        &engine,
        &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000a03"),
    )
    .unwrap();
    let SyncReport::Succeeded {
        conflict_copies, ..
    } = &report
    else {
        panic!("expected success, got {report:?}");
    };
    assert_eq!(conflict_copies.len(), 1, "{conflict_copies:?}");
    let copy = conflict_copies[0].clone();
    assert_eq!(device.read("a.md").as_deref(), Some("from other"));
    assert_eq!(
        device.read(copy.as_str()).as_deref(),
        Some("from this device")
    );
    let remote = client.fetch(&drive_scope(), &token()).unwrap();
    assert_eq!(remote.heads.len(), 1, "{:?}", remote.heads);
    let tip = drive::merge_heads(&remote).unwrap().snapshot;
    assert_eq!(
        tip.hash_of(&path("a.md")),
        Some(ContentHash::of(b"from other"))
    );
    assert_eq!(
        tip.hash_of(&copy),
        Some(ContentHash::of(b"from this device"))
    );
    let again = run_sync(
        &engine,
        &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000a04"),
    )
    .unwrap();
    assert!(
        matches!(&again, SyncReport::Succeeded { conflict_copies, .. } if conflict_copies.is_empty()),
        "{again:?}"
    );
}

#[test]
fn drive_refresh_without_a_google_client_fails_closed_and_sends_nothing() {
    let drive = Arc::new(Mutex::new(DriveStore::default()));
    drive.lock().unwrap().token_reply = Some((
        200,
        json!({"access_token": "new-access", "expires_in": 3599, "scope": drive::DRIVE_SCOPE}),
    ));
    let server = drive_server(Arc::clone(&drive));
    let (credentials, binding, _key) = expiring_token(ProviderKind::GoogleDrive, 1);
    // No secret in this build, or the person set their own client id.
    let engine = EngineParts {
        google_client: None,
        ..drive_engine(&server, &credentials)
    };
    let device = TestVault::new("drive-refresh-no-secret");
    std::fs::write(device.root.join("a.md"), "hello").unwrap();
    let error = run_sync(
        &engine,
        &device.request(ProviderKind::GoogleDrive, &binding, "abcde00000000078"),
    )
    .unwrap_err();
    assert!(matches!(error, SyncError::Config { .. }), "{error}");
    assert_eq!(drive.lock().unwrap().refreshes, 0, "no token request");
}

//! Loopback tests for the production GitHub and Drive adapters.
//! The HTTP client is reqwest. Only the provider is scripted.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::credentials::{CredentialKey, CredentialStore, MemoryCredentialStore};
use crate::drive::{self, DriveClient, DriveCommitDoc, DriveScope};
use crate::engine::{EngineParts, RunRequest, run_sync};
use crate::error::SyncError;
use crate::github::{DevicePoll, GitHubClient, GitHubRemote, GitHubScope, git_blob_sha};
use crate::http::{CancelToken, RecordingSleeper, ReqwestTransport};
use crate::model::{
    BindingId, ContentHash, FileKind, FileRecord, OperationId, ProviderKind, Snapshot,
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
}

fn git_server(store: Arc<Mutex<GitStore>>) -> ScriptedServer {
    ScriptedServer::spawn(move |request| {
        let path = request.path.split('?').next().unwrap_or(&request.path).to_owned();
        let mut git = store.lock().expect("git");
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
                git.patch_races -= 1;
                let current = git.refs.get(&branch).cloned().unwrap_or_default();
                let tree = git
                    .commits
                    .get(&current)
                    .and_then(|commit| commit.get("tree"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_owned();
                let other = format!("race{}", git.commits.len() + 1);
                git.commits.insert(
                    other.clone(),
                    json!({"message": "other writer", "tree": tree, "parents": [current]}),
                );
                git.refs.insert(branch, other);
                return json_reply(422, json!({"message": "update is not a fast-forward"}));
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
            if sha.ends_with("truncated") {
                return json_reply(200, json!({"sha": sha, "truncated": true, "tree": []}));
            }
            let Some(entries) = git.trees.get(sha).cloned() else {
                return json_reply(404, json!({"message": "not found"}));
            };
            return json_reply(200, json!({"sha": sha, "truncated": false, "tree": entries}));
        }
        if request.method == "GET" && path.contains("/git/blobs/") {
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
fn github_non_fast_forward_is_retried_against_the_new_tip() {
    let store = Arc::new(Mutex::new(GitStore {
        blobs: BTreeMap::new(),
        trees: BTreeMap::new(),
        commits: BTreeMap::new(),
        refs: BTreeMap::new(),
        patch_races: 0,
        protected: false,
    }));
    let server = git_server(Arc::clone(&store));
    let sleeper = RecordingSleeper::default();
    let client = github_client(
        &server.base(),
        &sleeper,
        CancelToken::new(),
        Duration::from_secs(2),
    );
    let (snapshot, bytes) = snap_text(&[("a.md", "alpha")]);
    let empty = GitHubRemote {
        commit: None,
        tree: None,
        snapshot: Snapshot::default(),
        blobs: BTreeMap::new(),
    };
    let head = client
        .publish(&scope(), &token(), &empty, &snapshot, &bytes, "op-base")
        .unwrap();
    store.lock().unwrap().patch_races = 1;
    let (changed, bytes) = snap_text(&[("a.md", "beta")]);
    let fetched = client.fetch(&scope(), &token()).unwrap();
    assert_eq!(fetched.commit.as_deref(), Some(head.as_str()));
    let raced = client
        .publish(&scope(), &token(), &fetched, &changed, &bytes, "op-race")
        .unwrap();
    let fetched = client.fetch(&scope(), &token()).unwrap();
    assert_eq!(fetched.commit.as_deref(), Some(raced.as_str()));
    assert_eq!(store.lock().unwrap().patch_races, 0);
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
    let scope = DriveScope {
        folder_id: "folderFolder1".to_owned(),
        folder_name: "Notes".to_owned(),
    };
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
        operation: OperationId::parse("abcde00000000001").unwrap(),
        protected: Default::default(),
        resolutions: Vec::new(),
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
        operation: OperationId::parse("abcde00000000002").unwrap(),
        protected: Default::default(),
        resolutions: Vec::new(),
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

#[derive(Default)]
struct DriveStore {
    files: Vec<DriveFile>,
    next_id: u64,
    incomplete: bool,
    sessions: BTreeMap<String, DriveFile>,
}

struct DriveFile {
    id: String,
    name: String,
    mime: String,
    parent: String,
    role: Option<String>,
    bytes: Vec<u8>,
}

fn drive_server(store: Arc<Mutex<DriveStore>>) -> ScriptedServer {
    {
        let mut drive = store.lock().unwrap();
        drive.files.push(DriveFile {
            id: "folderFolder1".to_owned(),
            name: "Notes".to_owned(),
            mime: "application/vnd.google-apps.folder".to_owned(),
            parent: "root".to_owned(),
            role: None,
            bytes: Vec::new(),
        });
    }
    let port = Arc::new(std::sync::atomic::AtomicU16::new(0));
    let port_for_handler = Arc::clone(&port);
    let server = ScriptedServer::spawn(move |request| {
        let mut drive = store.lock().expect("drive");
        let bound = port_for_handler.load(std::sync::atomic::Ordering::SeqCst);
        if drive.incomplete && request.path.contains("/drive/v3/files?") {
            return json_reply(200, json!({"incompleteSearch": true, "files": []}));
        }
        if request.path.starts_with("/drive/v3/files/generateIds") {
            drive.next_id += 1;
            let id = format!("generated{:08}", drive.next_id);
            return json_reply(200, json!({"ids": [id], "space": "drive"}));
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
        if request.method == "GET"
            && (request.path.contains("fields=id,name,mimeType") || request.path.contains("/drive/v3/files/folderFolder1"))
        {
            return json_reply(
                200,
                json!({
                    "id": "folderFolder1",
                    "name": "Notes",
                    "mimeType": "application/vnd.google-apps.folder",
                    "trashed": false,
                    "capabilities": {"canAddChildren": true}
                }),
            );
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
        transport: ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap(),
        cancel,
        sleeper,
        request_timeout: timeout,
    }
}

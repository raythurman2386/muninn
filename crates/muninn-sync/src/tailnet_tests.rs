//! Loopback tests for the Tailscale store: the real host server, a real
//! reqwest peer, and the engine on synthetic vaults. No tailnet is involved.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::credentials::{CredentialKey, CredentialStore, MemoryCredentialStore};
use crate::engine::{EngineParts, RunRequest, SyncReport, run_sync};
use crate::error::SyncError;
use crate::http::{
    CancelToken, HttpRequest, HttpTransport, RecordingSleeper, ReqwestTransport, ensure_allowed,
};
use crate::model::{BindingId, ContentHash, OperationId, ProviderKind, Resolution};
use crate::pathutil::RelativePath;
use crate::secret::{Secret, TokenSet};
use crate::tailnet::{
    self, CommitStore, DirStore, HttpStore, TailnetHost, TailnetHostConfig, TailnetRole,
    TailnetScope, validate_tailnet_host,
};

const KEY: &str = "k3y-for-tests-only-0123456789abcdefABCDEF";

struct Dirs {
    base: PathBuf,
}

impl Dirs {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "muninn-tailnet-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&base).unwrap();
        Self { base }
    }

    fn dir(&self, name: &str) -> PathBuf {
        let path = self.base.join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn host(store: &Path, allowed: &[&str]) -> TailnetHost {
    TailnetHost::start(TailnetHostConfig {
        port: 0,
        store_dir: store.to_path_buf(),
        key: Secret::new(KEY),
        allowed_logins: allowed.iter().map(|s| (*s).to_owned()).collect(),
    })
    .unwrap()
}

fn base(host: &TailnetHost) -> String {
    format!("http://{}", host.addr())
}

fn engine(
    credentials: &MemoryCredentialStore,
    api: Option<String>,
) -> EngineParts<ReqwestTransport, &MemoryCredentialStore> {
    EngineParts {
        transport: ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap(),
        credentials,
        cancel: CancelToken::new(),
        sleeper: Box::new(RecordingSleeper::default()),
        github_api: String::new(),
        github_oauth: String::new(),
        drive_api: String::new(),
        drive_upload: String::new(),
        drive_oauth: String::new(),
        tailnet_api: api,
    }
}

fn bind(credentials: &MemoryCredentialStore) -> BindingId {
    let binding = BindingId::fresh();
    credentials
        .put(
            &CredentialKey {
                provider: ProviderKind::Tailscale,
                binding: binding.clone(),
            },
            &TokenSet {
                access: Secret::new(KEY),
                refresh: None,
                expires_unix_ms: None,
                account: String::new(),
            },
        )
        .unwrap();
    binding
}

fn request(
    root: &Path,
    state: &Path,
    binding: &BindingId,
    role: TailnetRole,
    op: &str,
) -> RunRequest {
    RunRequest {
        root: root.to_path_buf(),
        state_dir: state.to_path_buf(),
        binding: binding.clone(),
        provider: ProviderKind::Tailscale,
        github: None,
        drive: None,
        tailnet: Some(TailnetScope { role }),
        operation: OperationId::parse(op).unwrap(),
        protected: Default::default(),
        resolutions: Vec::new(),
    }
}

fn peer_role() -> TailnetRole {
    TailnetRole::Peer {
        host: "minis.tail1234.ts.net".to_owned(),
    }
}

fn http_store<'a>(
    base: &str,
    key: &'a Secret,
    sleeper: &'a RecordingSleeper,
) -> HttpStore<'a, ReqwestTransport> {
    HttpStore {
        base: base.to_owned(),
        transport: ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap(),
        key,
        cancel: CancelToken::new(),
        sleeper,
        request_timeout: Duration::from_secs(10),
    }
}

fn commit_bytes(id: &str, parents: &[&str], entries: &[(&str, &[u8])]) -> Vec<u8> {
    let entries: Vec<_> = entries
        .iter()
        .map(|(path, bytes)| {
            let sha = ContentHash::of(bytes).to_hex();
            serde_json::json!({"path": path, "sha256": sha, "size": bytes.len(), "blob_id": sha})
        })
        .collect();
    serde_json::json!({
        "schema": 1,
        "kind": "muninn-sync-commit",
        "id": id,
        "operation_id": format!("op-{id}"),
        "parents": parents,
        "entries": entries,
        "tombstones": [],
    })
    .to_string()
    .into_bytes()
}

#[test]
fn tailnet_host_names_and_allowlist() {
    assert!(validate_tailnet_host("minis.tail1234.ts.net").is_ok());
    for bad in [
        "minis",
        "minis.ts.net",
        "Minis.tail1234.ts.net",
        "minis.tail1234.ts.net.evil.com",
        "-x.tail1234.ts.net",
        "a b.tail1234.ts.net",
        "",
    ] {
        assert!(
            validate_tailnet_host(bad).is_err(),
            "{bad} should be refused"
        );
    }
    assert!(ensure_allowed("https://minis.tail1234.ts.net/muninn-sync/v1/state", &[]).is_ok());
    assert!(ensure_allowed("http://minis.tail1234.ts.net/muninn-sync/v1/state", &[]).is_err());
    assert!(ensure_allowed("https://ts.net.evil.example/x", &[]).is_err());
    assert!(ensure_allowed("https://100.64.0.1/x", &[]).is_err());
}

#[test]
fn tailnet_server_requires_the_key_and_optional_login() {
    let dirs = Dirs::new("auth");
    let server = host(&dirs.dir("store"), &["raymond@example.com"]);
    let transport = ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap();
    let send = |auth: Option<&str>, login: Option<&str>| {
        let mut headers = Vec::new();
        if let Some(auth) = auth {
            headers.push(("authorization".to_owned(), auth.to_owned()));
        }
        if let Some(login) = login {
            headers.push(("tailscale-user-login".to_owned(), login.to_owned()));
        }
        transport
            .execute(&HttpRequest {
                method: "GET".to_owned(),
                url: format!("{}/muninn-sync/v1/state", base(&server)),
                headers,
                body: Vec::new(),
                mutation: false,
                timeout: Duration::from_secs(10),
            })
            .unwrap()
            .status
    };
    assert_eq!(send(None, Some("raymond@example.com")), 401);
    assert_eq!(
        send(Some("Bearer wrong-key"), Some("raymond@example.com")),
        401
    );
    assert_eq!(send(Some(&format!("Bearer {KEY}")), None), 403);
    assert_eq!(
        send(Some(&format!("Bearer {KEY}")), Some("mallory@example.com")),
        403
    );
    assert_eq!(
        send(Some(&format!("Bearer {KEY}")), Some("Raymond@Example.com")),
        200
    );

    let wrong = Secret::new("wrong-key-wrong-key-wrong-key-wrong-key");
    let sleeper = RecordingSleeper::default();
    let peer = http_store(&base(&server), &wrong, &sleeper);
    assert!(matches!(peer.state(), Err(SyncError::Auth { .. })));
}

#[test]
fn tailnet_store_is_create_only_and_fast_forward_only() {
    let dirs = Dirs::new("immutable");
    let store_dir = dirs.dir("store");
    let server = host(&store_dir, &[]);
    let key = Secret::new(KEY);
    let sleeper = RecordingSleeper::default();
    let peer = http_store(&base(&server), &key, &sleeper);

    let bytes = b"hello";
    let sha = ContentHash::of(bytes).to_hex();
    assert!(matches!(
        peer.put_blob(&sha, b"other bytes"),
        Err(SyncError::Protocol { .. })
    ));
    peer.put_blob(&sha, bytes).unwrap();
    peer.put_blob(&sha, bytes).unwrap();
    assert_eq!(peer.blob(&sha).unwrap(), bytes);
    assert!(matches!(
        peer.blob(&ContentHash::of(b"absent").to_hex()),
        Err(SyncError::NotFound { .. })
    ));

    let first = commit_bytes("commit0001", &[], &[("a.md", bytes)]);
    peer.put_commit("commit0001", &first).unwrap();
    // Same bytes again: an idempotent retry after a lost response.
    peer.put_commit("commit0001", &first).unwrap();
    let changed = commit_bytes("commit0001", &[], &[]);
    assert!(matches!(
        peer.put_commit("commit0001", &changed),
        Err(SyncError::ConflictRemote { .. })
    ));
    // A second root, or a child of a stale head, would fork the store.
    let fork = commit_bytes("commit0002", &[], &[]);
    assert!(matches!(
        peer.put_commit("commit0002", &fork),
        Err(SyncError::ConflictRemote { .. })
    ));
    let child = commit_bytes("commit0003", &["commit0001"], &[]);
    peer.put_commit("commit0003", &child).unwrap();
    let stale = commit_bytes("commit0004", &["commit0001"], &[]);
    assert!(matches!(
        peer.put_commit("commit0004", &stale),
        Err(SyncError::ConflictRemote { .. })
    ));
    let missing_blob = commit_bytes(
        "commit0005",
        &["commit0003"],
        &[("b.md", b"never uploaded")],
    );
    assert!(matches!(
        peer.put_commit("commit0005", &missing_blob),
        Err(SyncError::Protocol { .. })
    ));
    let remote = tailnet::listing(&peer).unwrap();
    assert_eq!(remote.heads, vec!["commit0003".to_owned()]);
    // The host store on disk agrees with what the peer saw.
    let local = DirStore::new(store_dir.clone());
    assert_eq!(local.state().unwrap().commits.len(), 2);

    // Oversized and chunked bodies are refused before any bytes are stored.
    for head in [
        format!(
            "PUT /muninn-sync/v1/blobs/{sha} HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {KEY}\r\nContent-Length: 999999999\r\n\r\n"
        ),
        format!(
            "PUT /muninn-sync/v1/blobs/{sha} HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {KEY}\r\nTransfer-Encoding: chunked\r\n\r\n"
        ),
    ] {
        let mut stream = std::net::TcpStream::connect(server.addr()).unwrap();
        stream.write_all(head.as_bytes()).unwrap();
        let mut reply = String::new();
        let _ = stream.read_to_string(&mut reply);
        assert!(
            reply.starts_with("HTTP/1.1 413") || reply.starts_with("HTTP/1.1 411"),
            "{reply}"
        );
    }
}

#[test]
fn tailnet_engine_host_and_peer_converge_with_tombstones() {
    let dirs = Dirs::new("engine");
    let store_dir = dirs.dir("store");
    let server = host(&store_dir, &[]);
    let credentials = MemoryCredentialStore::default();
    let host_binding = bind(&credentials);
    let peer_binding = bind(&credentials);
    let host_vault = dirs.dir("host-vault");
    let host_state = dirs.dir("host-state");
    let peer_vault = dirs.dir("peer-vault");
    let peer_state = dirs.dir("peer-state");
    std::fs::create_dir_all(host_vault.join("notes")).unwrap();
    std::fs::write(host_vault.join("notes/a.md"), "from host").unwrap();
    std::fs::write(host_vault.join("old.md"), "delete me").unwrap();
    let host_role = TailnetRole::Host {
        store_dir: store_dir.clone(),
    };
    let host_engine = engine(&credentials, None);
    let peer_engine = engine(&credentials, Some(base(&server)));

    let report = run_sync(
        &host_engine,
        &request(
            &host_vault,
            &host_state,
            &host_binding,
            host_role.clone(),
            "abcd000000000001",
        ),
    )
    .unwrap();
    assert!(matches!(report, SyncReport::Succeeded { .. }));

    std::fs::write(peer_vault.join("peer.md"), "from peer").unwrap();
    let report = run_sync(
        &peer_engine,
        &request(
            &peer_vault,
            &peer_state,
            &peer_binding,
            peer_role(),
            "abcd000000000002",
        ),
    )
    .unwrap();
    assert!(matches!(report, SyncReport::Succeeded { .. }));
    assert_eq!(
        std::fs::read_to_string(peer_vault.join("notes/a.md")).unwrap(),
        "from host"
    );

    std::fs::remove_file(peer_vault.join("old.md")).unwrap();
    run_sync(
        &peer_engine,
        &request(
            &peer_vault,
            &peer_state,
            &peer_binding,
            peer_role(),
            "abcd000000000003",
        ),
    )
    .unwrap();
    run_sync(
        &host_engine,
        &request(
            &host_vault,
            &host_state,
            &host_binding,
            host_role,
            "abcd000000000004",
        ),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(host_vault.join("peer.md")).unwrap(),
        "from peer"
    );
    assert!(!host_vault.join("old.md").exists());
    let remote = tailnet::listing(&DirStore::new(store_dir)).unwrap();
    assert_eq!(remote.heads.len(), 1, "the store never forks");
    let head = &remote.commits[&remote.heads[0]];
    assert!(head.tombstones.is_empty() || head.tombstones.iter().all(|t| t.path == "old.md"));
}

#[test]
fn tailnet_stale_peer_is_refused_and_edit_conflicts_use_the_planner() {
    let dirs = Dirs::new("conflict");
    let store_dir = dirs.dir("store");
    let server = host(&store_dir, &[]);
    let credentials = MemoryCredentialStore::default();
    let a = bind(&credentials);
    let b = bind(&credentials);
    let (a_vault, a_state) = (dirs.dir("a-vault"), dirs.dir("a-state"));
    let (b_vault, b_state) = (dirs.dir("b-vault"), dirs.dir("b-state"));
    let peer = engine(&credentials, Some(base(&server)));
    std::fs::write(a_vault.join("n.md"), "base").unwrap();
    run_sync(
        &peer,
        &request(&a_vault, &a_state, &a, peer_role(), "abcd000000000011"),
    )
    .unwrap();
    run_sync(
        &peer,
        &request(&b_vault, &b_state, &b, peer_role(), "abcd000000000012"),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(b_vault.join("n.md")).unwrap(),
        "base"
    );

    // A publishes an edit; B, still on the old head, edits the same note.
    std::fs::write(a_vault.join("n.md"), "edit from a").unwrap();
    run_sync(
        &peer,
        &request(&a_vault, &a_state, &a, peer_role(), "abcd000000000013"),
    )
    .unwrap();
    std::fs::write(b_vault.join("n.md"), "edit from b").unwrap();

    // A publish that names the old head as parent is refused outright.
    let key = Secret::new(KEY);
    let sleeper = RecordingSleeper::default();
    let store = http_store(&base(&server), &key, &sleeper);
    let remote = tailnet::listing(&store).unwrap();
    let old_head = remote
        .commits
        .values()
        .find(|commit| commit.operation_id == "abcd000000000011")
        .unwrap()
        .id
        .clone();
    let mut desired = crate::model::Snapshot::default();
    let path = RelativePath::parse("n.md").unwrap();
    let bytes = b"edit from b".to_vec();
    desired.files.insert(
        path.clone(),
        crate::model::FileRecord {
            hash: ContentHash::of(&bytes),
            size: bytes.len() as u64,
            kind: crate::model::FileKind::Markdown,
        },
    );
    let local = std::iter::once((path.clone(), bytes)).collect();
    assert!(matches!(
        tailnet::publish(
            &store,
            &remote,
            &desired,
            &[old_head],
            &[],
            &local,
            "stale-op"
        ),
        Err(SyncError::ConflictRemote { .. })
    ));

    // The engine replans against the new head and reports a normal conflict.
    let report = run_sync(
        &peer,
        &request(&b_vault, &b_state, &b, peer_role(), "abcd000000000014"),
    )
    .unwrap();
    let SyncReport::Conflicts { conflicts } = report else {
        panic!("expected a conflict");
    };
    assert_eq!(conflicts.len(), 1);
    assert_eq!(
        std::fs::read_to_string(b_vault.join("n.md")).unwrap(),
        "edit from b"
    );

    let mut resolve = request(&b_vault, &b_state, &b, peer_role(), "abcd000000000015");
    resolve.resolutions = vec![(path, Resolution::KeepLocal)];
    assert!(matches!(
        run_sync(&peer, &resolve).unwrap(),
        SyncReport::Succeeded { .. }
    ));
    run_sync(
        &peer,
        &request(&a_vault, &a_state, &a, peer_role(), "abcd000000000016"),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(a_vault.join("n.md")).unwrap(),
        "edit from b"
    );
}

#[test]
fn tailnet_host_down_is_offline_and_leaves_the_vault_alone() {
    let dirs = Dirs::new("offline");
    let unused = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("http://{}", unused.local_addr().unwrap());
    drop(unused);
    let credentials = MemoryCredentialStore::default();
    let binding = bind(&credentials);
    let vault = dirs.dir("vault");
    let state = dirs.dir("state");
    std::fs::write(vault.join("keep.md"), "local only").unwrap();
    let peer = engine(&credentials, Some(dead));
    let error = run_sync(
        &peer,
        &request(&vault, &state, &binding, peer_role(), "abcd000000000021"),
    )
    .expect_err("host is down");
    assert!(matches!(error, SyncError::Offline { .. }), "{error:?}");
    assert_eq!(
        std::fs::read_to_string(vault.join("keep.md")).unwrap(),
        "local only"
    );
    assert_eq!(std::fs::read_dir(&vault).unwrap().count(), 1);
}

/// Two devices race through the engine: B lands a commit on the host after
/// A has read the heads and before A's commit arrives. The host refuses A's
/// commit (its parents are no longer the heads); A replans and keeps both.
#[test]
fn tailnet_two_device_race_through_the_engine_keeps_both_changes() {
    let dirs = Dirs::new("race");
    let store_dir = dirs.dir("store");
    let server = host(&store_dir, &[]);
    let host_base = base(&server);
    let b_vault = dirs.dir("b-vault");
    let b_state = dirs.dir("b-state");
    std::fs::write(b_vault.join("racer.md"), "from racer").unwrap();
    let fired = std::sync::Arc::new(std::sync::Mutex::new(false));
    let fired_in_proxy = std::sync::Arc::clone(&fired);
    let forward_to = host_base.clone();
    let proxy = crate::testkit::ScriptedServer::spawn(move |request| {
        if request.method == "PUT" && request.path.contains("/commits/") {
            let mut fired = fired_in_proxy.lock().unwrap();
            if !*fired {
                *fired = true;
                let credentials = MemoryCredentialStore::default();
                let b = bind(&credentials);
                let racer = engine(&credentials, Some(forward_to.clone()));
                let report = run_sync(
                    &racer,
                    &request_for(&b_vault, &b_state, &b, "abcd000000000092"),
                )
                .unwrap();
                assert!(matches!(report, SyncReport::Succeeded { .. }));
            }
        }
        let transport = ReqwestTransport::for_tests(&["127.0.0.1"]).unwrap();
        let response = transport
            .execute(&HttpRequest {
                method: request.method.clone(),
                url: format!("{forward_to}{}", request.path),
                headers: request
                    .headers
                    .iter()
                    .filter(|(name, _)| {
                        !matches!(
                            name.to_ascii_lowercase().as_str(),
                            "host" | "content-length"
                        )
                    })
                    .cloned()
                    .collect(),
                body: request.body.clone(),
                mutation: request.method != "GET",
                timeout: Duration::from_secs(10),
            })
            .unwrap();
        crate::testkit::ScriptedReply::Response {
            status: response.status,
            headers: response
                .headers
                .into_iter()
                .filter(|(name, _)| {
                    !matches!(
                        name.to_ascii_lowercase().as_str(),
                        "content-length" | "transfer-encoding" | "connection"
                    )
                })
                .collect(),
            body: response.body,
        }
    })
    .unwrap();

    let credentials = MemoryCredentialStore::default();
    let a = bind(&credentials);
    let (a_vault, a_state) = (dirs.dir("a-vault"), dirs.dir("a-state"));
    std::fs::write(a_vault.join("a.md"), "from a").unwrap();
    let peer = engine(&credentials, Some(format!("http://{}", proxy.addr)));
    let report = run_sync(
        &peer,
        &request_for(&a_vault, &a_state, &a, "abcd000000000091"),
    )
    .unwrap();
    assert!(matches!(report, SyncReport::Succeeded { .. }));
    assert!(*fired.lock().unwrap(), "the race must have fired");
    assert_eq!(
        std::fs::read_to_string(a_vault.join("racer.md")).unwrap(),
        "from racer"
    );

    // The host holds one head with both files.
    let key = Secret::new(KEY);
    let sleeper = RecordingSleeper::default();
    let store = http_store(&host_base, &key, &sleeper);
    let remote = tailnet::listing(&store).unwrap();
    assert_eq!(remote.heads.len(), 1, "{:?}", remote.heads);
    let head = remote.commits[&remote.heads[0]].snapshot().unwrap();
    assert!(head.get(&RelativePath::parse("a.md").unwrap()).is_some());
    assert!(
        head.get(&RelativePath::parse("racer.md").unwrap())
            .is_some()
    );
}

fn request_for(root: &Path, state: &Path, binding: &BindingId, op: &str) -> RunRequest {
    request(root, state, binding, peer_role(), op)
}

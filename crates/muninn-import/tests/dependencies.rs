//! The import crate stays a local leaf.
//!
//! `cargo metadata` is the resolved dependency graph Cargo actually builds, so
//! this test fails if a future version of `anydoc` (or a feature) pulls in an
//! HTTP client, or if the crate grows a second direct dependency.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::process::Command;

/// Crates that would let conversion call the network. None may appear in the
/// `muninn-import` dependency tree.
const HTTP_CLIENTS: &[&str] = &[
    "attohttpc",
    "awc",
    "curl",
    "h2",
    "hyper",
    "hyper-util",
    "isahc",
    "reqwest",
    "surf",
    "ureq",
];

fn metadata() -> String {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        // No `--offline`: metadata resolves every platform's dependencies, and
        // CI's cache may not hold a Windows-only crate on Linux (#189 CI).
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("metadata is UTF-8")
}

/// Resolved nodes as `(package id, direct dependency ids)`.
///
/// The subset of `resolve.nodes` we need is flat and string-only; parsing it
/// keeps the crate dependency-free (no `serde_json` dev-dependency).
fn resolve_nodes(json: &str) -> Vec<(String, Vec<String>)> {
    let resolve = json.find("\"resolve\":").expect("metadata has resolve");
    let nodes = resolve
        + json[resolve..]
            .find("\"nodes\":[")
            .expect("resolve has nodes");
    let mut cursor = nodes + "\"nodes\":[".len();
    let mut parsed = Vec::new();
    while let Some(relative) = json[cursor..].find("{\"id\":\"") {
        let id_start = cursor + relative + "{\"id\":\"".len();
        let id_end = id_start + json[id_start..].find('"').expect("id terminator");
        let id = json[id_start..id_end].to_string();

        let deps_marker = "\"dependencies\":[";
        let deps_start = id_end + json[id_end..].find(deps_marker).expect("node dependencies");
        let deps_start = deps_start + deps_marker.len();
        let deps_end = deps_start + json[deps_start..].find(']').expect("dependency list end");
        let deps_text = &json[deps_start..deps_end];
        let dependencies = if deps_text.is_empty() {
            Vec::new()
        } else {
            deps_text
                .split("\",\"")
                .map(|dep| dep.trim_matches('"').to_string())
                .collect()
        };

        parsed.push((id, dependencies));
        cursor = deps_end + 1;
    }
    parsed
}

fn crate_name(id: &str) -> &str {
    id.rsplit('#')
        .next()
        .and_then(|tail| tail.split('@').next())
        .unwrap_or(id)
}

#[test]
fn dependency_tree_is_local_and_leaf_only() {
    let json = metadata();
    let nodes = resolve_nodes(&json);
    let graph: HashMap<&str, &[String]> = nodes
        .iter()
        .map(|(id, deps)| (id.as_str(), deps.as_slice()))
        .collect();

    let root = nodes
        .iter()
        .map(|(id, _)| id.as_str())
        .find(|id| id.contains("/muninn-import#") || id.contains("#muninn-import@"))
        .expect("muninn-import is in the resolve graph");

    let direct: BTreeSet<&str> = graph[root].iter().map(|id| crate_name(id)).collect();
    assert_eq!(
        direct,
        BTreeSet::from(["anydoc"]),
        "muninn-import must depend only on anydoc"
    );

    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([root]);
    let mut tree = BTreeSet::new();
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        for dependency in graph.get(id).copied().unwrap_or_default() {
            tree.insert(crate_name(dependency));
            queue.push_back(dependency.as_str());
        }
    }

    let clients: Vec<&str> = HTTP_CLIENTS
        .iter()
        .copied()
        .filter(|client| tree.contains(client))
        .collect();
    assert!(
        clients.is_empty(),
        "muninn-import must not link an HTTP client, found {clients:?}"
    );
}

//! Three-way planner.
//!
//! Inputs are complete snapshots. A missing path means "not in this snapshot",
//! which the inventory/adapters only produce after a full, successful listing.
//! The planner never uses mtimes and never treats "we failed to look" as a delete.
//!
//! On a first bind (`base == None`) the plan is a union. Same-path disagreements
//! are conflicts. Nothing is deleted.

use std::collections::{BTreeMap, BTreeSet};

use crate::SyncError;
use crate::model::{
    Conflict, ConflictKind, ContentHash, Plan, Rename, Resolution, Side, Snapshot, Tombstone,
};
use crate::pathutil::RelativePath;

pub fn plan(base: Option<&Snapshot>, local: &Snapshot, remote: &Snapshot) -> Plan {
    match base {
        None => first_bind(local, remote),
        Some(base) => with_base(base, local, remote),
    }
}

fn first_bind(local: &Snapshot, remote: &Snapshot) -> Plan {
    let mut desired = Snapshot::default();
    let mut conflicts = Vec::new();
    let mut pulls = Vec::new();
    let mut pushes = Vec::new();
    let mut paths = BTreeSet::new();
    paths.extend(local.files.keys().cloned());
    paths.extend(remote.files.keys().cloned());
    for path in paths {
        match (local.get(&path), remote.get(&path)) {
            (Some(left), Some(right)) if left.hash == right.hash => {
                desired.files.insert(path, left.clone());
            }
            (Some(left), Some(right)) => {
                conflicts.push(Conflict {
                    path,
                    kind: ConflictKind::AddAdd,
                    base: None,
                    local: Some(left.hash),
                    remote: Some(right.hash),
                    other_path: None,
                });
            }
            (Some(left), None) => {
                desired.files.insert(path.clone(), left.clone());
                pushes.push(path);
            }
            (None, Some(right)) => {
                desired.files.insert(path.clone(), right.clone());
                pulls.push(path);
            }
            (None, None) => {}
        }
    }
    Plan {
        desired,
        conflicts,
        tombstones: Vec::new(),
        pulls,
        local_deletes: Vec::new(),
        pushes,
        renames: Vec::new(),
        first_bind: true,
    }
}

fn with_base(base: &Snapshot, local: &Snapshot, remote: &Snapshot) -> Plan {
    let mut local_renames = unique_renames(base, local);
    for rename in &mut local_renames {
        rename.side = Side::Local;
    }
    let mut remote_renames = unique_renames(base, remote);
    for rename in &mut remote_renames {
        rename.side = Side::Remote;
    }
    let mut conflicts = Vec::new();
    let mut accepted: Vec<Rename> = Vec::new();

    let local_targets: BTreeMap<&RelativePath, &Rename> = local_renames
        .iter()
        .map(|rename| (&rename.from, rename))
        .collect();
    let remote_targets: BTreeMap<&RelativePath, &Rename> = remote_renames
        .iter()
        .map(|rename| (&rename.from, rename))
        .collect();

    let mut origins = BTreeSet::new();
    origins.extend(local_targets.keys().copied().cloned());
    origins.extend(remote_targets.keys().copied().cloned());

    for origin in origins {
        match (local_targets.get(&origin), remote_targets.get(&origin)) {
            (Some(left), Some(right)) if left.to == right.to && left.hash == right.hash => {
                accepted.push(Rename {
                    from: left.from.clone(),
                    to: left.to.clone(),
                    hash: left.hash,
                    side: Side::Both,
                });
            }
            (Some(left), Some(right)) => {
                conflicts.push(Conflict {
                    path: origin,
                    kind: ConflictKind::RenameRename,
                    base: base.hash_of(&left.from),
                    local: Some(left.hash),
                    remote: Some(right.hash),
                    other_path: Some(if left.to != right.to {
                        right.to.clone()
                    } else {
                        left.to.clone()
                    }),
                });
            }
            (Some(left), None) => {
                if remote_blocks_rename(base, remote, left) {
                    conflicts.push(Conflict {
                        path: left.from.clone(),
                        kind: if remote.get(&left.from).is_none() {
                            ConflictKind::RenameDelete
                        } else {
                            ConflictKind::RenameEdit
                        },
                        base: base.hash_of(&left.from),
                        local: Some(left.hash),
                        remote: remote.hash_of(&left.from),
                        other_path: Some(left.to.clone()),
                    });
                } else {
                    accepted.push((*left).clone());
                }
            }
            (None, Some(right)) => {
                if local_blocks_rename(base, local, right) {
                    conflicts.push(Conflict {
                        path: right.from.clone(),
                        kind: if local.get(&right.from).is_none() {
                            ConflictKind::RenameDelete
                        } else {
                            ConflictKind::RenameEdit
                        },
                        base: base.hash_of(&right.from),
                        local: local.hash_of(&right.from),
                        remote: Some(right.hash),
                        other_path: Some(right.to.clone()),
                    });
                } else {
                    accepted.push((*right).clone());
                }
            }
            (None, None) => {}
        }
    }

    // Virtual snapshots: rewrite accepted renames so the pathwise pass sees
    // the file still at its base path when only one side moved it, and at the
    // new path when both did. Easier: build rewritten local/remote where a
    // rename moves the record, and the pathwise pass runs on what remains.
    let local_view = rewrite(local, &accepted, Side::Local);
    let remote_view = rewrite(remote, &accepted, Side::Remote);

    let mut desired = Snapshot::default();
    let mut pulls = Vec::new();
    let mut pushes = Vec::new();
    let mut local_deletes = Vec::new();
    let mut tombstones = Vec::new();

    let mut paths = BTreeSet::new();
    paths.extend(base.files.keys().cloned());
    paths.extend(local_view.files.keys().cloned());
    paths.extend(remote_view.files.keys().cloned());

    for path in paths {
        if conflicts
            .iter()
            .any(|conflict| conflict.path == path || conflict.other_path.as_ref() == Some(&path))
        {
            continue;
        }
        let item_conflicts = classify(
            base,
            &local_view,
            &remote_view,
            &path,
            &mut desired,
            &mut pulls,
            &mut pushes,
            &mut local_deletes,
            &mut tombstones,
        );
        conflicts.extend(item_conflicts);
    }

    if !conflicts.is_empty() {
        // A conflict makes the desired tree unsafe to apply. Keep it empty so
        // a caller that forgets `has_conflicts` does not publish a partial tree.
        desired.files.clear();
        pulls.clear();
        pushes.clear();
        local_deletes.clear();
        tombstones.clear();
    } else {
        // Both-side renames are already at the new path in the views. One-side
        // renames were rewritten back to the origin for classification, so put
        // the file at its new path now.
        for rename in &accepted {
            if rename.side == Side::Both {
                continue;
            }
            if let Some(record) = desired.files.remove(&rename.from) {
                desired.files.insert(rename.to.clone(), record);
            }
            if rename.side == Side::Remote {
                // Local still has the old path; the apply must write the new
                // path and remove the old one.
                pulls.retain(|path| path != &rename.from);
                pulls.push(rename.to.clone());
                if local.get(&rename.from).is_some() {
                    local_deletes.push(rename.from.clone());
                }
            }
            if rename.side == Side::Local {
                pushes.retain(|path| path != &rename.from);
                if !pushes.contains(&rename.to) {
                    pushes.push(rename.to.clone());
                }
            }
        }
        // Drop no-op deletes where the desired tree still has the path.
        local_deletes.retain(|path| !desired.files.contains_key(path));
        pulls.retain(|path| desired.files.contains_key(path));
        dedup(&mut pulls);
        dedup(&mut pushes);
        dedup(&mut local_deletes);
    }

    Plan {
        desired,
        conflicts,
        tombstones,
        pulls,
        local_deletes,
        pushes,
        renames: accepted,
        first_bind: false,
    }
}

fn dedup(paths: &mut Vec<RelativePath>) {
    let mut seen = BTreeSet::new();
    paths.retain(|path| seen.insert(path.clone()));
}

fn classify(
    base: &Snapshot,
    local: &Snapshot,
    remote: &Snapshot,
    path: &RelativePath,
    desired: &mut Snapshot,
    pulls: &mut Vec<RelativePath>,
    pushes: &mut Vec<RelativePath>,
    local_deletes: &mut Vec<RelativePath>,
    tombstones: &mut Vec<Tombstone>,
) -> Vec<Conflict> {
    let b = base.get(path);
    let l = local.get(path);
    let r = remote.get(path);
    let mut conflicts = Vec::new();
    match (b, l, r) {
        (_, Some(left), Some(right)) if left.hash == right.hash => {
            desired.files.insert(path.clone(), left.clone());
        }
        (Some(base_file), Some(left), Some(right)) if left.hash == base_file.hash => {
            desired.files.insert(path.clone(), right.clone());
            pulls.push(path.clone());
        }
        (Some(base_file), Some(left), Some(right)) if right.hash == base_file.hash => {
            desired.files.insert(path.clone(), left.clone());
            pushes.push(path.clone());
        }
        (Some(base_file), Some(left), Some(right)) => {
            conflicts.push(Conflict {
                path: path.clone(),
                kind: ConflictKind::EditEdit,
                base: Some(base_file.hash),
                local: Some(left.hash),
                remote: Some(right.hash),
                other_path: None,
            });
        }
        (None, Some(left), Some(right)) => {
            conflicts.push(Conflict {
                path: path.clone(),
                kind: ConflictKind::AddAdd,
                base: None,
                local: Some(left.hash),
                remote: Some(right.hash),
                other_path: None,
            });
        }
        (Some(base_file), Some(left), None) if left.hash == base_file.hash => {
            local_deletes.push(path.clone());
            tombstones.push(Tombstone {
                path: path.clone(),
                base_hash: base_file.hash,
            });
        }
        (Some(base_file), Some(left), None) => {
            conflicts.push(Conflict {
                path: path.clone(),
                kind: ConflictKind::EditDelete,
                base: Some(base_file.hash),
                local: Some(left.hash),
                remote: None,
                other_path: None,
            });
        }
        (Some(base_file), None, Some(right)) if right.hash == base_file.hash => {
            tombstones.push(Tombstone {
                path: path.clone(),
                base_hash: base_file.hash,
            });
        }
        (Some(base_file), None, Some(_)) => {
            conflicts.push(Conflict {
                path: path.clone(),
                kind: ConflictKind::DeleteEdit,
                base: Some(base_file.hash),
                local: None,
                remote: remote.hash_of(path),
                other_path: None,
            });
        }
        (Some(base_file), None, None) => {
            tombstones.push(Tombstone {
                path: path.clone(),
                base_hash: base_file.hash,
            });
        }
        (None, Some(left), None) => {
            desired.files.insert(path.clone(), left.clone());
            pushes.push(path.clone());
        }
        (None, None, Some(right)) => {
            desired.files.insert(path.clone(), right.clone());
            pulls.push(path.clone());
        }
        (None, None, None) => {}
    }
    conflicts
}

fn remote_blocks_rename(base: &Snapshot, remote: &Snapshot, rename: &Rename) -> bool {
    match (base.get(&rename.from), remote.get(&rename.from)) {
        (Some(base_file), Some(remote_file)) => remote_file.hash != base_file.hash,
        (Some(_), None) => true,
        _ => remote
            .get(&rename.to)
            .is_some_and(|file| file.hash != rename.hash),
    }
}

fn local_blocks_rename(base: &Snapshot, local: &Snapshot, rename: &Rename) -> bool {
    match (base.get(&rename.from), local.get(&rename.from)) {
        (Some(base_file), Some(local_file)) => local_file.hash != base_file.hash,
        (Some(_), None) => true,
        _ => local
            .get(&rename.to)
            .is_some_and(|file| file.hash != rename.hash),
    }
}

/// Rewrite a snapshot so accepted renames are expressed at the base path when
/// only this side moved the file. Both-side renames stay at the destination.
fn rewrite(snapshot: &Snapshot, renames: &[Rename], side: Side) -> Snapshot {
    let mut out = snapshot.clone();
    for rename in renames {
        let applies = match rename.side {
            Side::Both => true,
            other => other == side,
        };
        if !applies {
            continue;
        }
        if rename.side == Side::Both {
            continue;
        }
        if let Some(record) = out.files.remove(&rename.to) {
            out.files.insert(rename.from.clone(), record);
        }
    }
    out
}

/// Pair unique content-hash deletes with unique adds. Ambiguous matches are
/// left as delete+add so we never guess.
fn unique_renames(base: &Snapshot, side: &Snapshot) -> Vec<Rename> {
    let mut deleted: BTreeMap<ContentHash, Vec<RelativePath>> = BTreeMap::new();
    for (path, record) in &base.files {
        match side.get(path) {
            Some(now) if now.hash == record.hash => {}
            _ => deleted.entry(record.hash).or_default().push(path.clone()),
        }
    }
    let mut added: BTreeMap<ContentHash, Vec<RelativePath>> = BTreeMap::new();
    for (path, record) in &side.files {
        match base.get(path) {
            Some(was) if was.hash == record.hash => {}
            _ => added.entry(record.hash).or_default().push(path.clone()),
        }
    }
    let mut renames = Vec::new();
    for (hash, froms) in &deleted {
        let Some(tos) = added.get(hash) else {
            continue;
        };
        if froms.len() == 1 && tos.len() == 1 && froms[0] != tos[0] {
            renames.push(Rename {
                from: froms[0].clone(),
                to: tos[0].clone(),
                hash: *hash,
                side: Side::Local, // caller overwrites the side label
            });
        }
    }
    renames
}

/// Apply explicit choices to a conflicted plan.
///
/// `KeepBoth` writes the remote bytes beside the local file under a
/// `(remote)` name and keeps the local bytes at the original path. The
/// returned snapshot is the tree to publish and apply. If either side's hash
/// no longer matches the conflict, the caller must replan instead.
pub fn resolve(
    local: &Snapshot,
    remote: &Snapshot,
    conflicts: &[Conflict],
    choices: &[(RelativePath, Resolution)],
) -> Result<Snapshot, SyncError> {
    if choices.len() != conflicts.len() {
        return Err(SyncError::Config {
            detail: "every conflict needs a choice before sync can continue".to_owned(),
        });
    }
    let mut desired = Snapshot::default();
    // Start from the union of non-conflicting paths, preferring local when equal.
    let mut paths = BTreeSet::new();
    paths.extend(local.files.keys().cloned());
    paths.extend(remote.files.keys().cloned());
    for path in &paths {
        if conflicts.iter().any(|conflict| &conflict.path == path) {
            continue;
        }
        if let Some(record) = local.get(path).or_else(|| remote.get(path)).cloned() {
            // If both exist and differ, this path should have been a conflict.
            if let (Some(left), Some(right)) = (local.get(path), remote.get(path))
                && left.hash != right.hash
            {
                return Err(SyncError::Protocol {
                    detail: format!("{path} differs but was not in the conflict list"),
                });
            }
            desired.files.insert(path.clone(), record);
        }
    }
    for conflict in conflicts {
        let choice = choices
            .iter()
            .find(|(path, _)| path == &conflict.path)
            .map(|(_, choice)| *choice)
            .ok_or_else(|| SyncError::Config {
                detail: format!("missing choice for {}", conflict.path),
            })?;
        match choice {
            Resolution::KeepLocal => {
                if let Some(record) = local.get(&conflict.path).cloned() {
                    desired.files.insert(conflict.path.clone(), record);
                }
            }
            Resolution::UseRemote => {
                if let Some(record) = remote.get(&conflict.path).cloned() {
                    desired.files.insert(conflict.path.clone(), record);
                }
            }
            Resolution::KeepBoth => {
                if let Some(record) = local.get(&conflict.path).cloned() {
                    desired.files.insert(conflict.path.clone(), record);
                }
                if let Some(record) = remote.get(&conflict.path).cloned() {
                    let copy = conflict_copy_path(&conflict.path, &desired)?;
                    desired.files.insert(copy, record);
                }
            }
        }
    }
    Ok(desired)
}

/// A three-way merge that cannot stop on a conflict, for two remote heads
/// no person is around to choose between. Paths where one side kept the base
/// take the other side. Where both sides changed a path differently, content
/// wins over a delete, and when both have content `left` keeps the path while
/// `right` is written beside it as `<stem> (conflict <label>)<ext>`. Rename
/// detection is not needed: a rename is a delete plus an add, and neither side
/// loses bytes. Returns the merged tree and the conflict copies it created.
///
/// The result depends only on its inputs, so two devices that merge the same
/// heads at once produce the same tree, and the next merge is clean.
pub fn merge_keep_both(
    base: Option<&Snapshot>,
    left: &Snapshot,
    right: &Snapshot,
    label: &str,
) -> Result<(Snapshot, Vec<RelativePath>), SyncError> {
    let empty = Snapshot::default();
    let base = base.unwrap_or(&empty);
    let mut paths = BTreeSet::new();
    paths.extend(base.files.keys().cloned());
    paths.extend(left.files.keys().cloned());
    paths.extend(right.files.keys().cloned());
    let mut merged = Snapshot::default();
    let mut clashes = Vec::new();
    for path in paths {
        let b = base.get(&path).map(|record| record.hash);
        let l = left.get(&path);
        let r = right.get(&path);
        let (lh, rh) = (l.map(|record| record.hash), r.map(|record| record.hash));
        let pick = if lh == rh || rh == b {
            l
        } else if lh == b {
            r
        } else {
            match (l, r) {
                (Some(l), Some(r)) => {
                    clashes.push((path.clone(), r.clone()));
                    Some(l)
                }
                (Some(only), None) | (None, Some(only)) => Some(only),
                (None, None) => None,
            }
        };
        if let Some(record) = pick {
            merged.files.insert(path, record.clone());
        }
    }
    let mut copies = Vec::new();
    for (path, record) in clashes {
        let copy = named_copy_path(&path, &merged, &format!("conflict {label}"))?;
        merged.files.insert(copy.clone(), record);
        copies.push(copy);
    }
    Ok((merged, copies))
}

fn conflict_copy_path(path: &RelativePath, desired: &Snapshot) -> Result<RelativePath, SyncError> {
    named_copy_path(path, desired, "remote")
}

fn named_copy_path(
    path: &RelativePath,
    desired: &Snapshot,
    tag: &str,
) -> Result<RelativePath, SyncError> {
    let name = path.file_name();
    let (stem, ext) = name
        .rsplit_once('.')
        .map(|(stem, ext)| (stem.to_owned(), format!(".{ext}")))
        .unwrap_or_else(|| (name.to_owned(), String::new()));
    let parent = path.parent();
    for index in 0..50 {
        let file = if index == 0 {
            format!("{stem} ({tag}){ext}")
        } else {
            format!("{stem} ({tag} {}){ext}", index + 1)
        };
        let candidate = match &parent {
            Some(parent) => format!("{parent}/{file}"),
            None => file,
        };
        let Ok(rel) = RelativePath::parse(&candidate) else {
            continue;
        };
        if !desired.files.contains_key(&rel) {
            return Ok(rel);
        }
    }
    Err(SyncError::Limit {
        detail: format!("could not find a free conflict-copy name for {path}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentHash, FileKind, FileRecord};

    #[test]
    fn keep_both_merge_never_drops_content() {
        let base = snap(&[
            ("a.md", "base"),
            ("b.md", "b"),
            ("c.md", "c"),
            ("d.md", "d"),
        ]);
        // left edits a, deletes b, edits c; right edits a, edits b, deletes d.
        let left = snap(&[("a.md", "left"), ("c.md", "c2"), ("d.md", "d")]);
        let right = snap(&[("a.md", "right"), ("b.md", "b2"), ("c.md", "c")]);
        let (merged, copies) = merge_keep_both(Some(&base), &left, &right, "id").unwrap();
        let copy = RelativePath::parse("a (conflict id).md").unwrap();
        assert_eq!(copies, vec![copy.clone()]);
        let expect = snap(&[
            ("a.md", "left"),
            ("a (conflict id).md", "right"),
            ("b.md", "b2"),
            ("c.md", "c2"),
        ]);
        assert_eq!(merged, expect);
        // Same inputs, same tree: concurrent mergers converge.
        assert_eq!(
            merge_keep_both(Some(&base), &left, &right, "id").unwrap().0,
            merged
        );
    }

    fn snap(pairs: &[(&str, &str)]) -> Snapshot {
        let mut snapshot = Snapshot::default();
        for (path, text) in pairs {
            let bytes = text.as_bytes();
            snapshot.files.insert(
                RelativePath::parse(path).unwrap(),
                FileRecord {
                    hash: ContentHash::of(bytes),
                    size: bytes.len() as u64,
                    kind: FileKind::of_name(path),
                },
            );
        }
        snapshot
    }

    fn path(text: &str) -> RelativePath {
        RelativePath::parse(text).unwrap()
    }

    #[test]
    fn first_bind_unions_and_does_not_delete() {
        let local = snap(&[("a.md", "local-only"), ("both.md", "same")]);
        let remote = snap(&[("b.md", "remote-only"), ("both.md", "same")]);
        let plan = plan(None, &local, &remote);
        assert!(plan.first_bind);
        assert!(plan.local_deletes.is_empty());
        assert!(plan.tombstones.is_empty());
        assert!(plan.conflicts.is_empty());
        assert!(plan.desired.get(&path("a.md")).is_some());
        assert!(plan.desired.get(&path("b.md")).is_some());
        assert_eq!(plan.pushes, vec![path("a.md")]);
        assert_eq!(plan.pulls, vec![path("b.md")]);
    }

    #[test]
    fn first_bind_same_path_disagreement_conflicts() {
        let local = snap(&[("a.md", "left")]);
        let remote = snap(&[("a.md", "right")]);
        let plan = plan(None, &local, &remote);
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].kind, ConflictKind::AddAdd);
        assert!(plan.desired.files.is_empty());
        assert!(plan.local_deletes.is_empty());
    }

    #[test]
    fn noop_when_everyone_agrees() {
        let base = snap(&[("a.md", "v")]);
        let plan = plan(Some(&base), &base, &base);
        assert!(plan.is_noop());
        assert_eq!(plan.desired, base);
    }

    #[test]
    fn one_sided_edit_transfers() {
        let base = snap(&[("a.md", "v1")]);
        let local = snap(&[("a.md", "v2")]);
        let remote = snap(&[("a.md", "v1")]);
        let pushed = plan(Some(&base), &local, &remote);
        assert_eq!(pushed.pushes, vec![path("a.md")]);
        assert!(pushed.pulls.is_empty());
        assert_eq!(
            pushed.desired.hash_of(&path("a.md")),
            local.hash_of(&path("a.md"))
        );

        let pulled = plan(Some(&base), &remote, &local);
        assert_eq!(pulled.pulls, vec![path("a.md")]);
        assert!(pulled.pushes.is_empty());
    }

    #[test]
    fn identical_edits_converge_without_transfer() {
        let base = snap(&[("a.md", "v1")]);
        let next = snap(&[("a.md", "v2")]);
        let plan = plan(Some(&base), &next, &next);
        assert!(plan.is_noop());
        assert_eq!(
            plan.desired.hash_of(&path("a.md")),
            next.hash_of(&path("a.md"))
        );
    }

    #[test]
    fn edit_edit_and_edit_delete_conflict() {
        let base = snap(&[("a.md", "v1"), ("b.md", "keep")]);
        let local = snap(&[("a.md", "local"), ("b.md", "changed")]);
        let remote = snap(&[("a.md", "remote")]);
        let plan = plan(Some(&base), &local, &remote);
        assert!(plan.has_conflicts());
        assert!(plan.desired.files.is_empty());
        let kinds: Vec<_> = plan.conflicts.iter().map(|c| c.kind).collect();
        assert!(kinds.contains(&ConflictKind::EditEdit));
        assert!(kinds.contains(&ConflictKind::EditDelete));
    }

    #[test]
    fn unchanged_remote_delete_removes_local_and_records_tombstone() {
        let base = snap(&[("a.md", "v1"), ("b.md", "stay")]);
        let local = base.clone();
        let remote = snap(&[("b.md", "stay")]);
        let plan = plan(Some(&base), &local, &remote);
        assert!(plan.conflicts.is_empty());
        assert_eq!(plan.local_deletes, vec![path("a.md")]);
        assert_eq!(plan.tombstones.len(), 1);
        assert_eq!(
            plan.tombstones[0].base_hash,
            base.hash_of(&path("a.md")).unwrap()
        );
        assert!(plan.desired.get(&path("a.md")).is_none());
        assert!(plan.desired.get(&path("b.md")).is_some());
    }

    #[test]
    fn local_delete_of_unchanged_remote_does_not_pull_it_back() {
        let base = snap(&[("a.md", "v1")]);
        let local = snap(&[]);
        let remote = base.clone();
        let plan = plan(Some(&base), &local, &remote);
        assert!(plan.conflicts.is_empty());
        assert!(plan.pulls.is_empty());
        assert!(plan.desired.files.is_empty());
        assert_eq!(plan.tombstones.len(), 1);
    }

    #[test]
    fn exact_rename_is_detected_and_ambiguous_rename_is_not() {
        let base = snap(&[("a.md", "body")]);
        let local = snap(&[("b.md", "body")]);
        let remote = base.clone();
        let renamed = plan(Some(&base), &local, &remote);
        assert!(renamed.conflicts.is_empty(), "{renamed:?}");
        assert_eq!(renamed.renames.len(), 1);
        assert_eq!(renamed.renames[0].from, path("a.md"));
        assert_eq!(renamed.renames[0].to, path("b.md"));
        assert!(renamed.desired.get(&path("b.md")).is_some());
        assert!(renamed.desired.get(&path("a.md")).is_none());

        let ambiguous_base = snap(&[("a.md", "same"), ("b.md", "same")]);
        let swapped = snap(&[("b.md", "same"), ("a.md", "same")]);
        let ambiguous = plan(Some(&ambiguous_base), &swapped, &ambiguous_base);
        assert!(
            ambiguous.renames.is_empty(),
            "two files with identical bytes must not be paired"
        );
    }

    #[test]
    fn rename_against_edit_conflicts() {
        let base = snap(&[("a.md", "body")]);
        let local = snap(&[("b.md", "body")]);
        let remote = snap(&[("a.md", "edited")]);
        let plan = plan(Some(&base), &local, &remote);
        assert!(plan.has_conflicts());
        assert_eq!(plan.conflicts[0].kind, ConflictKind::RenameEdit);
        assert!(plan.desired.files.is_empty());
    }

    #[test]
    fn divergent_renames_conflict() {
        let base = snap(&[("a.md", "body")]);
        let local = snap(&[("b.md", "body")]);
        let remote = snap(&[("c.md", "body")]);
        let plan = plan(Some(&base), &local, &remote);
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].kind, ConflictKind::RenameRename);
    }

    #[test]
    fn planning_the_desired_tree_twice_is_a_noop() {
        let base = snap(&[("a.md", "v1"), ("gone.md", "x")]);
        let local = snap(&[("a.md", "v2"), ("new.md", "n")]);
        let remote = snap(&[("a.md", "v1")]);
        let once = plan(Some(&base), &local, &remote);
        assert!(!once.has_conflicts());
        let twice = plan(Some(&once.desired), &once.desired, &once.desired);
        assert!(twice.is_noop());
    }

    #[test]
    fn offline_client_does_not_resurrect_a_deleted_file() {
        // Base has the file. Remote deleted it. Local still has the original.
        let base = snap(&[("a.md", "v1")]);
        let local = base.clone();
        let remote = snap(&[]);
        let plan = plan(Some(&base), &local, &remote);
        assert!(plan.desired.get(&path("a.md")).is_none());
        assert!(!plan.pushes.contains(&path("a.md")));
    }

    #[test]
    fn keep_both_writes_a_remote_copy_name() {
        let local = snap(&[("a.md", "left")]);
        let remote = snap(&[("a.md", "right")]);
        let conflicts = vec![Conflict {
            path: path("a.md"),
            kind: ConflictKind::EditEdit,
            base: Some(ContentHash::of(b"base")),
            local: local.hash_of(&path("a.md")),
            remote: remote.hash_of(&path("a.md")),
            other_path: None,
        }];
        let desired = resolve(
            &local,
            &remote,
            &conflicts,
            &[(path("a.md"), Resolution::KeepBoth)],
        )
        .unwrap();
        assert_eq!(desired.hash_of(&path("a.md")), local.hash_of(&path("a.md")));
        assert!(desired.get(&path("a (remote).md")).is_some());
    }
}

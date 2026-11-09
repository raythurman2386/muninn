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
            // A one-sided rename onto a name the other side now uses for
            // different bytes is declined: no rename and no conflict. The
            // pathwise pass then sees a clean delete of `from` and an AddAdd
            // at `to`, so neither file is overwritten.
            (Some(left), None) if target_taken(remote, left) => {}
            (None, Some(right)) if target_taken(local, right) => {}
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

/// True when `other` holds different bytes at the rename's target, which a
/// rename on this side would otherwise overwrite.
fn target_taken(other: &Snapshot, rename: &Rename) -> bool {
    other
        .get(&rename.to)
        .is_some_and(|file| file.hash != rename.hash)
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
///
/// Only a path that is gone can be a rename source. A path that still exists
/// with new bytes was edited, not moved: treating it as moved would put the
/// old bytes back at the path and drop the edit (for example when a
/// keep-both copy holds the old version beside the new one).
fn unique_renames(base: &Snapshot, side: &Snapshot) -> Vec<Rename> {
    let mut deleted: BTreeMap<ContentHash, Vec<RelativePath>> = BTreeMap::new();
    for (path, record) in &base.files {
        if side.get(path).is_none() {
            deleted.entry(record.hash).or_default().push(path.clone());
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

/// Apply choices to a conflicted plan.
///
/// Paths outside the conflict list take the three-way result against `base`:
/// a side that kept the base takes the other side, so a delete or rename made
/// on one side in the same run is kept, not undone. With no base (first bind)
/// it is the union, as in [`plan`].
///
/// `KeepBoth` keeps the local bytes at the original path and writes the
/// remote bytes beside them as `<stem> (remote)<ext>`. Copy names are picked
/// after every kept path is placed, so a copy never takes a path another
/// file needs. Returns the tree to publish and apply, and the copies it
/// created. If either side's hash no longer matches the conflict, the caller
/// must replan instead.
pub fn resolve(
    base: Option<&Snapshot>,
    local: &Snapshot,
    remote: &Snapshot,
    conflicts: &[Conflict],
    choices: &[(RelativePath, Resolution)],
) -> Result<(Snapshot, Vec<RelativePath>), SyncError> {
    if choices.len() != conflicts.len() {
        return Err(SyncError::Config {
            detail: "every conflict needs a choice before sync can continue".to_owned(),
        });
    }
    let mut desired = Snapshot::default();
    let mut paths = BTreeSet::new();
    paths.extend(local.files.keys().cloned());
    paths.extend(remote.files.keys().cloned());
    for path in &paths {
        if conflicts.iter().any(|conflict| &conflict.path == path) {
            continue;
        }
        let left = local.get(path);
        let right = remote.get(path);
        let kept = match (base.and_then(|base| base.get(path)), left, right) {
            (_, Some(l), Some(r)) if l.hash == r.hash => Some(l),
            // One side kept the base: the other side's state wins, including
            // a delete or the old half of a rename.
            (Some(b), Some(l), r) if l.hash == b.hash => r,
            (Some(b), l, Some(r)) if r.hash == b.hash => l,
            (_, Some(_), Some(_)) => {
                return Err(SyncError::Protocol {
                    detail: format!("{path} differs but was not in the conflict list"),
                });
            }
            // Only one side has it and it is not a base delete: content wins.
            (_, l, r) => l.or(r),
        };
        if let Some(record) = kept {
            desired.files.insert(path.clone(), record.clone());
        }
    }
    let mut chosen = Vec::with_capacity(conflicts.len());
    for conflict in conflicts {
        let choice = choices
            .iter()
            .find(|(path, _)| path == &conflict.path)
            .map(|(_, choice)| *choice)
            .ok_or_else(|| SyncError::Config {
                detail: format!("missing choice for {}", conflict.path),
            })?;
        chosen.push((conflict, choice));
    }
    // Place every path a choice keeps first, then name the copies.
    for (conflict, choice) in &chosen {
        let record = match choice {
            Resolution::KeepLocal | Resolution::KeepBoth => local.get(&conflict.path),
            Resolution::UseRemote => remote.get(&conflict.path),
        };
        if let Some(record) = record {
            desired.files.insert(conflict.path.clone(), record.clone());
        }
    }
    let mut copies = Vec::new();
    for (conflict, choice) in &chosen {
        if *choice != Resolution::KeepBoth {
            continue;
        }
        let Some(record) = remote.get(&conflict.path) else {
            continue;
        };
        if local.get(&conflict.path).map(|l| l.hash) == Some(record.hash) {
            continue;
        }
        let copy = conflict_copy_path(&conflict.path, &desired)?;
        desired.files.insert(copy.clone(), record.clone());
        copies.push(copy);
    }
    Ok((desired, copies))
}

/// Paths in `base` that `desired` no longer has, as tombstones for a publish.
#[must_use]
pub fn tombstones_for(base: Option<&Snapshot>, desired: &Snapshot) -> Vec<Tombstone> {
    base.map(|base| {
        base.files
            .iter()
            .filter(|(path, _)| !desired.files.contains_key(*path))
            .map(|(path, record)| Tombstone {
                path: path.clone(),
                base_hash: record.hash,
            })
            .collect()
    })
    .unwrap_or_default()
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
        let (desired, copies) = resolve(
            None,
            &local,
            &remote,
            &conflicts,
            &[(path("a.md"), Resolution::KeepBoth)],
        )
        .unwrap();
        assert_eq!(desired.hash_of(&path("a.md")), local.hash_of(&path("a.md")));
        assert_eq!(
            desired.hash_of(&path("a (remote).md")),
            remote.hash_of(&path("a.md"))
        );
        assert_eq!(copies, vec![path("a (remote).md")]);
    }

    /// The device that wrote the old version then reads the keep-both result:
    /// the path holds the other text and a copy holds its own. That is an
    /// edit plus an add, not a rename; reading it as a rename put the old
    /// bytes back at the path and dropped the other device's edit.
    #[test]
    fn an_edit_beside_a_copy_of_the_old_bytes_is_not_a_rename() {
        let base = snap(&[("a.md", "mine")]);
        let local = base.clone();
        let remote = snap(&[("a.md", "theirs"), ("a (remote).md", "mine")]);
        let planned = plan(Some(&base), &local, &remote);
        assert!(!planned.has_conflicts(), "{:?}", planned.conflicts);
        assert!(planned.renames.is_empty(), "{:?}", planned.renames);
        assert_eq!(planned.desired, remote);
        assert!(planned.local_deletes.is_empty());
        // Same for a Drive keep-both merge copy.
        let remote = snap(&[("a.md", "theirs"), ("a (conflict 1234abcd).md", "mine")]);
        assert_eq!(plan(Some(&base), &local, &remote).desired, remote);
    }

    /// Resolve every conflict "keep both", as the app does.
    fn keep_both(base: &Snapshot, local: &Snapshot, remote: &Snapshot) -> Snapshot {
        let planned = plan(Some(base), local, remote);
        if !planned.has_conflicts() {
            return planned.desired;
        }
        let choices: Vec<_> = planned
            .conflicts
            .iter()
            .map(|c| (c.path.clone(), Resolution::KeepBoth))
            .collect();
        resolve(Some(base), local, remote, &planned.conflicts, &choices)
            .unwrap()
            .0
    }

    /// R1 case A: this side renames c -> c2 while the other side creates a
    /// new c2. The rename must not land on the other side's new note.
    #[test]
    fn local_rename_onto_a_name_the_remote_just_created_keeps_both() {
        let base = snap(&[("c.md", "M")]);
        let local = snap(&[("c2.md", "M")]);
        let remote = snap(&[("c.md", "M"), ("c2.md", "N")]);
        let planned = plan(Some(&base), &local, &remote);
        assert!(planned.renames.is_empty(), "{:?}", planned.renames);
        assert_eq!(planned.conflicts.len(), 1, "{:?}", planned.conflicts);
        assert_eq!(planned.conflicts[0].kind, ConflictKind::AddAdd);
        assert_eq!(planned.conflicts[0].path, path("c2.md"));
        let desired = keep_both(&base, &local, &remote);
        assert_eq!(desired, snap(&[("c2.md", "M"), ("c2 (remote).md", "N")]));
        assert!(plan(Some(&desired), &desired, &desired).is_noop());
    }

    /// R1 case B: the other side renamed c -> c2 while this side created a
    /// new, never-synced c2. This side's note must not be overwritten.
    #[test]
    fn remote_rename_onto_a_name_this_side_just_created_keeps_both() {
        let base = snap(&[("c.md", "M")]);
        let local = snap(&[("c.md", "M"), ("c2.md", "N")]);
        let remote = snap(&[("c2.md", "M")]);
        let planned = plan(Some(&base), &local, &remote);
        assert!(planned.renames.is_empty(), "{:?}", planned.renames);
        assert_eq!(planned.conflicts.len(), 1, "{:?}", planned.conflicts);
        assert_eq!(planned.conflicts[0].kind, ConflictKind::AddAdd);
        let desired = keep_both(&base, &local, &remote);
        assert_eq!(desired, snap(&[("c2.md", "N"), ("c2 (remote).md", "M")]));
        assert!(plan(Some(&desired), &desired, &desired).is_noop());
    }

    /// R2: a rename whose source the other side edited and whose target the
    /// other side created. It used to be a RenameEdit whose target `resolve`
    /// then refused on every run; now it is a delete-edit plus an add-add,
    /// and every version is kept.
    #[test]
    fn rename_against_an_edit_and_a_new_note_at_the_target_does_not_stick() {
        let base = snap(&[("c.md", "M")]);
        let local = snap(&[("c2.md", "M")]);
        let remote = snap(&[("c.md", "M2"), ("c2.md", "N")]);
        let desired = keep_both(&base, &local, &remote);
        assert_eq!(
            desired,
            snap(&[
                ("c (remote).md", "M2"),
                ("c2.md", "M"),
                ("c2 (remote).md", "N")
            ])
        );
        // The mirror: the other side renamed, this side edited and created.
        let desired = keep_both(&base, &remote, &local);
        let kept: BTreeSet<_> = desired.files.values().map(|r| r.hash).collect();
        for text in ["M", "M2", "N"] {
            assert!(
                kept.contains(&ContentHash::of(text.as_bytes())),
                "{desired:?}"
            );
        }
    }

    /// R3: both devices renamed the same note, to different names, and this
    /// side already has a different note at the name the other side picked.
    /// It used to be a RenameRename whose `other_path` hid that name from the
    /// pathwise pass, so `resolve` refused it on every run. Both renames are
    /// now declined: a delete of the old name, an add of this side's new
    /// name, and an AddAdd at the taken name, with every text kept.
    #[test]
    fn rename_rename_onto_a_name_this_side_just_created_keeps_both() {
        let base = snap(&[("c.md", "M")]);
        let local = snap(&[("d.md", "M"), ("e.md", "N")]);
        let remote = snap(&[("e.md", "M")]);
        let planned = plan(Some(&base), &local, &remote);
        assert!(planned.renames.is_empty(), "{:?}", planned.renames);
        assert_eq!(planned.conflicts.len(), 1, "{:?}", planned.conflicts);
        assert_eq!(planned.conflicts[0].kind, ConflictKind::AddAdd);
        assert_eq!(planned.conflicts[0].path, path("e.md"));
        let desired = keep_both(&base, &local, &remote);
        assert_eq!(
            desired,
            snap(&[("d.md", "M"), ("e.md", "N"), ("e (remote).md", "M")])
        );
        assert!(plan(Some(&desired), &desired, &desired).is_noop());
    }

    /// R3 mirror: the other side renamed c -> e, this side renamed c -> d,
    /// and the other side already has a different note at d.
    #[test]
    fn rename_rename_onto_a_name_the_remote_just_created_keeps_both() {
        let base = snap(&[("c.md", "M")]);
        let local = snap(&[("d.md", "M")]);
        let remote = snap(&[("e.md", "M"), ("d.md", "N")]);
        let planned = plan(Some(&base), &local, &remote);
        assert!(planned.renames.is_empty(), "{:?}", planned.renames);
        assert_eq!(planned.conflicts.len(), 1, "{:?}", planned.conflicts);
        assert_eq!(planned.conflicts[0].kind, ConflictKind::AddAdd);
        assert_eq!(planned.conflicts[0].path, path("d.md"));
        let desired = keep_both(&base, &local, &remote);
        assert_eq!(
            desired,
            snap(&[("d.md", "M"), ("d (remote).md", "N"), ("e.md", "M")])
        );
        assert!(plan(Some(&desired), &desired, &desired).is_noop());
    }

    /// A real rename onto a free name, or onto the same bytes, still applies.
    #[test]
    fn a_rename_onto_a_free_or_identical_target_is_still_a_rename() {
        let base = snap(&[("c.md", "M")]);
        let planned = plan(Some(&base), &snap(&[("c2.md", "M")]), &base);
        assert_eq!(planned.renames.len(), 1);
        assert_eq!(planned.desired, snap(&[("c2.md", "M")]));
        let planned = plan(
            Some(&base),
            &snap(&[("c2.md", "M")]),
            &snap(&[("c.md", "M"), ("c2.md", "M")]),
        );
        assert!(!planned.has_conflicts(), "{:?}", planned.conflicts);
        assert_eq!(planned.desired, snap(&[("c2.md", "M")]));
    }

    /// Duplicate a note, then edit the original, on one device: an edit and
    /// an add, never a rename that drops the edit.
    #[test]
    fn duplicate_then_edit_the_original_keeps_both() {
        let base = snap(&[("a.md", "X")]);
        let local = snap(&[("a.md", "Y"), ("b.md", "X")]);
        let planned = plan(Some(&base), &local, &base);
        assert!(planned.renames.is_empty());
        assert_eq!(planned.desired, local);
    }

    /// A same-note edit on both sides resolved "keep both" must not undo the
    /// other one-sided changes in the same run (deletes, renames, edits).
    #[test]
    fn keep_both_keeps_one_sided_deletes_renames_and_edits_beside_a_conflict() {
        let base = snap(&[
            ("a.md", "base"),
            ("b.md", "b"),
            ("c.md", "moved"),
            ("d.md", "d"),
            ("e.md", "e"),
        ]);
        // local: edit a, delete b, rename c -> c2. remote: edit a, delete d, edit e.
        let local = snap(&[
            ("a.md", "left"),
            ("c2.md", "moved"),
            ("d.md", "d"),
            ("e.md", "e"),
        ]);
        let remote = snap(&[
            ("a.md", "right"),
            ("b.md", "b"),
            ("c.md", "moved"),
            ("e.md", "e2"),
        ]);
        let planned = plan(Some(&base), &local, &remote);
        assert_eq!(planned.conflicts.len(), 1, "{:?}", planned.conflicts);
        assert_eq!(planned.conflicts[0].kind, ConflictKind::EditEdit);
        let choices = vec![(path("a.md"), Resolution::KeepBoth)];
        let (desired, copies) =
            resolve(Some(&base), &local, &remote, &planned.conflicts, &choices).unwrap();
        assert_eq!(
            desired,
            snap(&[
                ("a.md", "left"),
                ("a (remote).md", "right"),
                ("c2.md", "moved"),
                ("e.md", "e2"),
            ])
        );
        assert_eq!(copies, vec![path("a (remote).md")]);
        let mut tombs: Vec<_> = tombstones_for(Some(&base), &desired)
            .into_iter()
            .map(|tomb| tomb.path.as_str().to_owned())
            .collect();
        tombs.sort();
        assert_eq!(tombs, vec!["b.md", "c.md", "d.md"]);
        // Both sides then hold `desired`; the next plan is clean.
        let next = plan(Some(&desired), &desired, &desired);
        assert!(next.is_noop());
    }

    /// Every conflict kind resolves under "keep both" without losing bytes,
    /// and a copy never takes a path another conflict keeps.
    #[test]
    fn keep_both_resolves_every_conflict_kind_without_dropping_content() {
        let base = snap(&[
            ("edit.md", "base"),
            ("edit (remote).md", "base2"),
            ("ed.md", "ed"),
            ("de.md", "de"),
            ("mv.md", "mv"),
        ]);
        let local = snap(&[
            ("edit.md", "L1"),
            ("edit (remote).md", "L2"),
            ("ed.md", "ed-local"),
            ("mv-local.md", "mv"),
            ("new.md", "add-local"),
        ]);
        let remote = snap(&[
            ("edit.md", "R1"),
            ("edit (remote).md", "R2"),
            ("de.md", "de-remote"),
            ("mv.md", "mv-remote"),
            ("new.md", "add-remote"),
        ]);
        let planned = plan(Some(&base), &local, &remote);
        let kinds: BTreeSet<_> = planned
            .conflicts
            .iter()
            .map(|c| format!("{:?}", c.kind))
            .collect();
        for kind in [
            "EditEdit",
            "EditDelete",
            "DeleteEdit",
            "AddAdd",
            "RenameEdit",
        ] {
            assert!(kinds.contains(kind), "{kind} missing from {kinds:?}");
        }
        let choices: Vec<_> = planned
            .conflicts
            .iter()
            .map(|c| (c.path.clone(), Resolution::KeepBoth))
            .collect();
        let (desired, copies) =
            resolve(Some(&base), &local, &remote, &planned.conflicts, &choices).unwrap();
        let kept: BTreeSet<_> = desired.files.values().map(|r| r.hash).collect();
        for text in [
            "L1",
            "L2",
            "R1",
            "R2",
            "ed-local",
            "de-remote",
            "mv",
            "mv-remote",
            "add-local",
            "add-remote",
        ] {
            assert!(
                kept.contains(&ContentHash::of(text.as_bytes())),
                "{text} was dropped: {desired:?}"
            );
        }
        assert_eq!(
            desired.hash_of(&path("edit (remote).md")),
            Some(ContentHash::of(b"L2")),
            "a copy must not overwrite a path a conflict keeps"
        );
        assert_eq!(copies.len(), 5, "{copies:?}");
        let next = plan(Some(&desired), &desired, &desired);
        assert!(next.is_noop());
    }
}

//! Strict local inventory. A failure is a failure: missing permission, a
//! symlink, a special file, or a truncated walk never becomes a tombstone.

use std::collections::BTreeSet;
use std::path::Path;

use crate::error::SyncError;
use crate::fsutil::{self, EntryKind};
use crate::limits::{DEFAULT_LIMITS, Exclusion, ExclusionReason, Limits, component_exclusion};
use crate::model::{ContentHash, FileKind, FileRecord, Snapshot, VaultIdentity};
use crate::pathutil::RelativePath;

#[derive(Clone, Debug)]
pub struct Inventory {
    pub identity: VaultIdentity,
    pub snapshot: Snapshot,
    pub exclusions: Vec<Exclusion>,
    pub total_bytes: u64,
}

pub fn inventory(root: &Path, limits: Limits) -> Result<Inventory, SyncError> {
    let identity = VaultIdentity::of_canonical(root)?;
    let canonical = root.canonicalize().map_err(SyncError::io)?;
    let dir = fsutil::open_root(&canonical)?;
    let mut out = Inventory {
        identity,
        snapshot: Snapshot::default(),
        exclusions: Vec::new(),
        total_bytes: 0,
    };
    let mut folds = BTreeSet::new();
    walk(&dir, None, limits, &mut folds, &mut out)?;
    Ok(out)
}

pub fn inventory_default(root: &Path) -> Result<Inventory, SyncError> {
    inventory(root, DEFAULT_LIMITS)
}

fn walk(
    dir: &fsutil::RootDir,
    rel: Option<&RelativePath>,
    limits: Limits,
    folds: &mut BTreeSet<String>,
    out: &mut Inventory,
) -> Result<(), SyncError> {
    let names = dir.list(rel)?;
    for entry in names {
        let reason = component_exclusion(&entry.name);
        let child_text = match rel {
            Some(parent) => format!("{parent}/{}", entry.name),
            None => entry.name.clone(),
        };
        if let Some(reason) = reason {
            out.exclusions.push(Exclusion {
                path: child_text,
                reason,
            });
            continue;
        }
        if child_text.split('/').count() > limits.max_depth {
            return Err(SyncError::Limit {
                detail: format!(
                    "{child_text} is nested more than {} levels",
                    limits.max_depth
                ),
            });
        }
        let child = RelativePath::parse(&child_text)?;
        let meta = dir.metadata(&child)?.ok_or_else(|| SyncError::Inventory {
            detail: format!("{child} disappeared during the inventory"),
        })?;
        match meta.kind {
            EntryKind::Dir => walk(dir, Some(&child), limits, folds, out)?,
            EntryKind::File { bytes, hardlinked } => {
                if hardlinked {
                    return Err(SyncError::Unsupported {
                        path: child.to_string(),
                        detail: "hard link; sync would change another name".to_owned(),
                    });
                }
                if bytes > limits.max_file_bytes {
                    return Err(SyncError::Limit {
                        detail: format!(
                            "{child} is {bytes} bytes; the per-file limit is {}",
                            limits.max_file_bytes
                        ),
                    });
                }
                if out.snapshot.files.len() >= limits.max_files {
                    return Err(SyncError::Limit {
                        detail: format!("more than {} files", limits.max_files),
                    });
                }
                let data = dir.read_file(&child, limits.max_file_bytes)?;
                let size = data.len() as u64;
                if out.total_bytes.saturating_add(size) > limits.max_total_bytes {
                    return Err(SyncError::Limit {
                        detail: format!(
                            "vault exceeds the {} byte sync limit",
                            limits.max_total_bytes
                        ),
                    });
                }
                let fold = child.fold_key();
                if !folds.insert(fold) {
                    return Err(SyncError::Path {
                        detail: format!(
                            "{child} collides with another path when compared case-insensitively"
                        ),
                    });
                }
                out.total_bytes += size;
                out.snapshot.files.insert(
                    child.clone(),
                    FileRecord {
                        hash: ContentHash::of(&data),
                        size,
                        kind: FileKind::of_name(child.file_name()),
                    },
                );
            }
        }
    }
    Ok(())
}

/// Read one included file's current hash, or `None` when it is absent.
/// Symlinks and unreadable files are errors, not absence.
pub fn file_hash(root: &Path, rel: &RelativePath) -> Result<Option<ContentHash>, SyncError> {
    let canonical = root.canonicalize().map_err(SyncError::io)?;
    let dir = fsutil::open_root(&canonical)?;
    match dir.metadata(rel)? {
        None => Ok(None),
        Some(meta) => match meta.kind {
            EntryKind::Dir => Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "expected a file".to_owned(),
            }),
            EntryKind::File {
                hardlinked: true, ..
            } => Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "hard link".to_owned(),
            }),
            EntryKind::File { .. } => {
                let bytes = dir.read_file(rel, DEFAULT_LIMITS.max_file_bytes)?;
                Ok(Some(ContentHash::of(&bytes)))
            }
        },
    }
}

pub fn exclusion_summary(exclusions: &[Exclusion]) -> String {
    if exclusions.is_empty() {
        return String::new();
    }
    let mut hidden = 0;
    let mut temporary = 0;
    let mut os = 0;
    for exclusion in exclusions {
        match exclusion.reason {
            ExclusionReason::Hidden => hidden += 1,
            ExclusionReason::Temporary => temporary += 1,
            ExclusionReason::OperatingSystem => os += 1,
        }
    }
    format!("excluded by contract: {hidden} hidden, {temporary} temporary, {os} operating-system")
}

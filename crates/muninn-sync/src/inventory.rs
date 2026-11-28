//! Strict local inventory. A failure is a failure: missing permission, a
//! symlink, a special file, or a truncated walk never becomes a tombstone.
//!
//! The walk goes through [`VaultStore`], never `std::fs`, so the same code runs
//! over the desktop filesystem ([`FsStore`](muninn_core::FsStore)) and a
//! browser's vault mirror. Paths handed to the store are vault-relative; the
//! store's root is the only path the engine knows.
//!
//! The store's contract decides what can be refused. It reports a symbolic link
//! through [`symlink_metadata`](VaultStore::symlink_metadata) and a multiply
//! linked file through [`StoreMeta::links`](muninn_core::StoreMeta::links); a
//! store whose directory listing skips an entry the vault cannot represent (as
//! the native one does for a link or a device node) simply never walks it. The
//! pre-store walk used a `NOFOLLOW` directory handle, so a swap between the
//! metadata check and the read is now the store's responsibility.

use std::collections::BTreeSet;
use std::path::Path;

#[cfg(feature = "native")]
use crate::fsutil;
use muninn_core::{EntryKind, VaultStore};

use crate::error::SyncError;
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

/// The identity of the vault `store` serves.
///
/// The store spells its root; for a native store that is the canonical
/// filesystem path, which is exactly what [`VaultIdentity::of_canonical`]
/// hashes. A browser store has no path, so its root label is the spelling the
/// identity is computed from. The root must exist and be a folder, as it must
/// for a run to have a vault at all.
pub fn vault_identity(store: &dyn VaultStore) -> Result<VaultIdentity, SyncError> {
    let root = store.root();
    let canonical = store.canonicalize(root).map_err(|error| SyncError::Io {
        detail: format!("could not canonicalize {}: {error}", root.display()),
    })?;
    if !store.is_dir(&canonical) {
        return Err(SyncError::Inventory {
            detail: format!("{} is not a directory", canonical.display()),
        });
    }
    Ok(VaultIdentity::of_root(&canonical))
}

/// Inventory the vault behind `store`.
pub fn inventory_with(store: &dyn VaultStore, limits: Limits) -> Result<Inventory, SyncError> {
    let mut out = Inventory {
        identity: vault_identity(store)?,
        snapshot: Snapshot::default(),
        exclusions: Vec::new(),
        total_bytes: 0,
    };
    let mut folds = BTreeSet::new();
    walk(store, store.root(), None, limits, &mut folds, &mut out)?;
    Ok(out)
}

/// Inventory a native vault folder.
#[cfg(feature = "native")]
pub fn inventory(root: &Path, limits: Limits) -> Result<Inventory, SyncError> {
    let canonical = root.canonicalize().map_err(|error| SyncError::Io {
        detail: format!("could not canonicalize {}: {error}", root.display()),
    })?;
    fsutil::require_real_dir(&canonical)?;
    inventory_with(&muninn_core::FsStore::new(canonical), limits)
}

/// Inventory a native vault folder with the default limits.
#[cfg(feature = "native")]
pub fn inventory_default(root: &Path) -> Result<Inventory, SyncError> {
    inventory(root, DEFAULT_LIMITS)
}

fn walk(
    store: &dyn VaultStore,
    dir: &Path,
    rel: Option<&RelativePath>,
    limits: Limits,
    folds: &mut BTreeSet<String>,
    out: &mut Inventory,
) -> Result<(), SyncError> {
    let mut entries = store.read_dir(dir).map_err(|error| SyncError::Inventory {
        detail: format!("could not read {}: {error}", display_rel(rel)),
    })?;
    // The native walk listed names in byte order; keep that order across
    // stores so the exclusion list a report carries does not depend on the
    // host's directory order.
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    for entry in entries {
        let Some(name) = entry.name.to_str() else {
            return Err(SyncError::Path {
                detail: format!("non-utf8 name under {}", display_rel(rel)),
            });
        };
        let reason = component_exclusion(name);
        let child_text = match rel {
            Some(parent) => format!("{parent}/{name}"),
            None => name.to_owned(),
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
        let meta = match store.symlink_metadata(&entry.path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(SyncError::Inventory {
                    detail: format!("{child} disappeared during the inventory"),
                });
            }
            Err(error) => return Err(SyncError::io(error)),
        };
        if meta.symlink {
            return Err(SyncError::Unsupported {
                path: child.to_string(),
                detail: "symlink".to_owned(),
            });
        }
        match meta.kind {
            EntryKind::Dir => walk(store, &entry.path, Some(&child), limits, folds, out)?,
            EntryKind::File => {
                if meta.links > 1 {
                    return Err(SyncError::Unsupported {
                        path: child.to_string(),
                        detail: "hard link; sync would change another name".to_owned(),
                    });
                }
                let bytes = meta.len;
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
                let data = read_file(store, &child, &entry.path, limits.max_file_bytes)?;
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
pub(crate) fn file_hash_with(
    store: &dyn VaultStore,
    rel: &RelativePath,
) -> Result<Option<ContentHash>, SyncError> {
    let path = rel.under(store.root());
    let meta = match store.symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(SyncError::io(error)),
    };
    if meta.symlink {
        return Err(SyncError::Unsupported {
            path: rel.to_string(),
            detail: "symlink".to_owned(),
        });
    }
    match meta.kind {
        EntryKind::Dir => Err(SyncError::Unsupported {
            path: rel.to_string(),
            detail: "expected a file".to_owned(),
        }),
        EntryKind::File => {
            if meta.links > 1 {
                return Err(SyncError::Unsupported {
                    path: rel.to_string(),
                    detail: "hard link".to_owned(),
                });
            }
            if meta.len > DEFAULT_LIMITS.max_file_bytes {
                return Err(SyncError::Limit {
                    detail: format!(
                        "{rel} is {} bytes; the limit is {}",
                        meta.len, DEFAULT_LIMITS.max_file_bytes
                    ),
                });
            }
            let bytes = read_file(store, rel, &path, DEFAULT_LIMITS.max_file_bytes)?;
            Ok(Some(ContentHash::of(&bytes)))
        }
    }
}

/// Read one vault file for the engine, capped at `max_bytes`.
pub(crate) fn read_tracked_file(
    store: &dyn VaultStore,
    rel: &RelativePath,
    max_bytes: u64,
) -> Result<Vec<u8>, SyncError> {
    read_file(store, rel, &rel.under(store.root()), max_bytes)
}

fn read_file(
    store: &dyn VaultStore,
    rel: &RelativePath,
    path: &Path,
    max_bytes: u64,
) -> Result<Vec<u8>, SyncError> {
    let bytes = store.read_bytes(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => SyncError::Inventory {
            detail: format!("{rel} disappeared while it was being read"),
        },
        _ => SyncError::io(error),
    })?;
    if bytes.len() as u64 > max_bytes {
        return Err(SyncError::Limit {
            detail: format!("{rel} grew past {max_bytes} bytes while it was being read"),
        });
    }
    Ok(bytes)
}

fn display_rel(rel: Option<&RelativePath>) -> String {
    rel.map(ToString::to_string)
        .unwrap_or_else(|| "/".to_owned())
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

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::MemoryStore;

    #[test]
    fn a_memory_store_inventory_reads_without_a_filesystem() {
        let store = MemoryStore::new("/Muninn sample");
        store.put_file("notes/One.md", b"one");
        store.put_file("assets/pic.png", &[0xff, 0xfe]);
        store.put_file(".hidden", b"skip");
        store.put_file("root.md", b"root");

        let inventory = inventory_with(&store, DEFAULT_LIMITS).unwrap();
        assert_eq!(
            inventory
                .snapshot
                .files
                .keys()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["assets/pic.png", "notes/One.md", "root.md"]
        );
        assert_eq!(
            inventory
                .snapshot
                .hash_of(&RelativePath::parse("root.md").unwrap()),
            Some(ContentHash::of(b"root"))
        );
        assert_eq!(inventory.total_bytes, 3 + 2 + 4);
        assert_eq!(inventory.exclusions.len(), 1);
        assert_eq!(inventory.exclusions[0].path, ".hidden");
        assert_eq!(
            inventory.identity,
            VaultIdentity::of_root(Path::new("/Muninn sample"))
        );
    }

    #[test]
    fn the_identity_matches_the_native_canonical_identity_for_a_real_folder() {
        let root = std::env::temp_dir().join(format!(
            "muninn-identity-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.md"), "a").unwrap();
        let canonical = root.canonicalize().unwrap();
        let store = muninn_core::FsStore::new(canonical.clone());
        assert_eq!(
            vault_identity(&store).unwrap(),
            VaultIdentity::of_canonical(&root).unwrap(),
            "the store-based identity is the pre-seam identity"
        );
        assert_eq!(
            inventory_with(&store, DEFAULT_LIMITS).unwrap().identity,
            VaultIdentity::of_canonical(&root).unwrap()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_hard_link_is_refused_before_it_can_enter_the_snapshot() {
        let dir = std::env::temp_dir().join(format!(
            "muninn-hardlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "a").unwrap();
        std::fs::hard_link(dir.join("a.md"), dir.join("b.md")).unwrap();
        let store = muninn_core::FsStore::new(dir.canonicalize().unwrap());
        let error = inventory_with(&store, DEFAULT_LIMITS).unwrap_err();
        assert!(
            matches!(
                error,
                SyncError::Unsupported { ref detail, .. } if detail.contains("hard link")
            ),
            "{error:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! Apply a planned snapshot one file at a time.
//!
//! Each file is checkpointed, journaled, then replaced. A crash resumes at the
//! next file that is not marked done. Dirty editor paths and hash mismatches
//! abort before the first byte changes.
//!
//! Vault bytes flow through [`VaultStore`] and host state through [`StateHome`]:
//! the apply has no filesystem path of its own. The native wrappers below build
//! the desktop store and state home and keep the path-based API unchanged.

use std::collections::BTreeSet;
use std::path::PathBuf;

use muninn_core::{StateHome, VaultStore};

use crate::error::SyncError;
use crate::inventory::{file_hash_with, read_tracked_file};
use crate::journal::{FileAction, FileStep, Journal, JournalFile, JournalPhase};
use crate::model::ContentHash;
use crate::pathutil::RelativePath;
use crate::state_home;

#[derive(Clone)]
pub struct ApplyWrite {
    pub path: RelativePath,
    pub bytes: Vec<u8>,
    pub before: Option<ContentHash>,
    pub after: ContentHash,
}

#[derive(Clone)]
pub struct ApplyDelete {
    pub path: RelativePath,
    pub before: ContentHash,
}

pub struct ApplyRequest {
    /// Native vault root. The store-based apply does not read it; the native
    /// wrappers resolve it into an [`FsStore`](muninn_core::FsStore).
    pub root: PathBuf,
    /// Native sync state directory. Likewise only the native wrappers use it.
    pub state_dir: PathBuf,
    pub journal: Journal,
    pub writes: Vec<ApplyWrite>,
    pub deletes: Vec<ApplyDelete>,
    /// Editor buffers that must not be overwritten. Checked before any write.
    pub protected: BTreeSet<RelativePath>,
    /// Stop after this many files have been checkpointed, before replacing them.
    pub fault_after_checkpoint: Option<usize>,
    /// Stop after this many files have been fully replaced.
    pub fault_after_files: Option<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied,
    Faulted { completed_files: usize },
}

/// Apply a plan against the native state directory named in `request`.
///
/// The engine calls `apply_plan_with` with its own vault store and state home;
/// this wrapper keeps the path-based API for direct callers and tests unchanged.
#[cfg(feature = "native")]
pub fn apply_plan(request: &ApplyRequest) -> Result<ApplyOutcome, SyncError> {
    let canonical = request.root.canonicalize().map_err(SyncError::io)?;
    crate::fsutil::require_real_dir(&canonical)?;
    apply_plan_with(
        &muninn_core::FsStore::new(canonical),
        &state_home::NativeStateHome::new(request.state_dir.clone()),
        request,
    )
}

/// Apply a plan whose vault bytes live behind `store` and whose journal and
/// checkpoints live in `home`.
pub(crate) fn apply_plan_with(
    store: &dyn VaultStore,
    home: &dyn StateHome,
    request: &ApplyRequest,
) -> Result<ApplyOutcome, SyncError> {
    let mut journal = if request.journal.files.is_empty() {
        let mut journal = request.journal.clone();
        journal.files = journal_files(&request.writes, &request.deletes);
        journal.phase = JournalPhase::Applying;
        journal
    } else {
        request.journal.clone()
    };
    for file in &journal.files {
        let path = RelativePath::parse(&file.path)?;
        if request.protected.contains(&path) {
            return Err(SyncError::DirtyBuffer {
                path: path.to_string(),
            });
        }
    }
    preflight(store, &journal)?;
    journal.store_from(home)?;
    for file in &journal.files {
        let path = RelativePath::parse(&file.path)?;
        if request.protected.contains(&path) {
            return Err(SyncError::DirtyBuffer {
                path: path.to_string(),
            });
        }
    }
    preflight(store, &journal)?;
    let mut completed = journal
        .files
        .iter()
        .filter(|file| file.step == FileStep::Done)
        .count();
    for index in 0..journal.files.len() {
        if journal.files[index].step == FileStep::Done {
            continue;
        }
        checkpoint_one(store, home, &mut journal, index)?;
        if request.fault_after_checkpoint == Some(index + 1) {
            return Ok(ApplyOutcome::Faulted {
                completed_files: completed,
            });
        }
        replace_one(store, home, request, &mut journal, index)?;
        completed += 1;
        if request.fault_after_files == Some(completed) {
            return Ok(ApplyOutcome::Faulted {
                completed_files: completed,
            });
        }
    }
    journal.phase = JournalPhase::Applied;
    journal.store_from(home)?;
    Ok(ApplyOutcome::Applied)
}

#[cfg(feature = "native")]
pub fn resume_apply(request: &ApplyRequest) -> Result<ApplyOutcome, SyncError> {
    let home = state_home::NativeStateHome::new(request.state_dir.clone());
    let Some(saved) = Journal::load_from(&home)? else {
        return Err(SyncError::Journal {
            detail: "there is no journal to resume".to_owned(),
        });
    };
    if !saved.needs_apply_resume() && saved.phase != JournalPhase::Applying {
        return Err(SyncError::Journal {
            detail: "journal is not in a resumable phase".to_owned(),
        });
    }
    let request = ApplyRequest {
        journal: saved,
        writes: request.writes.clone(),
        deletes: request.deletes.clone(),
        root: request.root.clone(),
        state_dir: request.state_dir.clone(),
        protected: request.protected.clone(),
        fault_after_checkpoint: request.fault_after_checkpoint,
        fault_after_files: request.fault_after_files,
    };
    // Files are already recorded; do not rebuild them.
    if request.journal.files.is_empty() {
        return Err(SyncError::Journal {
            detail: "resumable journal has no files".to_owned(),
        });
    }
    let canonical = request.root.canonicalize().map_err(SyncError::io)?;
    crate::fsutil::require_real_dir(&canonical)?;
    apply_plan_with(&muninn_core::FsStore::new(canonical), &home, &request)
}

fn journal_files(writes: &[ApplyWrite], deletes: &[ApplyDelete]) -> Vec<JournalFile> {
    let mut files = Vec::new();
    for write in writes {
        files.push(JournalFile {
            path: write.path.to_string(),
            action: FileAction::Write,
            before: write.before.map(|hash| hash.to_hex()),
            after: Some(write.after.to_hex()),
            step: FileStep::Pending,
            checkpoint: None,
        });
    }
    for delete in deletes {
        files.push(JournalFile {
            path: delete.path.to_string(),
            action: FileAction::Delete,
            before: Some(delete.before.to_hex()),
            after: None,
            step: FileStep::Pending,
            checkpoint: None,
        });
    }
    files
}

fn preflight(store: &dyn VaultStore, journal: &Journal) -> Result<(), SyncError> {
    for file in &journal.files {
        let path = RelativePath::parse(&file.path)?;
        let current = file_hash_with(store, &path)?;
        let expected = match file.step {
            FileStep::Done => file.after.as_deref(),
            FileStep::Pending | FileStep::Checkpointed => file.before.as_deref(),
        };
        let expected = match expected {
            Some(hex_hash) => Some(ContentHash::parse(hex_hash)?),
            None => None,
        };
        if current != expected {
            return Err(SyncError::LocalChanged {
                path: path.to_string(),
            });
        }
    }
    Ok(())
}

fn checkpoint_one(
    store: &dyn VaultStore,
    home: &dyn StateHome,
    journal: &mut Journal,
    index: usize,
) -> Result<(), SyncError> {
    if journal.files[index].step != FileStep::Pending {
        return Ok(());
    }
    let path = RelativePath::parse(&journal.files[index].path)?;
    if journal.files[index].before.is_some() {
        let bytes = read_tracked_file(store, &path, crate::limits::DEFAULT_LIMITS.max_file_bytes)?;
        let key = state_home::checkpoint_key(&journal.files[index].path);
        home.put(&key, &bytes).map_err(SyncError::io)?;
        // The journal still records a path relative to the state dir: the
        // checkpoint key is that same path, so older journals resume unchanged.
        journal.files[index].checkpoint = Some(key);
    }
    journal.files[index].step = FileStep::Checkpointed;
    journal.phase = JournalPhase::Applying;
    journal.store_from(home)
}

fn replace_one(
    store: &dyn VaultStore,
    home: &dyn StateHome,
    request: &ApplyRequest,
    journal: &mut Journal,
    index: usize,
) -> Result<(), SyncError> {
    let path = RelativePath::parse(&journal.files[index].path)?;
    match journal.files[index].action {
        FileAction::Write => {
            let write = request
                .writes
                .iter()
                .find(|write| write.path == path)
                .ok_or_else(|| SyncError::Journal {
                    detail: format!("staged bytes for {path} are missing"),
                })?;
            if ContentHash::of(&write.bytes).to_hex()
                != journal.files[index].after.clone().unwrap_or_default()
            {
                return Err(SyncError::Journal {
                    detail: format!("staged bytes for {path} do not match the journal"),
                });
            }
            write_vault_file(store, &path, &write.bytes)?;
        }
        FileAction::Delete => remove_vault_file(store, &path)?,
    }
    journal.files[index].step = FileStep::Done;
    journal.store_from(home)
}

/// Replace one vault file: create the folders the path implies, refuse to
/// replace a folder or a link, then write atomically through the store.
fn write_vault_file(
    store: &dyn VaultStore,
    rel: &RelativePath,
    bytes: &[u8],
) -> Result<(), SyncError> {
    if let Some(parent) = rel.parent() {
        store
            .create_dir_all(&parent.under(store.root()))
            .map_err(SyncError::io)?;
    }
    let path = rel.under(store.root());
    match store.symlink_metadata(&path) {
        Ok(meta) if meta.symlink => {
            return Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "symlink".to_owned(),
            });
        }
        Ok(meta) if !meta.is_file() => {
            return Err(SyncError::Unsupported {
                path: rel.to_string(),
                detail: "refusing to replace a non-file".to_owned(),
            });
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(SyncError::io(error)),
    }
    store.write_atomic(&path, bytes).map_err(SyncError::io)
}

/// Remove one vault file. A missing file is success; a folder, link, or hard
/// link is refused.
fn remove_vault_file(store: &dyn VaultStore, rel: &RelativePath) -> Result<(), SyncError> {
    let path = rel.under(store.root());
    match store.symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(SyncError::io(error)),
        Ok(meta) if meta.symlink => Err(SyncError::Unsupported {
            path: rel.to_string(),
            detail: "symlink".to_owned(),
        }),
        Ok(meta) if !meta.is_file() || meta.links > 1 => Err(SyncError::Unsupported {
            path: rel.to_string(),
            detail: "refusing to delete a directory, link, or hard link".to_owned(),
        }),
        Ok(_) => store.remove_file(&path).map_err(SyncError::io),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::JournalPhase;
    use crate::model::{BindingId, ContentHash, OperationId, RunId};
    use std::fs;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "muninn-apply-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn journal() -> Journal {
        Journal {
            schema: crate::SCHEMA_VERSION,
            run_id: RunId::fresh().as_str().to_owned(),
            operation_id: OperationId::fresh().as_str().to_owned(),
            binding_id: BindingId::fresh().as_str().to_owned(),
            vault_identity: "abc".to_owned(),
            provider: "github".to_owned(),
            phase: JournalPhase::Published,
            remote_head: Some("head".to_owned()),
            published: true,
            files: Vec::new(),
        }
    }

    #[test]
    fn apply_replaces_a_file_and_resume_finishes_after_a_crash() {
        let root = scratch("root");
        let state = scratch("state");
        fs::write(root.join("a.md"), "old").unwrap();
        let bytes = b"new".to_vec();
        let request = ApplyRequest {
            root: root.clone(),
            state_dir: state.clone(),
            journal: journal(),
            writes: vec![ApplyWrite {
                path: RelativePath::parse("a.md").unwrap(),
                before: Some(ContentHash::of(b"old")),
                after: ContentHash::of(&bytes),
                bytes: bytes.clone(),
            }],
            deletes: Vec::new(),
            protected: BTreeSet::new(),
            fault_after_checkpoint: None,
            fault_after_files: Some(0),
        };
        // fault_after_files Some(0) would fire before any file if we compare
        // after increment. Use checkpoint fault instead.
        let mut request = request;
        request.fault_after_files = None;
        request.fault_after_checkpoint = Some(1);
        let outcome = apply_plan(&request).unwrap();
        assert_eq!(outcome, ApplyOutcome::Faulted { completed_files: 0 });
        assert_eq!(fs::read(root.join("a.md")).unwrap(), b"old");
        let saved = Journal::load(&state).unwrap().unwrap();
        assert_eq!(saved.files[0].step, FileStep::Checkpointed);
        assert!(saved.files[0].checkpoint.is_some());

        request.fault_after_checkpoint = None;
        request.journal = saved;
        let outcome = resume_apply(&request).unwrap();
        assert_eq!(outcome, ApplyOutcome::Applied);
        assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "new");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn dirty_path_blocks_before_any_write() {
        let root = scratch("dirty-root");
        let state = scratch("dirty-state");
        fs::write(root.join("a.md"), "old").unwrap();
        fs::write(root.join("b.md"), "keep").unwrap();
        let mut protected = BTreeSet::new();
        protected.insert(RelativePath::parse("a.md").unwrap());
        let bytes = b"new".to_vec();
        let request = ApplyRequest {
            root: root.clone(),
            state_dir: state.clone(),
            journal: journal(),
            writes: vec![
                ApplyWrite {
                    path: RelativePath::parse("a.md").unwrap(),
                    before: Some(ContentHash::of(b"old")),
                    after: ContentHash::of(&bytes),
                    bytes,
                },
                ApplyWrite {
                    path: RelativePath::parse("b.md").unwrap(),
                    before: Some(ContentHash::of(b"keep")),
                    after: ContentHash::of(b"keep"),
                    bytes: b"keep".to_vec(),
                },
            ],
            deletes: Vec::new(),
            protected,
            fault_after_checkpoint: None,
            fault_after_files: None,
        };
        let error = apply_plan(&request).unwrap_err();
        assert!(matches!(error, SyncError::DirtyBuffer { .. }));
        assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "old");
        assert_eq!(fs::read_to_string(root.join("b.md")).unwrap(), "keep");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn delete_keeps_a_checkpoint_outside_the_vault() {
        let root = scratch("del-root");
        let state = scratch("del-state");
        fs::write(root.join("gone.md"), "body").unwrap();
        let request = ApplyRequest {
            root: root.clone(),
            state_dir: state.clone(),
            journal: journal(),
            writes: Vec::new(),
            deletes: vec![ApplyDelete {
                path: RelativePath::parse("gone.md").unwrap(),
                before: ContentHash::of(b"body"),
            }],
            protected: BTreeSet::new(),
            fault_after_checkpoint: None,
            fault_after_files: None,
        };
        assert_eq!(apply_plan(&request).unwrap(), ApplyOutcome::Applied);
        assert!(!root.join("gone.md").exists());
        let saved = Journal::load(&state).unwrap().unwrap();
        let checkpoint = state.join(saved.files[0].checkpoint.as_ref().unwrap());
        assert_eq!(fs::read_to_string(checkpoint).unwrap(), "body");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&state);
    }

    /// The store path writes a new nested file with exactly the bytes and the
    /// path the plan names, creates its folder, and leaves no temp file behind.
    #[test]
    fn a_store_apply_writes_the_planned_bytes_and_path() {
        let root = scratch("store-root");
        let state = scratch("store-state");
        let store = muninn_core::FsStore::new(root.canonicalize().unwrap());
        let home = state_home::NativeStateHome::new(state.clone());
        let bytes = b"# New\n\nbody\n".to_vec();
        let request = ApplyRequest {
            root: root.clone(),
            state_dir: state.clone(),
            journal: journal(),
            writes: vec![ApplyWrite {
                path: RelativePath::parse("notes/deep/New.md").unwrap(),
                before: None,
                after: ContentHash::of(&bytes),
                bytes: bytes.clone(),
            }],
            deletes: Vec::new(),
            protected: BTreeSet::new(),
            fault_after_checkpoint: None,
            fault_after_files: None,
        };
        assert_eq!(
            apply_plan_with(&store, &home, &request).unwrap(),
            ApplyOutcome::Applied
        );
        assert_eq!(fs::read(root.join("notes/deep/New.md")).unwrap(), bytes);
        let names: Vec<_> = fs::read_dir(root.join("notes/deep"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["New.md"], "no temp file is left behind");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&state);
    }
}

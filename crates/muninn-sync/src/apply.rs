//! Apply a planned snapshot one file at a time.
//!
//! Each file is checkpointed, journaled, then replaced. A crash resumes at the
//! next file that is not marked done. Dirty editor paths and hash mismatches
//! abort before the first byte changes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::error::SyncError;
use crate::fsutil::{self, RootDir};
use crate::inventory::file_hash;
use crate::journal::{FileAction, FileStep, Journal, JournalFile, JournalPhase};
use crate::model::ContentHash;
use crate::pathutil::RelativePath;

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
    pub root: PathBuf,
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

pub fn apply_plan(request: &ApplyRequest) -> Result<ApplyOutcome, SyncError> {
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
    preflight(&request.root, &journal)?;
    journal.store(&request.state_dir)?;
    for file in &journal.files {
        let path = RelativePath::parse(&file.path)?;
        if request.protected.contains(&path) {
            return Err(SyncError::DirtyBuffer {
                path: path.to_string(),
            });
        }
    }
    preflight(&request.root, &journal)?;
    let canonical = request.root.canonicalize().map_err(SyncError::io)?;
    let dir = fsutil::open_root(&canonical)?;
    let mut completed = journal
        .files
        .iter()
        .filter(|file| file.step == FileStep::Done)
        .count();
    for index in 0..journal.files.len() {
        if journal.files[index].step == FileStep::Done {
            continue;
        }
        checkpoint_one(&request.state_dir, &dir, &mut journal, index)?;
        if request.fault_after_checkpoint == Some(index + 1) {
            return Ok(ApplyOutcome::Faulted {
                completed_files: completed,
            });
        }
        replace_one(request, &dir, &mut journal, index)?;
        completed += 1;
        if request.fault_after_files == Some(completed) {
            return Ok(ApplyOutcome::Faulted {
                completed_files: completed,
            });
        }
    }
    journal.phase = JournalPhase::Applied;
    journal.store(&request.state_dir)?;
    Ok(ApplyOutcome::Applied)
}

pub fn resume_apply(request: &ApplyRequest) -> Result<ApplyOutcome, SyncError> {
    let Some(saved) = Journal::load(&request.state_dir)? else {
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
    apply_plan(&request)
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

fn preflight(root: &Path, journal: &Journal) -> Result<(), SyncError> {
    for file in &journal.files {
        let path = RelativePath::parse(&file.path)?;
        let current = file_hash(root, &path)?;
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
    state_dir: &Path,
    dir: &RootDir,
    journal: &mut Journal,
    index: usize,
) -> Result<(), SyncError> {
    if journal.files[index].step != FileStep::Pending {
        return Ok(());
    }
    let path = RelativePath::parse(&journal.files[index].path)?;
    if journal.files[index].before.is_some() {
        let bytes = dir.read_file(&path, crate::limits::DEFAULT_LIMITS.max_file_bytes)?;
        let rel_checkpoint = format!("checkpoints/{}", journal.files[index].path);
        let dest = state_dir.join(&rel_checkpoint);
        fsutil::write_outside(&dest, &bytes)?;
        journal.files[index].checkpoint = Some(rel_checkpoint);
    }
    journal.files[index].step = FileStep::Checkpointed;
    journal.phase = JournalPhase::Applying;
    journal.store(state_dir)
}

fn replace_one(
    request: &ApplyRequest,
    dir: &RootDir,
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
            dir.write_file(&path, &write.bytes)?;
        }
        FileAction::Delete => dir.remove_file(&path)?,
    }
    journal.files[index].step = FileStep::Done;
    journal.store(&request.state_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::JournalPhase;
    use crate::model::{BindingId, ContentHash, OperationId, RunId};
    use std::fs;

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
}

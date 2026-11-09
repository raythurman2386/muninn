//! Crash-safe journal stored outside the vault. A run that dies mid-apply
//! resumes from the last recorded file instead of guessing.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::SCHEMA_VERSION;
use crate::error::SyncError;
use crate::fsutil;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalPhase {
    Preparing,
    Published,
    Applying,
    Applied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStep {
    Pending,
    Checkpointed,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Write,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalFile {
    pub path: String,
    pub action: FileAction,
    pub before: Option<String>,
    pub after: Option<String>,
    pub step: FileStep,
    /// Path of the preimage, relative to the binding state directory.
    pub checkpoint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub schema: u32,
    pub run_id: String,
    pub operation_id: String,
    pub binding_id: String,
    pub vault_identity: String,
    pub provider: String,
    pub phase: JournalPhase,
    pub remote_head: Option<String>,
    pub published: bool,
    pub files: Vec<JournalFile>,
}

impl Journal {
    #[must_use]
    pub fn path(state_dir: &Path) -> PathBuf {
        state_dir.join("journal.json")
    }

    pub fn load(state_dir: &Path) -> Result<Option<Self>, SyncError> {
        let path = Self::path(state_dir);
        if !path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).map_err(SyncError::io)?;
        let journal: Self = serde_json::from_str(&text).map_err(|_| SyncError::Journal {
            detail: "journal.json is corrupt; sync is paused until it is repaired".to_owned(),
        })?;
        if journal.schema != SCHEMA_VERSION {
            return Err(SyncError::Journal {
                detail: format!("journal schema {} is not supported", journal.schema),
            });
        }
        Ok(Some(journal))
    }

    pub fn store(&self, state_dir: &Path) -> Result<(), SyncError> {
        if self.schema != SCHEMA_VERSION {
            return Err(SyncError::Journal {
                detail: "refusing to write an unsupported journal schema".to_owned(),
            });
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| SyncError::Journal {
            detail: error.to_string(),
        })?;
        fsutil::write_outside(&Self::path(state_dir), &bytes)
    }

    pub fn clear(state_dir: &Path) -> Result<(), SyncError> {
        let path = Self::path(state_dir);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(SyncError::io(error)),
        }
    }

    /// True when a publish was accepted (or an apply started) and local files
    /// still need to be brought in line. A `Preparing` journal is not resumed;
    /// the next Sync Now plans again.
    #[must_use]
    pub fn needs_apply_resume(&self) -> bool {
        matches!(self.phase, JournalPhase::Published | JournalPhase::Applying)
    }
}

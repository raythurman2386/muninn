//! Crash-safe journal stored outside the vault. A run that dies mid-apply
//! resumes from the last recorded file instead of guessing.

#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

use muninn_core::StateHome;
use serde::{Deserialize, Serialize};

use crate::SCHEMA_VERSION;
use crate::error::SyncError;
use crate::state_home;
#[cfg(feature = "native")]
use crate::state_home::NativeStateHome;

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
    /// The native path of the journal for a binding state directory.
    #[cfg(feature = "native")]
    #[must_use]
    pub fn path(state_dir: &Path) -> PathBuf {
        state_dir.join(state_home::JOURNAL)
    }

    /// Load the journal from a native state directory.
    ///
    /// The path-based API stays for callers that only know a `state_dir`; the
    /// engine itself goes through [`Journal::load_from`] so its IO is
    /// injectable. Native only: a browser has no state path.
    #[cfg(feature = "native")]
    pub fn load(state_dir: &Path) -> Result<Option<Self>, SyncError> {
        Self::load_from(&NativeStateHome::new(state_dir.to_path_buf()))
    }

    /// Load the journal through the state seam.
    ///
    /// The browser worker calls this with its OPFS state home; the native
    /// engine calls it with `NativeStateHome`.
    pub fn load_from(home: &dyn StateHome) -> Result<Option<Self>, SyncError> {
        let Some(bytes) = home.get(state_home::JOURNAL) else {
            return Ok(None);
        };
        let text = std::str::from_utf8(&bytes).map_err(|_| SyncError::Journal {
            detail: "journal.json is corrupt; sync is paused until it is repaired".to_owned(),
        })?;
        Ok(Some(parse_journal(text)?))
    }

    /// Store the journal into a native state directory.
    #[cfg(feature = "native")]
    pub fn store(&self, state_dir: &Path) -> Result<(), SyncError> {
        self.store_from(&NativeStateHome::new(state_dir.to_path_buf()))
    }

    /// Store the journal through the state seam.
    pub fn store_from(&self, home: &dyn StateHome) -> Result<(), SyncError> {
        if self.schema != SCHEMA_VERSION {
            return Err(SyncError::Journal {
                detail: "refusing to write an unsupported journal schema".to_owned(),
            });
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| SyncError::Journal {
            detail: error.to_string(),
        })?;
        home.put(state_home::JOURNAL, &bytes).map_err(SyncError::io)
    }

    /// Clear the journal from a native state directory.
    #[cfg(feature = "native")]
    pub fn clear(state_dir: &Path) -> Result<(), SyncError> {
        Self::clear_from(&NativeStateHome::new(state_dir.to_path_buf()))
    }

    /// Clear the journal through the state seam. Removal is idempotent in the
    /// trait contract, so there is no special case for an absent journal.
    pub fn clear_from(home: &dyn StateHome) -> Result<(), SyncError> {
        home.remove(state_home::JOURNAL).map_err(SyncError::io)
    }

    /// True when a publish was accepted (or an apply started) and local files
    /// still need to be brought in line. A `Preparing` journal is not resumed;
    /// the next Sync Now plans again.
    #[must_use]
    pub fn needs_apply_resume(&self) -> bool {
        matches!(self.phase, JournalPhase::Published | JournalPhase::Applying)
    }
}

/// Parse and validate one `journal.json` document, without touching the disk.
fn parse_journal(text: &str) -> Result<Journal, SyncError> {
    let journal: Journal = serde_json::from_str(text).map_err(|_| SyncError::Journal {
        detail: "journal.json is corrupt; sync is paused until it is repaired".to_owned(),
    })?;
    if journal.schema != SCHEMA_VERSION {
        return Err(SyncError::Journal {
            detail: format!("journal schema {} is not supported", journal.schema),
        });
    }
    Ok(journal)
}

/// Parse a journal document from arbitrary text.
///
/// Fuzz harness only: exposes [`parse_journal`] to `fuzz/` under the
/// off-by-default `fuzzing` feature. It is not part of the sync API.
#[cfg(feature = "fuzzing")]
pub fn parse_journal_for_fuzzing(text: &str) -> Result<(), SyncError> {
    parse_journal(text).map(|_| ())
}

//! Shared sync values. Paths and hashes are typed so a plan cannot be built
//! from an unchecked string.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::SyncError;
use crate::pathutil::RelativePath;

/// SHA-256 of the raw file bytes. This is the logical identity across GitHub
/// and Drive; git blob SHAs stay inside the GitHub adapter.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Self(out)
    }

    pub fn parse(hex_str: &str) -> Result<Self, SyncError> {
        let bytes = hex::decode(hex_str).map_err(|_| SyncError::Protocol {
            detail: "content hash is not hex".to_owned(),
        })?;
        let array: [u8; 32] = bytes.try_into().map_err(|_| SyncError::Protocol {
            detail: "content hash is not 32 bytes".to_owned(),
        })?;
        Ok(Self(array))
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", &self.to_hex()[..12])
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.to_hex()[..12])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Markdown,
    Attachment,
}

impl FileKind {
    #[must_use]
    pub fn of_name(name: &str) -> Self {
        if crate::limits::is_markdown_name(name) {
            Self::Markdown
        } else {
            Self::Attachment
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    GitHub,
    GoogleDrive,
}

impl ProviderKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::GitHub => "github",
            ProviderKind::GoogleDrive => "google_drive",
        }
    }

    pub fn parse(text: &str) -> Result<Self, SyncError> {
        match text {
            "github" => Ok(ProviderKind::GitHub),
            "google_drive" => Ok(ProviderKind::GoogleDrive),
            _ => Err(SyncError::Config {
                detail: format!("unknown sync provider {text}"),
            }),
        }
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable id for one vault binding. Generated locally; not a secret.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BindingId(String);

impl BindingId {
    pub fn parse(text: &str) -> Result<Self, SyncError> {
        if text.len() < 8
            || text.len() > 80
            || !text.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
        {
            return Err(SyncError::Config {
                detail: "binding id is not a hex or uuid string".to_owned(),
            });
        }
        Ok(Self(text.to_owned()))
    }

    #[must_use]
    pub fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BindingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RunId(String);

impl RunId {
    #[must_use]
    pub fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OperationId(String);

impl OperationId {
    pub fn parse(text: &str) -> Result<Self, SyncError> {
        if text.len() < 8
            || text.len() > 80
            || !text.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
        {
            return Err(SyncError::Config {
                detail: "operation id is not a hex or uuid string".to_owned(),
            });
        }
        Ok(Self(text.to_owned()))
    }

    #[must_use]
    pub fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// SHA-256 of the canonical absolute path. Local to this machine: it only
/// has to agree for two paths that canonicalize to the same directory.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VaultIdentity(String);

impl VaultIdentity {
    pub fn of_canonical(path: &std::path::Path) -> Result<Self, SyncError> {
        let canonical = path.canonicalize().map_err(|error| SyncError::Io {
            detail: format!("could not canonicalize {}: {error}", path.display()),
        })?;
        if !canonical.is_dir() {
            return Err(SyncError::Inventory {
                detail: format!("{} is not a directory", canonical.display()),
            });
        }
        let bytes = canonical.as_os_str().as_encoded_bytes();
        Ok(Self(ContentHash::of(bytes).to_hex()))
    }

    /// Rebuild an identity already computed by [`VaultIdentity::of_canonical`].
    /// The text is the hex digest, not a filesystem path.
    #[must_use]
    pub fn from_known_hex(text: &str) -> Self {
        Self(text.to_owned())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRecord {
    pub hash: ContentHash,
    pub size: u64,
    pub kind: FileKind,
}

/// A complete logical snapshot. Absence of a path means the path is not in
/// this snapshot. Callers must not build this from a partial listing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub files: BTreeMap<RelativePath, FileRecord>,
}

impl Snapshot {
    #[must_use]
    pub fn get(&self, path: &RelativePath) -> Option<&FileRecord> {
        self.files.get(path)
    }

    #[must_use]
    pub fn hash_of(&self, path: &RelativePath) -> Option<ContentHash> {
        self.files.get(path).map(|file| file.hash)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Local,
    Remote,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictKind {
    EditEdit,
    EditDelete,
    DeleteEdit,
    AddAdd,
    RenameEdit,
    RenameDelete,
    RenameRename,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub path: RelativePath,
    pub kind: ConflictKind,
    pub base: Option<ContentHash>,
    pub local: Option<ContentHash>,
    pub remote: Option<ContentHash>,
    /// The other path involved in a rename conflict.
    pub other_path: Option<RelativePath>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tombstone {
    pub path: RelativePath,
    pub base_hash: ContentHash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    KeepLocal,
    UseRemote,
    KeepBoth,
}

/// What to do when there are no conflicts. `desired` is the full logical tree
/// both sides should converge to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub desired: Snapshot,
    pub conflicts: Vec<Conflict>,
    pub tombstones: Vec<Tombstone>,
    /// Local paths whose bytes must be replaced or created from remote bytes.
    pub pulls: Vec<RelativePath>,
    /// Local paths that must be removed after their preimage is checkpointed.
    pub local_deletes: Vec<RelativePath>,
    /// Paths whose local bytes must be published. Already-matching paths are absent.
    pub pushes: Vec<RelativePath>,
    pub renames: Vec<Rename>,
    /// True when `base` was `None`. Deletes are impossible in that plan.
    pub first_bind: bool,
}

impl Plan {
    #[must_use]
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }

    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.conflicts.is_empty()
            && self.pulls.is_empty()
            && self.pushes.is_empty()
            && self.local_deletes.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    pub from: RelativePath,
    pub to: RelativePath,
    pub hash: ContentHash,
    pub side: Side,
}

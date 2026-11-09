//! What a sync run includes, and the hard stops that fail the run instead of
//! skipping a file and reporting success.

use crate::pathutil::RelativePath;

/// Fixed contract for this version. Changing a number is a behavior change
/// and belongs in the changelog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_files: usize,
    /// Directory depth of a file, counting the filename. `a/b/c.md` is 3.
    pub max_depth: usize,
    pub max_path_bytes: usize,
}

/// 20 MiB keeps a single file under GitHub's blob limit with room for base64,
/// and under a reasonable Drive resumable chunk. The total cap is the whole
/// included vault, not a silent per-file skip.
pub const DEFAULT_LIMITS: Limits = Limits {
    max_file_bytes: 20 * 1024 * 1024,
    max_total_bytes: 200 * 1024 * 1024,
    max_files: 5_000,
    max_depth: 12,
    max_path_bytes: 240,
};

/// A path left out on purpose. The run still reports these; they are not a
/// quiet success.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exclusion {
    pub path: String,
    pub reason: ExclusionReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExclusionReason {
    /// Dotfile or dot-directory: `.git`, `.muninn-trash`, editor swap files.
    Hidden,
    /// `*~`, `*.tmp`, `*.temp`, `.#*`.
    Temporary,
    /// `Thumbs.db` and `desktop.ini`. OS junk, not vault content.
    OperatingSystem,
}

impl ExclusionReason {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ExclusionReason::Hidden => "hidden or app state",
            ExclusionReason::Temporary => "temporary file",
            ExclusionReason::OperatingSystem => "operating-system metadata",
        }
    }
}

/// Classify a single path component. `None` means the component is included
/// and must be walked. This is the whole exclusion list.
#[must_use]
pub fn component_exclusion(name: &str) -> Option<ExclusionReason> {
    if name.starts_with('.') || name.starts_with(".#") || name.starts_with('~') {
        // A leading dot is hidden app/VCS state. `.#` and `~` are editor temps
        // when they are the prefix; a leading dot already covers `.#`.
        if name.starts_with('.') {
            return Some(ExclusionReason::Hidden);
        }
        return Some(ExclusionReason::Temporary);
    }
    let lower = name.to_ascii_lowercase();
    if lower == "thumbs.db" || lower == "desktop.ini" {
        return Some(ExclusionReason::OperatingSystem);
    }
    if lower.ends_with('~') || lower.ends_with(".tmp") || lower.ends_with(".temp") {
        return Some(ExclusionReason::Temporary);
    }
    None
}

/// Markdown notes, matched the same way the vault index matches them.
#[must_use]
pub fn is_markdown_name(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("md"))
}

/// A path the contract includes: not excluded, and every component is portable.
pub fn included_path(rel: &RelativePath) -> bool {
    rel.components()
        .all(|part| component_exclusion(part).is_none())
}

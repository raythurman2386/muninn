//! Portable relative paths. One validator, used for config, listings, and writes.
//!
//! A path that fails here never reaches the filesystem join. The rules are the
//! intersection of Windows, macOS, and Linux so a vault synced on one machine
//! can be checked out on another.

use std::fmt;
use std::path::{Component, Path};

use unicode_normalization::UnicodeNormalization;

use crate::error::SyncError;
use crate::limits::DEFAULT_LIMITS;

/// A vault-relative path using `/` separators. The inner string is NFC, has no
/// `.` or `..`, and has already passed [`RelativePath::parse`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativePath(String);

impl RelativePath {
    pub fn parse(raw: &str) -> Result<Self, SyncError> {
        validate(raw)?;
        let nfc: String = raw.nfc().collect();
        if nfc != raw {
            return Err(SyncError::Path {
                detail: format!("{raw} is not Unicode NFC; sync will not rename it silently"),
            });
        }
        Ok(Self(nfc))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn components(&self) -> std::str::Split<'_, char> {
        self.0.split('/')
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        self.components().count()
    }

    #[must_use]
    pub fn file_name(&self) -> &str {
        self.components().next_back().unwrap_or("")
    }

    /// Parent relative path, if this is not a single component.
    #[must_use]
    pub fn parent(&self) -> Option<RelativePath> {
        let (parent, _) = self.0.rsplit_once('/')?;
        RelativePath::parse(parent).ok()
    }

    /// Case-folded NFC key for collision checks. Rust's `to_lowercase` is the
    /// Unicode lowercase mapping, which is what we compare across devices.
    #[must_use]
    pub fn fold_key(&self) -> String {
        self.0.to_lowercase()
    }

    /// Join onto a canonical root for display and for the final containment
    /// check. The caller still has to open the path without following links.
    #[must_use]
    pub fn under(&self, root: &Path) -> std::path::PathBuf {
        let mut out = root.to_path_buf();
        for part in self.components() {
            out.push(part);
        }
        out
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate(raw: &str) -> Result<(), SyncError> {
    if raw.is_empty() {
        return Err(SyncError::Path {
            detail: "path is empty".to_owned(),
        });
    }
    if raw.len() > DEFAULT_LIMITS.max_path_bytes {
        return Err(SyncError::Path {
            detail: format!(
                "{raw} is longer than {} bytes",
                DEFAULT_LIMITS.max_path_bytes
            ),
        });
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return Err(SyncError::Path {
            detail: format!("{raw} is absolute"),
        });
    }
    // Windows drive and UNC prefixes, including mixed separators.
    let upper = raw.as_bytes();
    if upper.len() >= 2 && upper[1] == b':' {
        return Err(SyncError::Path {
            detail: format!("{raw} has a drive prefix"),
        });
    }
    if raw.contains('\\') || raw.contains('\0') {
        return Err(SyncError::Path {
            detail: format!("{raw} contains a separator or NUL"),
        });
    }
    if raw.chars().any(|ch| ch.is_control() || ch == '\u{7f}') {
        return Err(SyncError::Path {
            detail: format!("{raw} contains a control character"),
        });
    }
    let parts: Vec<&str> = raw.split('/').collect();
    if parts.len() > DEFAULT_LIMITS.max_depth {
        return Err(SyncError::Path {
            detail: format!(
                "{raw} is nested more than {} levels",
                DEFAULT_LIMITS.max_depth
            ),
        });
    }
    for part in &parts {
        validate_component(raw, part)?;
    }
    // `Path` components catch any remaining `.` / `..` / prefix the string
    // scan missed, including redundant separators.
    let path = Path::new(raw);
    for component in path.components() {
        match component {
            Component::Normal(name) => {
                let text = name.to_string_lossy();
                if text != *parts.iter().find(|part| *part == &text).unwrap_or(&"") {
                    // Fall through to the explicit checks below when the
                    // component simply matches. A mismatch means the OS parser
                    // rewrote the path.
                }
            }
            _ => {
                return Err(SyncError::Path {
                    detail: format!("{raw} is not a portable relative path"),
                });
            }
        }
    }
    if path.components().count() != parts.len() {
        return Err(SyncError::Path {
            detail: format!("{raw} is not a portable relative path"),
        });
    }
    Ok(())
}

fn validate_component(full: &str, part: &str) -> Result<(), SyncError> {
    if part.is_empty() || part == "." || part == ".." {
        return Err(SyncError::Path {
            detail: format!("{full} contains an empty, '.' or '..' segment"),
        });
    }
    if part.ends_with(' ') || part.ends_with('.') {
        return Err(SyncError::Path {
            detail: format!("{full} has a segment ending in a space or dot"),
        });
    }
    if part.contains(':')
        || part.contains('*')
        || part.contains('?')
        || part.contains('"')
        || part.contains('<')
        || part.contains('>')
        || part.contains('|')
    {
        return Err(SyncError::Path {
            detail: format!("{full} contains a character that is not portable"),
        });
    }
    let stem = part.split('.').next().unwrap_or(part);
    let reserved = matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if reserved {
        return Err(SyncError::Path {
            detail: format!("{full} uses a reserved device name"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_nested_note_and_attachment() {
        let note = RelativePath::parse("folder/Note.md").unwrap();
        assert_eq!(note.depth(), 2);
        assert_eq!(note.file_name(), "Note.md");
        assert_eq!(note.parent().unwrap().as_str(), "folder");
        assert!(RelativePath::parse("images/pic.png").is_ok());
    }

    #[test]
    fn rejects_traversal_and_windows_traps() {
        for raw in [
            "",
            "/etc/passwd",
            "\\\\server\\share",
            "C:/notes",
            "C:notes",
            "..",
            "../x",
            "a/../../b",
            "a/./b",
            "a//b",
            "a\\b",
            "a/b ",
            "a/b.",
            "CON",
            "con.txt",
            "aux.md",
            "folder/LPT1.md",
            "has:stream",
            "star*.md",
            "a/\u{0}b",
            "a/\n",
        ] {
            assert!(
                RelativePath::parse(raw).is_err(),
                "{raw} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_non_nfc() {
        // U+00E9 is NFC; U+0065 U+0301 is the same character in NFD.
        let nfd = "cafe\u{0301}.md";
        assert!(RelativePath::parse(nfd).is_err());
        assert!(RelativePath::parse("caf\u{e9}.md").is_ok());
    }
}

//! SAF-05 crash recovery: keep only dirty editor buffers outside the vault.
//!
//! The vault file is still the source of truth. This module is a best-effort
//! safety net for the window between the first edit and autosave (or while a
//! conflict/IO error keeps autosave paused): every dirty buffer is written to
//! the XDG state directory with the same temp-file-then-rename pattern as note
//! saves, so a `kill -9` (or a power loss) leaves either the previous copy or
//! the newest complete copy, never a truncated file.
//!
//! Layout:
//!
//! ```text
//! $XDG_STATE_HOME/muninn/recovery/<sanitized-rel>.txt
//! $XDG_STATE_HOME/muninn/recovery/<sanitized-rel>.json
//! ```
//!
//! The `.json` sidecar stores the exact vault-relative path, a Unix timestamp
//! and a stable FNV-1a hash of the canonical vault root (`vault`). The vault
//! id keeps two vaults that both contain `Alpha.md` from listing each other's
//! edits when they share the same user state directory. The sanitized file
//! name (slashes become `__`, a length cap appends a hash) is only a safe,
//! collision-resistant key. Buffer text lives in the `.txt` file exactly as
//! the user typed it.
//!
//! A recovery file is removed as soon as the buffer reaches disk or the user
//! discards it. Clean buffers are never written here.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Distinguishes temp files written by the same process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Longest sanitized stem before a hash is appended to keep names portable.
const NAME_CAP: usize = 120;

/// One recoverable dirty buffer, as listed by the startup banner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryEntry {
    /// Vault-relative note path (`folder/Name.md`).
    pub rel: String,
    /// Unix seconds when the buffer was last persisted.
    pub ts: u64,
}

/// The shared recovery directory, scoped to one vault by `vault`.
#[derive(Clone, Debug)]
pub(crate) struct RecoveryStore {
    dir: PathBuf,
    /// Stable id of the vault this store may list.
    vault: String,
}

impl RecoveryStore {
    /// Build a store for `vault_root` below an explicit state home. Tests use
    /// this to keep recovery files out of the real user state directory.
    pub(crate) fn at(state_home: &Path, vault_root: &Path) -> Self {
        Self {
            dir: state_home.join("muninn").join("recovery"),
            vault: vault_id(vault_root),
        }
    }

    /// The buffer file for `rel`.
    #[must_use]
    pub(crate) fn path_for(&self, rel: &str) -> PathBuf {
        self.key_for(rel, "txt")
    }

    fn meta_for(&self, rel: &str) -> PathBuf {
        self.key_for(rel, "json")
    }

    fn key_for(&self, rel: &str, extension: &str) -> PathBuf {
        self.dir.join(format!("{}.{extension}", sanitize(rel)))
    }

    /// Persist a dirty buffer. Only called for text that differs from the
    /// last saved copy; the caller owns debouncing.
    pub(crate) fn save(&self, rel: &str, text: &str) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        // The buffer is committed before the sidecar so a crash between the
        // two writes cannot advertise a recovery file that has no text.
        atomic_write(&self.path_for(rel), text.as_bytes())?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let meta = format!(
            "{{\"rel\":\"{}\",\"vault\":\"{}\",\"ts\":{ts}}}\n",
            escape(rel),
            self.vault
        );
        atomic_write(&self.meta_for(rel), meta.as_bytes())
    }

    /// Forget `rel`'s recovery copy. Missing files are not an error.
    pub(crate) fn clear(&self, rel: &str) {
        let _ = fs::remove_file(self.path_for(rel));
        let _ = fs::remove_file(self.meta_for(rel));
    }

    /// Read a persisted buffer back.
    pub(crate) fn load(&self, rel: &str) -> io::Result<String> {
        fs::read_to_string(self.path_for(rel))
    }

    /// List the recoverable buffers, newest last. Orphaned `.txt` files (no
    /// sidecar, for example from an interrupted first-ever write) are ignored.
    pub(crate) fn scan(&self) -> Vec<RecoveryEntry> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(meta) = fs::read_to_string(&path) else {
                continue;
            };
            if parse_string(&meta, "vault").as_deref() != Some(self.vault.as_str()) {
                continue;
            }
            let (Some(rel), Some(ts)) = (parse_rel(&meta), parse_ts(&meta)) else {
                continue;
            };
            if !self.path_for(&rel).is_file() {
                continue;
            }
            found.push(RecoveryEntry { rel, ts });
        }
        found.sort_by(|a, b| a.ts.cmp(&b.ts).then_with(|| a.rel.cmp(&b.rel)));
        found.dedup_by(|a, b| a.rel == b.rel);
        found
    }
}

/// The XDG state root: `$XDG_STATE_HOME`, else `$HOME/.local/state`, else a
/// temp fallback for platforms without either variable.
pub(crate) fn state_home() -> PathBuf {
    crate::settings::xdg_dir("XDG_STATE_HOME", &[".local", "state"], "muninn-state")
}

/// Stable 64-bit FNV-1a of the canonical vault path.
fn vault_id(root: &Path) -> String {
    let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in canonical.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// A filesystem-safe, stable key for a vault-relative path. Slashes become
/// `__`; every other non-portable character becomes `_`. Names longer than
/// [`NAME_CAP`] keep a hash suffix so truncation cannot collide silently.
fn sanitize(rel: &str) -> String {
    let mut out = String::with_capacity(rel.len());
    for c in rel.chars() {
        match c {
            '/' | '\\' => out.push_str("__"),
            c if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') => out.push(c),
            _ => out.push('_'),
        }
    }
    if out.len() > NAME_CAP {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in rel.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        out.truncate(NAME_CAP);
        out.push_str(&format!("_{hash:016x}"));
    }
    out
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Extract `rel` from the sidecar the writer above produced. Kept deliberately
/// small: this is not a general JSON parser, only the one-object format here.
fn parse_rel(meta: &str) -> Option<String> {
    parse_string(meta, "rel")
}

fn parse_string(meta: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = meta.find(&needle)? + needle.len();
    let mut chars = meta[start..].chars();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16).ok()?;
                    out.push(char::from_u32(code)?);
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

fn parse_ts(meta: &str) -> Option<u64> {
    let start = meta.find("\"ts\":")? + "\"ts\":".len();
    let digits: String = meta[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Write `data` beside `path` and rename it into place. `sync_all` makes a
/// `kill -9` safe even when the file system is replaying a journal.
fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("recovery");
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_file_name(format!(".{name}.tmp-{}-{counter}", std::process::id()));
    let result = (|| {
        let mut file = File::create(&temp)?;
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_names_bounded_and_distinct() {
        assert_eq!(sanitize("folder/Note.md"), "folder__Note.md");
        let long_a = format!("{}.md", "a".repeat(400));
        let long_b = format!("{}.md", "b".repeat(400));
        let a = sanitize(&long_a);
        let b = sanitize(&long_b);
        assert!(a.len() <= NAME_CAP + "_".len() + 16);
        assert_ne!(a, b, "a length cap must not merge different paths");
    }

    #[test]
    fn sidecar_round_trips_paths_with_quotes_and_unicode() {
        let rel = "Café/\"odd\"\\name.md";
        let text = format!(
            "{{\"rel\":\"{}\",\"vault\":\"abc\",\"ts\":42}}\n",
            escape(rel)
        );
        assert_eq!(parse_rel(&text).as_deref(), Some(rel));
        assert_eq!(parse_string(&text, "vault").as_deref(), Some("abc"));
        assert_eq!(parse_ts(&text), Some(42));
    }
}

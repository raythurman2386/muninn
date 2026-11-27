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
//! $XDG_STATE_HOME/muninn/recovery/<vault-id>~<sanitized-rel>.txt
//! $XDG_STATE_HOME/muninn/recovery/<vault-id>~<sanitized-rel>.json
//! $XDG_STATE_HOME/muninn/recovery/<vault-id>~<sanitized-rel>~kanban.txt
//! $XDG_STATE_HOME/muninn/recovery/<vault-id>~<sanitized-rel>~kanban.json
//! ```
//!
//! The `~kanban` pair is the Kanban page's own copy of that note. Write's
//! recovery autosave writes the first pair and never the second, so a crash
//! while Write still has unsaved text keeps the card edit as well.
//!
//! The `.json` sidecar stores the exact vault-relative path, a Unix timestamp
//! and a stable FNV-1a hash of the canonical vault root (`vault`). The vault
//! id keeps two vaults that both contain `Alpha.md` from listing each other's
//! edits when they share the same user state directory, and the file names
//! carry it too (#38 review M1), so saving or dirtying `Tools.excalidraw` in
//! one vault never overwrites or deletes another vault's copy of the same
//! relative path. Records in the older shared layout (`<sanitized-rel>.*`,
//! no vault prefix) are moved to their own vault's names when that vault
//! opens, so a pending recovery is never orphaned. The sanitized file
//! name (slashes become `__`, a length cap appends a hash) is only a safe,
//! collision-resistant key. Buffer text lives in the `.txt` file exactly as
//! the user typed it.
//!
//! A recovery file is removed as soon as the buffer reaches disk or the user
//! discards it. Clean buffers are never written here. Neither is a note whose
//! on-disk source is a locked envelope (per-note encryption, #132): its
//! plaintext must never reach this directory, so its copies are cleared
//! whenever a vault becomes live and never written.

#[cfg(feature = "native")]
use std::fs::{self, File};
use std::io;
#[cfg(feature = "native")]
use std::io::Write as _;
use std::path::{Path, PathBuf};
#[cfg(not(feature = "native"))]
use std::sync::Arc;
#[cfg(feature = "native")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "native")]
use std::time::{SystemTime, UNIX_EPOCH};

use muninn_core::StateHome;

/// Distinguishes temp files written by the same process.
#[cfg(feature = "native")]
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Longest sanitized stem before a hash is appended to keep names portable.
const NAME_CAP: usize = 120;

/// Which buffer a recovery file holds. Write and the Kanban page can both
/// have unsaved text for one note; each has its own file so one autosave
/// cannot replace the other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecoverySlot {
    /// The editor buffer (Write, and a board's drawing).
    #[default]
    Editor,
    /// The Kanban page's unsaved card edits for the same note.
    Kanban,
}

/// One recoverable dirty buffer, as listed by the startup banner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryEntry {
    /// Vault-relative note path (`folder/Name.md`).
    pub rel: String,
    /// Unix seconds when the buffer was last persisted.
    pub ts: u64,
    /// Which of the note's buffers this copy is.
    pub slot: RecoverySlot,
}

/// The shared recovery directory, scoped to one vault by `vault`.
#[derive(Clone)]
pub(crate) struct RecoveryStore {
    dir: PathBuf,
    /// Stable id of the vault this store may list.
    vault: String,
    /// The browser's durable store (#240 slice 6d). `None` when OPFS could not
    /// be mounted, which keeps the honest refusal: an unavailable store is
    /// never reported as "gone".
    #[cfg(not(feature = "native"))]
    home: Option<Arc<dyn StateHome>>,
}

impl std::fmt::Debug for RecoveryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryStore")
            .field("dir", &self.dir)
            .field("vault", &self.vault)
            .finish_non_exhaustive()
    }
}

impl RecoveryStore {
    /// Build a store for `vault_root` below an explicit state home. Tests use
    /// this to keep recovery files out of the real user state directory.
    pub(crate) fn at(state_home: &Path, vault_root: &Path) -> Self {
        let store = Self {
            dir: state_home.join("muninn").join("recovery"),
            vault: vault_id(vault_root),
            #[cfg(not(feature = "native"))]
            home: crate::state_home::registered(),
        };
        #[cfg(feature = "native")]
        store.adopt_legacy();
        store
    }

    /// Move this vault's records from the older shared layout to its own
    /// names. Other vaults' old records are left for them; best effort (a
    /// record that cannot be moved stays where it is and is not listed).
    #[cfg(feature = "native")]
    fn adopt_legacy(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // New names contain `~`, which `sanitize` never produces.
            if stem.contains('~')
                || stem.starts_with('.')
                || path.extension().and_then(|e| e.to_str()) != Some("json")
            {
                continue;
            }
            let Ok(meta) = fs::read_to_string(&path) else {
                continue;
            };
            if parse_string(&meta, "vault").as_deref() != Some(self.vault.as_str()) {
                continue;
            }
            let Some(rel) = parse_rel(&meta) else {
                continue;
            };
            if sanitize(&rel) != stem {
                continue;
            }
            // A copy under the new names was written after this one (only a
            // newer Muninn writes them), so the old record is stale: drop it
            // rather than leave it to come back once the new copy is cleared.
            if self.meta_for(&rel, RecoverySlot::Editor).exists() {
                let _ = fs::remove_file(path.with_extension("txt"));
                let _ = fs::remove_file(&path);
                continue;
            }
            let text = path.with_extension("txt");
            if text.is_file() && fs::rename(&text, self.path_for(&rel)).is_err() {
                continue;
            }
            let _ = fs::rename(&path, self.meta_for(&rel, RecoverySlot::Editor));
        }
    }

    /// The editor buffer file for `rel`.
    #[must_use]
    pub(crate) fn path_for(&self, rel: &str) -> PathBuf {
        self.path_for_slot(rel, RecoverySlot::Editor)
    }

    /// The buffer file for one of `rel`'s slots.
    #[must_use]
    pub(crate) fn path_for_slot(&self, rel: &str, slot: RecoverySlot) -> PathBuf {
        self.key_for(rel, slot, "txt")
    }

    #[cfg(feature = "native")]
    fn meta_for(&self, rel: &str, slot: RecoverySlot) -> PathBuf {
        self.key_for(rel, slot, "json")
    }

    fn key_for(&self, rel: &str, slot: RecoverySlot, extension: &str) -> PathBuf {
        self.dir
            .join(format!("{}.{extension}", key_name(&self.vault, rel, slot)))
    }

    /// Persist the editor's dirty buffer. Only called for text that differs
    /// from the last saved copy; the caller owns debouncing.
    pub(crate) fn save(&self, rel: &str, text: &str) -> io::Result<()> {
        self.save_slot(rel, RecoverySlot::Editor, text)
    }

    /// Persist one slot. The kanban slot is a different file from the
    /// editor's, so writing one leaves the other in place.
    #[cfg(feature = "native")]
    pub(crate) fn save_slot(&self, rel: &str, slot: RecoverySlot, text: &str) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        // The buffer is committed before the sidecar so a crash between the
        // two writes cannot advertise a recovery file that has no text.
        atomic_write(&self.path_for_slot(rel, slot), text.as_bytes())?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let meta = meta_json(&self.vault, rel, slot, ts);
        atomic_write(&self.meta_for(rel, slot), meta.as_bytes())
    }

    /// The browser has no filesystem, so the copy goes into the registered
    /// state home under the same layout the native directory uses (#240 slice
    /// 6d). An unavailable state home refuses — `Unsupported` keeps a quit from
    /// claiming unsaved edits are recoverable.
    #[cfg(not(feature = "native"))]
    pub(crate) fn save_slot(&self, rel: &str, slot: RecoverySlot, text: &str) -> io::Result<()> {
        web_save_slot(self.home.as_deref(), &self.vault, rel, slot, text)
    }

    /// Forget the editor's recovery copy. Missing files are not an error.
    pub(crate) fn clear(&self, rel: &str) {
        self.clear_slot(rel, RecoverySlot::Editor);
    }

    /// Forget one slot. The other slot for `rel` is left alone.
    pub(crate) fn clear_slot(&self, rel: &str, slot: RecoverySlot) {
        #[cfg(feature = "native")]
        {
            let _ = fs::remove_file(self.path_for_slot(rel, slot));
            let _ = fs::remove_file(self.meta_for(rel, slot));
        }
        #[cfg(not(feature = "native"))]
        web_clear_slot(self.home.as_deref(), &self.vault, rel, slot);
    }

    /// Read the editor's persisted buffer back.
    pub(crate) fn load(&self, rel: &str) -> io::Result<String> {
        self.load_slot(rel, RecoverySlot::Editor)
    }

    /// Read one slot's persisted buffer back.
    #[cfg(feature = "native")]
    pub(crate) fn load_slot(&self, rel: &str, slot: RecoverySlot) -> io::Result<String> {
        fs::read_to_string(self.path_for_slot(rel, slot))
    }

    #[cfg(not(feature = "native"))]
    pub(crate) fn load_slot(&self, rel: &str, slot: RecoverySlot) -> io::Result<String> {
        web_load_slot(self.home.as_deref(), &self.vault, rel, slot)
    }

    /// List the recoverable buffers, newest last. Orphaned `.txt` files (no
    /// sidecar, for example from an interrupted first-ever write) are ignored.
    #[cfg(feature = "native")]
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
            // Only this vault's own names; an old-layout record that could not
            // be adopted is not listed (its text is not where `load` looks).
            let own = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("{}~", self.vault)));
            if !own {
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
            let slot = slot_of(
                path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                &meta,
            );
            let text = self.path_for_slot(&rel, slot);
            if !text.is_file() {
                continue;
            }
            found.push(RecoveryEntry { rel, ts, slot });
        }
        found.sort_by(|a, b| {
            a.ts.cmp(&b.ts)
                .then_with(|| a.rel.cmp(&b.rel))
                .then_with(|| a.slot.cmp(&b.slot))
        });
        found.dedup_by(|a, b| a.rel == b.rel && a.slot == b.slot);
        found
    }

    /// The browser lists the registered state home's recovery keys for this
    /// vault (#240 slice 6d). Without a store there is nothing to list.
    #[cfg(not(feature = "native"))]
    pub(crate) fn scan(&self) -> Vec<RecoveryEntry> {
        web_scan(self.home.as_deref(), &self.vault)
    }
}

/// The shared part of a recovery key: the vault id, `~`, the sanitized
/// relative path, and `~kanban` for the card slot. The file extension is the
/// caller's.
fn key_name(vault: &str, rel: &str, slot: RecoverySlot) -> String {
    match slot {
        // The editor name is the original one, so copies written before the
        // kanban slot existed still load.
        RecoverySlot::Editor => format!("{vault}~{}", sanitize(rel)),
        RecoverySlot::Kanban => format!("{vault}~{}~kanban", sanitize(rel)),
    }
}

/// The state-home key below the virtual XDG root. Native joins the same name
/// onto `<state home>/muninn/recovery`.
fn key_rel(vault: &str, rel: &str, slot: RecoverySlot, extension: &str) -> String {
    format!("muninn/recovery/{}.{extension}", key_name(vault, rel, slot))
}

/// The browser build's store refusal. `Unsupported` is not a clock or path
/// failure, so callers report it as an unavailable sidecar, never as "gone".
fn unavailable() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "crash recovery needs the browser's state home",
    )
}

/// Persist one slot into the registered browser state home (#240 slice 6d).
///
/// The order is the native one: the buffer first, then the sidecar that
/// advertises it, so a crash between the two can never list a copy with no
/// text. `None` is an unavailable store and refuses; a copy that is genuinely
/// missing is [`io::ErrorKind::NotFound`] from the read side.
#[cfg_attr(feature = "native", allow(dead_code))]
fn web_save_slot(
    home: Option<&dyn StateHome>,
    vault: &str,
    rel: &str,
    slot: RecoverySlot,
    text: &str,
) -> io::Result<()> {
    let Some(home) = home else {
        return Err(unavailable());
    };
    home.put(&key_rel(vault, rel, slot, "txt"), text.as_bytes())?;
    let meta = meta_json(vault, rel, slot, home.now_secs());
    home.put(&key_rel(vault, rel, slot, "json"), meta.as_bytes())
}

/// Read one slot back from the registered browser state home. A key the store
/// does not hold is `NotFound`; an unavailable store is `Unsupported`.
#[cfg_attr(feature = "native", allow(dead_code))]
fn web_load_slot(
    home: Option<&dyn StateHome>,
    vault: &str,
    rel: &str,
    slot: RecoverySlot,
) -> io::Result<String> {
    let Some(home) = home else {
        return Err(unavailable());
    };
    let bytes = home.get(&key_rel(vault, rel, slot, "txt")).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "the browser state home has no recovery copy for this note",
        )
    })?;
    String::from_utf8(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "the recovery copy is not valid UTF-8",
        )
    })
}

/// Forget one slot in the browser state home. An unavailable store and a
/// missing key are both no-ops, like the native `remove_file` ignores.
#[cfg_attr(feature = "native", allow(dead_code))]
fn web_clear_slot(home: Option<&dyn StateHome>, vault: &str, rel: &str, slot: RecoverySlot) {
    let Some(home) = home else {
        return;
    };
    let _ = home.remove(&key_rel(vault, rel, slot, "txt"));
    let _ = home.remove(&key_rel(vault, rel, slot, "json"));
}

/// List the browser state home's recovery keys for one vault, newest last.
///
/// The checks are the native `scan`'s: only this vault's names, a sidecar that
/// parses and names the same vault, and a text key that exists. Orphaned text
/// (no sidecar) and shadowed sidecars (no text) are not offered.
#[cfg_attr(feature = "native", allow(dead_code))]
fn web_scan(home: Option<&dyn StateHome>, vault: &str) -> Vec<RecoveryEntry> {
    let Some(home) = home else {
        return Vec::new();
    };
    let prefix = "muninn/recovery/";
    let keys = home.list(prefix);
    let own = format!("{vault}~");
    let mut found = Vec::new();
    for key in &keys {
        let Some(name) = key.strip_prefix(prefix) else {
            continue;
        };
        if !name.ends_with(".json") || !name.starts_with(&own) {
            continue;
        }
        let Some(bytes) = home.get(key) else {
            continue;
        };
        let Ok(meta) = String::from_utf8(bytes) else {
            continue;
        };
        if parse_string(&meta, "vault").as_deref() != Some(vault) {
            continue;
        }
        let (Some(rel), Some(ts)) = (parse_rel(&meta), parse_ts(&meta)) else {
            continue;
        };
        let slot = slot_of(name, &meta);
        if home.get(&key_rel(vault, &rel, slot, "txt")).is_none() {
            continue;
        }
        found.push(RecoveryEntry { rel, ts, slot });
    }
    found.sort_by(|a, b| {
        a.ts.cmp(&b.ts)
            .then_with(|| a.rel.cmp(&b.rel))
            .then_with(|| a.slot.cmp(&b.slot))
    });
    found.dedup_by(|a, b| a.rel == b.rel && a.slot == b.slot);
    found
}

/// The XDG state root: `$XDG_STATE_HOME`, else `$HOME/.local/state`, else a
/// temp fallback for platforms without either variable.
///
/// native: the browser has no XDG state root.
#[cfg(feature = "native")]
pub(crate) fn state_home() -> PathBuf {
    crate::settings::xdg_dir("XDG_STATE_HOME", &[".local", "state"], "muninn-state")
}

#[cfg(not(feature = "native"))]
pub(crate) fn state_home() -> PathBuf {
    // A virtual root: the browser has no filesystem, and the registered state
    // home's keys (`muninn/...`) are the durable spelling. The path only labels
    // the store in diagnostics and the recovery-path accessors.
    PathBuf::from("/Muninn state")
}

/// Delete every recovery copy that belongs to a locked note and drop its banner
/// rows, for both the editor and the kanban slot.
///
/// Per-note encryption (#132): a recovery `.txt` is the editor buffer as
/// plaintext, so keeping one for a note whose on-disk source is an envelope
/// would leak the body outside the vault. A stale copy can exist when the note
/// was locked after the copy was written, so this runs whenever a vault becomes
/// live (startup, the end of a chunked scan, an explicit switch) rather than
/// only when a copy is written. A locked note relies on autosave instead.
///
/// Each slot is unlinked only when its text file is there, so a vault with many
/// locked notes and no copies costs one stat per slot, not an unlink. The text
/// file is also what makes this catch an orphan copy whose sidecar is gone
/// (which [`RecoveryStore::scan`] does not list).
pub(crate) fn clear_locked(
    vault: &muninn_core::Vault,
    store: &RecoveryStore,
    recovered: &mut Vec<RecoveryEntry>,
) {
    let locked: Vec<&str> = vault
        .notes
        .iter()
        .filter(|note| note.is_locked())
        .map(|note| note.rel.as_str())
        .collect();
    if locked.is_empty() {
        return;
    }
    for rel in &locked {
        #[cfg(feature = "native")]
        for slot in [RecoverySlot::Editor, RecoverySlot::Kanban] {
            // The text file is the plaintext; the sidecar is only an index.
            if store.path_for_slot(rel, slot).is_file() {
                store.clear_slot(rel, slot);
            }
        }
        #[cfg(not(feature = "native"))]
        for slot in [RecoverySlot::Editor, RecoverySlot::Kanban] {
            // The state home's remove is idempotent; a locked note must not
            // keep a plaintext copy in either slot (#240 slice 6d).
            store.clear_slot(rel, slot);
        }
    }
    recovered.retain(|entry| !locked.contains(&entry.rel.as_str()));
}

/// Stable 64-bit FNV-1a of the canonical vault path.
pub(crate) fn vault_id(root: &Path) -> String {
    #[cfg(feature = "native")]
    let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    // No symlinks in a browser: the path is already its own canonical form.
    #[cfg(not(feature = "native"))]
    let canonical = root.to_path_buf();
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

/// The sidecar that advertises one recovery copy: the exact vault-relative
/// path, the vault id, the slot when it is the kanban page's, and Unix
/// seconds. Native writes it beside the `.txt`; the browser writes the same
/// bytes to the state home, so a record can move between the two hosts.
fn meta_json(vault: &str, rel: &str, slot: RecoverySlot, ts: u64) -> String {
    let slot_field = match slot {
        RecoverySlot::Editor => String::new(),
        RecoverySlot::Kanban => ",\"slot\":\"kanban\"".to_owned(),
    };
    format!(
        "{{\"rel\":\"{}\",\"vault\":\"{}\"{slot_field},\"ts\":{ts}}}\n",
        escape(rel),
        vault
    )
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

/// The kanban slot is named `~kanban` before the extension. A sidecar
/// `"slot":"kanban"` says the same thing when the name was kept.
fn slot_of(name: &str, meta: &str) -> RecoverySlot {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    if stem.ends_with("~kanban") || parse_string(meta, "slot").as_deref() == Some("kanban") {
        RecoverySlot::Kanban
    } else {
        RecoverySlot::Editor
    }
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
#[cfg(feature = "native")]
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
    use muninn_core::MemoryStateHome;

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

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muninn-recovery-{tag}-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_kanban_slot_survives_an_editor_save_of_the_same_note() {
        let (state, vault) = (temp("state"), temp("vault"));
        let store = RecoveryStore::at(&state, &vault);
        store
            .save_slot("Board.md", RecoverySlot::Kanban, "card edit")
            .unwrap();
        store.save("Board.md", "typed in write").unwrap();
        store.save("Board.md", "typed again").unwrap();
        assert_eq!(store.load("Board.md").unwrap(), "typed again");
        assert_eq!(
            store.load_slot("Board.md", RecoverySlot::Kanban).unwrap(),
            "card edit"
        );
        let listed = store.scan();
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert!(
            listed
                .iter()
                .any(|e| { e.rel == "Board.md" && e.slot == RecoverySlot::Kanban })
        );
        store.clear("Board.md");
        assert_eq!(
            store.load_slot("Board.md", RecoverySlot::Kanban).unwrap(),
            "card edit",
            "clearing Write's copy leaves the kanban copy"
        );
        store.clear_slot("Board.md", RecoverySlot::Kanban);
        assert!(store.scan().is_empty());
    }

    #[test]
    fn two_vaults_sharing_a_state_home_never_touch_each_others_copies() {
        let (state, a, b) = (temp("state"), temp("vault-a"), temp("vault-b"));
        let (store_a, store_b) = (RecoveryStore::at(&state, &a), RecoveryStore::at(&state, &b));
        for rel in ["Alpha.md", "Tools.excalidraw"] {
            store_a.save(rel, "A's unsaved text").unwrap();
            // Vault B saves (clears) and then dirties the same relative path.
            store_b.save(rel, "B's text").unwrap();
            store_b.clear(rel);
            store_b.save(rel, "B again").unwrap();
            assert_eq!(store_a.load(rel).unwrap(), "A's unsaved text", "{rel}");
            store_b.clear(rel);
            assert_eq!(store_a.load(rel).unwrap(), "A's unsaved text", "{rel}");
        }
        let listed: Vec<String> = store_a.scan().into_iter().map(|e| e.rel).collect();
        assert_eq!(listed, ["Alpha.md", "Tools.excalidraw"]);
        assert!(store_b.scan().is_empty());
    }

    #[test]
    fn records_in_the_old_shared_layout_still_restore_for_their_own_vault() {
        let (state, a, b) = (temp("state"), temp("vault-a"), temp("vault-b"));
        let dir = state.join("muninn").join("recovery");
        fs::create_dir_all(&dir).unwrap();
        // Written by an older Muninn: `<sanitized-rel>.txt` / `.json`.
        let legacy = |rel: &str, root: &Path, text: &str| {
            fs::write(dir.join(format!("{}.txt", sanitize(rel))), text).unwrap();
            fs::write(
                dir.join(format!("{}.json", sanitize(rel))),
                format!(
                    "{{\"rel\":\"{}\",\"vault\":\"{}\",\"ts\":7}}\n",
                    escape(rel),
                    vault_id(root)
                ),
            )
            .unwrap();
        };
        legacy("folder/Old.md", &a, "pending note");
        legacy("Mine.md", &b, "B's pending note");
        let store = RecoveryStore::at(&state, &a);
        let listed = store.scan();
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0].rel, "folder/Old.md");
        assert_eq!(store.load("folder/Old.md").unwrap(), "pending note");
        store.clear("folder/Old.md");
        assert!(
            store.scan().is_empty(),
            "clearing removes the old record too"
        );
        // B's old record is left for B.
        let store_b = RecoveryStore::at(&state, &b);
        assert_eq!(store_b.load("Mine.md").unwrap(), "B's pending note");
        assert_eq!(store_b.scan().len(), 1);
    }

    #[test]
    fn an_old_layout_record_shadowed_by_a_newer_copy_is_never_offered_again() {
        let (state, a) = (temp("state"), temp("vault-a"));
        let dir = state.join("muninn").join("recovery");
        fs::create_dir_all(&dir).unwrap();
        let store = RecoveryStore::at(&state, &a);
        store.save("Tools.excalidraw", "newer copy").unwrap();
        // An older Muninn left a copy of the same board in the shared layout.
        fs::write(
            dir.join(format!("{}.txt", sanitize("Tools.excalidraw"))),
            "old copy",
        )
        .unwrap();
        fs::write(
            dir.join(format!("{}.json", sanitize("Tools.excalidraw"))),
            format!(
                "{{\"rel\":\"Tools.excalidraw\",\"vault\":\"{}\",\"ts\":7}}\n",
                vault_id(&a)
            ),
        )
        .unwrap();
        let store = RecoveryStore::at(&state, &a);
        assert_eq!(store.load("Tools.excalidraw").unwrap(), "newer copy");
        assert_eq!(store.scan().len(), 1, "listed once");
        // Saving the board clears its copy; the old one must not come back.
        store.clear("Tools.excalidraw");
        let store = RecoveryStore::at(&state, &a);
        assert!(
            store.scan().is_empty(),
            "the old copy came back: {:?}",
            store.load("Tools.excalidraw")
        );
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            0,
            "the shadowed old record is gone"
        );
    }

    // ---- #240 slice 6d: the browser twin over the state-home seam -----------

    #[test]
    fn the_browser_twin_saves_lists_loads_and_clears_a_copy() {
        let home = MemoryStateHome::new();
        let vault = "a1b2c3";
        web_save_slot(
            Some(&home),
            vault,
            "Notes/One.md",
            RecoverySlot::Editor,
            "typed once",
        )
        .unwrap();
        web_save_slot(
            Some(&home),
            vault,
            "Board.md",
            RecoverySlot::Kanban,
            "card edit",
        )
        .unwrap();

        assert_eq!(
            web_load_slot(Some(&home), vault, "Notes/One.md", RecoverySlot::Editor).unwrap(),
            "typed once"
        );
        assert_eq!(
            web_load_slot(Some(&home), vault, "Board.md", RecoverySlot::Kanban).unwrap(),
            "card edit"
        );
        // A newer copy replaces the old one; the other slot keeps its text.
        web_save_slot(
            Some(&home),
            vault,
            "Notes/One.md",
            RecoverySlot::Editor,
            "typed twice",
        )
        .unwrap();
        assert_eq!(
            web_load_slot(Some(&home), vault, "Notes/One.md", RecoverySlot::Editor).unwrap(),
            "typed twice"
        );
        assert_eq!(
            web_load_slot(Some(&home), vault, "Board.md", RecoverySlot::Kanban).unwrap(),
            "card edit"
        );

        let listed = web_scan(Some(&home), vault);
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert!(listed.iter().any(|entry| {
            entry.rel == "Notes/One.md" && entry.slot == RecoverySlot::Editor && entry.ts > 0
        }));
        assert!(
            listed
                .iter()
                .any(|entry| entry.rel == "Board.md" && entry.slot == RecoverySlot::Kanban)
        );
        // The sidecar carries the kanban slot the way the native one does.
        let meta = home
            .get(&key_rel(vault, "Board.md", RecoverySlot::Kanban, "json"))
            .expect("the sidecar");
        assert!(
            String::from_utf8(meta)
                .unwrap()
                .contains("\"slot\":\"kanban\"")
        );

        web_clear_slot(Some(&home), vault, "Notes/One.md", RecoverySlot::Editor);
        assert_eq!(
            web_load_slot(Some(&home), vault, "Notes/One.md", RecoverySlot::Editor)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound,
            "a cleared copy is genuinely gone"
        );
        assert_eq!(
            web_load_slot(Some(&home), vault, "Board.md", RecoverySlot::Kanban).unwrap(),
            "card edit",
            "clearing Write's copy leaves the kanban copy"
        );
        assert_eq!(web_scan(Some(&home), vault).len(), 1);
        web_clear_slot(Some(&home), vault, "Board.md", RecoverySlot::Kanban);
        assert!(web_scan(Some(&home), vault).is_empty());
    }

    #[test]
    fn a_browser_copy_belongs_to_its_own_vault() {
        let home = MemoryStateHome::new();
        web_save_slot(
            Some(&home),
            "vault-a",
            "Alpha.md",
            RecoverySlot::Editor,
            "A",
        )
        .unwrap();
        web_save_slot(
            Some(&home),
            "vault-b",
            "Alpha.md",
            RecoverySlot::Editor,
            "B",
        )
        .unwrap();
        assert_eq!(
            web_load_slot(Some(&home), "vault-a", "Alpha.md", RecoverySlot::Editor).unwrap(),
            "A"
        );
        let listed: Vec<String> = web_scan(Some(&home), "vault-b")
            .into_iter()
            .map(|entry| entry.rel)
            .collect();
        assert_eq!(listed, ["Alpha.md"]);
        web_clear_slot(Some(&home), "vault-a", "Alpha.md", RecoverySlot::Editor);
        assert_eq!(
            web_load_slot(Some(&home), "vault-b", "Alpha.md", RecoverySlot::Editor).unwrap(),
            "B",
            "clearing one vault's copy leaves the other's"
        );
    }

    #[test]
    fn a_browser_orphan_text_or_sidecar_is_not_offered() {
        let home = MemoryStateHome::new();
        let vault = "vault-a";
        web_save_slot(Some(&home), vault, "One.md", RecoverySlot::Editor, "text").unwrap();
        // A sidecar with no text (an interrupted first write) is not listed.
        home.remove(&key_rel(vault, "One.md", RecoverySlot::Editor, "txt"))
            .unwrap();
        assert!(web_scan(Some(&home), vault).is_empty());
        assert_eq!(
            web_load_slot(Some(&home), vault, "One.md", RecoverySlot::Editor)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        // Text with no sidecar (a crash between the two writes) is not listed.
        home.put(
            &key_rel(vault, "Two.md", RecoverySlot::Editor, "txt"),
            b"orphan",
        )
        .unwrap();
        assert!(web_scan(Some(&home), vault).is_empty());
    }

    #[test]
    fn a_missing_state_home_is_unavailable_not_empty() {
        let home = MemoryStateHome::new();
        // A store that is there but has no copy: genuinely gone.
        assert_eq!(
            web_load_slot(Some(&home), "vault-a", "Missing.md", RecoverySlot::Editor)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(web_scan(Some(&home), "vault-a").is_empty());
        // No store at all: unavailable, so a quit never claims the text is lost.
        assert_eq!(
            web_save_slot(None, "vault-a", "Missing.md", RecoverySlot::Editor, "x")
                .unwrap_err()
                .kind(),
            io::ErrorKind::Unsupported
        );
        assert_eq!(
            web_load_slot(None, "vault-a", "Missing.md", RecoverySlot::Editor)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Unsupported
        );
        assert!(web_scan(None, "vault-a").is_empty());
        web_clear_slot(None, "vault-a", "Missing.md", RecoverySlot::Editor);
        assert_eq!(
            web_load_slot(Some(&home), "vault-a", "Missing.md", RecoverySlot::Editor)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
    }
}

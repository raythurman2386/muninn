//! The sync engine's host-state seam (#240 slice 8a).
//!
//! A run keeps five kinds of small state beside the vault: the base snapshot,
//! the crash journal, the prepared apply list, the GitHub sha cache and staged
//! bytes. They are host state, not vault files, so the engine reaches them
//! through [`StateHome`] — the same flat, string-keyed byte store the rest of
//! the app uses for recovery copies, settings and lock records — instead of
//! calling `std::fs` itself. The browser twin will supply OPFS behind the same
//! keys; it never needs to know a native path existed.
//!
//! Keys are `/`-separated relative paths below the binding's state directory.
//! [`NativeStateHome`] joins each key onto that directory without re-spelling
//! it, so bytes land exactly where earlier builds put them:
//!
//! | key | native path below `<sync_home>/bindings/<binding>/` |
//! | --- | --- |
//! | `base.json` | `base.json` |
//! | `journal.json` | `journal.json` |
//! | `prepared.json` | `prepared.json` |
//! | `github-blob-cache.json` | `github-blob-cache.json` |
//! | `staged/<relative path>` | `staged/<relative path>` |
//! | `checkpoints/<relative path>` | `checkpoints/<relative path>` |
//!
//! Writes keep `fsutil::write_outside`'s temp-file + rename rule, so a
//! reader never observes a half-written state file. The key spelling is the
//! contract the native and browser twins share; the path column is native-only
//! detail pinned by the tests below.

#[cfg(feature = "native")]
use std::io;

#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

pub use muninn_core::StateHome;

/// The base snapshot: the remote state the last successful run applied.
pub const BASE: &str = "base.json";
/// The crash-recovery journal, used to resume an interrupted local apply.
pub const JOURNAL: &str = "journal.json";
/// The prepared apply list and its staged-bytes manifest.
pub const PREPARED: &str = "prepared.json";
/// GitHub blob sha → content hash cache.
pub const SHA_CACHE: &str = "github-blob-cache.json";

/// Key for the staged bytes of one vault path.
#[must_use]
pub fn staged_key(path: &str) -> String {
    format!("staged/{path}")
}

/// Key for the checkpoint preimage of one vault path.
#[must_use]
pub fn checkpoint_key(path: &str) -> String {
    format!("checkpoints/{path}")
}

/// [`StateHome`] over the existing native per-binding state directory.
///
/// This is the desktop implementation; the browser twin spells the same keys
/// in its OPFS directory. It is a wrapper around the old layout — its root is
/// the `state_dir` the app already computes — not a new directory scheme.
/// Parent creation and atomic replacement still come from
/// `fsutil::write_outside`, so on-disk bytes are unchanged.
#[cfg(feature = "native")]
pub struct NativeStateHome {
    root: PathBuf,
}

#[cfg(feature = "native")]
impl NativeStateHome {
    /// A home rooted at an existing (or about-to-be-created) state directory.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The native path a key resolves to. Exposed so tests can pin the layout
    /// and the browser twin can prove key-for-key equality with its own
    /// mapping.
    #[must_use]
    pub fn path_for(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }
}

#[cfg(feature = "native")]
impl StateHome for NativeStateHome {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        // The trait's contract is "absent → None, never an error". An unreadable
        // state file is treated the same way a missing one is; corruption is
        // caught by the JSON parsers above the seam with their existing errors.
        crate::fsutil::read_outside(&self.path_for(key)).ok()
    }

    fn put(&self, key: &str, bytes: &[u8]) -> io::Result<()> {
        crate::fsutil::write_outside(&self.path_for(key), bytes).map_err(|error| match error {
            // Preserve the exact detail string callers used to see from
            // `write_outside`; only the type changes at the seam boundary.
            crate::SyncError::Io { detail } => io::Error::other(detail),
            other => io::Error::other(other.to_string()),
        })
    }

    fn remove(&self, key: &str) -> io::Result<()> {
        match std::fs::remove_file(self.path_for(key)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn list(&self, prefix: &str) -> Vec<String> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, base, out);
                } else if let Ok(rel) = path.strip_prefix(base)
                    && let Some(key) = rel.to_str()
                {
                    out.push(key.replace('\\', "/"));
                }
            }
        }
        let mut keys = Vec::new();
        walk(&self.root, &self.root, &mut keys);
        keys.retain(|key| key.starts_with(prefix));
        keys.sort();
        keys
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;
    use crate::journal::{Journal, JournalPhase};
    use crate::model::{BindingId, OperationId, RunId};
    use muninn_core::MemoryStateHome;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "muninn-state-home-{name}-{}-{}",
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

    fn journal_bytes() -> Vec<u8> {
        let journal = Journal {
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
        };
        serde_json::to_vec_pretty(&journal).unwrap()
    }

    /// Every state item the engine stores round-trips through the memory twin:
    /// put → get → equal, remove → gone, and the prefix list narrows correctly.
    #[test]
    fn every_state_item_round_trips_through_memory() {
        let home = MemoryStateHome::new();
        let items = [
            (BASE.to_owned(), br#"{"schema":1,"entries":{}}"#.to_vec()),
            (JOURNAL.to_owned(), journal_bytes()),
            (
                PREPARED.to_owned(),
                br#"{"head":"abc","writes":[],"deletes":[],"entries":{}}"#.to_vec(),
            ),
            (
                SHA_CACHE.to_owned(),
                br#"{"schema":1,"entries":{"sha":"hash"}}"#.to_vec(),
            ),
            (staged_key("notes/a.md"), b"staged bytes".to_vec()),
            (checkpoint_key("notes/a.md"), b"preimage".to_vec()),
        ];
        for (key, bytes) in &items {
            assert_eq!(home.get(key), None, "{key} starts absent");
            home.put(key, bytes).unwrap();
            assert_eq!(home.get(key), Some(bytes.clone()), "{key} round-trips");
        }

        assert_eq!(
            home.list("staged/"),
            [staged_key("notes/a.md")],
            "prefix list returns staged bytes only"
        );
        assert_eq!(home.list("checkpoints/"), [checkpoint_key("notes/a.md")]);
        assert_eq!(home.list("").len(), items.len(), "all keys are listed");

        for (key, _) in &items {
            home.remove(key).unwrap();
            home.remove(key).unwrap(); // idempotent
            assert_eq!(home.get(key), None, "{key} is gone after remove");
        }
        assert!(home.list("").is_empty());
    }

    /// The native twin resolves keys to the exact paths the pre-refactor code
    /// wrote, and the bytes it writes are the bytes it was given.
    #[test]
    fn native_home_preserves_the_existing_layout() {
        let state = scratch("native");
        let home = NativeStateHome::new(state.clone());
        let items = [
            (BASE, "base.json", b"base bytes".to_vec()),
            (JOURNAL, "journal.json", journal_bytes()),
            (PREPARED, "prepared.json", b"prepared bytes".to_vec()),
            (
                SHA_CACHE,
                "github-blob-cache.json",
                b"sha cache bytes".to_vec(),
            ),
            ("staged/notes/a.md", "staged/notes/a.md", b"staged".to_vec()),
            (
                "checkpoints/notes/a.md",
                "checkpoints/notes/a.md",
                b"preimage".to_vec(),
            ),
        ];
        for (key, rel, bytes) in &items {
            assert_eq!(
                home.path_for(key),
                state.join(rel),
                "key {key} keeps its path"
            );
            home.put(key, bytes).unwrap();
            assert_eq!(fs::read(state.join(rel)).unwrap(), *bytes, "bytes at {rel}");
            assert_eq!(home.get(key), Some(bytes.clone()), "readback of {key}");
        }

        assert_eq!(
            home.list("staged/"),
            ["staged/notes/a.md"],
            "native list mirrors the key prefix"
        );
        assert_eq!(home.list("checkpoints/"), ["checkpoints/notes/a.md"]);

        for (_, rel, _) in &items {
            home.remove(rel).unwrap(); // a key is its own relative path
            home.remove(rel).unwrap();
            assert!(!state.join(rel).exists(), "{rel} is gone after remove");
        }
        let _ = fs::remove_dir_all(&state);
    }
}

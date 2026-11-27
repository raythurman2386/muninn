//! The durable key-value seam for host state that is not a vault file (#240
//! slice 6d).
//!
//! A vault's bytes go through [`VaultStore`](crate::VaultStore). The three
//! small pieces of host state beside it — crash-recovery copies, the settings
//! file and the lock high-water record — have their own home:
//!
//! ```text
//! <state home>/muninn/recovery/<vault-id>~<sanitized-rel>.txt
//! <state home>/muninn/settings.json
//! <state home>/muninn/lock-revs/<vault-id>.json
//! ```
//!
//! Natively that home is the XDG directories, and the desktop keeps its exact
//! `std::fs` code. In a browser it is an OPFS-backed store, because neither an
//! XDG path nor a synchronous filesystem exists there. [`StateHome`] is the
//! small seam between the two: a flat, string-keyed byte store with async
//! durability supplied by the host behind a synchronous read mirror.
//!
//! Keys are `/`-separated relative paths below the state home, the same
//! spelling the desktop joins onto `$XDG_STATE_HOME` (or `$XDG_CONFIG_HOME` for
//! settings). [`MemoryStateHome`] is the pure in-memory implementation the
//! browser twins' tests use, exactly as [`MemoryStore`](crate::MemoryStore) is
//! the contract for a browser vault store.

use std::collections::BTreeMap;
use std::io;
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// A flat, durable key-value store for the small host state beside a vault.
///
/// The trait is object-safe: the browser registers one `Arc<dyn StateHome>` at
/// boot and the shared UI reads it synchronously. Implementations must
/// preserve these semantics:
///
/// - [`get`](StateHome::get) returns the exact bytes put under a key, and
///   `None` — never an error — when the key is absent.
/// - [`put`](StateHome::put) replaces a key's bytes whole; a reader never
///   observes a partial value.
/// - [`remove`](StateHome::remove) is idempotent: removing an absent key is
///   not an error.
/// - [`list`](StateHome::list) returns every key with the prefix, sorted.
pub trait StateHome: Send + Sync {
    /// The bytes stored under `key`, or `None` when the store has no such key.
    fn get(&self, key: &str) -> Option<Vec<u8>>;

    /// Store `bytes` under `key`, replacing any previous value.
    ///
    /// # Errors
    ///
    /// A store that is open read-only (a second browser tab) or cannot reach
    /// its backing bytes reports why.
    fn put(&self, key: &str, bytes: &[u8]) -> io::Result<()>;

    /// Remove `key`. Not finding it is not an error.
    fn remove(&self, key: &str) -> io::Result<()>;

    /// Every key that starts with `prefix`, sorted.
    fn list(&self, prefix: &str) -> Vec<String>;

    /// Unix seconds from the host's wall clock, for records that carry one
    /// (a recovery copy's timestamp).
    ///
    /// The default is `SystemTime`, which the desktop and tests can use. The
    /// browser store overrides it: `std`'s clock is not implemented on
    /// `wasm32-unknown-unknown`, so the browser reads `Date.now()` instead.
    fn now_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs())
    }
}

/// A [`StateHome`] in memory: the contract a browser store must reproduce.
///
/// Used by the browser twins' headless tests and as the shape the OPFS store
/// mirrors. It has no clock of its own beyond the trait's default.
#[derive(Debug, Default)]
pub struct MemoryStateHome {
    entries: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl MemoryStateHome {
    /// An empty state home.
    #[must_use]
    pub fn new() -> MemoryStateHome {
        MemoryStateHome::default()
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Vec<u8>>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl StateHome for MemoryStateHome {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.entries().get(key).cloned()
    }

    fn put(&self, key: &str, bytes: &[u8]) -> io::Result<()> {
        self.entries().insert(key.to_owned(), bytes.to_vec());
        Ok(())
    }

    fn remove(&self, key: &str) -> io::Result<()> {
        self.entries().remove(key);
        Ok(())
    }

    fn list(&self, prefix: &str) -> Vec<String> {
        self.entries()
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_round_trips_and_removal_is_idempotent() {
        let home = MemoryStateHome::new();
        assert_eq!(home.get("muninn/settings.json"), None);
        home.put("muninn/settings.json", b"{}").unwrap();
        assert_eq!(home.get("muninn/settings.json"), Some(b"{}".to_vec()));
        home.put("muninn/settings.json", b"{\"theme\":\"dark\"}")
            .unwrap();
        assert_eq!(
            home.get("muninn/settings.json"),
            Some(b"{\"theme\":\"dark\"}".to_vec()),
            "a put replaces the whole value"
        );
        home.remove("muninn/settings.json").unwrap();
        home.remove("muninn/settings.json").unwrap();
        assert_eq!(home.get("muninn/settings.json"), None);
    }

    #[test]
    fn listing_filters_by_prefix_and_sorts() {
        let home = MemoryStateHome::new();
        for key in [
            "muninn/recovery/b.txt",
            "muninn/lock-revs/v.json",
            "muninn/recovery/a.json",
        ] {
            home.put(key, b"x").unwrap();
        }
        assert_eq!(
            home.list("muninn/recovery/"),
            ["muninn/recovery/a.json", "muninn/recovery/b.txt"]
        );
        assert_eq!(home.list("muninn/lock-revs/").len(), 1);
        assert!(home.list("nothing/").is_empty());
    }

    #[test]
    fn the_default_clock_is_unix_seconds() {
        assert!(MemoryStateHome::new().now_secs() > 1_600_000_000);
    }
}

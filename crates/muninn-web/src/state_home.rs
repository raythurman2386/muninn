//! The browser's durable state home (#240 slice 6d).
//!
//! OPFS is a Promise API, but crash recovery, settings and the lock high-water
//! record are synchronous Rust in `muninn-ui`. The browser build answers them
//! from an in-memory mirror, exactly as [`crate::OpfsStore`] answers the
//! vault's synchronous [`muninn_core::VaultStore`]:
//!
//! - the mirror is loaded from OPFS at boot, before the app starts;
//! - every successful write marks its key in a dirty set;
//! - the async half in `opfs.rs` drains that set back to OPFS from the poll
//!   loop, and a reload walks OPFS back into the mirror.
//!
//! Keys are `/`-separated paths below the state home root, the same spelling
//! the desktop joins onto its XDG roots (`muninn/settings.json`,
//! `muninn/recovery/…`, `muninn/lock-revs/…`).
//!
//! The window between two flushes is the same honest limitation as the vault's:
//! a crash before the drain loses the newest copy. A recovery copy exists to
//! narrow that window, so it is a good trade: the copy is the thing the window
//! would otherwise lose.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use muninn_core::StateHome;

/// The browser's state home: a synchronous mirror of an OPFS key-value store.
pub struct OpfsStateHome {
    mirror: Mutex<BTreeMap<String, Vec<u8>>>,
    writable: AtomicBool,
    /// Keys whose mirror state has not reached OPFS yet.
    dirty: Mutex<BTreeSet<String>>,
}

impl OpfsStateHome {
    /// An empty home. `writable` is false for a tab that could not take the
    /// single-writer lock: reads work and every write is refused.
    #[must_use]
    pub fn new(writable: bool) -> OpfsStateHome {
        OpfsStateHome {
            mirror: Mutex::new(BTreeMap::new()),
            writable: AtomicBool::new(writable),
            dirty: Mutex::new(BTreeSet::new()),
        }
    }

    /// Seed a key without marking it dirty: the boot snapshot of bytes already
    /// in OPFS.
    pub fn load(&self, key: &str, bytes: &[u8]) {
        self.mirror().insert(key.to_owned(), bytes.to_vec());
    }

    /// Whether this tab holds the write lock.
    #[must_use]
    pub fn is_writable(&self) -> bool {
        self.writable.load(Ordering::Relaxed)
    }

    /// Stop accepting and flushing writes. Edits already in the mirror stay
    /// readable for the rest of the session.
    pub fn set_writable(&self, writable: bool) {
        self.writable.store(writable, Ordering::Relaxed);
    }

    /// Whether a flush has anything to do.
    #[must_use]
    pub fn has_dirty(&self) -> bool {
        !self.dirty().is_empty()
    }

    /// How many distinct keys are waiting to be flushed.
    #[must_use]
    pub fn dirty_count(&self) -> usize {
        self.dirty().len()
    }

    /// The bytes the mirror holds for `key`.
    #[must_use]
    pub fn read_bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.mirror().get(key).cloned()
    }

    /// The next flush's work. Taking the plan clears the dirty set, so a write
    /// that lands while the async flush awaits re-marks its key for the next
    /// drain. [`OpfsStateHome::requeue`] puts a failed plan back.
    #[must_use]
    pub fn take_flush_plan(&self) -> StateFlushPlan {
        let dirty = {
            let mut dirty = self.dirty();
            std::mem::take(&mut *dirty)
        };
        let mirror = self.mirror();
        let mut plan = StateFlushPlan::default();
        for key in dirty {
            if mirror.contains_key(&key) {
                plan.writes.push(key);
            } else {
                plan.removals.push(key);
            }
        }
        plan
    }

    /// Put a failed plan back in the dirty set.
    pub fn requeue(&self, plan: &StateFlushPlan) {
        let mut dirty = self.dirty();
        for key in plan.removals.iter().chain(&plan.writes) {
            dirty.insert(key.clone());
        }
    }

    fn mirror(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Vec<u8>>> {
        self.mirror
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn dirty(&self) -> std::sync::MutexGuard<'_, BTreeSet<String>> {
        self.dirty
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn require_writable(&self) -> io::Result<()> {
        if self.is_writable() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the browser state home is open read-only: another tab holds the vault's write lock",
            ))
        }
    }
}

impl StateHome for OpfsStateHome {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.mirror().get(key).cloned()
    }

    fn put(&self, key: &str, bytes: &[u8]) -> io::Result<()> {
        self.require_writable()?;
        self.mirror().insert(key.to_owned(), bytes.to_vec());
        self.dirty().insert(key.to_owned());
        Ok(())
    }

    fn remove(&self, key: &str) -> io::Result<()> {
        self.require_writable()?;
        // Removing an absent key is a no-op, and so is leaving it out of the
        // journal: the boot walk already put OPFS and the mirror in step.
        if self.mirror().remove(key).is_some() {
            self.dirty().insert(key.to_owned());
        }
        Ok(())
    }

    fn list(&self, prefix: &str) -> Vec<String> {
        self.mirror()
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// Unix seconds from `Date.now()`: `std`'s clock is not implemented on
    /// `wasm32-unknown-unknown`, so the browser supplies its own.
    fn now_secs(&self) -> u64 {
        #[cfg(target_arch = "wasm32")]
        {
            (js_sys::Date::now() / 1000.0) as u64
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs())
        }
    }
}

/// One flush's work: key removals, then key writes, both sorted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateFlushPlan {
    pub removals: Vec<String>,
    pub writes: Vec<String>,
}

impl StateFlushPlan {
    /// True when the dirty set was empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.removals.is_empty() && self.writes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> OpfsStateHome {
        OpfsStateHome::new(true)
    }

    #[test]
    fn a_boot_load_is_not_dirty_and_a_put_marks_it() {
        let home = home();
        home.load("muninn/settings.json", b"{}");
        assert!(!home.has_dirty(), "a snapshot writes nothing");
        home.put("muninn/settings.json", b"{\"theme\":\"dark\"}")
            .unwrap();
        assert_eq!(home.dirty_count(), 1);
        assert_eq!(
            home.read_bytes("muninn/settings.json"),
            Some(b"{\"theme\":\"dark\"}".to_vec())
        );
    }

    #[test]
    fn the_plan_writes_present_keys_and_removes_absent_ones() {
        let home = home();
        home.load("muninn/recovery/a.txt", b"old");
        home.put("muninn/recovery/a.txt", b"new").unwrap();
        home.remove("muninn/recovery/a.txt").unwrap();
        home.put("muninn/lock-revs/v.json", b"{}").unwrap();
        let plan = home.take_flush_plan();
        assert!(!home.has_dirty(), "taking the plan drains the journal");
        assert_eq!(plan.removals, ["muninn/recovery/a.txt"]);
        assert_eq!(plan.writes, ["muninn/lock-revs/v.json"]);
    }

    #[test]
    fn removing_an_absent_key_is_a_no_op() {
        let home = home();
        home.remove("muninn/nothing").unwrap();
        assert!(!home.has_dirty());
        assert!(home.take_flush_plan().is_empty());
    }

    #[test]
    fn requeue_puts_a_failed_flush_back() {
        let home = home();
        home.put("muninn/settings.json", b"{}").unwrap();
        let plan = home.take_flush_plan();
        assert!(!home.has_dirty());
        home.requeue(&plan);
        assert_eq!(home.take_flush_plan().writes, ["muninn/settings.json"]);
    }

    #[test]
    fn listing_filters_by_prefix_in_key_order() {
        let home = home();
        home.load("muninn/recovery/1.txt", b"a");
        home.load("muninn/recovery/2.txt", b"b");
        home.load("muninn/settings.json", b"{}");
        assert_eq!(
            home.list("muninn/recovery/"),
            ["muninn/recovery/1.txt", "muninn/recovery/2.txt"]
        );
    }

    #[test]
    fn a_read_only_home_reads_but_refuses_every_write() {
        let home = OpfsStateHome::new(false);
        home.load("muninn/settings.json", b"{\"theme\":\"dark\"}");
        assert_eq!(
            home.get("muninn/settings.json"),
            Some(b"{\"theme\":\"dark\"}".to_vec())
        );
        assert_eq!(
            home.put("muninn/settings.json", b"{}").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            home.remove("muninn/settings.json").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(!home.has_dirty());
        assert_eq!(
            home.get("muninn/settings.json"),
            Some(b"{\"theme\":\"dark\"}".to_vec()),
            "a refused write leaves the mirror untouched"
        );
    }
}

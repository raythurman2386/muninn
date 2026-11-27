//! Per-note encryption (#133): keep an unlocked note's session, and remember
//! the highest revision each note has been sealed at.
//!
//! Two pieces of state live here, both owned by [`crate::State`] and never
//! written to `settings.json`:
//!
//! - [`NoteSessions`] maps a note's vault-relative path to the [`SessionKey`]
//!   its password produced. Only the derived key is kept: the password is
//!   dropped as soon as [`muninn_core::unlock_session`] returns, and the
//!   plaintext lives in the editor buffer, which is cleared when the session
//!   ends.
//! - [`LockRevs`] is a local high-water record of the newest envelope revision
//!   seen for each note, so an envelope rolled back to an older revision does
//!   not silently decrypt to old content after a restart. It stores note paths
//!   and revision numbers, never a key, password or body.
//!
//! A session ends on an explicit lock, a vault switch, quitting, or after
//! [`SESSION_IDLE_SECS`] without use. Losing focus is deliberately not a
//! trigger: copying text out of an unlocked note into another window must not
//! lock it.

use std::collections::{BTreeMap, HashMap};
#[cfg(feature = "native")]
use std::fs;
use std::io;
use std::path::Path;
#[cfg(feature = "native")]
use std::path::PathBuf;

use muninn_core::{LockError, SessionKey};

/// Idle seconds after which an unlocked note is locked again.
///
/// Fifteen minutes is long enough to read a paragraph, copy a line into a chat
/// window, or leave the desk for a moment without losing the session, and
/// short enough that a note unlocked by accident (a shared screen, a demo) is
/// not left open for the rest of the day. It is only a backstop: an explicit
/// lock, a vault switch and quitting all end a session sooner, and blur never
/// does.
pub(crate) const SESSION_IDLE_SECS: f64 = 15.0 * 60.0;

/// One unlocked note: its derived key and when it was last used.
struct SessionEntry {
    /// The derived key from [`muninn_core::unlock_session`]. Dropping it
    /// zeroizes the key.
    key: SessionKey,
    /// Clock ([`crate::State::now`]) the entry was inserted or last used.
    last_used: f64,
}

/// Every note currently unlocked in this window, keyed by `Note.rel`.
#[derive(Default)]
pub(crate) struct NoteSessions {
    entries: HashMap<String, SessionEntry>,
}

impl NoteSessions {
    /// Start (or renew) the session for `rel`. A re-unlock replaces the old
    /// entry, so the key and the idle clock are both fresh.
    pub(crate) fn insert(&mut self, rel: String, key: SessionKey, now: f64) {
        self.entries.insert(
            rel,
            SessionEntry {
                key,
                last_used: now,
            },
        );
    }

    /// End the session for `rel`. True when the note was unlocked.
    pub(crate) fn remove(&mut self, rel: &str) -> bool {
        self.entries.remove(rel).is_some()
    }

    /// Take the key for `rel` out of the map, for a rename or a move that has
    /// to re-label it. The session stops being reachable by the old path; the
    /// caller re-inserts it under the new one once the file has moved. `None`
    /// when the note was not unlocked.
    pub(crate) fn take(&mut self, rel: &str) -> Option<SessionKey> {
        self.entries.remove(rel).map(|entry| entry.key)
    }

    /// End every session: another vault is opening, or the window is going.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Is `rel` unlocked in this window? Never derives a key or reads a note.
    pub(crate) fn contains(&self, rel: &str) -> bool {
        self.entries.contains_key(rel)
    }

    /// The key for `rel`, if it is unlocked, marking the note as used at
    /// `now`. The save path seals through this; using the key is what keeps
    /// the idle clock fresh.
    pub(crate) fn get_mut(&mut self, rel: &str, now: f64) -> Option<&mut SessionKey> {
        let entry = self.entries.get_mut(rel)?;
        entry.last_used = now;
        Some(&mut entry.key)
    }

    /// Count `rel` as used at `now` without taking its key. True when the note
    /// was unlocked: an edit is use even when autosave is paused.
    pub(crate) fn touch(&mut self, rel: &str, now: f64) -> bool {
        match self.entries.get_mut(rel) {
            Some(entry) => {
                entry.last_used = now;
                true
            }
            None => false,
        }
    }

    /// End every session that has been idle for [`SESSION_IDLE_SECS`] and
    /// return their rels (sorted, so a caller's work is deterministic). The
    /// keys are dropped, and so zeroized, here.
    pub(crate) fn expire(&mut self, now: f64) -> Vec<String> {
        let mut expired: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| now - entry.last_used >= SESSION_IDLE_SECS)
            .map(|(rel, _)| rel.clone())
            .collect();
        expired.sort();
        for rel in &expired {
            self.entries.remove(rel);
        }
        expired
    }
}

/// Why a note could not be unlocked.
///
/// Deliberately has no `Display`: the UI layer owns the wording (and must not
/// turn [`LockError::AuthenticationFailed`] into a hint about the password).
#[derive(Debug)]
pub enum UnlockError {
    /// The envelope could not be opened: wrong password, altered bytes, wrong
    /// note id, or an unsupported format.
    Lock(LockError),
    /// The envelope is older than the newest revision this vault has already
    /// seen for that note: a rollback. The note is left locked.
    Rollback { found: u64, recorded: u64 },
    /// The note id is not in the current vault.
    MissingNote,
    /// The high-water record could not be written. The note is left locked.
    Storage(io::Error),
}

impl From<LockError> for UnlockError {
    fn from(error: LockError) -> Self {
        UnlockError::Lock(error)
    }
}

/// How a revision compares with the newest one already seen for a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RevStatus {
    /// Older than the recorded revision: a rollback.
    Older,
    /// The revision that was already seen.
    Same,
    /// Newer, or nothing recorded yet.
    Newer,
}

/// The local high-water record: the newest envelope revision seen per note.
///
/// Stored as one JSON object per vault at
/// `$XDG_STATE_HOME/muninn/lock-revs/<vault-id>.json`, keyed by
/// `Note.rel`. It is device-local and not synced: the sync providers do not
/// order their snapshots by revision, so there is nothing remote to trust.
/// The record only ever raises, and a lower `rev` than the record fails closed
/// (the caller keeps the note locked and reports [`UnlockError::Rollback`]).
#[derive(Debug)]
pub(crate) struct LockRevs {
    #[cfg(feature = "native")]
    path: PathBuf,
    revs: BTreeMap<String, u64>,
}

impl LockRevs {
    /// Load the record for `vault_root` below `state_home`. A missing or
    /// corrupt file starts empty: the record is a safety net, and refusing to
    /// open the vault over it would be worse than losing it.
    pub(crate) fn at(state_home: &Path, vault_root: &Path) -> Self {
        let path = state_home
            .join("muninn")
            .join("lock-revs")
            .join(format!("{}.json", crate::recovery::vault_id(vault_root)));
        // native: the browser keeps the high-water mark in memory only until a
        // web store exists (slice 6), so a reload starts from zero.
        #[cfg(feature = "native")]
        let revs = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<BTreeMap<String, u64>>(&text).ok())
            .unwrap_or_default();
        #[cfg(not(feature = "native"))]
        let revs = BTreeMap::new();
        #[cfg(not(feature = "native"))]
        let _ = path;
        Self {
            #[cfg(feature = "native")]
            path,
            revs,
        }
    }

    /// Compare `rev` with the recorded revision for `rel`.
    pub(crate) fn check(&self, rel: &str, rev: u64) -> RevStatus {
        match self.revs.get(rel) {
            Some(&recorded) if rev < recorded => RevStatus::Older,
            Some(&recorded) if rev == recorded => RevStatus::Same,
            _ => RevStatus::Newer,
        }
    }

    /// The newest revision recorded for `rel` (0 when none).
    pub(crate) fn recorded(&self, rel: &str) -> u64 {
        self.revs.get(rel).copied().unwrap_or(0)
    }

    /// Raise the mark for `rel` to `rev` and persist it. A revision that is
    /// not higher writes nothing, so the record never goes backwards.
    pub(crate) fn record(&mut self, rel: &str, rev: u64) -> io::Result<()> {
        if rev <= self.recorded(rel) {
            return Ok(());
        }
        self.revs.insert(rel.to_owned(), rev);
        self.save()
    }

    /// Move the high-water mark from `old` to `new` and persist it once.
    ///
    /// A rename or a move changes the note's path but not its revision
    /// history: keeping the mark under the old path would let an old envelope
    /// of the renamed note open again. If `new` already has a mark (for
    /// example the target path existed before), the larger of the two wins so
    /// the record still only ever raises. A no-op when `old` has no record.
    pub(crate) fn move_rel(&mut self, old: &str, new: &str) -> io::Result<()> {
        let Some(old_rev) = self.revs.remove(old) else {
            return Ok(());
        };
        let rev = self
            .revs
            .get(new)
            .copied()
            .map_or(old_rev, |seen| seen.max(old_rev));
        self.revs.insert(new.to_owned(), rev);
        self.save()
    }

    /// Write the whole record. The caller has already updated it.
    #[cfg(feature = "native")]
    fn save(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string(&self.revs).map_err(io::Error::other)?;
        muninn_core::atomic_write(&self.path, json.as_bytes())
    }

    /// The in-memory record is the whole record in a browser build: the
    /// envelope revision still rises within the session, and a reload starts
    /// the high-water mark over until slice 6 gives it a durable store.
    #[cfg(not(feature = "native"))]
    fn save(&self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn key_for(note: &str) -> SessionKey {
        let envelope = muninn_core::lock("body", "pw", note).expect("lock");
        muninn_core::unlock_session(&envelope, "pw", note)
            .expect("unlock")
            .0
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muninn-lock-{tag}-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_session_expires_after_fifteen_idle_minutes() {
        let mut sessions = NoteSessions::default();
        sessions.insert("Secret.md".to_owned(), key_for("Secret"), 0.0);
        assert!(sessions.contains("Secret.md"));
        assert!(sessions.expire(899.0).is_empty(), "899 s is still inside");
        assert!(sessions.contains("Secret.md"));
        assert_eq!(sessions.expire(900.0), ["Secret.md"]);
        assert!(!sessions.contains("Secret.md"));
    }

    #[test]
    fn a_reinsert_renews_the_idle_clock() {
        let mut sessions = NoteSessions::default();
        sessions.insert("Secret.md".to_owned(), key_for("Secret"), 0.0);
        sessions.insert("Secret.md".to_owned(), key_for("Secret"), 400.0);
        assert!(
            sessions.expire(1299.0).is_empty(),
            "the clock restarted at 400 s"
        );
        assert_eq!(sessions.expire(1300.0), ["Secret.md"]);
    }

    #[test]
    fn clear_drops_every_session() {
        let mut sessions = NoteSessions::default();
        sessions.insert("A.md".to_owned(), key_for("A"), 0.0);
        sessions.insert("B.md".to_owned(), key_for("B"), 0.0);
        sessions.clear();
        assert!(!sessions.contains("A.md"));
        assert!(!sessions.contains("B.md"));
        assert!(sessions.expire(10_000.0).is_empty());
    }

    #[test]
    fn remove_reports_whether_the_note_was_unlocked() {
        let mut sessions = NoteSessions::default();
        sessions.insert("A.md".to_owned(), key_for("A"), 0.0);
        assert!(sessions.remove("A.md"), "the session was there");
        assert!(!sessions.remove("A.md"), "and now it is not");
    }

    #[test]
    fn touch_renews_the_idle_clock() {
        let mut sessions = NoteSessions::default();
        sessions.insert("Secret.md".to_owned(), key_for("Secret"), 0.0);
        assert!(sessions.touch("Secret.md", 500.0), "the note is unlocked");
        assert!(
            sessions.expire(1000.0).is_empty(),
            "the clock restarted at 500 s"
        );
        assert_eq!(sessions.expire(1400.0), ["Secret.md"]);
        // A note that was never unlocked is not touched into existence.
        assert!(!sessions.touch("Other.md", 1500.0));
        assert!(!sessions.contains("Other.md"));
    }

    #[test]
    fn get_mut_hands_out_the_key_and_renews_the_clock() {
        let mut sessions = NoteSessions::default();
        sessions.insert("Secret.md".to_owned(), key_for("Secret"), 0.0);
        assert!(sessions.get_mut("Other.md", 100.0).is_none());
        {
            let key = sessions.get_mut("Secret.md", 400.0).expect("session");
            assert_eq!(key.note(), "Secret", "the stored key is the one used");
        }
        assert!(
            sessions.expire(1299.0).is_empty(),
            "the clock restarted at 400 s"
        );
        assert_eq!(sessions.expire(1300.0), ["Secret.md"]);
    }

    #[test]
    fn take_hands_the_key_over_and_ends_the_old_session() {
        let mut sessions = NoteSessions::default();
        sessions.insert("Secret.md".to_owned(), key_for("Secret"), 0.0);
        let key = sessions.take("Secret.md").expect("the session was there");
        assert_eq!(key.note(), "Secret");
        assert!(!sessions.contains("Secret.md"), "the old path is gone");
        assert!(sessions.take("Secret.md").is_none());
    }

    #[test]
    fn a_moved_high_water_follows_the_note_and_removes_the_old_path() {
        let (state, vault) = (temp_dir("state"), temp_dir("vault"));
        let mut revs = LockRevs::at(&state, &vault);
        revs.record("Old.md", 4).expect("record");
        revs.move_rel("Old.md", "New.md").expect("move");
        assert_eq!(revs.recorded("New.md"), 4);
        assert_eq!(revs.recorded("Old.md"), 0, "the old path is removed");
        assert_eq!(revs.check("New.md", 3), RevStatus::Older);

        let reloaded = LockRevs::at(&state, &vault);
        assert_eq!(reloaded.recorded("New.md"), 4, "the move was written once");
        assert_eq!(reloaded.recorded("Old.md"), 0);
    }

    #[test]
    fn a_move_keeps_the_larger_mark_when_the_target_has_one() {
        let (state, vault) = (temp_dir("state"), temp_dir("vault"));
        let mut revs = LockRevs::at(&state, &vault);
        revs.record("Old.md", 2).expect("record old");
        revs.record("New.md", 7).expect("record new");
        revs.move_rel("Old.md", "New.md").expect("move");
        assert_eq!(revs.recorded("New.md"), 7, "the larger mark wins");
        assert_eq!(revs.recorded("Old.md"), 0);

        revs.record("Old.md", 9).expect("record old again");
        revs.move_rel("Old.md", "New.md").expect("move");
        assert_eq!(revs.recorded("New.md"), 9, "and it still only raises");
    }

    #[test]
    fn a_move_without_an_old_record_is_a_no_op() {
        let (state, vault) = (temp_dir("state"), temp_dir("vault"));
        let mut revs = LockRevs::at(&state, &vault);
        revs.record("Other.md", 3).expect("record");
        revs.move_rel("Missing.md", "New.md")
            .expect("nothing to move");
        assert_eq!(revs.recorded("New.md"), 0);
        assert_eq!(revs.recorded("Other.md"), 3, "the rest is untouched");
    }

    #[test]
    fn a_recorded_revision_compares_older_same_and_newer() {
        let (state, vault) = (temp_dir("state"), temp_dir("vault"));
        let mut revs = LockRevs::at(&state, &vault);
        assert_eq!(revs.check("A.md", 1), RevStatus::Newer, "nothing recorded");
        revs.record("A.md", 3).expect("record");
        assert_eq!(revs.check("A.md", 2), RevStatus::Older);
        assert_eq!(revs.check("A.md", 3), RevStatus::Same);
        assert_eq!(revs.check("A.md", 4), RevStatus::Newer);
        assert_eq!(revs.check("B.md", 1), RevStatus::Newer, "per note");
    }

    #[test]
    fn a_lower_record_does_not_lower_the_mark() {
        let (state, vault) = (temp_dir("state"), temp_dir("vault"));
        let mut revs = LockRevs::at(&state, &vault);
        revs.record("A.md", 5).expect("record");
        revs.record("A.md", 2).expect("a lower record is a no-op");
        assert_eq!(revs.check("A.md", 4), RevStatus::Older);
        assert_eq!(revs.check("A.md", 5), RevStatus::Same);
    }

    #[test]
    fn a_new_store_reads_the_persisted_record() {
        let (state, vault) = (temp_dir("state"), temp_dir("vault"));
        {
            let mut revs = LockRevs::at(&state, &vault);
            revs.record("folder/A.md", 4).expect("record");
        }
        let revs = LockRevs::at(&state, &vault);
        assert_eq!(revs.check("folder/A.md", 3), RevStatus::Older);
        assert_eq!(revs.recorded("folder/A.md"), 4);
    }

    #[test]
    fn a_corrupt_record_starts_empty() {
        let (state, vault) = (temp_dir("state"), temp_dir("vault"));
        fs::create_dir_all(state.join("muninn").join("lock-revs")).expect("dir");
        let path = LockRevs::at(&state, &vault).path.clone();
        fs::write(&path, "{ not json").expect("corrupt record");
        let mut revs = LockRevs::at(&state, &vault);
        assert_eq!(revs.check("A.md", 1), RevStatus::Newer);
        revs.record("A.md", 2).expect("a corrupt file is replaced");
        let revs = LockRevs::at(&state, &vault);
        assert_eq!(revs.recorded("A.md"), 2);
    }
}

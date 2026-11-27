//! The browser state home across a reload (#240 slice 6d).
//!
//! The unit tests pin the mirror and the journal; this drives the shape the
//! boot has — the app's three stores write their keys, the poll loop drains
//! the plan to OPFS, and a reload walks those bytes back into a fresh mirror
//! before the first frame.

use muninn_core::StateHome;
use muninn_web::OpfsStateHome;

/// The keys the three consumers write: settings, a recovery pair and the lock
/// high-water record. They spell the native layout below the state home.
const SETTINGS: &str = "muninn/settings.json";
const RECOVERY_TEXT: &str = "muninn/recovery/a1b2c3~Notes__One.md.txt";
const RECOVERY_META: &str = "muninn/recovery/a1b2c3~Notes__One.md.json";
const LOCK_REVS: &str = "muninn/lock-revs/a1b2c3.json";

#[test]
fn settings_recovery_and_lock_keys_survive_a_reload() {
    let home = OpfsStateHome::new(true);
    home.put(SETTINGS, br#"{"theme":"dark","autosave_delay":1.4}"#)
        .unwrap();
    home.put(RECOVERY_TEXT, b"typed but not saved").unwrap();
    home.put(
        RECOVERY_META,
        br#"{"rel":"Notes/One.md","vault":"a1b2c3","ts":1700000000}"#,
    )
    .unwrap();
    home.put(LOCK_REVS, br#"{"Secret.md":4}"#).unwrap();

    // The poll loop's drain: every key's current mirror bytes.
    let plan = home.take_flush_plan();
    assert_eq!(plan.removals.len(), 0);
    assert_eq!(plan.writes.len(), 4, "{plan:?}");
    assert!(!home.has_dirty(), "taking the plan drains the journal");

    // A reload: the boot walk loads those bytes into a fresh mirror.
    let reloaded = OpfsStateHome::new(true);
    for key in &plan.writes {
        let bytes = home.read_bytes(key).expect("the bytes are in the mirror");
        reloaded.load(key, &bytes);
    }

    assert_eq!(
        reloaded.get(SETTINGS).as_deref(),
        Some(&br#"{"theme":"dark","autosave_delay":1.4}"#[..]),
        "settings survive the reload"
    );
    assert_eq!(
        reloaded.get(RECOVERY_TEXT).as_deref(),
        Some(&b"typed but not saved"[..]),
        "the crash-recovery copy survives the reload"
    );
    assert_eq!(
        reloaded.list("muninn/recovery/"),
        [RECOVERY_META, RECOVERY_TEXT],
        "both halves of the recovery slot are listed, the same order the scan walks"
    );
    assert_eq!(
        reloaded.get(LOCK_REVS).as_deref(),
        Some(&br#"{"Secret.md":4}"#[..]),
        "the lock high-water mark survives the reload"
    );
    assert!(!reloaded.has_dirty(), "a boot snapshot is not written back");
}

#[test]
fn a_cleared_recovery_copy_is_removed_on_the_next_drain() {
    let home = OpfsStateHome::new(true);
    home.put(RECOVERY_TEXT, b"typed").unwrap();
    home.put(RECOVERY_META, b"{}").unwrap();
    let _ = home.take_flush_plan();

    // Saving the note clears the copy: the after-reload mirror must not hold it.
    home.remove(RECOVERY_TEXT).unwrap();
    home.remove(RECOVERY_META).unwrap();
    let plan = home.take_flush_plan();
    assert_eq!(
        plan.removals,
        [RECOVERY_META, RECOVERY_TEXT],
        "removals, not writes: a cleared slot stays cleared"
    );
    assert!(plan.writes.is_empty());
}

#[test]
fn a_read_only_tab_reads_the_state_home_but_never_writes_it() {
    let home = OpfsStateHome::new(false);
    home.load(SETTINGS, br#"{"theme":"light"}"#);
    assert_eq!(
        home.get(SETTINGS).as_deref(),
        Some(&br#"{"theme":"light"}"#[..])
    );
    assert!(
        home.put(SETTINGS, b"{}").is_err(),
        "a read-only tab refuses settings writes"
    );
    assert!(home.put(RECOVERY_TEXT, b"x").is_err());
    assert!(!home.has_dirty(), "a refused write leaves nothing to flush");
}

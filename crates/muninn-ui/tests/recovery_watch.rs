//! SAF-05 crash recovery and SAF-03 external-change watcher tests.
//!
//! Each test uses an isolated XDG state home beside the temp vault, so these
//! cases never read or write the developer's real recovery directory.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use muninn_core::testing::TempVault;
use muninn_i18n::{Key, t};
use muninn_ui::{ConflictReason, RecoveryEntry, RecoverySlot, State, poll, view};
use rune::{Env as RuneEnv, Runtime, Tx};

/// A temp vault plus its private XDG state home.
struct TestEnv {
    vault: TempVault,
    home: PathBuf,
}

impl TestEnv {
    fn new(files: &[(&str, &str)]) -> Self {
        let vault = TempVault::new(files);
        let home = vault.path().with_extension("state");
        Self { vault, home }
    }

    /// A fixture the test built itself (a large vault, for the chunked path).
    fn from_vault(vault: TempVault) -> Self {
        let home = vault.path().with_extension("state");
        Self { vault, home }
    }

    fn state(&self) -> State {
        State::with_state_home(self.vault.path().to_path_buf(), self.home.clone())
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

/// Poll until the watcher has indexed the externally created `New.md`.
///
/// A brand-new note is not in the index yet, so the watcher can only notice it
/// through the vault folder's mtime. That mtime is coarse on Windows and can
/// trail the file's creation under load, so the wait is bounded by **wall
/// time**, not by the simulated poll clock: each iteration advances `t` past
/// the watch interval and then sleeps briefly. A healthy filesystem exits on
/// the first iteration; a stuck watcher still fails, just after the deadline.
/// The deadline outlasts a fully parallel CI runner: 15 s was missed once on a
/// windows-latest run whose watcher thread was starved by the rest of the
/// suite (the same tree passed the same job minutes earlier), and doubling a
/// stuck watcher's failure time costs nothing the assertion cares about.
fn wait_for_new_note(s: &mut State, tx: &mut Tx) {
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut t = 2.7;
    // Poll for the side effects (the watcher may autosave a dirty buffer), but
    // only the note being indexed ends the wait: `poll` returning true means
    // *some* change, not this one (#172).
    while s.vault.resolve("New").is_none() {
        assert!(
            Instant::now() < deadline,
            "the watcher never reported the new note within 15 s"
        );
        let _ = poll(s, t, tx);
        std::thread::sleep(Duration::from_millis(100));
        t += 2.0;
    }
}

#[test]
fn dirty_buffer_is_persisted_and_a_save_removes_it() {
    let env = TestEnv::new(&[("Alpha.md", "# Alpha\n")]);
    let mut s = env.state();
    let id = s.vault.resolve("Alpha").expect("Alpha");
    s.load_note(id, false);
    let rel = s.vault.notes[id].rel.clone();
    let copy = s.recovery_path(&rel);
    let mut tx = Tx::default();

    // Clean buffers are never copied.
    assert!(!poll(&mut s, 0.0, &mut tx));
    assert!(!copy.exists());

    s.editor.insert("unsaved text\n");
    assert!(s.dirty());
    assert!(!poll(&mut s, 0.01, &mut tx));
    assert!(!copy.exists(), "the short debounce has not elapsed");
    assert!(!poll(&mut s, 0.4, &mut tx));
    assert!(copy.is_file(), "a dirty buffer becomes recoverable");
    assert_eq!(
        fs::read_to_string(&copy).expect("recovery copy"),
        s.editor.text
    );

    assert!(s.save_now());
    assert!(!s.dirty());
    assert!(!copy.exists(), "a clean save clears the recovery copy");
}

#[test]
fn restart_lists_the_recovery_and_restore_marks_the_editor_dirty() {
    let env = TestEnv::new(&[("Alpha.md", "# Alpha\n")]);
    let text = "# Alpha\n\nTyped just before the crash.\n";
    let rel;
    {
        let mut s = env.state();
        let id = s.vault.resolve("Alpha").expect("Alpha");
        s.load_note(id, false);
        rel = s.vault.notes[id].rel.clone();
        s.editor.replace_text(text);
        let mut tx = Tx::default();
        poll(&mut s, 0.0, &mut tx);
        poll(&mut s, 0.4, &mut tx);
        assert!(s.recovery_path(&rel).is_file());
    }

    // A new State on the same state home is the next launch after a kill -9.
    let s = env.state();
    assert_eq!(s.recovered.len(), 1, "the banner data survives");
    assert_eq!(s.recovered[0].rel, rel);

    let mut rt = Runtime::new(s, view, (1180.0, 780.0), RuneEnv::new(true, 2.0));
    rt.frame(0.0);
    assert!(rt.find("recovery-banner").is_some(), "banner is visible");
    assert!(rt.find("recovery-restore-all").is_some());

    let mut restored = rt.state;
    assert!(!restored.dirty(), "a fresh launch starts clean");
    assert!(restored.restore_recovery(&rel));
    assert_eq!(restored.editor.text, text, "the typed text is intact");
    assert!(restored.dirty(), "restored text opens as an unsaved buffer");
    assert_eq!(
        restored
            .selected
            .and_then(|id| restored.vault.notes.get(id))
            .map(|n| n.rel.as_str()),
        Some(rel.as_str()),
        "restore selects the note"
    );

    assert!(restored.save_now());
    assert!(!restored.dirty());
    assert!(
        !restored.recovery_path(&rel).exists(),
        "saving the restored buffer clears the copy"
    );
    assert!(restored.recovered.is_empty(), "the banner row is gone");
}

#[test]
fn discard_removes_the_copy_without_touching_the_note() {
    let env = TestEnv::new(&[("Alpha.md", "# Alpha\n")]);
    let rel;
    {
        let mut s = env.state();
        let id = s.vault.resolve("Alpha").expect("Alpha");
        s.load_note(id, false);
        rel = s.vault.notes[id].rel.clone();
        s.editor.insert("discard me\n");
        let mut tx = Tx::default();
        poll(&mut s, 0.0, &mut tx);
        poll(&mut s, 0.4, &mut tx);
        assert!(s.recovery_path(&rel).is_file());
    }
    let mut s = env.state();
    assert!(s.discard_recovery(&rel));
    assert!(!s.recovery_path(&rel).exists());
    assert!(s.recovered.is_empty());
    assert_eq!(
        fs::read_to_string(env.vault.path().join("Alpha.md")).expect("note"),
        "# Alpha\n"
    );
}

#[test]
fn external_create_is_visible_on_the_next_watch_tick() {
    let env = TestEnv::new(&[("Alpha.md", "# Alpha\n")]);
    let mut s = env.state();
    let mut tx = Tx::default();
    assert!(!poll(&mut s, 1.0, &mut tx), "nothing changed yet");

    fs::write(env.vault.path().join("New.md"), "# New\n").expect("external create");
    // Directory mtimes tick coarsely (Windows), so keep polling through the
    // interval boundary until the snapshot sees the new entry.
    wait_for_new_note(&mut s, &mut tx);
    assert_eq!(s.vault.notes.len(), 2);
    assert!(s.vault.resolve("New").is_some(), "sidebar data rescanned");
}

#[test]
fn external_create_while_dirty_saves_then_reloads_without_losing_text() {
    // A second, clean note gives the watcher a reliable mtime signal. New-note
    // detection from the folder mtime is covered by the test above; this test
    // is about saving a dirty buffer before the reload, and on Windows the
    // folder mtime can lag (the #183 flake).
    let env = TestEnv::new(&[("Alpha.md", "# Alpha\n"), ("Side.md", "# Side\n")]);
    let mut s = env.state();
    let id = s.vault.resolve("Alpha").expect("Alpha");
    s.load_note(id, false);
    let path = s.vault.notes[id].path.clone();
    let mut tx = Tx::default();
    assert!(!poll(&mut s, 1.0, &mut tx));

    s.editor.insert("typed before the external create\n");
    fs::write(env.vault.path().join("New.md"), "# New\n").expect("external create");
    // Change the clean note too, so the reload has a metadata delta that does
    // not depend on the new file's directory entry being observed (#183).
    fs::write(env.vault.path().join("Side.md"), "# Side\n\nchanged\n").expect("external edit");
    // Keep polling until New is indexed. poll() also returns true when
    // autosave writes the dirty buffer, so a true poll is not enough.
    wait_for_new_note(&mut s, &mut tx);

    assert!(s.vault.resolve("New").is_some(), "the new note is indexed");
    assert!(!s.dirty(), "the dirty buffer was saved before reloading");
    assert!(
        fs::read_to_string(&path)
            .expect("saved note")
            .contains("typed before"),
        "the edit reached its own file, not the new note"
    );
    assert!(
        s.editor.text.contains("typed before"),
        "the editor keeps showing the saved text"
    );
}

#[test]
fn external_delete_of_a_clean_note_reloads_the_vault() {
    let env = TestEnv::new(&[("Alpha.md", "# Alpha\n"), ("Beta.md", "# Beta\n")]);
    let mut s = env.state();
    let alpha = s.vault.resolve("Alpha").expect("Alpha");
    s.load_note(alpha, false);
    let mut tx = Tx::default();
    assert!(!poll(&mut s, 1.0, &mut tx));

    fs::remove_file(env.vault.path().join("Beta.md")).expect("external delete");
    let mut t = 2.7;
    while !poll(&mut s, t, &mut tx) {
        std::thread::sleep(std::time::Duration::from_millis(120));
        t += 0.8;
        assert!(t < 8.0, "the watcher never reported the delete");
    }
    assert_eq!(s.vault.notes.len(), 1);
    assert!(s.vault.resolve("Beta").is_none());
    assert!(
        s.selected_path
            .as_ref()
            .is_some_and(|p| p.ends_with("Alpha.md"))
    );
}

const MINE: &str = "# Alpha\n\nMy in-progress edit.\n";
const THEIRS: &str = "# Alpha\n\nChanged externally, with more text.\n";

#[test]
fn external_edit_of_the_dirty_note_raises_a_conflict_and_keeps_the_buffer() {
    let env = TestEnv::new(&[("Alpha.md", "# Alpha\n")]);
    let mut s = env.state();
    let id = s.vault.resolve("Alpha").expect("Alpha");
    s.load_note(id, false);
    let path = s.vault.notes[id].path.clone();
    let mut tx = Tx::default();
    assert!(!poll(&mut s, 1.0, &mut tx));

    s.editor.replace_text(MINE);
    fs::write(&path, THEIRS).expect("external edit");
    assert!(poll(&mut s, 2.7, &mut tx), "raising a conflict is visible");

    let conflict = s.conflict.as_ref().expect("typed conflict");
    assert_eq!(conflict.reason, ConflictReason::Modified);
    assert_eq!(s.editor.text, MINE, "the dirty buffer is never clobbered");
    assert_eq!(fs::read_to_string(&path).expect("disk"), THEIRS);
}

#[test]
fn external_edit_of_a_clean_open_note_reloads_its_content() {
    let env = TestEnv::new(&[("Alpha.md", "# Alpha\n")]);
    let mut s = env.state();
    let id = s.vault.resolve("Alpha").expect("Alpha");
    s.load_note(id, false);
    let path = s.vault.notes[id].path.clone();
    let mut tx = Tx::default();
    assert!(!poll(&mut s, 1.0, &mut tx));

    fs::write(&path, THEIRS).expect("external edit");
    assert!(poll(&mut s, 2.7, &mut tx));
    assert_eq!(s.editor.text, THEIRS, "a clean buffer follows the disk");
    assert!(!s.dirty());
    assert!(s.conflict.is_none());
}

#[test]
fn an_unchanged_vault_keeps_the_watcher_quiet() {
    let env = TestEnv::new(&[("Alpha.md", "# Alpha\n")]);
    let mut s = env.state();
    let mut tx = Tx::default();
    for t in [1.6, 3.2, 4.8, 6.4] {
        assert!(!poll(&mut s, t, &mut tx), "poll at {t} must be quiet");
    }
}

/// A path well past the banner's 190 pt label budget.
const LONG_REL: &str =
    "Projects/Sisters/a-very-long-note-name-that-runs-well-past-the-label-budget.md";

fn banner_for(recovered: Vec<RecoveryEntry>) -> Runtime<State> {
    let env = TestEnv::new(&[("Alpha.md", "# Alpha\n")]);
    let mut s = env.state();
    s.recovered = recovered;
    let mut rt = Runtime::new(s, view, (1400.0, 900.0), RuneEnv::new(true, 2.0));
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);
    rt
}

fn element<'a>(els: &'a [rune::ElementInfo], key: &str) -> Option<&'a rune::ElementInfo> {
    els.iter()
        .find(|el| el.key.as_deref() == Some(key) && !el.exiting)
}

/// #68: two recovery copies of one note differ only by their slot suffix.
/// A long path must not truncate that suffix away, and the whole path stays
/// available as the label's help.
#[test]
fn a_long_path_keeps_the_suffix_that_distinguishes_the_two_rows() {
    let rt = banner_for(vec![
        RecoveryEntry {
            rel: LONG_REL.to_owned(),
            ts: 1,
            slot: RecoverySlot::Editor,
        },
        RecoveryEntry {
            rel: LONG_REL.to_owned(),
            ts: 2,
            slot: RecoverySlot::Kanban,
        },
    ]);
    let els = rt.elements();

    for (index, suffix) in [
        (0, t(Key::RecoveryWriteEdits)),
        (1, t(Key::RecoveryKanbanEdits)),
    ] {
        let slot = element(&els, &format!("recovery-slot-{index}"))
            .unwrap_or_else(|| panic!("row {index} keeps its slot label"));
        assert_eq!(slot.text.as_deref(), Some(suffix));
        assert!(
            slot.rect[2] > 0.0 && slot.rect[3] > 0.0,
            "row {index}'s suffix is laid out: {:?}",
            slot.rect
        );

        let path = element(&els, &format!("recovery-label-{index}")).expect("row's path");
        assert_eq!(path.text.as_deref(), Some(LONG_REL));
        assert!(
            path.rect[2] <= 190.5,
            "row {index}'s path stays within the label budget: {:?}",
            path.rect
        );
        assert_eq!(
            path.help.as_deref(),
            Some(LONG_REL),
            "the cut path's help keeps the whole path"
        );
    }
}

/// A lone copy has no suffix; it still truncates its long path cleanly and
/// offers the whole path as help.
#[test]
fn a_single_row_still_truncates_its_long_path_cleanly() {
    let rt = banner_for(vec![RecoveryEntry {
        rel: LONG_REL.to_owned(),
        ts: 1,
        slot: RecoverySlot::Editor,
    }]);
    let els = rt.elements();

    assert!(
        element(&els, "recovery-slot-0").is_none(),
        "a lone copy has no slot suffix"
    );
    let path = element(&els, "recovery-label-0").expect("the path row");
    assert_eq!(path.text.as_deref(), Some(LONG_REL));
    assert!(
        path.rect[2] <= 190.5,
        "the long path is cut to the label budget: {:?}",
        path.rect
    );
    assert_eq!(
        path.help.as_deref(),
        Some(LONG_REL),
        "the cut path's help keeps the whole path"
    );
}

/// #132: a locked note's body must never reach the crash-recovery directory.
const SECRET_MARKER: &str = "TOP SECRET BODY";

fn lock_note(text: &str) -> String {
    // The fixture locks the vault-root `Secret.md`; its id has no `.md`.
    muninn_core::lock(text, "pw", "Secret").expect("lock")
}

/// Any recovery file under `env`'s state home whose bytes contain `needle`.
fn recovery_file_with(env: &TestEnv, needle: &str) -> Option<PathBuf> {
    let dir = env.home.join("muninn").join("recovery");
    fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        let text = fs::read_to_string(&path).ok()?;
        text.contains(needle).then_some(path)
    })
}

#[test]
fn a_locked_note_never_gets_a_plaintext_recovery_copy() {
    let envelope = lock_note("# Secret\n\nsecret body\n");
    let env = TestEnv::new(&[("Secret.md", envelope.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");
    assert!(
        s.vault.notes[id].is_locked(),
        "the envelope is a locked note"
    );
    s.load_note(id, false);
    let rel = s.vault.notes[id].rel.clone();
    s.editor.replace_text("# Secret\n\nTOP SECRET BODY\n");
    assert!(s.dirty());

    let mut tx = Tx::default();
    poll(&mut s, 0.0, &mut tx);
    // Past RECOVERY_DEBOUNCE, before autosave.
    poll(&mut s, 0.4, &mut tx);
    assert!(s.dirty(), "the buffer is still unsaved");
    assert!(
        !s.recovery_path(&rel).exists(),
        "a locked note gets no editor copy"
    );
    assert!(
        !s.kanban_recovery_path(&rel).exists(),
        "a locked note gets no kanban copy"
    );
    assert_eq!(
        recovery_file_with(&env, SECRET_MARKER),
        None,
        "no file under the recovery directory holds the body"
    );
    assert_eq!(
        s.editor.text, "# Secret\n\nTOP SECRET BODY\n",
        "the in-memory buffer is untouched"
    );
}

#[test]
fn a_stale_plaintext_copy_is_cleared_once_the_note_is_locked() {
    let body = "# Secret\n\nTOP SECRET BODY\n";
    let env = TestEnv::new(&[("Secret.md", "# Secret\n\nplain for now\n")]);
    let rel;
    let stale;
    {
        let mut s = env.state();
        let id = s.vault.resolve("Secret").expect("Secret");
        s.load_note(id, false);
        rel = s.vault.notes[id].rel.clone();
        s.editor.replace_text(body);
        let mut tx = Tx::default();
        poll(&mut s, 0.0, &mut tx);
        poll(&mut s, 0.4, &mut tx);
        stale = s.recovery_path(&rel);
        assert!(
            stale.is_file(),
            "the copy exists while the note is plaintext"
        );
        assert_eq!(fs::read_to_string(&stale).expect("copy"), body);
    }
    // Lock the note on disk, as the lock layer does.
    fs::write(env.vault.path().join("Secret.md"), lock_note(body)).expect("lock");

    // The next launch must not offer it, and must delete the plaintext.
    let s = env.state();
    assert!(
        !s.recovered.iter().any(|entry| entry.rel == rel),
        "the banner never lists a locked note's plaintext"
    );
    assert!(!stale.exists(), "the stale copy is deleted on load");
    assert_eq!(recovery_file_with(&env, SECRET_MARKER), None);
}

/// The chunked path has no note bodies at startup: the sweep must run when the
/// finished scan is installed, not only in the constructor.
#[test]
fn the_chunked_scan_sweeps_a_locked_note_once_the_vault_lands() {
    let body = "# Secret\n\nTOP SECRET BODY\n";
    let mut files: Vec<(String, String)> = (0..120)
        .map(|i| (format!("Note {i:03}.md"), format!("# Note {i:03}\n")))
        .collect();
    files.push((
        "Secret.md".to_owned(),
        "# Secret\n\nplain for now\n".to_owned(),
    ));
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let env = TestEnv::from_vault(TempVault::new(&refs));

    let rel;
    let stale;
    {
        let mut s = env.state();
        assert!(s.loading(), "121 notes take the chunked path");
        let mut tx = Tx::default();
        let mut t = 0.0;
        while s.loading() {
            poll(&mut s, t, &mut tx);
            t += 0.05;
        }
        let id = s.vault.resolve("Secret").expect("Secret");
        s.load_note(id, false);
        rel = s.vault.notes[id].rel.clone();
        s.editor.replace_text(body);
        poll(&mut s, 0.0, &mut tx);
        poll(&mut s, 0.4, &mut tx);
        stale = s.recovery_path(&rel);
        assert!(
            stale.is_file(),
            "the copy exists while the note is plaintext"
        );
    }
    fs::write(env.vault.path().join("Secret.md"), lock_note(body)).expect("lock");

    let mut s = env.state();
    assert!(s.loading(), "a large vault scans in chunks");
    assert!(
        s.recovered.iter().any(|entry| entry.rel == rel),
        "the copy is listed before the scan finishes"
    );
    let mut tx = Tx::default();
    let mut t = 2.0;
    while s.loading() {
        poll(&mut s, t, &mut tx);
        t += 0.05;
    }
    assert!(
        s.vault
            .notes
            .iter()
            .any(|note| note.rel == rel && note.is_locked()),
        "the installed vault marks the note locked"
    );
    assert!(
        !s.recovered.iter().any(|entry| entry.rel == rel),
        "installing the scan swept the banner row"
    );
    assert!(!stale.exists(), "the stale copy is deleted");
}

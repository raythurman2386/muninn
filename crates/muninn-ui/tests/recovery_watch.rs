//! SAF-05 crash recovery and SAF-03 external-change watcher tests.
//!
//! Each test uses an isolated XDG state home beside the temp vault, so these
//! cases never read or write the developer's real recovery directory.

use std::fs;
use std::path::PathBuf;

use muninn_core::testing::TempVault;
use muninn_ui::{ConflictReason, State, poll, view};
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

    fn state(&self) -> State {
        State::with_state_home(self.vault.path().to_path_buf(), self.home.clone())
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
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
    let mut t = 2.7;
    while !(poll(&mut s, t, &mut tx) || s.vault.resolve("New").is_some()) {
        std::thread::sleep(std::time::Duration::from_millis(60));
        t += 0.6;
        assert!(t < 8.0, "the watcher never reported the new note");
    }
    assert_eq!(s.vault.notes.len(), 2);
    assert!(s.vault.resolve("New").is_some(), "sidebar data rescanned");
}

#[test]
fn external_create_while_dirty_saves_then_reloads_without_losing_text() {
    let env = TestEnv::new(&[("Alpha.md", "# Alpha\n")]);
    let mut s = env.state();
    let id = s.vault.resolve("Alpha").expect("Alpha");
    s.load_note(id, false);
    let path = s.vault.notes[id].path.clone();
    let mut tx = Tx::default();
    assert!(!poll(&mut s, 1.0, &mut tx));

    s.editor.insert("typed before the external create\n");
    fs::write(env.vault.path().join("New.md"), "# New\n").expect("external create");
    // Keep watching through the interval boundary until the reload lands;
    // directory mtimes tick coarsely on Windows, so this is a bounded burst.
    let mut t = 2.7;
    while !poll(&mut s, t, &mut tx) {
        std::thread::sleep(std::time::Duration::from_millis(120));
        t += 0.8;
        assert!(t < 8.0, "the watcher never reported the new note");
    }

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

//! Per-note encryption (#134): rename, move, folder rename and conflicts.
//!
//! A locked note's envelope authenticates its note id, so a rename or a move
//! re-encrypts the file while it is unlocked. These drive the real state and,
//! where the assertion is about what is drawn, the real Rune runtime. Every
//! test keeps a private XDG state home beside its temp vault.

use std::fs;
use std::path::PathBuf;

use muninn_core::lock;
use muninn_core::testing::TempVault;
use muninn_i18n::{Key, t};
use muninn_ui::{ConflictReason, ConflictState, State, UnlockError, on_key, poll, view};
use rune::{Env, Runtime};

const PW: &str = "correct horse battery staple";
const BODY: &str = "# Secret\n\nTOP SECRET BODY\n";
const MARKER: &str = "TOP SECRET BODY";
const ENVELOPE_HEADER: &str = "MUNINN-NOTE-LOCK v2";

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

    fn path(&self, name: &str) -> PathBuf {
        self.vault.path().join(name)
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.path(name)).expect("read note")
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

fn envelope(note: &str) -> String {
    lock(BODY, PW, note).expect("lock")
}

fn type_into(field: &mut rune::TextEdit, text: &str) {
    field.replace_text(text);
    field.anchor = text.len();
    field.focus = text.len();
}

fn runtime(state: State) -> Runtime<State> {
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn unlock_and_open(env: &TestEnv, name: &str) -> (State, usize) {
    let mut s = env.state();
    let id = s.vault.resolve(name).expect("note");
    s.unlock_note(id, PW).expect("unlock");
    // The vault opens with the first note selected; selecting it again would
    // be a no-op, so drop the selection to force a real editor load.
    s.selected = None;
    s.load_note(id, false);
    (s, id)
}

#[test]
fn renaming_an_unlocked_locked_note_re_encrypts_under_the_new_name() {
    let env = TestEnv::new(&[("Secret.md", envelope("Secret").as_str())]);
    let (mut s, id) = unlock_and_open(&env, "Secret");
    assert_eq!(s.editor.text, BODY);

    s.begin_rename_note(id);
    assert!(s.rename_prompt.is_some(), "an unlocked note may rename");
    type_into(&mut s.rename_field, "Renamed");
    s.submit_rename_note();

    assert_eq!(s.error, None, "the rename succeeded");
    assert!(!env.path("Secret.md").exists(), "the old path is gone");
    let disk = env.read("Renamed.md");
    assert!(
        disk.starts_with(&format!("{ENVELOPE_HEADER}\n")),
        "the new file is an envelope: {disk:?}"
    );
    assert!(!disk.contains(MARKER), "the body is not on disk");
    assert_eq!(
        muninn_core::unlock(&disk, PW, "Renamed").expect("opens under the new id"),
        BODY,
        "the same password opens the same body"
    );

    let id = s.vault.resolve("Renamed").expect("Renamed is indexed");
    assert!(s.vault.notes[id].is_unlocked(), "the session followed it");
    assert_eq!(s.editor.text, BODY, "the editor still shows the body");
    assert!(
        s.vault.search(MARKER).contains(&id),
        "the renamed note is still searchable in this session"
    );

    // The high-water mark moved with the file: an old rev-1 envelope written
    // at the new name is a rollback for a fresh state, not a valid unlock.
    fs::write(env.path("Renamed.md"), envelope("Renamed")).expect("roll back the file");
    let mut fresh = env.state();
    let id = fresh.vault.resolve("Renamed").expect("Renamed");
    assert!(
        matches!(fresh.unlock_note(id, PW), Err(UnlockError::Rollback { .. })),
        "the rev-1 envelope is older than the rename's revision"
    );
    assert!(!fresh.is_note_unlocked("Renamed.md"));
}

#[test]
fn moving_an_unlocked_locked_note_keeps_the_password_and_the_body() {
    let env = TestEnv::new(&[("Secret.md", envelope("Secret").as_str())]);
    fs::create_dir(env.path("Archive")).expect("folder");
    let (mut s, id) = unlock_and_open(&env, "Secret");

    s.begin_move_note(id);
    assert_eq!(s.move_picker, Some(id), "an unlocked note may move");
    s.move_note_to(id, "Archive");

    assert_eq!(s.error, None);
    assert!(!env.path("Secret.md").exists());
    let disk = env.read("Archive/Secret.md");
    assert!(disk.starts_with(&format!("{ENVELOPE_HEADER}\n")));
    assert!(!disk.contains(MARKER));
    assert_eq!(
        muninn_core::unlock(&disk, PW, "Archive/Secret").expect("opens under the new id"),
        BODY
    );

    let id = s.vault.resolve("Archive/Secret").expect("moved note");
    assert_eq!(s.editor.text, BODY);
    assert!(s.vault.notes[id].is_unlocked());
    assert!(s.vault.search(MARKER).contains(&id));

    // The old name's mark moved too, so a rev-1 file at the new path fails.
    fs::write(env.path("Archive/Secret.md"), envelope("Archive/Secret")).expect("roll back");
    let mut fresh = env.state();
    let id = fresh
        .vault
        .resolve("Archive/Secret")
        .expect("Archive/Secret");
    assert!(matches!(
        fresh.unlock_note(id, PW),
        Err(UnlockError::Rollback { .. })
    ));
}

#[test]
fn renaming_or_moving_a_note_that_is_not_unlocked_is_refused() {
    let envelope = envelope("Secret");
    let env = TestEnv::new(&[("Secret.md", envelope.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");

    s.begin_rename_note(id);
    assert_eq!(s.error.as_deref(), Some(t(Key::LockRefusedRename)));
    assert!(s.rename_prompt.is_none(), "no prompt is opened");

    s.error = None;
    s.begin_move_note(id);
    assert_eq!(s.error.as_deref(), Some(t(Key::LockRefusedRename)));
    assert!(s.move_picker.is_none(), "no picker is opened");

    assert_eq!(
        env.read("Secret.md"),
        envelope,
        "the file is byte-for-byte unchanged"
    );
    assert!(env.path("Secret.md").exists());
}

#[test]
fn a_folder_holding_a_locked_note_refuses_the_folder_rename() {
    let env = TestEnv::new(&[
        ("Vault/Secret.md", envelope("Vault/Secret").as_str()),
        ("Vault/Plain.md", "# Plain\n"),
    ]);
    let mut s = env.state();

    s.begin_rename_folder("Vault");
    assert!(s.rename_folder_prompt.is_some());
    type_into(&mut s.rename_folder_field, "Renamed");
    s.submit_rename_folder();

    assert_eq!(s.error.as_deref(), Some(t(Key::LockRefusedFolderRename)));
    assert!(s.rename_folder_prompt.is_none(), "the prompt is cancelled");
    assert!(
        env.path("Vault/Secret.md").exists(),
        "the locked note keeps its path"
    );
    assert!(env.path("Vault/Plain.md").exists());
    assert!(!env.path("Renamed").exists(), "the folder did not move");
}

#[test]
fn loading_theirs_on_a_locked_note_drops_the_session_and_shows_the_lock_page() {
    let env = TestEnv::new(&[("Secret.md", envelope("Secret").as_str())]);
    let (mut s, id) = unlock_and_open(&env, "Secret");
    assert_eq!(s.editor.text, BODY);

    s.conflict = Some(ConflictState {
        note_id: id,
        expected: None,
        observed: None,
        reason: ConflictReason::Modified,
    });
    assert!(s.resolve_conflict_load_theirs());

    assert!(
        s.editor.text.is_empty(),
        "the envelope never reaches the buffer"
    );
    assert!(!s.is_note_unlocked("Secret.md"), "the session ended");
    assert!(s.vault.notes[id].readable_source().is_none());
    assert!(s.vault.search(MARKER).is_empty());

    let rt = runtime(s);
    assert!(
        rt.find("locked-note-page").is_some(),
        "the note is shown locked, ready for a fresh unlock"
    );
}

#[test]
fn the_conflict_bar_explains_instead_of_offering_save_copy_for_a_locked_note() {
    let env = TestEnv::new(&[("Secret.md", envelope("Secret").as_str())]);
    let (mut s, id) = unlock_and_open(&env, "Secret");
    assert_eq!(s.editor.text, BODY);
    s.conflict = Some(ConflictState {
        note_id: id,
        expected: None,
        observed: None,
        reason: ConflictReason::Modified,
    });

    let rt = runtime(s);
    assert!(rt.find("conflict-keep-mine").is_some());
    assert!(rt.find("conflict-load-theirs").is_some());
    assert!(
        rt.find("conflict-save-copy").is_none(),
        "a locked note never offers Save copy"
    );
    let label = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("conflict-save-copy-locked"))
        .and_then(|element| element.text)
        .expect("the replacement explanation is drawn");
    assert_eq!(label, t(Key::LockConflictSaveCopy));
}

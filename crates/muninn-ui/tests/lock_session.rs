//! Per-note encryption (#133): the unlocked-note session and the rollback
//! high-water record.
//!
//! Each test keeps a private XDG state home beside its temp vault, so the
//! session record never touches the developer's real state directory.

use std::fs;
use std::path::PathBuf;

use muninn_core::testing::TempVault;
use muninn_core::{lock, unlock_session};
use muninn_ui::{State, UnlockError, poll};
use rune::Tx;

const PW: &str = "correct horse battery staple";
const BODY: &str = "# Secret\n\nTOP SECRET BODY\n";
const MARKER: &str = "TOP SECRET BODY";

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

/// An envelope for the vault-root `Secret.md`, whose note id is `Secret`.
fn secret_envelope(text: &str) -> String {
    lock(text, PW, "Secret").expect("lock")
}

#[test]
fn the_right_password_returns_the_body_and_starts_a_session() {
    let envelope = secret_envelope(BODY);
    let env = TestEnv::new(&[("Secret.md", envelope.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");

    assert_eq!(s.unlock_note(id, PW).expect("unlock"), BODY);
    assert!(s.is_note_unlocked("Secret.md"));
    assert!(!s.is_note_unlocked("Other.md"));
}

#[test]
fn a_wrong_password_fails_closed_and_stores_no_session() {
    let envelope = secret_envelope(BODY);
    let env = TestEnv::new(&[("Secret.md", envelope.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");

    let error = s.unlock_note(id, "not the password").expect_err("wrong pw");
    assert!(matches!(
        error,
        UnlockError::Lock(muninn_core::LockError::AuthenticationFailed)
    ));
    assert!(!s.is_note_unlocked("Secret.md"));
    assert!(s.vault.notes[id].is_locked(), "the note stays locked");
}

#[test]
fn an_envelope_for_another_note_is_wrong_note() {
    let envelope = lock(BODY, PW, "Other").expect("lock");
    let env = TestEnv::new(&[("Secret.md", envelope.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");

    let error = s.unlock_note(id, PW).expect_err("another note's envelope");
    assert!(matches!(
        error,
        UnlockError::Lock(muninn_core::LockError::WrongNote)
    ));
    assert!(!s.is_note_unlocked("Secret.md"));
}

#[test]
fn an_idle_session_locks_the_note_and_clears_its_buffers() {
    let envelope = secret_envelope(BODY);
    let env = TestEnv::new(&[("Secret.md", envelope.as_str()), ("Other.md", "# Other\n")]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");
    let other = s.vault.resolve("Other").expect("Other");
    let mut tx = Tx::default();
    // The unlocked clock starts here, at t = 0.
    assert!(!poll(&mut s, 0.0, &mut tx));

    assert_eq!(s.unlock_note(id, PW).expect("unlock"), BODY);
    s.load_note(id, false);
    // Layer 5's save path seals the envelope and keeps `saved` as plaintext,
    // so a clean unlocked buffer is exactly this state: plaintext in the
    // editor, nothing to autosave.
    s.editor.replace_text(BODY);
    s.saved = BODY.to_owned();

    assert!(!poll(&mut s, 899.0, &mut tx), "899 s is not a change");
    assert_eq!(s.editor.text, BODY, "the session is still open");
    assert!(s.is_note_unlocked("Secret.md"));

    assert!(poll(&mut s, 900.0, &mut tx), "locking the note is visible");
    assert!(s.editor.text.is_empty(), "the buffer is cleared");
    assert!(!s.is_note_unlocked("Secret.md"));

    // Revisiting the note must not bring the plaintext back from the parked
    // editor snapshot (text or undo history).
    s.load_note(other, false);
    s.load_note(id, false);
    assert!(
        !s.editor.text.contains(MARKER),
        "the parked buffer is gone: {:?}",
        s.editor.text
    );
    assert!(
        s.vault.notes[id].is_locked(),
        "and the note is locked again"
    );
}

#[test]
fn a_rolled_back_envelope_is_refused() {
    let rev1 = secret_envelope(BODY);
    let env = TestEnv::new(&[("Secret.md", rev1.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");

    // The first unlock records revision 1.
    assert_eq!(s.unlock_note(id, PW).expect("rev 1"), BODY);

    // Seal revision 2 with the same key and write it over the note.
    let (mut key, _) = unlock_session(&rev1, PW, "Secret").expect("session key");
    let rev2 = key.seal("# Secret\n\nrevision two\n").expect("seal");
    s.vault.write_note(id, &rev2).expect("write rev 2");
    assert_eq!(
        s.unlock_note(id, PW).expect("rev 2"),
        "# Secret\n\nrevision two\n"
    );

    // An attacker (or a stale copy) puts the older envelope back.
    s.vault.write_note(id, &rev1).expect("roll back");
    assert!(matches!(
        s.unlock_note(id, PW),
        Err(UnlockError::Rollback {
            found: 1,
            recorded: 2
        })
    ));
}

#[test]
fn seal_note_seals_with_the_session_key_and_renews_it() {
    let envelope = secret_envelope(BODY);
    let env = TestEnv::new(&[("Secret.md", envelope.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");

    // No session yet: nothing to seal with.
    assert!(s.seal_note(id, BODY).is_none());

    assert_eq!(s.unlock_note(id, PW).expect("unlock"), BODY);
    let first = s
        .seal_note(id, BODY)
        .expect("unlocked")
        .expect("first seal");
    assert_eq!(
        muninn_core::unlock(&first, PW, "Secret").expect("the envelope opens"),
        BODY
    );
    let second = s
        .seal_note(id, BODY)
        .expect("unlocked")
        .expect("second seal");
    // The source envelope was revision 1, so the session writes 2 and 3.
    assert!(first.contains("\nrev=2\n"), "{first}");
    assert!(second.contains("\nrev=3\n"), "{second}");
    let nonce = |envelope: &str| {
        envelope
            .lines()
            .find(|line| line.starts_with("aead="))
            .expect("aead line")
            .to_owned()
    };
    assert_ne!(nonce(&first), nonce(&second), "a fresh nonce per seal");
    assert_ne!(first, second);

    // Sealing does not write: the note on disk is still the envelope it was.
    assert_eq!(
        fs::read_to_string(env.vault.path().join("Secret.md")).expect("note"),
        envelope
    );
    assert!(s.is_note_unlocked("Secret.md"));
}

#[test]
fn a_save_of_an_unlocked_note_writes_the_envelope() {
    let envelope = secret_envelope(BODY);
    let env = TestEnv::new(&[("Secret.md", envelope.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");
    assert_eq!(s.unlock_note(id, PW).expect("unlock"), BODY);

    // The unlocked buffer as layer 5 shows it: plaintext in the editor and a
    // plaintext `saved` copy to compare against.
    let new_body = "# Secret\n\nA new body after unlocking.\n";
    s.editor.replace_text(new_body);
    s.saved = BODY.to_owned();
    assert!(s.dirty());
    assert!(s.save_now(), "the sealed save reaches disk");

    let disk = fs::read_to_string(env.vault.path().join("Secret.md")).expect("note");
    assert!(disk.starts_with("MUNINN-NOTE-LOCK v2\n"), "{disk}");
    assert!(
        !disk.contains("A new body"),
        "the new body is encrypted, not written"
    );
    assert!(!disk.contains(MARKER), "and the old body is gone too");
    assert_eq!(
        muninn_core::unlock(&disk, PW, "Secret").expect("the envelope opens"),
        new_body
    );
    assert_eq!(s.editor.text, new_body, "the buffer stays plaintext");
    assert!(!s.dirty(), "the buffer matches what was saved");
    assert!(s.is_note_unlocked("Secret.md"), "saving renews the session");
    assert!(s.vault.notes[id].is_locked(), "the index sees the envelope");
}

#[test]
fn a_save_without_a_session_is_refused() {
    let envelope = secret_envelope(BODY);
    let env = TestEnv::new(&[("Secret.md", envelope.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");
    assert_eq!(s.unlock_note(id, PW).expect("unlock"), BODY);
    // The session ended (an explicit lock, or the idle timer).
    s.forget_unlocked_note(id);
    assert!(!s.is_note_unlocked("Secret.md"));

    // Type into the buffer again with no key to seal it with.
    s.editor.replace_text("# Secret\n\nTyped after the lock.\n");
    assert!(s.dirty());
    assert!(
        !s.save_now(),
        "a locked note with no session is never written as plaintext"
    );

    let disk = fs::read_to_string(env.vault.path().join("Secret.md")).expect("note");
    assert!(disk.starts_with("MUNINN-NOTE-LOCK v2\n"), "{disk}");
    assert_eq!(disk, envelope, "the envelope is byte-for-byte untouched");
    assert!(!disk.contains("Typed after the lock"));
    assert!(s.dirty(), "the buffer is kept, still dirty");
    assert!(s.vault.notes[id].is_locked());
}

#[test]
fn keeping_mine_on_a_locked_note_seals_the_buffer() {
    let envelope = secret_envelope(BODY);
    let env = TestEnv::new(&[("Secret.md", envelope.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");
    assert_eq!(s.unlock_note(id, PW).expect("unlock"), BODY);
    let path = env.vault.path().join("Secret.md");

    // Another writer changes the note after it was loaded, so the save lands in
    // the conflict flow instead of overwriting.
    let theirs = secret_envelope("# Secret\n\nchanged elsewhere, with a longer body\n");
    fs::write(&path, &theirs).expect("external write");
    s.editor.replace_text("# Secret\n\nmine, kept\n");
    s.saved = BODY.to_owned();
    assert!(s.save_now(), "the changed disk state raises a conflict");
    assert!(s.conflict.is_some());
    assert_eq!(
        fs::read_to_string(&path).expect("note"),
        theirs,
        "the save wrote nothing"
    );

    assert!(s.resolve_conflict_keep_mine());
    let disk = fs::read_to_string(&path).expect("note");
    assert!(disk.starts_with("MUNINN-NOTE-LOCK v2\n"), "{disk}");
    assert!(!disk.contains("mine, kept"), "kept as ciphertext only");
    assert_eq!(
        muninn_core::unlock(&disk, PW, "Secret").expect("the envelope opens"),
        "# Secret\n\nmine, kept\n"
    );
    assert!(s.conflict.is_none());
}

#[test]
fn saving_a_copy_of_a_locked_note_is_refused() {
    let envelope = secret_envelope(BODY);
    let env = TestEnv::new(&[("Secret.md", envelope.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");
    assert_eq!(s.unlock_note(id, PW).expect("unlock"), BODY);
    let path = env.vault.path().join("Secret.md");

    let theirs = secret_envelope("# Secret\n\nchanged elsewhere, with a longer body\n");
    fs::write(&path, &theirs).expect("external write");
    s.editor.replace_text("# Secret\n\nmine, kept\n");
    s.saved = BODY.to_owned();
    assert!(s.save_now(), "a conflict is raised");
    assert!(s.conflict.is_some());

    assert!(
        !s.resolve_conflict_save_copy(),
        "a plaintext sibling would carry the body, and a same-id copy would not open"
    );
    assert!(s.conflict.is_some(), "the conflict is still up");
    assert_eq!(fs::read_to_string(&path).expect("note"), theirs);
    assert_eq!(
        fs::read_dir(env.vault.path()).expect("vault").count(),
        1,
        "no copy was created"
    );
}

#[test]
fn a_dirty_buffer_keeps_the_session_open_past_the_unlock_timeout() {
    let envelope = secret_envelope(BODY);
    let env = TestEnv::new(&[("Secret.md", envelope.as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");
    let mut tx = Tx::default();
    assert!(!poll(&mut s, 0.0, &mut tx));

    assert_eq!(s.unlock_note(id, PW).expect("unlock"), BODY);
    s.load_note(id, false);
    // Leave `saved` on the envelope, so the plaintext buffer is dirty.
    s.editor.replace_text(BODY);
    assert!(s.dirty(), "the buffer counts as use");

    // A dirty buffer touches the session on every tick. This poll is long past
    // the 900 s since the unlock, so without the touch the note would already
    // be locked (and its buffer purged) here.
    let _ = poll(&mut s, 899.0, &mut tx);
    assert!(s.is_note_unlocked("Secret.md"));
    assert_eq!(s.editor.text, BODY);
    let _ = poll(&mut s, 1700.0, &mut tx);
    assert!(
        s.is_note_unlocked("Secret.md"),
        "800 s past the unlock, the edit renewed the clock"
    );
    assert_eq!(s.editor.text, BODY);

    // The autosave at the dirty tick sealed the buffer into a fresh envelope
    // (`save_now` seals a locked note, never writes plaintext) and made the
    // buffer clean by setting `saved` to the plaintext, so the clock now runs
    // from the last dirty tick at 1700 s.
    assert!(!s.dirty());
    let disk = fs::read_to_string(env.vault.path().join("Secret.md")).expect("note");
    assert!(disk.starts_with("MUNINN-NOTE-LOCK v2\n"), "{disk}");
    assert!(!disk.contains(MARKER), "the body never reached the file");
    assert_eq!(
        muninn_core::unlock(&disk, PW, "Secret").expect("the sealed body"),
        BODY
    );
    let _ = poll(&mut s, 2599.0, &mut tx);
    assert!(s.is_note_unlocked("Secret.md"), "899 s after the last use");
    assert!(poll(&mut s, 2600.0, &mut tx), "the expiry is visible");
    assert!(!s.is_note_unlocked("Secret.md"));
    assert!(s.editor.text.is_empty());
}

#[test]
fn switching_vaults_ends_the_session_and_keeps_the_record_per_vault() {
    let envelope_a = secret_envelope(BODY);
    let other_vault = TempVault::new(&[("Secret.md", "# Other vault\n")]);
    let env = TestEnv::new(&[("Secret.md", envelope_a.as_str())]);
    let mut s = State::with_state_home(env.vault.path().to_path_buf(), env.home.clone());
    let id = s.vault.resolve("Secret").expect("Secret");
    assert_eq!(s.unlock_note(id, PW).expect("unlock"), BODY);
    assert!(s.is_note_unlocked("Secret.md"));

    assert!(s.switch_vault(other_vault.path().to_path_buf()));
    assert!(
        !s.is_note_unlocked("Secret.md"),
        "the other vault may have a note with the same name"
    );

    assert!(s.switch_vault(env.vault.path().to_path_buf()));
    assert!(
        !s.is_note_unlocked("Secret.md"),
        "switching back does not restore the old session"
    );
}

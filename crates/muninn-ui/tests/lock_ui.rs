//! Per-note encryption (#134): the lock/unlock UI.
//!
//! The password prompt, the locked-note page, the sidebar glyph and the two
//! note actions are driven through the real state and (where the assertion is
//! about what is drawn) the real Rune runtime. Every test keeps a private XDG
//! state home beside its temp vault, so no session record or recovery copy
//! reaches the developer's real state directory.

use std::fs;
use std::path::PathBuf;

use muninn_core::lock;
use muninn_core::testing::TempVault;
use muninn_i18n::{Key, t};
use muninn_ui::{
    Layer, LockPromptKind, Mode, PaletteAction, PaletteItem, State, on_key, poll, view,
};
use rune::{Env, KeyEvent, Mods, Runtime, Tx};

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

fn envelope(text: &str) -> String {
    lock(text, PW, "Secret").expect("lock")
}

fn runtime(state: State) -> Runtime<State> {
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

fn type_into(field: &mut rune::TextEdit, text: &str) {
    field.replace_text(text);
    field.anchor = text.len();
    field.focus = text.len();
}

fn fill_prompt(s: &mut State, password: &str, confirm: Option<&str>) {
    let prompt = s.lock_prompt.as_mut().expect("the prompt is open");
    type_into(&mut prompt.password, password);
    if let Some(confirm) = confirm {
        type_into(&mut prompt.confirm, confirm);
    }
}

fn find_action(s: &State, action: PaletteAction) -> usize {
    s.palette_items()
        .iter()
        .position(|item| *item == PaletteItem::Action(action))
        .unwrap_or_else(|| panic!("{action:?} is not offered"))
}

#[test]
fn a_locked_note_loads_empty_and_shows_the_lock_page() {
    let env = TestEnv::new(&[("Secret.md", envelope(BODY).as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");
    assert!(s.vault.notes[id].is_locked());

    s.load_note(id, false);
    assert!(
        s.editor.text.is_empty(),
        "the envelope never reaches the buffer: {:?}",
        s.editor.text
    );
    assert!(s.saved.is_empty());
    assert!(!s.dirty());
    assert!(!s.editor.focused, "there is no editor to type into");

    let rt = runtime(s);
    assert!(
        rt.find("locked-note-page").is_some(),
        "the note page is replaced by the lock page"
    );
    assert!(rt.find("editor").is_none(), "no editor is drawn");
    assert!(!rt.state.editor.text.contains(MARKER));
    assert!(!rt.state.editor.text.contains(ENVELOPE_HEADER));
}

#[test]
fn unlocking_with_the_prompt_puts_the_body_back() {
    let env = TestEnv::new(&[("Secret.md", envelope(BODY).as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");
    assert!(s.vault.search(MARKER).is_empty(), "locked: not indexed");

    s.begin_unlock_note(id);
    assert_eq!(
        s.lock_prompt.as_ref().map(|prompt| prompt.kind),
        Some(LockPromptKind::Unlock)
    );
    fill_prompt(&mut s, PW, None);
    s.submit_lock_prompt();

    assert!(s.lock_prompt.is_none(), "a successful unlock closes it");
    assert_eq!(s.editor.text, BODY);
    assert!(s.is_note_unlocked("Secret.md"));
    assert!(
        s.vault.search(MARKER).contains(&id),
        "the unlock indexes the body while the session lasts"
    );

    let rt = runtime(s);
    assert!(rt.find("locked-note-page").is_none());
    assert!(rt.find("editor").is_some(), "the editor is back");
    assert!(rt.state.editor.text.contains(MARKER));
}

#[test]
fn a_wrong_password_keeps_the_note_locked_and_the_prompt_open() {
    let env = TestEnv::new(&[("Secret.md", envelope(BODY).as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");

    s.begin_unlock_note(id);
    fill_prompt(&mut s, "not the password", None);
    s.submit_lock_prompt();

    let prompt = s.lock_prompt.as_ref().expect("the prompt stays open");
    assert_eq!(prompt.error, Some(Key::UnlockWrongPassword));
    assert!(prompt.password.text.is_empty(), "the field is cleared");
    assert!(!s.is_note_unlocked("Secret.md"));
    assert!(s.vault.notes[id].is_locked());
    assert!(s.editor.text.is_empty(), "the buffer stays empty");
    assert!(s.vault.search(MARKER).is_empty());
}

#[test]
fn locking_from_the_prompt_writes_only_the_envelope() {
    let env = TestEnv::new(&[("Plain.md", BODY)]);
    let mut s = env.state();
    let id = s.vault.resolve("Plain").expect("Plain");
    assert!(!s.vault.notes[id].is_locked());

    s.begin_lock_note(id);
    assert_eq!(
        s.lock_prompt.as_ref().map(|prompt| prompt.kind),
        Some(LockPromptKind::Lock)
    );
    fill_prompt(&mut s, PW, Some(PW));
    s.submit_lock_prompt();

    assert!(s.lock_prompt.is_none(), "a successful lock closes it");
    let disk = env.read("Plain.md");
    assert!(
        disk.starts_with(&format!("{ENVELOPE_HEADER}\n")),
        "the file is an envelope: {disk:?}"
    );
    assert!(!disk.contains(MARKER), "the body is not on disk");
    assert_eq!(
        muninn_core::unlock(&disk, PW, "Plain").expect("the envelope opens"),
        BODY,
        "and it opens with the password that was typed"
    );

    assert!(s.editor.text.is_empty(), "the plaintext buffer is dropped");
    assert!(!s.dirty());
    assert!(s.vault.notes[id].is_locked());
    assert!(
        s.vault.search(MARKER).is_empty(),
        "the body leaves the index with the lock"
    );
    assert!(
        !s.recovery_path("Plain.md").exists(),
        "a locked note never gets a plaintext recovery copy"
    );
    assert!(!s.kanban_recovery_path("Plain.md").exists());

    let rt = runtime(s);
    assert!(rt.find("locked-note-page").is_some());
    assert!(rt.find("editor").is_none());
}

#[test]
fn a_password_mismatch_writes_nothing() {
    let env = TestEnv::new(&[("Plain.md", BODY)]);
    let mut s = env.state();
    let id = s.vault.resolve("Plain").expect("Plain");

    s.begin_lock_note(id);
    fill_prompt(&mut s, PW, Some("something else"));
    s.submit_lock_prompt();

    let prompt = s.lock_prompt.as_ref().expect("the prompt stays open");
    assert_eq!(prompt.error, Some(Key::LockPasswordMismatch));
    assert!(
        prompt.confirm.text.is_empty(),
        "the confirmation is cleared"
    );
    assert_eq!(
        env.read("Plain.md"),
        BODY,
        "a mismatch writes nothing at all"
    );
    assert!(!s.vault.notes[id].is_locked());
}

#[test]
fn a_prompt_with_no_password_asks_for_one() {
    let env = TestEnv::new(&[("Plain.md", BODY)]);
    let mut s = env.state();
    let id = s.vault.resolve("Plain").expect("Plain");

    s.begin_lock_note(id);
    s.submit_lock_prompt();

    let prompt = s.lock_prompt.as_ref().expect("the prompt stays open");
    assert_eq!(prompt.error, Some(Key::LockPasswordEmpty));
    assert_eq!(env.read("Plain.md"), BODY);
}

#[test]
fn the_palette_offers_lock_or_unlock_for_the_selected_note() {
    let secret = envelope(BODY);
    let env = TestEnv::new(&[("Plain.md", BODY), ("Secret.md", secret.as_str())]);
    let mut s = env.state();
    let plain = s.vault.resolve("Plain").expect("Plain");
    let locked = s.vault.resolve("Secret").expect("Secret");

    s.load_note(plain, false);
    s.open_palette();
    assert!(
        s.palette_items()
            .contains(&PaletteItem::Action(PaletteAction::LockNote)),
        "a plain note offers Lock note"
    );
    assert!(
        !s.palette_items()
            .contains(&PaletteItem::Action(PaletteAction::UnlockNote)),
        "and not Unlock note"
    );
    let index = find_action(&s, PaletteAction::LockNote);
    let mut tx = Tx::default();
    s.palette_run_index(index, &mut tx);
    assert_eq!(
        s.lock_prompt.as_ref().map(|prompt| prompt.kind),
        Some(LockPromptKind::Lock),
        "the action opens the lock prompt"
    );
    s.cancel_lock_prompt();

    s.load_note(locked, false);
    assert!(s.editor.text.is_empty(), "a locked note's buffer is empty");
    assert!(
        !s.can_convert_to_kanban(),
        "Convert to Kanban is refused for a locked note, even with an empty buffer"
    );
    s.open_palette();
    assert!(
        s.palette_items()
            .contains(&PaletteItem::Action(PaletteAction::UnlockNote)),
        "a locked note offers Unlock note"
    );
    assert!(
        !s.palette_items()
            .contains(&PaletteItem::Action(PaletteAction::ConvertToKanban)),
        "Convert to Kanban is not offered over a locked note"
    );
    assert!(
        !s.palette_items()
            .contains(&PaletteItem::Action(PaletteAction::LockNote)),
        "and not Lock note"
    );
    let index = find_action(&s, PaletteAction::UnlockNote);
    s.palette_run_index(index, &mut tx);
    assert_eq!(
        s.lock_prompt.as_ref().map(|prompt| prompt.kind),
        Some(LockPromptKind::Unlock),
        "the action opens the unlock prompt"
    );

    // A locked note unlocked in this window offers Lock note again: the
    // session only lasts until it ends.
    s.cancel_lock_prompt();
    s.unlock_note(locked, PW).expect("unlock");
    s.load_note(locked, false);
    assert!(s.vault.notes[locked].is_unlocked());
    assert!(
        !s.can_convert_to_kanban(),
        "a locked note is never converted to Kanban"
    );
    s.open_palette();
    assert!(
        s.palette_items()
            .contains(&PaletteItem::Action(PaletteAction::LockNote)),
        "an unlocked locked note can be locked again"
    );
    assert!(
        !s.palette_items()
            .contains(&PaletteItem::Action(PaletteAction::UnlockNote)),
        "it does not ask for the password it already has"
    );
    let index = find_action(&s, PaletteAction::LockNote);
    s.palette_run_index(index, &mut tx);
    assert_eq!(
        s.lock_prompt.as_ref().map(|prompt| prompt.kind),
        Some(LockPromptKind::Lock),
        "re-locking asks for the new password"
    );

    // With no note selected neither action is offered.
    s.cancel_lock_prompt();
    s.selected = None;
    s.open_palette();
    assert!(
        !s.palette_items().iter().any(|item| matches!(
            item,
            PaletteItem::Action(PaletteAction::LockNote | PaletteAction::UnlockNote)
        )),
        "the actions are note-specific"
    );
}

#[test]
fn the_locked_note_menu_offers_unlock_and_drops_rename_and_move() {
    let secret = envelope(BODY);
    let env = TestEnv::new(&[("Plain.md", BODY), ("Secret.md", secret.as_str())]);
    let mut s = env.state();
    let plain = s.vault.resolve("Plain").expect("Plain");
    let locked = s.vault.resolve("Secret").expect("Secret");

    s.open_note_menu(locked, 8.0, 40.0);
    let mut rt = runtime(s);
    assert!(rt.find("menu-unlock").is_some(), "the menu offers unlock");
    assert!(rt.find("menu-lock").is_none());
    assert!(
        rt.find("menu-rename").is_none(),
        "renaming a locked note would change its id"
    );
    assert!(rt.find("menu-move").is_none());
    assert!(
        rt.find("menu-open_external").is_none(),
        "the envelope is never handed to the OS"
    );
    assert!(rt.find("menu-reveal").is_some());
    assert!(rt.find("menu-delete").is_some());

    rt.update(|s, _| {
        s.menu = None;
        s.open_note_menu(plain, 8.0, 40.0);
    });
    frame(&mut rt);
    assert!(rt.find("menu-lock").is_some(), "a plain note offers lock");
    assert!(rt.find("menu-unlock").is_none());
    assert!(rt.find("menu-rename").is_some());
    assert!(rt.find("menu-move").is_some());
    assert!(rt.find("menu-open_external").is_some());

    // A locked note unlocked in this window offers Lock note (re-lock) and
    // still hides the envelope-unsafe actions.
    rt.update(move |s, _| {
        s.menu = None;
        s.unlock_note(locked, PW).expect("unlock");
        s.load_note(locked, false);
        s.open_note_menu(locked, 8.0, 40.0);
    });
    frame(&mut rt);
    assert!(
        rt.find("menu-lock").is_some(),
        "an unlocked locked note offers re-lock"
    );
    assert!(rt.find("menu-unlock").is_none());
    assert!(rt.find("menu-rename").is_none());
    assert!(rt.find("menu-move").is_none());
    assert!(rt.find("menu-open_external").is_none());
    assert!(rt.find("menu-reveal").is_some());
    assert!(rt.find("menu-delete").is_some());
}

#[test]
fn command_chords_do_not_open_layers_behind_the_prompt() {
    let env = TestEnv::new(&[("Plain.md", BODY)]);
    let mut s = env.state();
    let id = s.vault.resolve("Plain").expect("Plain");
    s.begin_lock_note(id);
    let mut rt = runtime(s);

    for key in [
        KeyEvent {
            key: rune::Key::Command('?'),
            mods: Mods {
                cmd: true,
                shift: true,
                ..Mods::default()
            },
        },
        KeyEvent {
            key: rune::Key::Command('p'),
            mods: Mods {
                cmd: true,
                shift: true,
                ..Mods::default()
            },
        },
    ] {
        rt.key(key);
        frame(&mut rt);
        assert!(
            !rt.state.help,
            "Help does not open behind the password card"
        );
        assert!(
            !rt.state.palette_open(),
            "the palette does not open behind it"
        );
        assert!(
            rt.state.lock_prompt.is_some(),
            "the prompt keeps the keyboard"
        );
    }
}

const KANBAN: &str = "\
---
kanban-plugin: board
---

## To do

- [ ] ship it
";

#[test]
fn a_locked_kanban_never_opens_on_the_page_or_writes_plaintext() {
    let secret = lock(KANBAN, PW, "Board").expect("lock");
    let env = TestEnv::new(&[("Board.md", secret.as_str()), ("Other.md", "# Other\n")]);
    let mut s = env.state();
    let id = s.vault.resolve("Board").expect("Board");
    let other = s.vault.resolve("Other").expect("Other");
    assert!(s.vault.notes[id].is_locked());
    assert!(
        !s.vault.notes[id].is_kanban(),
        "the envelope itself is not a board"
    );

    s.set_mode(Mode::Kanban);
    s.open_kanban(id);
    assert_eq!(s.mode, Mode::Kanban, "the test drives the page mode");
    assert!(
        s.kanban.is_none(),
        "the page has no board: the envelope is never parsed"
    );

    let mut rt = runtime(s);
    assert!(
        rt.find("locked-note-page").is_some(),
        "the lock page outranks the Kanban page"
    );
    assert!(rt.find("kanban-page").is_none());
    assert_eq!(
        env.read("Board.md"),
        secret,
        "the file stays the envelope byte for byte"
    );

    // Even after this session unlocks the note, the page must not parse it:
    // its file is still the envelope and the page's save writes plaintext.
    rt.update(move |s, _| {
        s.unlock_note(id, PW).expect("unlock");
        s.load_note(other, false);
        s.load_note(id, false);
        s.set_mode(Mode::Kanban);
        s.open_kanban(id);
    });
    frame(&mut rt);
    assert!(rt.state.vault.notes[id].is_unlocked());
    assert!(
        rt.state.kanban.is_none(),
        "an unlocked locked note is still not a kanban"
    );
    assert_eq!(
        env.read("Board.md"),
        secret,
        "nothing wrote the plaintext board"
    );
}

#[test]
fn escape_closes_the_prompt() {
    let env = TestEnv::new(&[("Secret.md", envelope(BODY).as_str())]);
    let mut s = env.state();
    let id = s.vault.resolve("Secret").expect("Secret");
    s.begin_unlock_note(id);
    let mut rt = runtime(s);
    assert_eq!(rt.state.topmost_layer(), Some(Layer::NoteLock));

    rt.key(KeyEvent {
        key: rune::Key::Escape,
        mods: Mods::default(),
    });
    frame(&mut rt);
    assert!(rt.state.lock_prompt.is_none(), "Escape cancels the prompt");
    assert_eq!(rt.state.topmost_layer(), None);
}

#[test]
fn the_prompt_fields_are_secure_and_named() {
    let env = TestEnv::new(&[("Plain.md", BODY)]);
    let mut s = env.state();
    let id = s.vault.resolve("Plain").expect("Plain");
    s.begin_lock_note(id);

    let prompt = s.lock_prompt.as_ref().expect("the prompt is open");
    assert!(prompt.password.is_secure(), "the password is masked");
    assert!(prompt.confirm.is_secure(), "the confirmation is masked");
    assert!(prompt.password.focused, "the password takes the keyboard");
    assert!(!s.editor.focused && !s.search.focused);

    let mut rt = runtime(s);
    let element = |key: &str| {
        rt.elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
            .unwrap_or_else(|| panic!("missing element {key}"))
    };
    assert_eq!(
        element("lock-prompt").label.as_deref(),
        Some(t(Key::LockNoteTitle)),
        "the card is a named dialog"
    );
    assert_eq!(element("lock-prompt").role, "dialog");
    assert_eq!(
        element("lock-password").label.as_deref(),
        Some(t(Key::LockPassword))
    );
    assert_eq!(
        element("lock-confirm").label.as_deref(),
        Some(t(Key::LockConfirmPassword))
    );
    assert!(
        element("lock-warning").text.is_some(),
        "the lost-password warning is shown in lock mode"
    );

    // Unlock mode has no confirmation to retype, and no warning.
    rt.update(|s, _| {
        s.cancel_lock_prompt();
        s.begin_unlock_note(id);
    });
    frame(&mut rt);
    assert!(rt.find("lock-confirm").is_none());
    assert!(rt.find("lock-warning").is_none());
    assert!(rt.find("lock-password").is_some());
}

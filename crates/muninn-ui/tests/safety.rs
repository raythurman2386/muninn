//! SAF-04 trash/undo, SAF-07 startup errors and EDT-06 Escape focus tests.
//!
//! These drive the real Rune runtime against temporary vaults, like `ui.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, State, on_key, poll, shortcut, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime, Tx};

fn runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state")),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn runtime_at(root: PathBuf) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::with_state_home(root.clone(), root.with_extension("state")),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn escape() -> KeyEvent {
    KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    }
}

fn f2() -> KeyEvent {
    KeyEvent {
        key: Key::Function(2),
        mods: Mods::default(),
    }
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} present"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

/// Let row/banner mount and exit animations finish before asserting on the tree.
fn settle(rt: &mut Runtime<State>) {
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
}

fn trashed_files(root: &Path, name: &str) -> Vec<PathBuf> {
    let trash = root.join(".muninn-trash");
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(&trash) else {
        return found;
    };
    for slot in entries.flatten() {
        let mut stack = vec![slot.path()];
        while let Some(dir) = stack.pop() {
            let Ok(items) = fs::read_dir(&dir) else {
                continue;
            };
            for item in items.flatten() {
                let path = item.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.file_name().is_some_and(|n| n == name) {
                    found.push(path);
                }
            }
        }
    }
    found
}

#[test]
fn delete_moves_to_trash_and_undo_restores_content_and_links() {
    let tmp = TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
        ("Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
    ]);
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.update(|s, _| s.load_note(alpha, false));
    rt.frame(1.0 / 60.0);

    rt.update(|s, _| {
        s.delete_note(alpha);
        s.confirm_delete();
    });
    rt.frame(1.0 / 60.0);
    settle(&mut rt);

    assert!(!tmp.path().join("Alpha.md").exists(), "moved, not left");
    let trashed = trashed_files(tmp.path(), "Alpha.md");
    assert_eq!(trashed.len(), 1, "exactly one trashed copy");
    assert!(
        trashed[0].starts_with(tmp.path().join(".muninn-trash")),
        "{}",
        trashed[0].display()
    );
    assert!(
        fs::read_to_string(&trashed[0])
            .expect("trash body")
            .contains("See [[Beta]]"),
        "content is intact in the trash"
    );
    // A scan (and so the sidebar) hides the trashed note.
    assert_eq!(rt.state.vault.notes.len(), 1);
    assert!(rt.state.vault.resolve("Alpha").is_none());
    assert!(rt.state.trash_undo.is_some());
    assert!(rt.find("trash-undo").is_some(), "Undo is offered");

    click(&mut rt, "trash-undo");

    assert!(tmp.path().join("Alpha.md").is_file(), "undo moved it back");
    assert!(!trashed[0].exists(), "the trash copy is gone");
    assert!(rt.state.trash_undo.is_none());
    assert!(rt.state.error.is_none());
    let alpha = rt.state.vault.resolve("Alpha").expect("restored Alpha");
    let beta = rt.state.vault.resolve("Beta").expect("Beta");
    assert!(rt.state.vault.note(alpha).links.contains(&beta));
    assert!(rt.state.vault.note(beta).links.contains(&alpha));
    assert_eq!(
        rt.state.vault.note(alpha).source,
        "# Alpha\n\nSee [[Beta]].\n"
    );
}

#[test]
fn opening_another_note_expires_the_undo_toast() {
    let tmp = TempVault::new(&[("Alpha.md", "alpha"), ("Beta.md", "beta")]);
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.delete_note(alpha);
    rt.state.confirm_delete();
    assert!(rt.state.trash_undo.is_some());

    let beta = rt.state.vault.resolve("Beta").expect("Beta");
    rt.state.load_note(beta, true);
    assert!(
        rt.state.trash_undo.is_none(),
        "next note action expires undo"
    );
    // The file stays recoverable in the trash even after the toast expires.
    assert_eq!(trashed_files(tmp.path(), "Alpha.md").len(), 1);
}

#[test]
fn unused_file_as_vault_shows_the_error_page_not_an_empty_vault() {
    let file = std::env::temp_dir().join(format!("muninn-file-vault-{}.md", std::process::id()));
    drop(fs::remove_file(&file));
    fs::write(&file, "# this is a file, not a folder\n").expect("fixture file");

    let mut rt = runtime_at(file.clone());
    assert!(rt.state.startup_error.is_some());
    assert!(rt.state.vault.notes.is_empty());
    assert!(rt.find("vault-error").is_some(), "error page is rendered");
    assert!(rt.find("vault-retry").is_some(), "Retry is offered");
    assert!(
        rt.find("no-notes-hint").is_none(),
        "must not claim the vault is empty"
    );

    // Still a file: Retry reports again instead of silently clearing.
    rt.update(|s, _| {
        assert!(!s.retry_startup());
    });
    rt.frame(1.0 / 60.0);
    settle(&mut rt);
    assert!(rt.find("vault-error").is_some());

    // Fix the vault, then Retry recovers and opens the first note.
    fs::remove_file(&file).expect("unblock");
    fs::create_dir_all(&file).expect("vault dir");
    fs::write(file.join("Now.md"), "# Now\n").expect("note");
    rt.update(|s, _| {
        assert!(s.retry_startup());
    });
    rt.frame(1.0 / 60.0);
    settle(&mut rt);
    assert!(rt.state.startup_error.is_none());
    assert!(rt.find("vault-error").is_none());
    assert_eq!(rt.state.vault.notes.len(), 1);
    assert!(rt.state.selected.is_some());

    drop(rt);
    drop(fs::remove_dir_all(&file));
}

#[test]
fn skipped_files_are_reported_without_failing_the_vault() {
    let tmp = TempVault::new(&[("Good.md", "# Good\n")]);
    fs::write(tmp.path().join("Broken.md"), [0xff, 0xfe, 0x00]).expect("invalid utf8");

    let rt = runtime(&tmp);
    assert_eq!(rt.state.vault.skipped_count(), 1);
    assert!(rt.find("skipped-count").is_some(), "the count is visible");
}

#[test]
fn escape_in_the_editor_blurs_and_reenables_f2_and_delete() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Beta.md", "# Beta\n")]);

    // The focused-field handler itself must clear the app flag synchronously:
    // F2/Delete read it before the shell's blur reaches the next frame. This
    // mirrors the editor's `on_input_key`: shortcut first, then blur.
    let mut s =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    let alpha = s.vault.resolve("Alpha").expect("Alpha");
    s.load_note(alpha, true);
    s.focus_editor();
    let mut tx = Tx::default();
    let handled = shortcut(&mut s, &escape(), &mut tx);
    assert!(!handled, "the editor's Escape is not an app shortcut");
    s.blur_text();
    assert!(!s.editor.focused, "Escape clears editor focus in the event");
    assert!(!s.search.focused);
    assert!(on_key(&mut s, &f2(), &mut tx), "F2 is no longer gated");
    assert_eq!(s.rename_prompt.as_ref().map(|p| p.id), Some(alpha));

    // The same path through the real runtime: click the field, press Escape,
    // then F2 and Delete work on the next key event.
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.update(|s, _| s.load_note(alpha, true));
    rt.frame(1.0 / 60.0);
    click(&mut rt, "editor");
    assert!(rt.state.editor.focused, "editor takes focus");

    rt.key(escape());
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.editor.focused, "Escape clears editor focus");
    assert!(!rt.state.search.focused, "Escape clears search focus too");

    rt.key(f2());
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.rename_prompt.as_ref().map(|p| p.id),
        Some(alpha),
        "F2 works after Escape"
    );
    rt.state.cancel_rename_note();
    rt.key(KeyEvent {
        key: Key::Delete,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.delete_confirm,
        Some(muninn_ui::DeleteConfirm::Note(alpha)),
        "Delete works after Escape"
    );
}

#[test]
fn switching_modes_does_not_leave_stale_focus_flags() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut s =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    s.focus_editor();
    s.set_mode(Mode::Graph);
    assert!(
        !s.editor.focused,
        "the unmounted editor cannot stay focused"
    );
    s.set_mode(Mode::Preview);
    s.focus_search();
    assert!(!s.editor.focused, "focus_search clears the editor flag");
    s.set_mode(Mode::Editor);
    assert!(s.editor.focused && !s.search.focused);
}

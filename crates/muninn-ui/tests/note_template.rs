//! Default note template: a new note starts with the configured Markdown, or
//! with the original heading-only body when no template is set.
//!
//! These drive the real Rune runtime headlessly, like `ui.rs`.

use std::fs;
use std::path::PathBuf;

use muninn_core::testing::TempVault;
use muninn_ui::{Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn runtime(tmp: &TempVault, note_template: &str) -> Runtime<State> {
    let settings = Settings {
        note_template: note_template.to_owned(),
        ..Settings::default()
    };
    let mut rt = Runtime::new(
        State::with_settings(
            tmp.path().to_path_buf(),
            tmp.path().with_extension("state"),
            tmp.path().with_extension("config"),
            settings,
        ),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn command(c: char) -> KeyEvent {
    KeyEvent {
        key: Key::Command(c),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    }
}

fn create_note(rt: &mut Runtime<State>, name: &str) -> PathBuf {
    rt.key(command('n'));
    rt.frame(1.0 / 60.0);
    rt.state.new_note_field.replace_text(name);
    rt.state.submit_new_note();
    rt.frame(1.0 / 60.0);
    rt.state.selected_path.clone().expect("new note is open")
}

#[test]
fn a_new_note_starts_with_the_configured_template() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n\nExisting.\n")]);
    let mut rt = runtime(&tmp, "---\ntags: []\n---");

    let path = create_note(&mut rt, "Templated");
    let body = fs::read_to_string(&path).expect("new note body");

    assert!(
        body.starts_with("---\ntags: []\n---\n"),
        "the template is prepended as-is, with one newline before the heading: {body:?}"
    );
    assert_eq!(body, "---\ntags: []\n---\n# Templated\n\n");
    assert!(
        rt.state
            .vault
            .notes
            .iter()
            .any(|note| note.rel == "Templated.md"),
        "the templated note is still indexed as a normal note"
    );
    assert!(
        rt.state.editor.text.starts_with("---\ntags: []\n---\n"),
        "the editor opens the same templated body: {:?}",
        rt.state.editor.text
    );
}

#[test]
fn an_empty_template_keeps_the_original_note_body() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n\nExisting.\n")]);
    let mut rt = runtime(&tmp, "");

    let path = create_note(&mut rt, "Plain");
    let body = fs::read_to_string(&path).expect("new note body");

    assert_eq!(body, "# Plain\n\n", "empty means byte-identical behaviour");
    assert_eq!(rt.state.editor.text, body);
}

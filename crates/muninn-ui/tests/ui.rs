//! Headless interaction tests for the Muninn shell.
//!
//! These drive the real Rune runtime against a temporary vault: no window and no snapshots.

use std::fs;

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Scope, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime, Tx};

fn temp_vault() -> TempVault {
    TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]] and [[Gamma]].\n"),
        ("Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
        ("Gamma.md", "# Gamma\n\nA quiet note.\n"),
    ])
}

fn runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::new(tmp.path().to_path_buf()),
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

#[test]
fn note_rows_open_notes_in_the_editor() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let beta = rt.state.vault.resolve("Beta").expect("Beta");
    let (x, y) = rt.find(&format!("note-row-{beta}")).expect("Beta row");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.selected, Some(beta));
    assert!(rt.state.editor.text.contains("Back to"));
    assert_eq!(rt.state.mode, Mode::Editor);
}

#[test]
fn editing_autosaves_after_an_idle() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    let path = rt.state.vault.notes[alpha].path.clone();
    rt.state.focus_editor();
    rt.state.editor.insert("A new sentence.\n");
    rt.frame(0.1);
    rt.frame(0.8);
    let saved = fs::read_to_string(path).expect("saved note");
    assert!(saved.contains("A new sentence."), "{saved}");
    assert!(!rt.state.dirty());
}

#[test]
fn command_n_creates_and_opens_a_note() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    rt.key(command('n'));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.vault.notes.len(), 4);
    assert!(
        rt.state
            .selected_path
            .as_ref()
            .is_some_and(|p| p.ends_with("Untitled.md"))
    );
    assert_eq!(rt.state.mode, Mode::Editor);
    assert!(rt.state.editor.text.starts_with("# Untitled"));
}

#[test]
fn graph_mode_mounts_nodes_and_local_mode_trims_them() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.load_note(alpha, false);
    rt.state.set_mode(Mode::Graph);
    rt.state.scope = Scope::Global;
    rt.state.needs_fit = false;
    rt.frame(1.0 / 60.0);
    assert!(rt.find("graph-node-0").is_some());
    assert_eq!(rt.state.visible_notes().len(), 3);

    rt.state.scope = Scope::Local;
    let local = rt.state.visible_notes();
    // Alpha links to Beta and Gamma, so the local view keeps all three.
    assert_eq!(local.len(), 3);
}

#[test]
fn clicking_a_missing_link_creates_the_note() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let mut tx = Tx::default();
    rt.state.open_link("missing:Orchard", &mut tx);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.vault.notes.iter().any(|n| n.title == "Orchard"));
    assert_eq!(rt.state.mode, Mode::Editor);
}

#[test]
fn preview_rewrites_only_real_links() {
    let tmp = temp_vault();
    let rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    let preview = rt
        .state
        .vault
        .preview(&rt.state.vault.notes[alpha].source, alpha);
    assert!(preview.contains("[Beta](<note:Beta>)"), "{preview}");
    assert!(preview.contains("[Gamma](<note:Gamma>)"), "{preview}");
    assert!(!preview.contains("[["));
}

#[test]
fn command_k_focuses_the_search_field() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    rt.state.set_mode(Mode::Preview);
    rt.frame(1.0 / 60.0);
    rt.key(command('k'));
    rt.frame(1.0 / 60.0);
    assert!(rt.state.search.focused, "search should request focus");
    rt.key(KeyEvent {
        key: Key::Text("g".into()),
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.search.text, "g");
}

#[test]
fn graph_shortcut_round_trips_to_the_last_note_mode() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    rt.state.set_mode(Mode::Preview);
    rt.key(command('g'));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.mode, Mode::Graph);
    rt.key(command('g'));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.mode, Mode::Preview);
}

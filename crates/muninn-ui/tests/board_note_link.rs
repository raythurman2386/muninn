//! Board slice 8 (#73): linking an element to a vault note, headless.
//!
//! Input goes in as pointer and key events on the canvas and the Link note
//! prompt; the tests read the in-memory board, the hint, the open note and
//! the saved file.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_core::{Board, Shape};
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, PhysicalInput, Runtime};
use serde_json::{Value, json};

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-note-link-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn vault() -> TempVault {
    TempVault::new(&[
        ("Alpha.md", "# Alpha\n"),
        ("projects/Beta.md", "# Beta\n"),
        ("Draw.excalidraw", DRAW),
    ])
}

fn launch(root: &Path) -> Runtime<State> {
    let (config, state_home) = (unique_dir("config"), unique_dir("state"));
    let state = State::with_settings(
        root.to_path_buf(),
        state_home,
        config.clone(),
        Settings::load_from(&config),
    );
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn key(rt: &mut Runtime<State>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    frames(rt, 1);
}

fn plain(rt: &mut Runtime<State>, k: Key) {
    key(rt, k, Mods::default());
}

fn text(rt: &mut Runtime<State>, t: &str) {
    for c in t.chars() {
        plain(rt, Key::Text(c.to_string()));
    }
    frames(rt, 1);
}

fn command(rt: &mut Runtime<State>, c: char) {
    key(
        rt,
        Key::Command(c),
        Mods {
            cmd: true,
            ..Mods::default()
        },
    );
}

fn hold_command(rt: &mut Runtime<State>, on: bool) {
    rt.physical_input(PhysicalInput::Modifiers(Mods {
        cmd: on,
        ..Mods::default()
    }));
}

fn open(rt: &mut Runtime<State>) {
    command(rt, '4');
    frames(rt, 2);
    let i = rt
        .state
        .boards
        .iter()
        .position(|b| b.file.title == "Draw")
        .expect("Draw");
    let (x, y) = rt.find(&format!("board-row-{i}")).expect("row");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 60);
}

/// Window point of scene point `p`.
fn at(rt: &Runtime<State>, p: [f32; 2]) -> (f32, f32) {
    let r = rt.presented("board-canvas").expect("canvas").0;
    let s = rt.state.board_view().expect("open").screen(p);
    (r[0] + s[0], r[1] + s[1])
}

fn click(rt: &mut Runtime<State>, p: [f32; 2]) {
    let (x, y) = at(rt, p);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    // Past the double-click interval: since #73 a second click on the same
    // shape opens its label editor, which is not what a test's next step wants.
    frames(rt, 40);
}

fn model(rt: &Runtime<State>) -> &Board {
    rt.state
        .board_open
        .as_ref()
        .and_then(|o| o.model.as_ref())
        .expect("model")
}

fn shape(rt: &Runtime<State>, id: &str) -> Shape {
    model(rt)
        .shape(id)
        .cloned()
        .unwrap_or_else(|| panic!("no {id}"))
}

fn selection(rt: &Runtime<State>) -> Vec<String> {
    rt.state.board_selection().to_vec()
}

fn text_of(rt: &Runtime<State>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_else(|| {
            panic!(
                "missing text {key}; keys: {:?}",
                rt.elements()
                    .iter()
                    .filter_map(|e| e.key.clone())
                    .collect::<Vec<_>>()
            )
        })
}

fn linked(rt: &Runtime<State>, id: &str) -> Option<String> {
    shape(rt, id).note_link()
}

fn saved_link(tmp: &TempVault, id: &str) -> Value {
    let text = fs::read_to_string(tmp.path().join("Draw.excalidraw")).expect("saved board");
    let board: Value = serde_json::from_str(&text).expect("json");
    board["elements"]
        .as_array()
        .expect("elements")
        .iter()
        .find(|e| e["id"] == id)
        .expect("element")["link"]
        .clone()
}

const RECT: [f32; 2] = [30.0, 20.0];

/// Select the rectangle, then type a note name into the prompt opened by ⌘L.
fn link_rect(rt: &mut Runtime<State>, typed: &str) {
    click(rt, RECT);
    command(rt, 'l');
    assert!(rt.find("board-note-link-prompt").is_some(), "⌘L opens it");
    text(rt, typed);
    plain(rt, Key::Enter);
    frames(rt, 2);
}

#[test]
fn typing_a_note_name_links_the_selection_and_persists() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    click(&mut rt, RECT);
    assert_eq!(selection(&rt), vec!["keep-rect".to_owned()]);

    command(&mut rt, 'l');
    assert!(rt.find("board-note-link-prompt").is_some());
    assert!(rt.state.is_typing(), "the prompt has the keyboard");
    text(&mut rt, "Beta");
    plain(&mut rt, Key::Enter);
    frames(&mut rt, 2);

    assert!(
        rt.find("board-note-link-prompt").is_none(),
        "Enter closes the prompt"
    );
    assert_eq!(
        linked(&rt, "keep-rect").as_deref(),
        Some("projects/Beta"),
        "the note resolves to its vault-relative path"
    );
    assert_eq!(
        shape(&rt, "keep-rect").extra["link"],
        json!("muninn://note/projects/Beta")
    );
    assert_eq!(
        shape(&rt, "keep-text").extra["link"],
        Value::Null,
        "only the selection is linked"
    );
    assert_eq!(
        rt.state.board_open.as_ref().unwrap().status,
        muninn_ui::BoardStatus::Unsaved,
        "linking is an edit"
    );
    let hint = text_of(&rt, "board-link-status");
    assert!(hint.contains("Beta"), "the hint names the note: {hint}");

    command(&mut rt, 's');
    assert_eq!(
        saved_link(&tmp, "keep-rect"),
        json!("muninn://note/projects/Beta")
    );
    // Saved and read back, the link is still there.
    let reloaded = Board::load(tmp.path().join("Draw.excalidraw")).expect("reload");
    assert_eq!(
        reloaded
            .shape("keep-rect")
            .and_then(Shape::note_link)
            .as_deref(),
        Some("projects/Beta")
    );
}

#[test]
fn a_vault_relative_path_links_too_and_a_clear_is_one_undo() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    // A path-qualified name (with the `.md` the file has) works as well.
    link_rect(&mut rt, "projects/Beta.md");
    assert_eq!(linked(&rt, "keep-rect").as_deref(), Some("projects/Beta"));

    // ⌘L prefills the current target and selects it; Backspace clears it.
    click(&mut rt, RECT);
    command(&mut rt, 'l');
    assert_eq!(rt.state.board_note_link_field.text, "projects/Beta");
    plain(&mut rt, Key::Backspace);
    plain(&mut rt, Key::Enter);
    frames(&mut rt, 2);
    assert_eq!(shape(&rt, "keep-rect").extra["link"], Value::Null);
    assert!(linked(&rt, "keep-rect").is_none());

    // One undo puts the link back (setting and clearing are one edit each).
    command(&mut rt, 'z');
    assert_eq!(linked(&rt, "keep-rect").as_deref(), Some("projects/Beta"));
    command(&mut rt, 'z');
    assert_eq!(shape(&rt, "keep-rect").extra["link"], Value::Null);
}

#[test]
fn cmd_enter_and_cmd_click_open_the_linked_note_but_a_plain_click_only_selects() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    link_rect(&mut rt, "Beta");
    assert_eq!(rt.state.mode, Mode::Board);

    // A plain click still only selects.
    let before = rt.state.selected;
    click(&mut rt, RECT);
    assert_eq!(rt.state.mode, Mode::Board);
    assert_eq!(selection(&rt), vec!["keep-rect".to_owned()]);
    assert_eq!(rt.state.selected, before, "no note opened");

    // ⌘Enter opens the selected element's note.
    key(
        &mut rt,
        Key::Enter,
        Mods {
            cmd: true,
            ..Mods::default()
        },
    );
    frames(&mut rt, 3);
    assert_eq!(rt.state.mode, Mode::Editor);
    let opened = rt.state.selected.expect("a note is open");
    assert_eq!(rt.state.vault.note(opened).title, "Beta");

    // Back on Board, ⌘click on the element opens it without selecting first.
    open(&mut rt);
    // The board object is kept across modes, so drop the earlier selection.
    rt.update(|s, _| s.set_board_selection(Vec::new()));
    frames(&mut rt, 2);
    assert!(selection(&rt).is_empty());
    hold_command(&mut rt, true);
    click(&mut rt, RECT);
    hold_command(&mut rt, false);
    frames(&mut rt, 3);
    assert_eq!(rt.state.mode, Mode::Editor);
    let opened = rt.state.selected.expect("a note is open");
    assert_eq!(rt.state.vault.note(opened).title, "Beta");
}

#[test]
fn an_external_or_missing_link_is_not_opened() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    // Put a non-note URL on the rectangle through the model (as a file from
    // Excalidraw could have): a command-click must leave it alone.
    rt.update(|s, _| {
        let model = s
            .board_open
            .as_mut()
            .and_then(|open| open.model.as_mut())
            .expect("model");
        for element in &mut model.elements {
            if let muninn_core::Element::Shape(shape) = element
                && shape.id == "keep-rect"
            {
                shape
                    .extra
                    .insert("link".into(), json!("https://example.com/keep"));
            }
        }
    });
    frames(&mut rt, 2);
    hold_command(&mut rt, true);
    click(&mut rt, RECT);
    hold_command(&mut rt, false);
    frames(&mut rt, 2);
    assert_eq!(rt.state.mode, Mode::Board, "an external URL is not opened");
    assert_eq!(
        shape(&rt, "keep-rect").extra["link"],
        json!("https://example.com/keep"),
        "and it is left verbatim"
    );
}

#[test]
fn an_unknown_note_keeps_the_prompt_open_with_an_error() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    click(&mut rt, RECT);
    command(&mut rt, 'l');
    text(&mut rt, "Nope");
    plain(&mut rt, Key::Enter);
    frames(&mut rt, 2);

    assert!(
        rt.find("board-note-link-prompt").is_some(),
        "the prompt stays open"
    );
    assert!(rt.state.is_typing());
    let error = text_of(&rt, "board-note-link-error");
    assert!(error.contains("Nope"), "{error}");
    assert_eq!(linked(&rt, "keep-rect"), None, "no link was written");

    // Fix the name in the still-open field: Backspace over "Nope" and retry.
    for _ in 0..4 {
        plain(&mut rt, Key::Backspace);
    }
    text(&mut rt, "Beta");
    plain(&mut rt, Key::Enter);
    frames(&mut rt, 2);
    assert!(rt.find("board-note-link-prompt").is_none());
    assert_eq!(linked(&rt, "keep-rect").as_deref(), Some("projects/Beta"));
}

#[test]
fn the_toolbar_button_opens_the_prompt_and_links_every_selected_element() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    click(&mut rt, RECT);
    let (x, y) = rt
        .find("board-tool-link-note")
        .expect("the Link note button");
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    frames(&mut rt, 2);
    assert!(rt.find("board-note-link-prompt").is_some());
    text(&mut rt, "Alpha");
    plain(&mut rt, Key::Enter);
    frames(&mut rt, 2);
    assert_eq!(linked(&rt, "keep-rect").as_deref(), Some("Alpha"));
}

#[test]
fn linking_two_selected_elements_is_one_history_step() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    command(&mut rt, 'a');
    assert_eq!(selection(&rt).len(), 2, "the two pickable elements");
    command(&mut rt, 'l');
    text(&mut rt, "Beta");
    plain(&mut rt, Key::Enter);
    frames(&mut rt, 2);
    assert_eq!(linked(&rt, "keep-rect").as_deref(), Some("projects/Beta"));
    assert_eq!(linked(&rt, "keep-text").as_deref(), Some("projects/Beta"));
    assert_eq!(
        rt.state
            .board_open
            .as_ref()
            .unwrap()
            .edit
            .history
            .undo_len(),
        1,
        "one edit for the whole selection"
    );
    command(&mut rt, 'z');
    assert_eq!(linked(&rt, "keep-rect"), None);
    assert_eq!(linked(&rt, "keep-text"), None);
}

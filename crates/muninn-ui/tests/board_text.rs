//! Board slice 6: placing and editing text, headless.
//!
//! Input goes in as pointer and key events on the canvas and the in-canvas
//! editor; the tests read the in-memory board and the saved file.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_core::{Board, Shape};
use muninn_ui::{BoardStatus, BoardTool, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};
use serde_json::Value;

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-text-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn vault() -> TempVault {
    TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", DRAW)])
}

fn launch(root: &Path) -> Runtime<State> {
    launch_in(root, &unique_dir("state"))
}

fn launch_in(root: &Path, state_home: &Path) -> Runtime<State> {
    let (config, state_home) = (unique_dir("config"), state_home.to_path_buf());
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
    plain(rt, Key::Text(t.into()));
}

fn command(rt: &mut Runtime<State>, c: char, shift: bool) {
    key(
        rt,
        Key::Command(c),
        Mods {
            cmd: true,
            shift,
            ..Mods::default()
        },
    );
}

fn open(rt: &mut Runtime<State>) {
    rt.key(KeyEvent {
        key: Key::Command('4'),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
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
    frames(rt, 2);
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

fn status(rt: &Runtime<State>) -> BoardStatus {
    rt.state.board_open.as_ref().expect("open").status.clone()
}

const TEXT: [f32; 2] = [30.0, 460.0];
const EMPTY: [f32; 2] = [250.0, 100.0];

fn double_click(rt: &mut Runtime<State>, p: [f32; 2]) {
    let (x, y) = at(rt, p);
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.frame(1.0 / 60.0);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
    }
    frames(rt, 2);
}

fn editing(rt: &Runtime<State>) -> bool {
    rt.state.board_text.is_some()
}

fn field(rt: &Runtime<State>) -> String {
    rt.state.board_text_field.text.clone()
}

fn type_str(rt: &mut Runtime<State>, s: &str) {
    for c in s.chars() {
        if c == '\n' {
            plain(rt, Key::Enter);
        } else {
            text(rt, &c.to_string());
        }
    }
}

fn saved(tmp: &TempVault) -> Value {
    serde_json::from_str(&fs::read_to_string(tmp.path().join("Draw.excalidraw")).unwrap()).unwrap()
}

fn element<'a>(file: &'a Value, id: &str) -> &'a Value {
    file["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id}"))
}

#[test]
fn the_text_tool_places_typed_text_with_newlines_and_saves_excalidraw_fields() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let n = model(&rt).elements.len();
    for k in ["t", "8", "T"] {
        text(&mut rt, "v");
        text(&mut rt, k);
        assert_eq!(rt.state.board_tool, Some(BoardTool::Text), "{k}");
    }
    click(&mut rt, EMPTY);
    assert!(editing(&rt), "a click opens the editor");
    assert_eq!(rt.state.board_tool, None, "the tool goes back to selecting");
    assert!(
        rt.find("board-text-editor").is_some(),
        "the editor is drawn"
    );
    // Tool letters, digits and Delete are text while the editor has focus.
    type_str(&mut rt, "r1 ünï\ncode");
    plain(&mut rt, Key::Backspace);
    text(&mut rt, "e");
    assert_eq!(field(&rt), "r1 ünï\ncode");
    assert_eq!(rt.state.board_tool, None, "typing r or 1 picked no tool");
    assert_eq!(model(&rt).elements.len(), n, "nothing added before commit");
    plain(&mut rt, Key::Escape);
    assert!(!editing(&rt), "Esc commits");
    assert!(
        rt.state.mode == muninn_ui::Mode::Board,
        "that Esc did not leave Board"
    );
    assert_eq!(model(&rt).elements.len(), n + 1);
    assert_eq!(status(&rt), BoardStatus::Unsaved);
    let made = model(&rt).elements.last().unwrap().id().unwrap().to_owned();
    command(&mut rt, 's', false);
    assert_eq!(status(&rt), BoardStatus::Saved);
    let file = saved(&tmp);
    let e = element(&file, &made);
    assert_eq!(e["type"], "text");
    assert_eq!(e["text"], "r1 ünï\ncode");
    assert_eq!(e["originalText"], "r1 ünï\ncode");
    assert_eq!(e["fontSize"], 20.0);
    assert_eq!(e["fontFamily"], 5);
    assert_eq!(e["lineHeight"], 1.25);
    assert_eq!(e["containerId"], Value::Null);
    assert_eq!(e["autoResize"], true);
    assert!((e["x"].as_f64().unwrap() - 250.0).abs() < 1.0, "{e}");
    assert!((e["y"].as_f64().unwrap() - 100.0).abs() < 1.0, "{e}");
    let (w, h) = (e["width"].as_f64().unwrap(), e["height"].as_f64().unwrap());
    assert!(w > 40.0 && w < 200.0, "width {w}");
    assert!((h - 50.0).abs() < 0.01, "two lines of 20 × 1.25: {h}");
    let fixture = Board::from_json(DRAW).unwrap();
    for id in ["keep-rect", "keep-ellipse", "keep-text", "gone"] {
        assert_eq!(model(&rt).shape(id), fixture.shape(id), "{id}");
    }
}

#[test]
fn a_click_outside_commits_and_does_nothing_else() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let n = model(&rt).elements.len();
    text(&mut rt, "t");
    click(&mut rt, EMPTY);
    type_str(&mut rt, "hi");
    click(&mut rt, TEXT);
    assert!(!editing(&rt));
    assert_eq!(
        model(&rt).elements.len(),
        n + 1,
        "committed, no second text"
    );
    assert!(
        selection(&rt).is_empty(),
        "the committing click selected nothing"
    );
    assert!(!rt.state.board_text_swallow, "the swallowed click ended");
    // The next click (not a double click) selects as usual.
    frames(&mut rt, 60);
    click(&mut rt, TEXT);
    assert_eq!(selection(&rt), vec!["keep-text".to_owned()]);
}

#[test]
fn committing_empty_text_adds_nothing() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let n = model(&rt).elements.len();
    text(&mut rt, "t");
    click(&mut rt, EMPTY);
    type_str(&mut rt, "  \n ");
    plain(&mut rt, Key::Escape);
    assert!(!editing(&rt));
    assert_eq!(model(&rt).elements.len(), n);
    assert_eq!(status(&rt), BoardStatus::Clean, "nothing to save");
    command(&mut rt, 'z', false);
    assert_eq!(
        model(&rt).elements.len(),
        n,
        "nothing went into the history"
    );
}

#[test]
fn a_double_click_edits_existing_text_and_board_undo_redoes_the_commit() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let before = shape(&rt, "keep-text");
    double_click(&mut rt, TEXT);
    assert!(editing(&rt), "double click on text edits it");
    assert_eq!(field(&rt), "keep me");
    let hidden = rt.find("board-text-0").is_none() && rt.find("board-text-1").is_none();
    assert!(hidden || rt.find("board-text-editor").is_some());
    type_str(&mut rt, "!\nnow");
    // ⌘Z inside the editor is the field's own undo, never Board undo.
    command(&mut rt, 'z', false);
    assert!(editing(&rt));
    assert_eq!(shape(&rt, "keep-text"), before, "board undo did not fire");
    type_str(&mut rt, "w");
    let typed = field(&rt);
    assert!(typed.starts_with("keep me"), "{typed:?}");
    plain(&mut rt, Key::Escape);
    let edited = shape(&rt, "keep-text");
    assert_eq!(edited.text.as_ref().unwrap().text, typed);
    assert_eq!(edited.extra["originalText"], Value::String(typed.clone()));
    assert!(edited.version > before.version);
    assert_ne!(edited.version_nonce, before.version_nonce);
    assert!((edited.x - before.x).abs() < 1e-9 && (edited.y - before.y).abs() < 1e-9);
    command(&mut rt, 'z', false);
    let undone = shape(&rt, "keep-text");
    assert_eq!(undone.text, before.text, "undo restores the text");
    assert!(undone.version > edited.version);
    command(&mut rt, 'z', true);
    let redone = shape(&rt, "keep-text");
    assert_eq!(redone.text, edited.text);
    assert!(redone.version > undone.version);
}

#[test]
fn emptying_existing_text_deletes_it_and_undo_brings_it_back() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    double_click(&mut rt, TEXT);
    assert!(editing(&rt));
    for _ in 0.."keep me".len() {
        plain(&mut rt, Key::Backspace);
    }
    assert_eq!(field(&rt), "");
    plain(&mut rt, Key::Escape);
    assert!(
        shape(&rt, "keep-text").is_deleted,
        "empty text is a tombstone"
    );
    command(&mut rt, 'z', false);
    assert!(!shape(&rt, "keep-text").is_deleted);
}

#[test]
fn leaving_board_or_saving_commits_the_open_editor() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let n = model(&rt).elements.len();
    text(&mut rt, "t");
    click(&mut rt, EMPTY);
    type_str(&mut rt, "saved");
    // ⌘S is a Board chord: while typing it is the editor's, so commit
    // happens on the way out instead.
    rt.state.set_mode(muninn_ui::Mode::Editor);
    frames(&mut rt, 2);
    assert!(!editing(&rt));
    assert_eq!(model(&rt).elements.len(), n + 1);
    assert!(rt.state.save_board());
    let file = saved(&tmp);
    assert!(
        file["elements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["text"] == "saved")
    );
}

#[test]
fn focus_moving_elsewhere_commits_so_the_editor_never_strands_board_keys() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let n = model(&rt).elements.len();
    text(&mut rt, "t");
    click(&mut rt, EMPTY);
    type_str(&mut rt, "kept");
    // Something else takes the keyboard (the sidebar search, a dialog).
    rt.state.board_text_field.focused = false;
    rt.state.search.focused = true;
    frames(&mut rt, 2);
    assert!(!editing(&rt), "losing focus commits");
    assert_eq!(model(&rt).elements.len(), n + 1);
    rt.state.search.focused = false;
    frames(&mut rt, 1);
    text(&mut rt, "r");
    assert_eq!(
        rt.state.board_tool,
        Some(BoardTool::Rectangle),
        "Board keys work again"
    );
}

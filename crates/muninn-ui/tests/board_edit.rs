//! Board slice 5: select, move, delete, undo and redo, headless.
//!
//! Input goes in as pointer and key events on the canvas; the tests read the
//! selection, the in-memory board and the saved file.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_core::{Board, Shape};
use muninn_ui::{BoardStatus, Mode, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, PhysicalInput, Runtime};
use serde_json::Value;

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-edit-{tag}-{}-{}",
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

fn shifted() -> Mods {
    Mods {
        shift: true,
        ..Mods::default()
    }
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

fn hold_shift(rt: &mut Runtime<State>, on: bool) {
    rt.physical_input(PhysicalInput::Modifiers(Mods {
        shift: on,
        ..Mods::default()
    }));
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

/// Drag from scene point `a` to scene point `b` in small steps.
fn drag(rt: &mut Runtime<State>, a: [f32; 2], b: [f32; 2]) {
    let (x0, y0) = at(rt, a);
    let (x1, y1) = at(rt, b);
    rt.pointer_down(x0, y0);
    rt.frame(1.0 / 60.0);
    for step in 1..=8 {
        let f = step as f32 / 8.0;
        rt.pointer_move(x0 + (x1 - x0) * f, y0 + (y1 - y0) * f);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x1, y1);
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

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

const RECT: [f32; 2] = [30.0, 20.0];
const TEXT: [f32; 2] = [30.0, 460.0];
const LOCKED: [f32; 2] = [467.0, 465.0];
const EMPTY: [f32; 2] = [250.0, 100.0];

#[test]
fn a_click_selects_shift_click_adds_or_removes_and_esc_clears_first() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    click(&mut rt, RECT);
    assert_eq!(selection(&rt), ids(&["keep-rect"]));
    assert!(rt.find("board-selection").is_some(), "the outline is drawn");
    hold_shift(&mut rt, true);
    click(&mut rt, TEXT);
    assert_eq!(selection(&rt), ids(&["keep-rect", "keep-text"]));
    click(&mut rt, TEXT);
    assert_eq!(
        selection(&rt),
        ids(&["keep-rect"]),
        "Shift+click again takes it out"
    );
    hold_shift(&mut rt, false);
    // Locked and raw (frame) elements are never picked; a plain click there clears.
    click(&mut rt, LOCKED);
    assert!(selection(&rt).is_empty());
    click(&mut rt, [467.0, 20.0]);
    assert!(selection(&rt).is_empty());
    click(&mut rt, TEXT);
    assert_eq!(selection(&rt), ids(&["keep-text"]));
    click(&mut rt, EMPTY);
    assert!(selection(&rt).is_empty());
    assert!(rt.find("board-selection").is_none());

    // Esc: the selection, then the tool, then Board itself.
    click(&mut rt, RECT);
    plain(&mut rt, Key::Escape);
    assert!(selection(&rt).is_empty());
    assert_eq!(rt.state.mode, Mode::Board);
    text(&mut rt, "r");
    plain(&mut rt, Key::Escape);
    assert_eq!(rt.state.board_tool, None);
    assert_eq!(rt.state.mode, Mode::Board);
    plain(&mut rt, Key::Escape);
    assert_ne!(rt.state.mode, Mode::Board);
    assert_eq!(status(&rt), BoardStatus::Clean, "selecting is not an edit");
}

#[test]
fn a_marquee_picks_whole_elements_and_shift_adds() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let view = rt.state.board_view().unwrap();
    // Marquees start at their bottom right: the toolbar sits over the
    // canvas's top left corner.
    drag(&mut rt, [80.0, 60.0], [-20.0, -20.0]);
    assert_eq!(selection(&rt), ids(&["keep-rect"]));
    assert_eq!(
        rt.state.board_view().unwrap(),
        view,
        "a marquee does not pan"
    );
    // Half of the text is not enough.
    drag(&mut rt, [40.0, 490.0], [-20.0, 440.0]);
    assert!(selection(&rt).is_empty());
    hold_shift(&mut rt, true);
    drag(&mut rt, [80.0, 60.0], [-20.0, -20.0]);
    drag(&mut rt, [90.0, 490.0], [-20.0, 440.0]);
    hold_shift(&mut rt, false);
    assert_eq!(selection(&rt), ids(&["keep-rect", "keep-text"]));
    // Over everything: still never the locked ellipse, the frame or the tombstone.
    drag(&mut rt, [520.0, 510.0], [-30.0, -30.0]);
    assert_eq!(selection(&rt), ids(&["keep-rect", "keep-text"]));
    command(&mut rt, 'a', false);
    assert_eq!(selection(&rt), ids(&["keep-rect", "keep-text"]));
    assert_eq!(status(&rt), BoardStatus::Clean);
}

#[test]
fn a_drag_moves_the_selection_and_bumps_each_element_once() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let before = shape(&rt, "keep-rect");
    let text_before = shape(&rt, "keep-text");
    // Pressing on an unselected element selects it and drags it.
    drag(&mut rt, RECT, [RECT[0] + 50.0, RECT[1] + 30.0]);
    let after = shape(&rt, "keep-rect");
    assert!(
        (after.x - 50.0).abs() < 0.6 && (after.y - 30.0).abs() < 0.6,
        "{after:?}"
    );
    assert_eq!(after.version, before.version + 1);
    assert_ne!(after.version_nonce, before.version_nonce);
    let updated = after.extra["updated"].as_i64().unwrap();
    assert!(updated > 1_700_000_000_000, "{updated}");
    assert_eq!(shape(&rt, "keep-text"), text_before, "the rest stays");
    assert_eq!(selection(&rt), ids(&["keep-rect"]));
    assert_eq!(status(&rt), BoardStatus::Unsaved);
    // A click on the selection (no travel) changes nothing.
    click(&mut rt, [RECT[0] + 50.0, RECT[1] + 30.0]);
    assert_eq!(shape(&rt, "keep-rect"), after);
}

#[test]
fn arrows_nudge_the_selection_and_pan_without_one() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let view = rt.state.board_view().unwrap();
    click(&mut rt, RECT);
    let v = shape(&rt, "keep-rect").version;
    plain(&mut rt, Key::Right);
    key(&mut rt, Key::Down, shifted());
    let s = shape(&rt, "keep-rect");
    assert_eq!((s.x, s.y), (1.0, 5.0));
    assert_eq!(s.version, v + 2, "each press is an edit");
    assert_eq!(
        rt.state.board_view().unwrap(),
        view,
        "no pan while selected"
    );
    plain(&mut rt, Key::Escape);
    plain(&mut rt, Key::Right);
    assert_ne!(rt.state.board_view().unwrap(), view, "arrows pan again");
}

#[test]
fn delete_and_backspace_leave_tombstones_and_never_touch_the_note() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let notes = rt.state.vault.notes.len();
    plain(&mut rt, Key::Delete);
    assert_eq!(
        status(&rt),
        BoardStatus::Clean,
        "nothing selected, nothing deleted"
    );
    click(&mut rt, RECT);
    let v = shape(&rt, "keep-rect").version;
    plain(&mut rt, Key::Delete);
    let s = shape(&rt, "keep-rect");
    assert!(s.is_deleted);
    assert_eq!(s.version, v + 1);
    assert!(selection(&rt).is_empty());
    click(&mut rt, RECT);
    assert!(selection(&rt).is_empty(), "a tombstone is not picked");
    click(&mut rt, TEXT);
    plain(&mut rt, Key::Backspace);
    assert!(shape(&rt, "keep-text").is_deleted);
    assert_eq!(rt.state.vault.notes.len(), notes);
    assert!(tmp.path().join("Alpha.md").exists());
    assert_eq!(rt.state.mode, Mode::Board);
}

#[test]
fn undo_and_redo_cover_drawing_moves_and_deletes_with_growing_versions() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let n = model(&rt).elements.len();
    text(&mut rt, "r");
    drag(&mut rt, [150.0, 150.0], [250.0, 220.0]);
    assert_eq!(model(&rt).elements.len(), n + 1);
    let made = model(&rt).elements.last().unwrap().id().unwrap().to_owned();
    text(&mut rt, "v");
    drag(&mut rt, [150.0, 180.0], [170.0, 200.0]);
    let moved = shape(&rt, &made);
    assert!((moved.x - 170.0).abs() < 0.6, "{moved:?}");
    plain(&mut rt, Key::Delete);
    assert!(shape(&rt, &made).is_deleted);

    let mut last = shape(&rt, &made).version;
    let mut step = |rt: &mut Runtime<State>, undo: bool| {
        if undo {
            command(rt, 'z', false);
        } else {
            command(rt, 'z', true);
        }
        let s = shape(rt, &made);
        assert!(s.version > last, "version {} after {last}", s.version);
        last = s.version;
        s
    };
    let s = step(&mut rt, true);
    assert!(!s.is_deleted && (s.x - 170.0).abs() < 0.6, "delete undone");
    assert_eq!(selection(&rt), vec![made.clone()]);
    let s = step(&mut rt, true);
    assert!((s.x - 150.0).abs() < 0.6, "move undone");
    let s = step(&mut rt, true);
    assert!(s.is_deleted, "drawing undone leaves a tombstone");
    assert_eq!(model(&rt).elements.len(), n + 1);
    let s = step(&mut rt, false);
    assert!(!s.is_deleted && (s.x - 150.0).abs() < 0.6, "redrawn");
    let s = step(&mut rt, false);
    assert!((s.x - 170.0).abs() < 0.6, "moved again");
    // Ctrl+Y is redo too.
    command(&mut rt, 'y', false);
    assert!(shape(&rt, &made).is_deleted);
    // Nothing more to redo; ⌘Z with an empty history is a no-op.
    let v = shape(&rt, &made).version;
    command(&mut rt, 'y', false);
    assert_eq!(shape(&rt, &made).version, v);
    // Old elements were never touched.
    let fixture = Board::from_json(DRAW).unwrap();
    for id in ["keep-rect", "keep-ellipse", "keep-text", "gone"] {
        assert_eq!(model(&rt).shape(id), fixture.shape(id), "{id}");
    }
}

#[test]
fn undo_after_a_save_makes_the_board_dirty_and_the_save_keeps_its_guards() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let mut rt = launch(tmp.path());
    open(&mut rt);
    drag(&mut rt, RECT, [RECT[0] + 40.0, RECT[1]]);
    command(&mut rt, 's', false);
    assert_eq!(status(&rt), BoardStatus::Saved);
    let saved: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let rect = |v: &Value| {
        v["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "keep-rect")
            .cloned()
            .unwrap()
    };
    assert!((rect(&saved)["x"].as_f64().unwrap() - 40.0).abs() < 0.6);
    assert_eq!(rect(&saved)["version"], 13);

    command(&mut rt, 'z', false);
    assert_eq!(
        status(&rt),
        BoardStatus::Unsaved,
        "undo after save is unsaved again"
    );
    command(&mut rt, 's', false);
    assert_eq!(status(&rt), BoardStatus::Saved);
    let again: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(rect(&again)["x"], 0, "back where it was");
    assert_eq!(
        rect(&again)["version"],
        14,
        "and a newer version than the save"
    );
    assert_eq!(Board::load(&path).unwrap().elements.len(), 5);

    // Another app edits the file: an undo is still an edit, and its save is refused.
    command(&mut rt, 'z', true);
    let theirs = fs::read_to_string(&path)
        .unwrap()
        .replace("keep me", "kept elsewhere");
    fs::write(&path, &theirs).unwrap();
    command(&mut rt, 's', false);
    assert!(matches!(status(&rt), BoardStatus::Failed(ref e) if e.contains("changed on disk")));
    assert_eq!(fs::read_to_string(&path).unwrap(), theirs);
}

#[test]
fn picking_a_tool_or_the_hand_clears_the_selection_and_the_hand_pans() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    click(&mut rt, RECT);
    text(&mut rt, "o");
    assert!(selection(&rt).is_empty());
    text(&mut rt, "v");
    click(&mut rt, RECT);
    text(&mut rt, "h");
    assert!(rt.state.board_hand && selection(&rt).is_empty());
    let view = rt.state.board_view().unwrap();
    let before = shape(&rt, "keep-rect");
    drag(&mut rt, RECT, [RECT[0] + 50.0, RECT[1] + 30.0]);
    assert_ne!(rt.state.board_view().unwrap(), view, "the hand pans");
    assert_eq!(shape(&rt, "keep-rect"), before, "and moves nothing");
    plain(&mut rt, Key::Escape);
    assert!(!rt.state.board_hand);
    assert_eq!(rt.state.mode, Mode::Board);
}

#[test]
fn typing_in_the_sidebar_search_never_deletes_selects_all_or_undoes_on_board() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    click(&mut rt, RECT);
    plain(&mut rt, Key::Right);
    let nudged = shape(&rt, "keep-rect");
    let (x, y) = rt.find("search-box").expect("search");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(&mut rt, 2);
    assert!(rt.state.search.focused);
    text(&mut rt, "ab");
    plain(&mut rt, Key::Backspace);
    plain(&mut rt, Key::Delete);
    command(&mut rt, 'a', false);
    command(&mut rt, 'z', false);
    plain(&mut rt, Key::Left);
    let now = shape(&rt, "keep-rect");
    assert!(
        !now.is_deleted,
        "Backspace/Delete edited the search, not the board"
    );
    assert_eq!(
        (now.x, now.version),
        (nudged.x, nudged.version),
        "no undo, no nudge"
    );
    assert_eq!(
        selection(&rt),
        ids(&["keep-rect"]),
        "Cmd+A selected the search text"
    );
}

#[test]
fn deletes_undos_and_moves_keep_the_recovery_copy_current_but_not_per_drag_frame() {
    let tmp = vault();
    let state_home = unique_dir("state");
    let mut rt = launch_in(tmp.path(), &state_home);
    open(&mut rt);
    // Keyed by vault since the #38 review (M1): ask the app for the path.
    let copy = rt.state.recovery_path("Draw.excalidraw");
    assert!(copy.starts_with(&state_home));
    let read = |p: &Path| Board::from_json(&std::fs::read_to_string(p).expect("copy")).unwrap();
    click(&mut rt, RECT);
    plain(&mut rt, Key::Delete);
    // Slice 7: the copy is written once the drawing rests (0.5 s).
    frames(&mut rt, 40);
    assert!(read(&copy).shape("keep-rect").unwrap().is_deleted);
    command(&mut rt, 'z', false);
    frames(&mut rt, 40);
    assert!(
        !read(&copy).shape("keep-rect").unwrap().is_deleted,
        "the undo is in the copy"
    );
    // Mid-drag the copy waits; it lands with the move.
    let before = read(&copy).shape("keep-rect").unwrap().x;
    let (x0, y0) = at(&rt, RECT);
    rt.pointer_down(x0, y0);
    rt.frame(1.0 / 60.0);
    for step in 1..=8 {
        rt.pointer_move(x0 + 10.0 * step as f32, y0);
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(
        read(&copy).shape("keep-rect").unwrap().x,
        before,
        "no write per drag frame"
    );
    rt.pointer_up(x0 + 80.0, y0);
    frames(&mut rt, 40);
    assert_eq!(
        read(&copy).shape("keep-rect").unwrap().x,
        shape(&rt, "keep-rect").x
    );
    assert_ne!(shape(&rt, "keep-rect").x, before);
}

#[test]
fn the_watcher_seeing_muninns_own_save_neither_refits_nor_drops_the_history() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let view = rt.state.board_view();
    click(&mut rt, TEXT);
    plain(&mut rt, Key::Delete);
    command(&mut rt, 's', false);
    assert_eq!(status(&rt), BoardStatus::Saved);
    // The watcher's vault reload (or re-entering Board) re-lists the boards.
    frames(&mut rt, 150);
    rt.state.refresh_boards();
    frames(&mut rt, 2);
    assert_eq!(rt.state.board_view(), view, "the view stays where it was");
    command(&mut rt, 'z', false);
    assert!(
        !shape(&rt, "keep-text").is_deleted,
        "the undo after the save still works"
    );
    assert_eq!(rt.state.board_view(), view);
}

#[test]
fn after_an_edit_a_resized_canvas_keeps_the_view_instead_of_refitting() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    click(&mut rt, TEXT);
    plain(&mut rt, Key::Delete);
    let view = rt.state.board_view();
    rt.set_viewport((1100.0, 780.0), 2.0);
    frames(&mut rt, 3);
    assert_eq!(
        rt.state.board_view(),
        view,
        "a delete must not leave the view refitting"
    );
    command(&mut rt, 'z', false);
    rt.set_viewport((1180.0, 780.0), 2.0);
    frames(&mut rt, 3);
    assert_eq!(rt.state.board_view(), view, "nor an undo");
}

#[test]
fn a_press_on_a_resize_handle_does_nothing_until_resizing_exists() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    click(&mut rt, RECT);
    assert_eq!(selection(&rt), ids(&["keep-rect"]));
    let before = shape(&rt, "keep-rect");
    // keep-rect is 0,0 → 60,40; its top-left handle sits OUTLINE_GAP (4)
    // outside that corner, and the bottom-right one likewise.
    for corner in [[0.0, 0.0], [60.0, 40.0]] {
        let (x, y) = at(&rt, corner);
        let off = if corner[0] == 0.0 { -4.0 } else { 4.0 };
        let (x0, y0) = (x + off, y + off);
        rt.pointer_down(x0, y0);
        rt.frame(1.0 / 60.0);
        for step in 1..=8 {
            rt.pointer_move(x0 + 7.5 * step as f32, y0 + 2.0 * step as f32);
            rt.frame(1.0 / 60.0);
        }
        rt.pointer_up(x0 + 60.0, y0 + 16.0);
        frames(&mut rt, 2);
        assert_eq!(
            shape(&rt, "keep-rect"),
            before,
            "grabbing the {corner:?} handle moved it"
        );
    }
    assert_eq!(status(&rt), BoardStatus::Clean, "no edit was recorded");
    assert_eq!(selection(&rt), ids(&["keep-rect"]), "the selection stays");
    command(&mut rt, 'z', false);
    assert_eq!(shape(&rt, "keep-rect"), before, "nothing to undo");
}

const LABEL: &str = include_str!("../../muninn-core/tests/fixtures/board/label.excalidraw");

#[test]
fn a_shapes_bound_label_moves_deletes_and_undoes_with_it_and_never_orphans() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", LABEL)]);
    let mut rt = launch(tmp.path());
    open(&mut rt);
    // A click on the label selects its container.
    click(&mut rt, [100.0, 50.0]);
    assert_eq!(selection(&rt), ids(&["box"]));
    let (b0, l0) = (shape(&rt, "box"), shape(&rt, "label"));
    drag(&mut rt, [100.0, 50.0], [140.0, 70.0]);
    let (b1, l1) = (shape(&rt, "box"), shape(&rt, "label"));
    let (dx, dy) = (b1.x - b0.x, b1.y - b0.y);
    assert!(dx > 30.0, "the box moved: {dx}");
    assert!(
        (l1.x - l0.x - dx).abs() < 1e-9 && (l1.y - l0.y - dy).abs() < 1e-9,
        "the label moved with it"
    );
    assert!(l1.version > l0.version);
    plain(&mut rt, Key::Delete);
    assert!(shape(&rt, "box").is_deleted && shape(&rt, "label").is_deleted);
    command(&mut rt, 'z', false);
    assert!(
        !shape(&rt, "box").is_deleted && !shape(&rt, "label").is_deleted,
        "undo brings both"
    );
    command(&mut rt, 'z', false);
    assert_eq!(
        (shape(&rt, "label").x, shape(&rt, "box").x),
        (l0.x, b0.x),
        "undo moves both back"
    );
    command(&mut rt, 'z', true);
    command(&mut rt, 'z', true);
    command(&mut rt, 's', false);
    let saved: Value =
        serde_json::from_str(&fs::read_to_string(tmp.path().join("Draw.excalidraw")).unwrap())
            .unwrap();
    let els = saved["elements"].as_array().unwrap();
    let alive = |id: &str| els.iter().any(|e| e["id"] == id && e["isDeleted"] == false);
    for e in els {
        if let Some(c) = e["containerId"].as_str() {
            assert!(
                e["isDeleted"] == true || alive(c),
                "orphaned label {}",
                e["id"]
            );
        }
    }
    assert!(!alive("box") && !alive("label"));
}

/// Draw with tool `t` from `a` to `b` and return the new element's id.
fn draw_with(rt: &mut Runtime<State>, t: &str, a: [f32; 2], b: [f32; 2]) -> String {
    text(rt, t);
    drag(rt, a, b);
    text(rt, "v");
    model(rt).elements.last().unwrap().id().unwrap().to_owned()
}

#[test]
fn a_selected_thin_line_arrow_or_small_shape_still_drags_from_itself() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let line = draw_with(&mut rt, "l", [150.0, 150.0], [250.0, 150.0]);
    let arrow = draw_with(&mut rt, "a", [150.0, 250.0], [250.0, 250.0]);
    let small = draw_with(&mut rt, "r", [320.0, 150.0], [326.0, 156.0]);
    for (id, grab) in [
        (&line, [200.0, 150.0]),
        (&arrow, [200.0, 250.0]),
        (&small, [323.0, 153.0]),
    ] {
        click(&mut rt, grab);
        assert_eq!(selection(&rt), vec![id.clone()], "{id} selected");
        let x0 = shape(&rt, id).x;
        drag(&mut rt, grab, [grab[0] + 30.0, grab[1] + 20.0]);
        let x1 = shape(&rt, id).x;
        assert!(
            (x1 - x0 - 30.0).abs() < 0.6,
            "{id}: dragging it moved it by {}",
            x1 - x0
        );
    }
}

#[test]
fn shift_clicking_a_label_deselects_its_shape_and_dragging_by_it_keeps_the_selection() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", LABEL)]);
    let mut rt = launch(tmp.path());
    open(&mut rt);
    let on_label = [100.0, 50.0];
    click(&mut rt, on_label);
    assert_eq!(selection(&rt), ids(&["box"]));
    hold_shift(&mut rt, true);
    click(&mut rt, on_label);
    hold_shift(&mut rt, false);
    assert!(
        selection(&rt).is_empty(),
        "Shift+click on the label takes its shape out"
    );
    click(&mut rt, on_label);
    hold_shift(&mut rt, true);
    click(&mut rt, [330.0, 0.0]);
    hold_shift(&mut rt, false);
    assert_eq!(selection(&rt), ids(&["box", "other"]));
    let (b0, o0) = (shape(&rt, "box").x, shape(&rt, "other").x);
    drag(&mut rt, on_label, [on_label[0] + 25.0, on_label[1]]);
    assert_eq!(
        selection(&rt),
        ids(&["box", "other"]),
        "dragging by the label keeps both"
    );
    assert!(
        (shape(&rt, "other").x - o0 - 25.0).abs() < 0.6,
        "the other shape moved too"
    );
    assert!((shape(&rt, "box").x - b0 - 25.0).abs() < 0.6);
}

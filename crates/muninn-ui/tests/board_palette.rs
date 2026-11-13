//! Issue #69: the Board's primary stroke and fill palette, headless.
//!
//! A swatch is picked from the palette the toolbar opens; the tests read the
//! chosen colours, the in-memory board, the saved file and the undo history.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_core::{Board, Shape};
use muninn_ui::{BoardStatus, Settings, State, on_key, poll, view};
use rune::{ElementInfo, Env, Key, KeyEvent, Mods, Runtime};
use serde_json::Value;

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-palette-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn vault() -> TempVault {
    TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", DRAW)])
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

/// Press the element with this key (a toolbar or palette control).
fn press(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
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

fn element(rt: &Runtime<State>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("missing element {key}"))
}

/// Open the palette and pick one swatch; the panel closes on the pick.
fn pick(rt: &mut Runtime<State>, swatch: &str) {
    press(rt, "board-palette-toggle");
    assert!(rt.find("board-palette").is_some(), "the panel is open");
    press(rt, swatch);
    assert!(
        rt.find("board-palette").is_none(),
        "the panel closes on a pick"
    );
}

const RECT: [f32; 2] = [30.0, 20.0];

#[test]
fn a_swatch_colours_the_next_shape_and_the_file_stores_its_hex() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    assert_eq!(rt.state.board_stroke, "#1e1e1e", "Excalidraw's default ink");
    assert_eq!(rt.state.board_fill, "transparent");

    pick(&mut rt, "board-stroke-red");
    assert_eq!(rt.state.board_stroke, "#e03131");
    pick(&mut rt, "board-fill-red");
    assert_eq!(rt.state.board_fill, "#ffc9c9");

    text(&mut rt, "r");
    drag(&mut rt, [150.0, 150.0], [250.0, 220.0]);
    text(&mut rt, "v");
    let made = model(&rt).elements.last().unwrap().id().unwrap().to_owned();
    let s = shape(&rt, &made);
    assert_eq!(s.stroke_color, "#e03131");
    assert_eq!(s.background_color, "#ffc9c9");
    assert_eq!(status(&rt), BoardStatus::Unsaved);

    // The colours are what Excalidraw reads back from the file.
    command(&mut rt, 's', false);
    assert_eq!(status(&rt), BoardStatus::Saved);
    let saved: Value =
        serde_json::from_str(&fs::read_to_string(tmp.path().join("Draw.excalidraw")).unwrap())
            .unwrap();
    let stored = saved["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == made.as_str())
        .expect("the drawn element");
    assert_eq!(stored["strokeColor"], "#e03131");
    assert_eq!(stored["backgroundColor"], "#ffc9c9");
    assert_eq!(
        Board::load(tmp.path().join("Draw.excalidraw"))
            .unwrap()
            .shape(&made)
            .map(|s| (s.stroke_color.as_str(), s.background_color.as_str())),
        Some(("#e03131", "#ffc9c9"))
    );

    // Drawing is one edit: undo takes the element back as a tombstone.
    command(&mut rt, 'z', false);
    assert!(shape(&rt, &made).is_deleted);
    command(&mut rt, 'z', true);
    assert_eq!(shape(&rt, &made).stroke_color, "#e03131");
}

#[test]
fn new_text_is_inked_in_the_chosen_stroke_and_takes_no_fill() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    pick(&mut rt, "board-stroke-blue");
    text(&mut rt, "t");
    click(&mut rt, [320.0, 320.0]);
    assert!(rt.state.board_text.is_some(), "the editor opened");
    for c in "hi".chars() {
        text(&mut rt, &c.to_string());
    }
    plain(&mut rt, Key::Escape);
    let made = model(&rt).elements.last().unwrap().id().unwrap().to_owned();
    let s = shape(&rt, &made);
    assert_eq!(s.stroke_color, "#1971c2");
    assert_eq!(s.background_color, "transparent");
    assert_eq!(s.text.as_ref().map(|t| t.text.as_str()), Some("hi"));
}

#[test]
fn recolouring_the_selection_is_one_undo_step_per_pick() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    click(&mut rt, RECT);
    assert_eq!(selection(&rt), vec!["keep-rect".to_owned()]);
    let before = shape(&rt, "keep-rect");
    let other = shape(&rt, "keep-ellipse");
    assert_eq!(
        (
            before.stroke_color.as_str(),
            before.background_color.as_str()
        ),
        ("#e03131", "#ffc9c9"),
        "the fixture's own colours"
    );

    // Stroke only: the fill stays as it was.
    pick(&mut rt, "board-stroke-blue");
    let after = shape(&rt, "keep-rect");
    assert_eq!(after.stroke_color, "#1971c2");
    assert_eq!(after.background_color, "#ffc9c9");
    assert_eq!(after.version, before.version + 1, "one bump");
    assert_eq!(
        selection(&rt),
        vec!["keep-rect".to_owned()],
        "picking a colour keeps the selection"
    );
    assert_eq!(status(&rt), BoardStatus::Unsaved);
    assert_eq!(shape(&rt, "keep-ellipse"), other, "the rest is untouched");

    // Fill only, including no fill at all.
    pick(&mut rt, "board-fill-transparent");
    assert_eq!(shape(&rt, "keep-rect").background_color, "transparent");
    assert_eq!(shape(&rt, "keep-rect").stroke_color, "#1971c2");
    assert_eq!(shape(&rt, "keep-rect").version, before.version + 2);

    // Each pick is its own step: undo the fill, then the stroke.
    command(&mut rt, 'z', false);
    assert_eq!(shape(&rt, "keep-rect").background_color, "#ffc9c9");
    assert_eq!(shape(&rt, "keep-rect").stroke_color, "#1971c2");
    command(&mut rt, 'z', false);
    assert_eq!(shape(&rt, "keep-rect").stroke_color, "#e03131");
    assert_eq!(shape(&rt, "keep-rect").background_color, "#ffc9c9");
    command(&mut rt, 'z', true);
    assert_eq!(shape(&rt, "keep-rect").stroke_color, "#1971c2");
}

#[test]
fn the_palette_buttons_are_named_focus_ringed_tab_stops_with_a_24_point_target() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    assert!(rt.find("board-palette").is_none(), "closed by default");
    press(&mut rt, "board-palette-toggle");

    let toggle = element(&rt, "board-palette-toggle");
    assert_eq!(toggle.role, "button");
    assert_eq!(toggle.label.as_deref(), Some("Close colour palette"));
    assert!(toggle.focusable);
    assert!(
        toggle.rect[2] >= 24.0 && toggle.rect[3] >= 24.0,
        "{:?}",
        toggle.rect
    );

    let panel = element(&rt, "board-palette");
    assert_eq!(panel.role, "group");
    assert_eq!(panel.label.as_deref(), Some("Board colours"));

    for (key, label) in [
        ("board-stroke-black", "Stroke black"),
        ("board-stroke-purple", "Stroke purple"),
        ("board-fill-transparent", "Fill transparent"),
        ("board-fill-blue", "Fill blue"),
    ] {
        let swatch = element(&rt, key);
        assert_eq!(swatch.role, "button", "{key}");
        assert_eq!(swatch.label.as_deref(), Some(label), "{key}");
        assert!(swatch.focusable, "{key} is a Tab stop");
        assert!(
            swatch.rect[2] >= 24.0 && swatch.rect[3] >= 24.0,
            "{key} is {}x{}",
            swatch.rect[2],
            swatch.rect[3]
        );
    }
}

#[test]
fn escape_closes_the_palette_before_the_selection_and_a_tool_closes_it_too() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt);
    click(&mut rt, RECT);
    press(&mut rt, "board-palette-toggle");
    assert!(rt.find("board-palette").is_some());
    plain(&mut rt, Key::Escape);
    assert!(rt.find("board-palette").is_none(), "Esc closes the palette");
    assert_eq!(
        selection(&rt),
        vec!["keep-rect".to_owned()],
        "and keeps the selection"
    );
    assert_eq!(rt.state.mode, muninn_ui::Mode::Board);

    press(&mut rt, "board-palette-toggle");
    text(&mut rt, "r");
    assert!(rt.find("board-palette").is_none(), "a tool closes it");
    assert_eq!(rt.state.board_stroke, "#1e1e1e", "the pick is not changed");
}

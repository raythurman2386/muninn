//! Board slice 7: write cost while drawing, autosave and the watcher,
//! headless.
//!
//! The config and state homes are `TempVault`s, so they are removed when a
//! test ends.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use muninn_core::testing::TempVault;
use muninn_core::{Board, Shape};
use muninn_ui::{BoardStatus, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");
const RECT: [f32; 2] = [30.0, 20.0];

struct App {
    rt: Runtime<State>,
    config: TempVault,
    _state: TempVault,
}

fn vault() -> TempVault {
    TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", DRAW)])
}

fn launch(root: &Path) -> App {
    let (config, state_home) = (TempVault::new(&[]), TempVault::new(&[]));
    let state = State::with_settings(
        root.to_path_buf(),
        state_home.path().to_path_buf(),
        config.path().to_path_buf(),
        Settings::load_from(config.path()),
    );
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    App {
        rt,
        config,
        _state: state_home,
    }
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn plain(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
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

fn shape(rt: &Runtime<State>, id: &str) -> Shape {
    rt.state
        .board_open
        .as_ref()
        .and_then(|o| o.model.as_ref())
        .and_then(|m| m.shape(id).cloned())
        .unwrap_or_else(|| panic!("no {id}"))
}

fn status(rt: &Runtime<State>) -> BoardStatus {
    rt.state.board_open.as_ref().expect("open").status.clone()
}

/// Hold an arrow key for `n` repeats, one frame apart, and return every
/// distinct content `file` had along the way (missing counts as none).
fn hold(rt: &mut Runtime<State>, key: &Key, n: usize, file: &Path) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    for _ in 0..n {
        plain(rt, key.clone());
        frames(rt, 1);
        if let Ok(text) = fs::read_to_string(file) {
            seen.insert(text);
        }
    }
    seen
}

#[test]
fn holding_an_arrow_to_nudge_writes_the_recovery_copy_once_not_per_repeat() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    // Autosave off (10 s): this is about the copy of an unsaved drawing.
    app.rt.state.set_autosave_delay(10.0);
    open(&mut app.rt);
    click(&mut app.rt, RECT);
    let copy = app.rt.state.recovery_path("Draw.excalidraw");
    let x = shape(&app.rt, "keep-rect").x;
    let seen = hold(&mut app.rt, &Key::Right, 12, &copy);
    assert!(
        seen.len() <= 1,
        "the recovery copy was rewritten {} times during one held key",
        seen.len()
    );
    frames(&mut app.rt, 90);
    let saved = Board::from_json(&fs::read_to_string(&copy).expect("a copy once idle")).unwrap();
    assert_eq!(saved.shape("keep-rect").unwrap().x, x + 12.0);
    assert_eq!(shape(&app.rt, "keep-rect").x, x + 12.0);
}

#[test]
fn holding_an_arrow_to_pan_writes_settings_once_not_per_repeat() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    open(&mut app.rt);
    frames(&mut app.rt, 60);
    let file = app.config.path().join("muninn").join("settings.json");
    let before = fs::read_to_string(&file).unwrap_or_default();
    let seen = hold(&mut app.rt, &Key::Right, 12, &file);
    let changed = seen.iter().filter(|t| **t != before).count();
    assert!(
        changed <= 1,
        "settings.json was rewritten {changed} times during one held key"
    );
    frames(&mut app.rt, 90);
    let after = fs::read_to_string(&file).unwrap();
    assert_ne!(after, before, "the final view is remembered once idle");
    assert_eq!(status(&app.rt), BoardStatus::Clean);
}

fn board_file(tmp: &TempVault) -> std::path::PathBuf {
    tmp.path().join("Draw.excalidraw")
}

fn nudge(rt: &mut Runtime<State>) {
    click(rt, RECT);
    plain(rt, Key::Right);
    frames(rt, 1);
}

#[test]
fn autosave_writes_the_board_once_it_rests_and_not_mid_gesture() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    open(&mut app.rt);
    nudge(&mut app.rt);
    assert_eq!(status(&app.rt), BoardStatus::Unsaved);
    assert_eq!(
        fs::read_to_string(board_file(&tmp)).unwrap(),
        DRAW,
        "not at once"
    );
    // A move drag in flight is never saved half-way, however long it rests.
    let (x0, y0) = at(&app.rt, RECT);
    app.rt.pointer_down(x0 + 1.0, y0);
    app.rt.frame(1.0 / 60.0);
    app.rt.pointer_move(x0 + 41.0, y0);
    frames(&mut app.rt, 150);
    assert_eq!(
        fs::read_to_string(board_file(&tmp)).unwrap(),
        DRAW,
        "mid-drag"
    );
    app.rt.pointer_up(x0 + 41.0, y0);
    frames(&mut app.rt, 150);
    assert_eq!(
        status(&app.rt),
        BoardStatus::Saved,
        "autosaved once it rested"
    );
    let saved = Board::load(board_file(&tmp)).unwrap();
    assert_eq!(
        saved.shape("keep-rect").unwrap().x,
        shape(&app.rt, "keep-rect").x
    );
    assert!(
        !app.rt.state.recovery_path("Draw.excalidraw").exists(),
        "a saved board needs no recovery copy"
    );
}

#[test]
fn autosave_never_writes_over_a_file_another_app_changed() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    open(&mut app.rt);
    let theirs = DRAW.replace("keep me", "theirs");
    fs::write(board_file(&tmp), &theirs).unwrap();
    nudge(&mut app.rt);
    frames(&mut app.rt, 300);
    assert_eq!(fs::read_to_string(board_file(&tmp)).unwrap(), theirs);
    assert!(app.rt.state.board_open.as_ref().unwrap().conflict.is_some());
    assert!(app.rt.state.board_dirty(), "the drawing is kept");
}

#[test]
fn the_watcher_reloads_a_clean_open_board_changed_by_another_app() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    open(&mut app.rt);
    let view = app.rt.state.board_view();
    let theirs = DRAW.replace("keep me", "theirs");
    fs::write(board_file(&tmp), &theirs).unwrap();
    frames(&mut app.rt, 200);
    let text = shape(&app.rt, "keep-text").text.unwrap().text;
    assert_eq!(text, "theirs", "reloaded without leaving Board");
    assert_eq!(status(&app.rt), BoardStatus::Clean);
    assert_eq!(app.rt.state.board_view(), view, "the view stays");
}

#[test]
fn the_watcher_flags_a_dirty_open_board_changed_by_another_app_and_keeps_it() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    // Autosave off (10 s) so the drawing is still unsaved when theirs lands.
    app.rt.state.set_autosave_delay(10.0);
    open(&mut app.rt);
    nudge(&mut app.rt);
    let mine = shape(&app.rt, "keep-rect").x;
    let theirs = DRAW.replace("keep me", "theirs");
    fs::write(board_file(&tmp), &theirs).unwrap();
    frames(&mut app.rt, 200);
    let open = app.rt.state.board_open.as_ref().unwrap();
    assert!(open.conflict.is_some(), "flagged without leaving Board");
    assert_eq!(shape(&app.rt, "keep-rect").x, mine, "the drawing is kept");
    assert_eq!(fs::read_to_string(board_file(&tmp)).unwrap(), theirs);
    assert!(app.rt.find("board-keep-mine").is_some());
}

#[test]
fn the_board_list_follows_other_apps_while_on_board() {
    let tmp = TempVault::new(&[
        ("Alpha.md", "# Alpha\n"),
        ("Draw.excalidraw", DRAW),
        ("Other.excalidraw", DRAW),
    ]);
    let mut app = launch(tmp.path());
    open(&mut app.rt);
    let detail = |rt: &Runtime<State>, title: &str| {
        rt.state
            .boards
            .iter()
            .find(|b| b.file.title == title)
            .map(muninn_ui::BoardSummary::detail)
    };
    let before = detail(&app.rt, "Other");
    // Another app adds an element to Other, rewriting it in place: no folder
    // changes, so only a board watcher can see it.
    let mut more: serde_json::Value = serde_json::from_str(DRAW).unwrap();
    let mut extra = more["elements"][0].clone();
    extra["id"] = "added-by-them".into();
    more["elements"].as_array_mut().unwrap().push(extra);
    fs::write(
        tmp.path().join("Other.excalidraw"),
        serde_json::to_string(&more).unwrap(),
    )
    .unwrap();
    frames(&mut app.rt, 200);
    assert_ne!(
        detail(&app.rt, "Other"),
        before,
        "Other's count follows its file while Board is open"
    );
    // And a new board shows up (its folder changed).
    fs::write(tmp.path().join("New.excalidraw"), DRAW).unwrap();
    frames(&mut app.rt, 200);
    assert!(detail(&app.rt, "New").is_some(), "the new board is listed");
}

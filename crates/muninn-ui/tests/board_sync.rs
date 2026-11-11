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

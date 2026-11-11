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
    launch_with(root, TempVault::new(&[]))
}

fn launch_with(root: &Path, config: TempVault) -> App {
    let state_home = TempVault::new(&[]);
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
fn autosave_can_be_turned_off_in_settings() {
    let tmp = vault();
    let config = TempVault::new(&[("muninn/settings.json", r#"{"board_autosave": false}"#)]);
    assert!(!Settings::load_from(config.path()).board_autosave);
    assert!(Settings::default().board_autosave, "on by default");
    let mut app = launch_with(tmp.path(), config);
    open(&mut app.rt);
    nudge(&mut app.rt);
    frames(&mut app.rt, 900);
    assert_eq!(
        fs::read_to_string(board_file(&tmp)).unwrap(),
        DRAW,
        "15 s later"
    );
    assert_eq!(status(&app.rt), BoardStatus::Unsaved);
    assert!(app.rt.state.save_board(), "⌘S still saves");
    let kept = Settings::load_from(app.config.path());
    assert!(
        !kept.board_autosave,
        "the setting survives the app's own writes"
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

#[test]
fn a_resting_window_on_board_still_wakes_for_the_watcher_and_autosave() {
    // A real window that rests runs `poll` only at a wake it asked for, so
    // without one the watcher and autosave would wait for the next input.
    let tmp = vault();
    let mut app = launch(tmp.path());
    open(&mut app.rt);
    frames(&mut app.rt, 120);
    let now = app.rt.state.now;
    let wake = app
        .rt
        .wake_deadline()
        .expect("Board at rest asks for a wake");
    assert!(
        wake - now <= 1.5 + 1e-6,
        "the watcher's next look is due within its interval, not {:.2} s",
        wake - now
    );
    nudge(&mut app.rt);
    frames(&mut app.rt, 1);
    let now = app.rt.state.now;
    let wake = app
        .rt
        .wake_deadline()
        .expect("an unsaved board asks for a wake");
    assert!(
        wake - now <= 0.3,
        "the pending autosave and recovery copy wake soon, not in {:.2} s",
        wake - now
    );
}

// #41 review GK2: the watcher and an open text editor. Typed text that is
// not committed yet is unsaved work; another app's change to the board must
// raise the conflict, never reload the board under the editor.

const TEXT: [f32; 2] = [30.0, 460.0];

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

fn type_str(rt: &mut Runtime<State>, s: &str) {
    for c in s.chars() {
        plain(rt, Key::Text(c.to_string()));
    }
}

/// The fixture with keep-text changed by `f`, as another app would write it.
fn theirs(f: impl Fn(&mut serde_json::Value)) -> String {
    let mut doc: serde_json::Value = serde_json::from_str(DRAW).unwrap();
    let el = doc["elements"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| e["id"] == "keep-text")
        .unwrap();
    f(el);
    el["version"] = (el["version"].as_u64().unwrap() + 1).into();
    serde_json::to_string_pretty(&doc).unwrap()
}

fn disk_text(tmp: &TempVault) -> (String, bool) {
    let doc: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(board_file(tmp)).unwrap()).unwrap();
    let el = doc["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "keep-text")
        .unwrap()
        .clone();
    (
        el["text"].as_str().unwrap().to_owned(),
        el["isDeleted"] == true,
    )
}

fn conflict(rt: &Runtime<State>) -> bool {
    rt.state.board_open.as_ref().unwrap().conflict.is_some()
}

#[test]
fn typing_in_a_label_another_app_changed_raises_the_conflict_and_never_overwrites_theirs() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    open(&mut app.rt);
    double_click(&mut app.rt, TEXT);
    assert!(app.rt.state.board_text.is_some(), "editing keep-text");
    type_str(&mut app.rt, " LOCAL");
    let other = theirs(|el| {
        el["text"] = "keep me THEIRS".into();
        el["originalText"] = "keep me THEIRS".into();
    });
    fs::write(board_file(&tmp), &other).unwrap();
    frames(&mut app.rt, 200);
    assert!(
        conflict(&app.rt),
        "their change is flagged, not reloaded under the editor"
    );
    assert_eq!(
        app.rt.state.board_text_field.text, "keep me LOCAL",
        "the typing is kept"
    );
    plain(&mut app.rt, Key::Escape);
    frames(&mut app.rt, 300);
    assert_eq!(
        disk_text(&tmp),
        ("keep me THEIRS".to_owned(), false),
        "autosave never writes over their edit"
    );
    assert_eq!(
        shape(&app.rt, "keep-text").text.unwrap().text,
        "keep me LOCAL"
    );
    assert!(app.rt.state.board_dirty(), "mine is kept, unsaved");
    assert!(app.rt.find("board-keep-mine").is_some());
}

#[test]
fn typing_in_a_label_another_app_deleted_keeps_the_typed_text() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    open(&mut app.rt);
    double_click(&mut app.rt, TEXT);
    type_str(&mut app.rt, " LOCAL");
    fs::write(board_file(&tmp), theirs(|el| el["isDeleted"] = true.into())).unwrap();
    frames(&mut app.rt, 200);
    assert!(conflict(&app.rt), "their delete is flagged");
    assert_eq!(app.rt.state.board_text_field.text, "keep me LOCAL");
    plain(&mut app.rt, Key::Escape);
    frames(&mut app.rt, 300);
    assert_eq!(
        disk_text(&tmp),
        ("keep me".to_owned(), true),
        "theirs stays on disk"
    );
    let mine = shape(&app.rt, "keep-text");
    assert!(!mine.is_deleted);
    assert_eq!(
        mine.text.unwrap().text,
        "keep me LOCAL",
        "the typed text is kept"
    );
    let copy =
        fs::read_to_string(app.rt.state.recovery_path("Draw.excalidraw")).expect("a recovery copy");
    assert!(
        copy.contains("keep me LOCAL"),
        "and reaches the recovery copy"
    );
}

#[test]
fn an_open_editor_with_nothing_typed_waits_and_then_shows_their_change() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    open(&mut app.rt);
    double_click(&mut app.rt, TEXT);
    assert!(app.rt.state.board_text.is_some());
    let other = theirs(|el| {
        el["text"] = "keep me THEIRS".into();
        el["originalText"] = "keep me THEIRS".into();
    });
    fs::write(board_file(&tmp), &other).unwrap();
    frames(&mut app.rt, 200);
    assert!(
        app.rt.state.board_text.is_some(),
        "not reloaded under the editor"
    );
    plain(&mut app.rt, Key::Escape);
    frames(&mut app.rt, 300);
    assert_eq!(
        shape(&app.rt, "keep-text").text.unwrap().text,
        "keep me THEIRS"
    );
    assert_eq!(status(&app.rt), BoardStatus::Clean);
    assert_eq!(
        fs::read_to_string(board_file(&tmp)).unwrap(),
        other,
        "nothing written"
    );
}

// Raymond's hands-on report: the canvas chrome (the key hint bar, and the
// toolbar and zoom controls with it) slid back into place whenever the
// selection appeared or cleared.

/// The drawn rect of each piece of canvas chrome, found by what it shows so
/// the lookup does not depend on its key; and whether any is mid-animation.
fn chrome(rt: &Runtime<State>) -> (Vec<[f32; 4]>, bool) {
    let els = rt.elements();
    let hint = els
        .iter()
        .find(|e| {
            e.text
                .as_deref()
                .is_some_and(|t| t.starts_with("Click/drag select"))
        })
        .expect("the hint bar");
    let zoom = els
        .iter()
        .find(|e| e.key.as_deref() == Some("board-zoom"))
        .expect("the zoom label");
    let tool = els
        .iter()
        .find(|e| e.key.as_deref() == Some("board-toolbar"))
        .expect("the toolbar");
    let moving = hint.moving || zoom.moving || tool.moving;
    (vec![hint.rect, zoom.rect, tool.rect], moving)
}

#[test]
fn the_hint_bar_and_canvas_chrome_stay_put_when_the_selection_changes() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    open(&mut app.rt);
    frames(&mut app.rt, 60);
    let (rest, moving) = chrome(&app.rt);
    assert!(!moving, "settled before the first click");
    for (what, p) in [
        ("select", RECT),
        ("deselect", [250.0, 100.0]),
        ("select again", RECT),
    ] {
        let (x, y) = at(&app.rt, p);
        app.rt.pointer_down(x, y);
        app.rt.frame(1.0 / 60.0);
        app.rt.pointer_up(x, y);
        for i in 0..30 {
            app.rt.frame(1.0 / 60.0);
            let (now, moving) = chrome(&app.rt);
            assert!(!moving, "{what}: chrome mid-animation at frame {i}");
            assert_eq!(now, rest, "{what}: chrome moved at frame {i}");
        }
    }
    assert!(
        !app.rt.state.board_selection().is_empty(),
        "the clicks did select"
    );
}

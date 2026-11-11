//! Board slice 3: the read-only board canvas and its camera, headless.
//!
//! The camera is checked through `State::board_view` (scene → canvas
//! points) and the drawn rect of the world element, so a test sees the same
//! placement the screen does.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{BoardView, Mode, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime, WheelEvent};

const SMOKE: &str = include_str!("../../muninn-core/tests/fixtures/board/smoke.excalidraw");
const EMPTY: &str = r#"{"type":"excalidraw","version":2}"#;

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-canvas-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn vault() -> TempVault {
    TempVault::new(&[
        ("Alpha.md", "# Alpha\n"),
        ("Smoke.excalidraw", SMOKE),
        ("sketches/Empty.excalidraw", EMPTY),
        ("sketches/Broken.excalidraw", "{ not json"),
    ])
}

struct Session {
    rt: Runtime<State>,
    config: PathBuf,
    state_home: PathBuf,
}

fn launch(root: &Path, config: &Path, state_home: &Path) -> Runtime<State> {
    let state = State::with_settings(
        root.to_path_buf(),
        state_home.to_path_buf(),
        config.to_path_buf(),
        Settings::load_from(config),
    );
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn fresh(tmp: &TempVault) -> Session {
    let (config, state_home) = (unique_dir("config"), unique_dir("state"));
    let rt = launch(tmp.path(), &config, &state_home);
    Session {
        rt,
        config,
        state_home,
    }
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn plain(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
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

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 2);
}

/// Enter Board and open the board titled `title` from the list.
fn open(rt: &mut Runtime<State>, title: &str) {
    if rt.state.mode != Mode::Board {
        rt.key(command('4'));
        frames(rt, 2);
    }
    let i = rt
        .state
        .boards
        .iter()
        .position(|b| b.file.title == title)
        .unwrap_or_else(|| panic!("no board {title}"));
    click(rt, &format!("board-row-{i}"));
    // The canvas reports its size once its entry settles; the fit follows.
    frames(rt, 60);
}

fn canvas_rect(rt: &Runtime<State>) -> [f32; 4] {
    rt.presented("board-canvas").expect("board canvas").0
}

fn view_of(rt: &Runtime<State>) -> BoardView {
    rt.state.board_view().expect("an open board")
}

fn near(a: [f32; 2], b: [f32; 2], tol: f32) -> bool {
    (a[0] - b[0]).abs() <= tol && (a[1] - b[1]).abs() <= tol
}

fn bounds(rt: &Runtime<State>) -> [f32; 4] {
    let open = rt.state.board_open.as_ref().expect("open");
    open.scene.as_ref().expect("scene").bounds.expect("bounds")
}

#[test]
fn opening_a_board_fits_its_drawing_in_the_canvas() {
    let tmp = vault();
    let mut s = fresh(&tmp);
    open(&mut s.rt, "Smoke");
    let rt = &s.rt;
    let open = rt.state.board_open.as_ref().unwrap();
    let scene = open.scene.as_ref().unwrap();
    assert_eq!(
        (scene.drawn, scene.outlined),
        (7, 0),
        "every smoke element is drawn"
    );
    assert_eq!(scene.texts.len(), 1);
    assert_eq!(scene.texts[0].lines, ["Muninn board"]);

    let canvas = canvas_rect(rt);
    let view = view_of(rt);
    let b = bounds(rt);
    // The content's centre sits at the canvas's centre.
    let centre = view.screen([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]);
    assert!(
        near(centre, [canvas[2] / 2.0, canvas[3] / 2.0], 0.5),
        "{centre:?} {canvas:?}"
    );
    assert_eq!(view, BoardView::fitted(Some(b), [canvas[2], canvas[3]]));

    // The world element is drawn where the camera says (Rune scales it
    // about its centre; the camera accounts for that).
    let world = rt.presented("board-world").unwrap().0;
    let top_left = view.screen(scene.origin);
    assert!(
        near(
            [world[0], world[1]],
            [canvas[0] + top_left[0], canvas[1] + top_left[1]],
            0.6
        ),
        "{world:?}"
    );
    assert!((world[2] - scene.size[0] * view.zoom).abs() < 0.6);
    assert!(rt.find("board-text-0").is_some());
    assert_eq!(rt.find("board-placeholder"), None);
}

#[test]
fn a_drag_pans_and_the_wheel_zooms_about_the_pointer() {
    let tmp = vault();
    let mut s = fresh(&tmp);
    open(&mut s.rt, "Smoke");
    let rt = &mut s.rt;
    let canvas = canvas_rect(rt);
    let probe = [100.0, 50.0];
    let before = view_of(rt).screen(probe);

    let (x, y) = (canvas[0] + 200.0, canvas[1] + 150.0);
    rt.pointer_down(x, y);
    for step in 1..=10 {
        let f = step as f32 / 10.0;
        rt.pointer_move(x + 120.0 * f, y + 60.0 * f);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x + 120.0, y + 60.0);
    frames(rt, 2);
    let after = view_of(rt).screen(probe);
    assert!(
        near(after, [before[0] + 120.0, before[1] + 60.0], 0.01),
        "{before:?} → {after:?}"
    );
    let saved = Settings::load_from(&s.config).board.expect("saved board");
    assert_eq!(saved.rel, "Smoke.excalidraw");
    assert_eq!(saved.view, Some(view_of(rt)), "a drag end saves the view");

    // Wheel up zooms in; the scene point under the pointer stays put.
    let (px, py) = (canvas[0] + 300.0, canvas[1] + 200.0);
    let local = [px - canvas[0], py - canvas[1]];
    let under = view_of(rt).scene(local);
    let zoom = view_of(rt).zoom;
    rt.wheel_event(WheelEvent::new(0.0, -40.0).at(px, py));
    frames(rt, 1);
    let v = view_of(rt);
    assert!((v.zoom - zoom * 1.08).abs() < 1e-4, "{} {}", v.zoom, zoom);
    assert!(near(v.scene(local), under, 0.01), "pointer point moved");
    rt.wheel_event(WheelEvent::new(0.0, 40.0).at(px, py));
    frames(rt, 1);
    assert!((view_of(rt).zoom - zoom * 1.08 * 0.93).abs() < 1e-4);

    // Shift+wheel pans instead.
    let z = view_of(rt).zoom;
    let at = view_of(rt).screen(probe);
    rt.wheel_event(WheelEvent::new(0.0, 30.0).at(px, py).mods(Mods {
        shift: true,
        ..Mods::default()
    }));
    frames(rt, 1);
    assert_eq!(view_of(rt).zoom, z);
    assert!(near(view_of(rt).screen(probe), [at[0], at[1] + 30.0], 0.01));
}

#[test]
fn keys_and_buttons_zoom_pan_and_fit() {
    let tmp = vault();
    let mut s = fresh(&tmp);
    open(&mut s.rt, "Smoke");
    let rt = &mut s.rt;
    let canvas = canvas_rect(rt);
    let fitted = view_of(rt);
    let mid = [canvas[2] / 2.0, canvas[3] / 2.0];
    let at_mid = fitted.scene(mid);

    rt.key(plain(Key::Text("+".into())));
    frames(rt, 1);
    assert!((view_of(rt).zoom - fitted.zoom * 1.25).abs() < 1e-4);
    assert!(
        near(view_of(rt).scene(mid), at_mid, 0.01),
        "+ zooms about the centre"
    );
    rt.key(plain(Key::Text("-".into())));
    frames(rt, 1);
    assert!((view_of(rt).zoom - fitted.zoom).abs() < 1e-4);

    let p = view_of(rt).screen([0.0, 0.0]);
    rt.key(plain(Key::Right));
    rt.key(plain(Key::Down));
    frames(rt, 1);
    assert!(near(
        view_of(rt).screen([0.0, 0.0]),
        [p[0] - 60.0, p[1] - 60.0],
        0.01
    ));

    click(rt, "board-zoom-in");
    assert!(view_of(rt).zoom > fitted.zoom * 1.2);
    rt.key(plain(Key::Text("0".into())));
    frames(rt, 1);
    assert_eq!(view_of(rt), fitted, "0 refits");
    click(rt, "board-zoom-out");
    click(rt, "board-fit");
    assert_eq!(view_of(rt), fitted, "the fit button refits");
}

#[test]
fn leaving_board_keeps_the_view_and_rereads_the_file() {
    let tmp = vault();
    let mut s = fresh(&tmp);
    open(&mut s.rt, "Smoke");
    let rt = &mut s.rt;
    rt.key(plain(Key::Left));
    rt.key(plain(Key::Text("+".into())));
    frames(rt, 1);
    let moved = view_of(rt);

    rt.key(command('1'));
    frames(rt, 2);
    assert_eq!(rt.state.mode, Mode::Editor);
    assert!(rt.find("board-canvas").is_none());
    // Another app adds an element while Board is hidden.
    let path = tmp.path().join("Smoke.excalidraw");
    let mut scene: serde_json::Value = serde_json::from_str(SMOKE).unwrap();
    scene["elements"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "type": "rectangle", "id": "added", "x": 900, "y": 400, "width": 60, "height": 40
        }));
    fs::write(&path, scene.to_string()).unwrap();

    rt.key(command('4'));
    frames(rt, 3);
    let open = rt.state.board_open.as_ref().unwrap();
    assert_eq!(
        open.scene.as_ref().unwrap().drawn,
        8,
        "the file was read again"
    );
    assert_eq!(view_of(rt), moved, "the view did not move");
    // Muninn only read the board.
    assert_eq!(fs::read_to_string(&path).unwrap(), scene.to_string());
}

#[test]
fn each_board_keeps_its_view_for_the_session() {
    let tmp = vault();
    let mut s = fresh(&tmp);
    open(&mut s.rt, "Smoke");
    let rt = &mut s.rt;
    rt.key(plain(Key::Up));
    frames(rt, 1);
    let smoke = view_of(rt);
    open(rt, "Empty");
    assert!(rt.find("board-empty").is_some(), "an empty board says so");
    assert_eq!(view_of(rt).zoom, 1.0);
    open(rt, "Smoke");
    assert_eq!(view_of(rt), smoke);
    open(rt, "Broken");
    assert!(rt.find("board-canvas").is_none());
    assert!(
        rt.find("board-placeholder").is_some(),
        "a broken board is not drawn"
    );
}

#[test]
fn a_restart_reopens_the_last_board_at_its_view() {
    let tmp = vault();
    let mut s = fresh(&tmp);
    open(&mut s.rt, "Smoke");
    s.rt.key(plain(Key::Right));
    s.rt.key(plain(Key::Text("+".into())));
    frames(&mut s.rt, 1);
    let left = view_of(&s.rt);
    // Quit as the app does on close.
    s.rt.state.flush_before_quit();
    drop(s.rt);

    let mut rt = launch(tmp.path(), &s.config, &s.state_home);
    frames(&mut rt, 3);
    assert_eq!(rt.state.mode, Mode::Board, "the last mode");
    let path = rt
        .state
        .board_selected
        .clone()
        .expect("the last board is open");
    assert!(path.ends_with("Smoke.excalidraw"));
    assert_eq!(view_of(&rt), left, "at the view it had");
    assert!(rt.find("board-canvas").is_some());

    // A settings file from slice 2 (no board key) opens nothing.
    let settings = s.config.join("muninn").join("settings.json");
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("board");
    fs::write(&settings, value.to_string()).unwrap();
    let mut rt = launch(tmp.path(), &s.config, &s.state_home);
    frames(&mut rt, 2);
    assert_eq!(rt.state.mode, Mode::Board);
    assert_eq!(rt.state.board_open.as_ref().map(|o| o.path.clone()), None);
    assert!(rt.find("board-placeholder").is_some());
}

#[test]
fn switching_vaults_closes_the_open_board() {
    let tmp = vault();
    let other = TempVault::new(&[("Beta.md", "# Beta\n")]);
    let mut s = fresh(&tmp);
    open(&mut s.rt, "Smoke");
    assert!(s.rt.state.board_open.is_some());
    assert!(s.rt.state.switch_vault(other.path().to_path_buf()));
    assert!(s.rt.state.board_open.is_none());
    assert!(s.rt.state.boards.is_empty());
    assert_eq!(
        s.rt.state.board_session(),
        None,
        "nothing to restore in the new vault"
    );
}

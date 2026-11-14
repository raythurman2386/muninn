//! Issue #88: committed dark ink on a dark board, headless.
//!
//! A board whose `viewBackgroundColor` is dark still stores Excalidraw's
//! default `#1e1e1e` ink on elements written before #72. The canvas paints a
//! hue-preserving legible substitute for those, but the element and the
//! `.excalidraw` file keep the colour the file names. The colour maths is
//! unit-tested in `muninn_ui::board_scene` and `muninn_ui::contrast`; this
//! file checks what the app paints and what it saves.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{
    BoardScene, Mode, Settings, State, TEXT, contrast_ratio, meets, on_key, poll, view,
};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// One dark rectangle and one dark text element, both in Excalidraw's default
/// ink, on a dark paper.
const DARK: &str = r##"{
  "type": "excalidraw",
  "version": 2,
  "source": "test",
  "elements": [
    {"type": "rectangle", "id": "ink", "x": 0, "y": 0, "width": 80, "height": 40,
     "strokeColor": "#1e1e1e", "backgroundColor": "#1e1e1e", "fillStyle": "solid",
     "strokeWidth": 2, "opacity": 100, "isDeleted": false, "version": 1},
    {"type": "text", "id": "t", "x": 10, "y": 60, "width": 60, "height": 25,
     "text": "dark ink", "fontSize": 20, "strokeColor": "#1e1e1e",
     "backgroundColor": "transparent", "opacity": 100, "isDeleted": false, "version": 1}
  ],
  "appState": {"viewBackgroundColor": "#121212", "gridSize": null},
  "files": {}
}"##;

/// The same elements on Excalidraw's own white paper.
const LIGHT: &str = r##"{
  "type": "excalidraw",
  "version": 2,
  "source": "test",
  "elements": [
    {"type": "text", "id": "t", "x": 10, "y": 60, "width": 60, "height": 25,
     "text": "dark ink", "fontSize": 20, "strokeColor": "#1e1e1e",
     "backgroundColor": "transparent", "opacity": 100, "isDeleted": false, "version": 1}
  ],
  "appState": {"viewBackgroundColor": "#ffffff", "gridSize": null},
  "files": {}
}"##;

/// A picked palette stroke that is already legible on either paper.
const PICKED: &str = r##"{
  "type": "excalidraw",
  "version": 2,
  "source": "test",
  "elements": [
    {"type": "text", "id": "t", "x": 10, "y": 60, "width": 60, "height": 25,
     "text": "picked", "fontSize": 20, "strokeColor": "#868e96",
     "backgroundColor": "transparent", "opacity": 100, "isDeleted": false, "version": 1}
  ],
  "appState": {"viewBackgroundColor": "#121212", "gridSize": null},
  "files": {}
}"##;

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-ink-{tag}-{}-{}",
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
        ("Agenda.md", "# Agenda\n"),
        ("Dark.excalidraw", DARK),
        ("Light.excalidraw", LIGHT),
        ("Picked.excalidraw", PICKED),
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

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 2);
}

fn open_board(rt: &mut Runtime<State>, title: &str) {
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
        .position(|b| b.file.title == title)
        .unwrap_or_else(|| panic!("no board {title}"));
    click(rt, &format!("board-row-{i}"));
    frames(rt, 60);
}

fn scene(rt: &Runtime<State>) -> &BoardScene {
    rt.state
        .board_open
        .as_ref()
        .and_then(|open| open.scene.as_ref().ok())
        .expect("an open board scene")
}

/// The colour the canvas actually paints text `text` with.
fn painted_text(rt: &Runtime<State>, text: &str) -> rune::Color {
    rt.elements()
        .into_iter()
        .find(|e| e.text.as_deref() == Some(text) && !e.exiting)
        .and_then(|e| e.text_color)
        .unwrap_or_else(|| panic!("missing painted text {text:?}"))
}

fn save(rt: &mut Runtime<State>) {
    rt.key(KeyEvent {
        key: Key::Command('s'),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    frames(rt, 2);
}

/// #88: `#1e1e1e` on `#121212` is 1.12:1 and invisible. The canvas paints a
/// legible substitute for the committed ink, and the file keeps the ink it
/// was saved with.
#[test]
fn dark_ink_on_a_dark_board_is_painted_legibly_and_saved_as_stored() {
    let vault = vault();
    let mut rt = launch(vault.path());
    open_board(&mut rt, "Dark");
    assert_eq!(rt.state.mode, Mode::Board);

    let paper = rune::color::hex("#121212").expect("paper");
    assert_eq!(scene(&rt).paper, paper);

    // The scene's text ink — what the canvas paints — clears the text floor.
    let ink = scene(&rt).texts[0].color;
    assert!(
        meets(ink, paper, TEXT),
        "scene text ink {} on {} is {:.2}",
        ink.hex(),
        paper.hex(),
        contrast_ratio(ink, paper),
    );
    // And so does what the runtime actually paints for that text.
    let painted = painted_text(&rt, "dark ink");
    assert!(
        meets(painted, paper, TEXT),
        "painted ink {} on {} is {:.2}",
        painted.hex(),
        paper.hex(),
        contrast_ratio(painted, paper),
    );
    assert_ne!(painted, rune::color::hex("#1e1e1e").expect("ink"));

    // Display-only: the model keeps the file's ink for both elements.
    let model = rt
        .state
        .board_open
        .as_ref()
        .and_then(|open| open.model.as_ref())
        .expect("the board model");
    for id in ["ink", "t"] {
        let stored = model.shape(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!(stored.stroke_color, "#1e1e1e", "stored ink of {id}");
    }

    // Saving writes those bytes back: no colour was rewritten.
    save(&mut rt);
    let saved = fs::read_to_string(vault.path().join("Dark.excalidraw")).expect("saved board");
    assert_eq!(
        saved.matches("\"strokeColor\": \"#1e1e1e\"").count(),
        2,
        "both elements keep Excalidraw's ink"
    );
    assert!(
        saved.contains("\"backgroundColor\": \"#1e1e1e\""),
        "the dark fill is kept too"
    );
}

/// Excalidraw's own ink on its own white paper is untouched: no substitution.
#[test]
fn default_ink_on_a_light_board_is_painted_as_stored() {
    let vault = vault();
    let mut rt = launch(vault.path());
    open_board(&mut rt, "Light");
    let paper = rune::color::hex("#ffffff").expect("paper");
    assert_eq!(scene(&rt).paper, paper);
    let stored = rune::color::hex("#1e1e1e").expect("ink");
    assert_eq!(scene(&rt).texts[0].color, stored);
    assert_eq!(painted_text(&rt, "dark ink"), stored);
}

/// A picked palette colour that clears the floor is painted unchanged.
#[test]
fn a_picked_colour_that_clears_the_floor_is_painted_unchanged() {
    let vault = vault();
    let mut rt = launch(vault.path());
    open_board(&mut rt, "Picked");
    let picked = rune::color::hex("#868e96").expect("picked");
    assert!(meets(picked, scene(&rt).paper, TEXT));
    assert_eq!(scene(&rt).texts[0].color, picked);
    assert_eq!(painted_text(&rt, "picked"), picked);
}

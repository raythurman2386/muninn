//! Board slice 7: boards outside Board mode. The sidebar lists them, a
//! note's `![[Board.excalidraw]]` embed shows a board placeholder that opens
//! it, and Board's heading with nothing picked says so.

use std::fs;
use std::path::Path;

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");

struct App {
    rt: Runtime<State>,
    _config: TempVault,
    _state: TempVault,
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
        _config: config,
        _state: state_home,
    }
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn mode(rt: &mut Runtime<State>, c: char) {
    rt.key(KeyEvent {
        key: Key::Command(c),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    frames(rt, 2);
}

fn tap(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no {key}"));
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    frames(rt, 3);
}

fn text_of(rt: &Runtime<State>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .and_then(|element| element.text)
        .unwrap_or_else(|| panic!("missing text {key}"))
}

fn vault() -> TempVault {
    TempVault::new(&[
        ("Home.md", "# Home\n\nThe plan: ![[Draw.excalidraw]]\n"),
        ("Draw.excalidraw", DRAW),
        ("projects/Sketch.excalidraw", DRAW),
    ])
}

#[test]
fn board_with_nothing_picked_says_no_board_selected_not_empty_board() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    mode(&mut app.rt, '4');
    assert!(app.rt.state.board_open.is_none());
    let title = text_of(&app.rt, "board-surface-title");
    assert_ne!(title, "Empty board", "nothing is picked; no board is empty");
    assert_eq!(title, "No board selected");
}

#[test]
fn the_sidebar_lists_boards_in_every_mode_and_opens_one_on_board() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    frames(&mut app.rt, 2);
    assert_eq!(app.rt.state.mode, Mode::Editor);
    assert!(app.rt.find("sidebar-boards").is_some(), "a Boards section");
    let titles: Vec<String> = (0..2)
        .map(|i| text_of(&app.rt, &format!("sidebar-board-title-{i}")))
        .collect();
    assert_eq!(titles, ["Draw", "Sketch"]);
    tap(&mut app.rt, "sidebar-board-1");
    assert_eq!(app.rt.state.mode, Mode::Board);
    let open = app.rt.state.board_open.as_ref().expect("opened");
    assert!(open.path.ends_with("projects/Sketch.excalidraw"));
    // A board another app adds shows up without a restart.
    fs::write(tmp.path().join("New.excalidraw"), DRAW).unwrap();
    mode(&mut app.rt, '1');
    frames(&mut app.rt, 200);
    assert!(
        (0..3).any(|i| app
            .rt
            .find(&format!("sidebar-board-title-{i}"))
            .is_some_and(|_| text_of(&app.rt, &format!("sidebar-board-title-{i}")) == "New")),
        "the new board is listed"
    );
}

#[test]
fn a_board_embed_in_a_note_opens_that_board() {
    let tmp = vault();
    let mut app = launch(tmp.path());
    mode(&mut app.rt, '2');
    assert!(
        app.rt
            .elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("Open board")),
        "the embed offers Open board"
    );
    assert!(app.rt.find("attachment-open-0").is_some());
    // What the button does (the reader's pointer routing is Rune's).
    let home = app.rt.state.selected.expect("Home");
    let embeds = app
        .rt
        .state
        .vault
        .attachments(&app.rt.state.editor.text, home);
    assert!(embeds[0].is_board(), "{embeds:?}");
    app.rt
        .state
        .open_board_at(embeds[0].path.clone().expect("Draw is there"));
    frames(&mut app.rt, 3);
    assert_eq!(app.rt.state.mode, Mode::Board);
    let open = app.rt.state.board_open.as_ref().expect("opened");
    assert!(open.path.ends_with("Draw.excalidraw"));
}

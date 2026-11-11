//! Board slice 8: the collapsible notes sidebar and a usable empty Board
//! (New board), headless.

use std::fs;
use std::path::{Path, PathBuf};

use muninn_core::Board;
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
    let config = TempVault::new(&[]);
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
        _config: config,
        _state: state_home,
    }
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn command(rt: &mut Runtime<State>, c: char) {
    rt.key(KeyEvent {
        key: Key::Command(c),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    frames(rt, 2);
}

fn press(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no {key}"));
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    frames(rt, 3);
}

/// Every file under `root`, relative, sorted (hidden ones too).
fn files(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else {
                out.push(
                    p.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn open_path(rt: &Runtime<State>) -> Option<PathBuf> {
    rt.state.board_open.as_ref().map(|o| o.path.clone())
}

fn sidebar(rt: &Runtime<State>) -> bool {
    rt.find("sidebar").is_some()
}

// ---- New board -------------------------------------------------------------

#[test]
fn board_mode_in_a_vault_with_no_boards_writes_nothing_and_offers_new_board() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let before = files(tmp.path());
    let mut app = launch(tmp.path());
    command(&mut app.rt, '4');
    frames(&mut app.rt, 60);
    assert_eq!(app.rt.state.mode, Mode::Board);
    assert!(
        app.rt.find("board-new-empty").is_some(),
        "the empty state offers New board"
    );
    assert!(
        app.rt.find("board-new").is_some(),
        "and so does the Boards panel"
    );
    assert_eq!(files(tmp.path()), before, "opening Board writes nothing");
}

#[test]
fn new_board_creates_an_untitled_board_in_the_vault_and_opens_it() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut app = launch(tmp.path());
    command(&mut app.rt, '4');
    frames(&mut app.rt, 10);
    press(&mut app.rt, "board-new-empty");
    let path = tmp.path().join("Untitled board.excalidraw");
    assert!(path.is_file(), "created: {:?}", files(tmp.path()));
    let board = Board::from_json(&fs::read_to_string(&path).unwrap()).expect("a valid board");
    assert_eq!(board.live_elements().count(), 0);
    assert_eq!(open_path(&app.rt), Some(path.clone()), "and opened");
    assert_eq!(app.rt.state.boards.len(), 1, "listed in Board");
    assert_eq!(
        app.rt.state.sidebar_boards.len(),
        1,
        "and in the sidebar's Boards"
    );
    assert!(app.rt.find("board-canvas").is_some());
    assert!(!app.rt.state.board_dirty(), "a fresh board is clean");
}

#[test]
fn new_board_names_never_collide() {
    let tmp = TempVault::new(&[
        ("Alpha.md", "# Alpha\n"),
        ("Untitled board.excalidraw", DRAW),
    ]);
    let mut app = launch(tmp.path());
    command(&mut app.rt, '4');
    frames(&mut app.rt, 10);
    press(&mut app.rt, "board-new");
    press(&mut app.rt, "board-new");
    let root = tmp.path();
    assert_eq!(
        fs::read_to_string(root.join("Untitled board.excalidraw")).unwrap(),
        DRAW,
        "the existing board is untouched"
    );
    assert!(root.join("Untitled board 2.excalidraw").is_file());
    assert!(root.join("Untitled board 3.excalidraw").is_file());
    assert_eq!(
        open_path(&app.rt),
        Some(root.join("Untitled board 3.excalidraw"))
    );
    assert_eq!(app.rt.state.boards.len(), 3);
}

#[test]
fn new_board_goes_next_to_the_open_board() {
    let tmp = TempVault::new(&[
        ("Alpha.md", "# Alpha\n"),
        ("Sketches/Plan.excalidraw", DRAW),
    ]);
    let mut app = launch(tmp.path());
    command(&mut app.rt, '4');
    frames(&mut app.rt, 10);
    press(&mut app.rt, "board-row-0");
    assert!(open_path(&app.rt).is_some_and(|p| p.ends_with("Sketches/Plan.excalidraw")));
    press(&mut app.rt, "board-new");
    let made = tmp.path().join("Sketches/Untitled board.excalidraw");
    assert!(made.is_file(), "{:?}", files(tmp.path()));
    assert_eq!(open_path(&app.rt), Some(made));
}

#[test]
fn new_board_with_no_board_open_goes_in_the_current_notes_folder_like_new_note() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Work/Beta.md", "# Beta\n")]);
    let mut app = launch(tmp.path());
    let beta = app
        .rt
        .state
        .vault
        .notes
        .iter()
        .position(|n| n.title == "Beta")
        .unwrap();
    app.rt.state.open_note(beta);
    command(&mut app.rt, '4');
    frames(&mut app.rt, 10);
    press(&mut app.rt, "board-new-empty");
    assert!(
        tmp.path().join("Work/Untitled board.excalidraw").is_file(),
        "{:?}",
        files(tmp.path())
    );
}

#[test]
fn notes_create_rename_and_delete_still_work_beside_a_new_board() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut app = launch(tmp.path());
    command(&mut app.rt, '4');
    frames(&mut app.rt, 10);
    press(&mut app.rt, "board-new-empty");
    command(&mut app.rt, '1');
    app.rt.state.create_note_named_in("Gamma", "");
    frames(&mut app.rt, 2);
    let gamma = app
        .rt
        .state
        .vault
        .notes
        .iter()
        .position(|n| n.title == "Gamma")
        .expect("created");
    app.rt.state.begin_rename_note(gamma);
    app.rt.state.rename_field.replace_text("Delta");
    app.rt.state.submit_rename_note();
    frames(&mut app.rt, 2);
    assert!(tmp.path().join("Delta.md").is_file());
    let delta = app
        .rt
        .state
        .vault
        .notes
        .iter()
        .position(|n| n.title == "Delta")
        .unwrap();
    app.rt.state.delete_note(delta);
    app.rt.state.confirm_delete();
    frames(&mut app.rt, 2);
    assert!(
        !tmp.path().join("Delta.md").exists(),
        "{:?}",
        files(tmp.path())
    );
    assert!(
        tmp.path().join("Untitled board.excalidraw").is_file(),
        "the board is untouched"
    );
    assert_eq!(app.rt.state.sidebar_boards.len(), 1);
}

// ---- The collapsible sidebar -------------------------------------------------

#[test]
fn entering_board_collapses_the_sidebar_and_leaving_restores_it() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", DRAW)]);
    let mut app = launch(tmp.path());
    assert!(sidebar(&app.rt), "shown in Write");
    command(&mut app.rt, '4');
    assert!(!sidebar(&app.rt), "collapsed on Board");
    assert!(app.rt.find("sidebar-toggle").is_some(), "with a way back");
    command(&mut app.rt, '1');
    assert!(sidebar(&app.rt), "back on leaving Board");
    press(&mut app.rt, "sidebar-toggle");
    assert!(!sidebar(&app.rt), "hidden by hand in Write");
    command(&mut app.rt, '4');
    command(&mut app.rt, '2');
    assert!(
        !sidebar(&app.rt),
        "leaving Board restores the hidden state too"
    );
}

#[test]
fn reopening_the_sidebar_on_board_is_kept_for_the_session() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", DRAW)]);
    let mut app = launch(tmp.path());
    command(&mut app.rt, '4');
    press(&mut app.rt, "sidebar-toggle");
    assert!(sidebar(&app.rt), "the button reopens it");
    command(&mut app.rt, '1');
    command(&mut app.rt, '4');
    assert!(sidebar(&app.rt), "still open on Board later in the session");
    command(&mut app.rt, '\\');
    assert!(!sidebar(&app.rt), "the shortcut closes it");
}

#[test]
fn the_sidebar_shortcut_does_nothing_while_typing() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut app = launch(tmp.path());
    command(&mut app.rt, 'k');
    assert!(
        app.rt.state.is_typing(),
        "the search field has the keyboard"
    );
    command(&mut app.rt, '\\');
    assert!(sidebar(&app.rt), "not toggled from inside a text field");
}

#[test]
fn toggling_the_sidebar_on_board_moves_the_chrome_at_once_without_a_slide() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", DRAW)]);
    let mut app = launch(tmp.path());
    command(&mut app.rt, '4');
    frames(&mut app.rt, 10);
    press(&mut app.rt, "board-row-0");
    frames(&mut app.rt, 60);
    for round in 0..2 {
        app.rt.key(KeyEvent {
            key: Key::Command('\\'),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        });
        let mut first = None;
        for i in 0..40 {
            app.rt.frame(1.0 / 60.0);
            let els = app.rt.elements();
            for key in [
                "board-hint",
                "board-controls",
                "board-toolbar",
                "board-canvas",
            ] {
                let e = els
                    .iter()
                    .find(|e| e.key.as_deref() == Some(key))
                    .unwrap_or_else(|| panic!("no {key}"));
                assert!(!e.moving, "round {round}: {key} mid-animation at frame {i}");
            }
            let hint = els
                .iter()
                .find(|e| e.key.as_deref() == Some("board-hint"))
                .unwrap()
                .rect;
            match first {
                None => first = Some(hint),
                Some(f) => assert_eq!(hint, f, "round {round}: the hint bar moved after frame 0"),
            }
        }
    }
}

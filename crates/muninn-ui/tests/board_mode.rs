//! Board slice 2: `Mode::Board` wiring. Shortcuts, the mode button, the
//! palette, Esc, note opens, the board list and last-mode persistence, all
//! headless.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, PaletteAction, PaletteItem, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const SCENE: &str = include_str!("../../muninn-core/tests/fixtures/board/scene.excalidraw");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn board_vault() -> TempVault {
    TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
        ("Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
        ("Plan.excalidraw", SCENE),
        (
            "sketches/Empty.excalidraw",
            r#"{"type":"excalidraw","version":2}"#,
        ),
        ("sketches/Broken.excalidraw", "{ not json"),
        (".hidden/Secret.excalidraw", SCENE),
    ])
}

fn state_at(root: &Path, config: &Path, state_home: &Path) -> State {
    State::with_settings(
        root.to_path_buf(),
        state_home.to_path_buf(),
        config.to_path_buf(),
        Settings::load_from(config),
    )
}

fn runtime(state: State) -> Runtime<State> {
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn fresh(tmp: &TempVault) -> (Runtime<State>, PathBuf) {
    let config = unique_dir("config");
    let state = state_at(tmp.path(), &config, &unique_dir("state"));
    (runtime(state), config)
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
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
}

fn key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
}

#[test]
fn cmd_4_shows_the_board_page_and_esc_returns_to_the_last_note_mode() {
    let tmp = board_vault();
    let (mut rt, _) = fresh(&tmp);
    rt.key(command('2'));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Preview);

    rt.key(command('4'));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Board);
    assert!(rt.find("board-page").is_some(), "the board page is mounted");
    assert!(rt.find("board-surface").is_some());
    assert!(rt.find("note-body").is_none(), "Board replaces the note");
    assert_eq!(
        rt.state.last_note_mode,
        Mode::Preview,
        "Board is not a note mode"
    );

    rt.key(key(Key::Escape));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Preview, "Esc leaves Board for Read");

    // ⌘4 toggles through the palette action the same way.
    rt.state.toggle_board();
    assert_eq!(rt.state.mode, Mode::Board);
    rt.state.toggle_board();
    assert_eq!(rt.state.mode, Mode::Preview);
}

#[test]
fn the_mode_button_and_the_palette_enter_board() {
    let tmp = board_vault();
    let (mut rt, _) = fresh(&tmp);
    assert!(rt.find("mode-board").is_some(), "a fourth mode button");
    click(&mut rt, "mode-board");
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Board);

    rt.key(command('1'));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Editor);

    assert!(PaletteAction::ALL.contains(&PaletteAction::ToggleBoard));
    rt.key(KeyEvent {
        key: Key::Command('p'),
        mods: Mods {
            cmd: true,
            shift: true,
            ..Mods::default()
        },
    });
    frame(&mut rt);
    for c in "whiteboard".chars() {
        rt.key(key(Key::Text(c.to_string())));
    }
    frame(&mut rt);
    assert_eq!(
        rt.state.palette_items(),
        vec![PaletteItem::Action(PaletteAction::ToggleBoard)]
    );
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert!(!rt.state.palette_open());
    assert_eq!(rt.state.mode, Mode::Board);
}

#[test]
fn moving_between_graph_board_and_the_note_modes_keeps_the_note_memory() {
    let tmp = board_vault();
    let (mut rt, _) = fresh(&tmp);
    rt.key(command('2'));
    rt.key(command('3'));
    rt.key(command('4'));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Board);
    assert_eq!(rt.state.last_note_mode, Mode::Preview);

    // ⌘G from Board opens the graph; Esc from the graph skips Board.
    rt.key(command('g'));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Graph);
    rt.key(key(Key::Escape));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Preview);

    // ⌘E from Board goes back to the last note mode, as from Graph.
    rt.key(command('4'));
    rt.key(command('e'));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Preview);
}

#[test]
fn opening_a_note_from_the_sidebar_leaves_board_for_write() {
    let tmp = board_vault();
    let (mut rt, _) = fresh(&tmp);
    rt.key(command('4'));
    frame(&mut rt);
    let beta = rt.state.vault.resolve("Beta").expect("Beta");
    rt.state.open_from_list(beta);
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Editor);
    assert_eq!(rt.state.selected, Some(beta));
}

#[test]
fn the_board_page_lists_vault_boards_with_element_counts() {
    let tmp = board_vault();
    let (mut rt, _) = fresh(&tmp);
    rt.key(command('4'));
    frame(&mut rt);

    let rels: Vec<_> = rt
        .state
        .boards
        .iter()
        .map(|b| b.file.rel.as_str())
        .collect();
    assert_eq!(
        rels,
        [
            "Plan.excalidraw",
            "sketches/Broken.excalidraw",
            "sketches/Empty.excalidraw"
        ],
        "hidden folders are skipped, as for notes"
    );
    // The fixture has 9 elements, one a tombstone; 8 are drawn.
    assert_eq!(rt.state.boards[0].detail(), "8 elements · 2 kept as-is");
    assert!(rt.state.boards[1].detail().starts_with("Could not read"));
    assert_eq!(rt.state.boards[2].detail(), "0 elements");
    assert_eq!(rt.state.vault.notes.len(), 2, "boards are not notes");
    assert!(rt.find("board-row-0").is_some());

    click(&mut rt, "board-row-0");
    frame(&mut rt);
    assert_eq!(
        rt.state.selected_board().map(|b| b.file.title.as_str()),
        Some("Plan")
    );
    let title = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("board-surface-title"))
        .and_then(|e| e.text);
    assert_eq!(title.as_deref(), Some("Plan"));

    // The list is read from disk on entry: a board removed meanwhile drops
    // out, and so does its selection. Nothing is written.
    let before = fs::read(tmp.path().join("Plan.excalidraw")).expect("bytes");
    fs::remove_file(tmp.path().join("sketches/Empty.excalidraw")).expect("remove");
    rt.key(command('1'));
    rt.key(command('4'));
    frame(&mut rt);
    assert_eq!(rt.state.boards.len(), 2);
    assert!(rt.state.selected_board().is_some());
    fs::remove_file(tmp.path().join("Plan.excalidraw")).expect("remove");
    rt.state.refresh_boards();
    assert!(rt.state.selected_board().is_none());
    assert_eq!(before, SCENE.as_bytes(), "the board was only read");
}

#[test]
fn board_is_the_restored_mode_after_a_restart() {
    let tmp = board_vault();
    let config = unique_dir("restart-config");
    let state_home = unique_dir("restart-state");
    let mut rt = runtime(state_at(tmp.path(), &config, &state_home));
    rt.key(command('4'));
    frame(&mut rt);
    assert_eq!(Settings::load_from(&config).mode, Mode::Board);
    drop(rt);

    let rt = runtime(state_at(tmp.path(), &config, &state_home));
    assert_eq!(rt.state.mode, Mode::Board);
    assert_eq!(rt.state.boards.len(), 3, "the list is read on restore too");
    assert!(rt.find("board-page").is_some());
}

#[test]
fn settings_written_before_board_still_load() {
    for (key, mode) in [
        ("editor", Mode::Editor),
        ("preview", Mode::Preview),
        ("graph", Mode::Graph),
        ("board", Mode::Board),
        ("something-newer", Mode::Editor),
    ] {
        let config = unique_dir("old-settings");
        let dir = config.join("muninn");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(
            dir.join("settings.json"),
            format!(r#"{{"theme":"dark","mode":"{key}","scope":"global"}}"#),
        )
        .expect("settings");
        assert_eq!(Settings::load_from(&config).mode, mode, "{key}");
    }
}

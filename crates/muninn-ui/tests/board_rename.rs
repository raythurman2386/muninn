//! Board rename in the app (Raymond, #43: "I can't seem to update the name of
//! the new board"): F2 on Board, Rename in a board row's right-click menu (the
//! Board panel and the notes sidebar's Boards section), and a double click on
//! the board's title. Headless.

use std::fs;
use std::path::{Path, PathBuf};

use muninn_core::Board;
use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");

struct App {
    rt: Runtime<State>,
}

fn launch(root: &Path, config: &Path, state_home: &Path) -> App {
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
    App { rt }
}

/// A vault plus its own config and state homes, kept for a relaunch.
struct Fixture {
    vault: TempVault,
    config: TempVault,
    state: TempVault,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Fixture {
        Fixture::with_config(files, &[])
    }
    fn with_config(files: &[(&str, &str)], config: &[(&str, &str)]) -> Fixture {
        Fixture {
            vault: TempVault::new(files),
            config: TempVault::new(config),
            state: TempVault::new(&[]),
        }
    }
    fn launch(&self) -> App {
        launch(self.vault.path(), self.config.path(), self.state.path())
    }
    fn path(&self, rel: &str) -> PathBuf {
        self.vault.path().join(rel)
    }
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn key(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    frames(rt, 2);
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

fn type_text(rt: &mut Runtime<State>, typed: &str) {
    for ch in typed.chars() {
        rt.key(KeyEvent {
            key: Key::Text(ch.to_string()),
            mods: Mods::default(),
        });
        rt.frame(1.0 / 60.0);
    }
    frames(rt, 2);
}

fn press(rt: &mut Runtime<State>, el: &str) {
    let (x, y) = rt.find(el).unwrap_or_else(|| panic!("no {el}"));
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    frames(rt, 3);
}

fn right_click(rt: &mut Runtime<State>, el: &str) {
    let (x, y) = rt.find(el).unwrap_or_else(|| panic!("no {el}"));
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    // The menu slides in under the row; settle before picking.
    frames(rt, 12);
}

fn double_click(rt: &mut Runtime<State>, el: &str) {
    let (x, y) = rt.find(el).unwrap_or_else(|| panic!("no {el}"));
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.frame(1.0 / 60.0);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
    }
    frames(rt, 3);
}

fn text_of(rt: &Runtime<State>, el: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(el))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn open_path(rt: &Runtime<State>) -> Option<PathBuf> {
    rt.state.board_open.as_ref().map(|o| o.path.clone())
}

/// Draw a rectangle on the open board's canvas (R, drag, back to Select).
fn draw_rect(rt: &mut Runtime<State>, dx: f32) {
    key(rt, Key::Text("r".into()));
    let r = rt.presented("board-canvas").expect("board canvas").0;
    let (x0, y0) = (r[0] + 200.0 + dx, r[1] + 200.0);
    rt.pointer_down(x0, y0);
    rt.frame(1.0 / 60.0);
    for step in 1..=6 {
        rt.pointer_move(x0 + 15.0 * step as f32, y0 + 10.0 * step as f32);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x0 + 90.0, y0 + 60.0);
    frames(rt, 2);
    key(rt, Key::Text("v".into()));
}

fn elements_on_disk(path: &Path) -> usize {
    Board::from_json(&fs::read_to_string(path).unwrap())
        .unwrap()
        .live_elements()
        .count()
}

/// Every file name under `dir` (recursively).
fn names_under(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(read) = fs::read_dir(dir) {
        for e in read.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(names_under(&p));
            } else {
                out.push(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    out
}

/// Board, New board: `Untitled board.excalidraw` at the vault root, open.
fn new_board(app: &mut App) {
    command(&mut app.rt, '4');
    frames(&mut app.rt, 10);
    press(&mut app.rt, "board-new-empty");
    frames(&mut app.rt, 10);
}

fn rename_field_open(rt: &Runtime<State>) -> bool {
    rt.find("board-rename-name").is_some()
}

// ---- F2 on Board -----------------------------------------------------------

#[test]
fn f2_renames_the_new_board_and_it_stays_open_unsaved_drawing_included() {
    // Explicit save, so the drawing is still unsaved when the rename starts.
    let fx = Fixture::with_config(
        &[("Alpha.md", "# Alpha\n")],
        &[("muninn/settings.json", r#"{"board_autosave": false}"#)],
    );
    let mut app = fx.launch();
    new_board(&mut app);
    let old = fx.path("Untitled board.excalidraw");
    assert_eq!(open_path(&app.rt), Some(old.clone()));
    draw_rect(&mut app.rt, 0.0);
    assert!(app.rt.state.board_dirty(), "a rectangle not saved yet");
    // The recovery copy of the unsaved drawing is written once it rests.
    frames(&mut app.rt, 90);
    assert!(
        names_under(fx.state.path())
            .iter()
            .any(|n| n.contains("Untitled_board")),
        "a recovery copy under the old name: {:?}",
        names_under(fx.state.path())
    );

    key(&mut app.rt, Key::Function(2));
    assert!(rename_field_open(&app.rt), "F2 opens the name field");
    assert!(app.rt.state.is_typing(), "and it has the keyboard");
    // The whole name is selected, so typing replaces it.
    type_text(&mut app.rt, "Roadmap");
    key(&mut app.rt, Key::Enter);

    let new = fx.path("Roadmap.excalidraw");
    assert!(!rename_field_open(&app.rt), "Enter commits and closes it");
    assert!(new.is_file(), "renamed on disk");
    assert!(!old.exists(), "the old file is gone");
    assert_eq!(
        open_path(&app.rt),
        Some(new.clone()),
        "still the open board"
    );
    assert_eq!(
        elements_on_disk(&new),
        1,
        "the unsaved rectangle was saved first"
    );
    assert_eq!(text_of(&app.rt, "board-surface-title"), "Roadmap");
    assert!(
        app.rt.state.sidebar_boards.iter().any(|b| b.path == new)
            && !app.rt.state.sidebar_boards.iter().any(|b| b.path == old),
        "the sidebar lists the new name only"
    );
    assert!(app.rt.state.notice.is_none(), "{:?}", app.rt.state.notice);

    // The watchers run (WATCH_INTERVAL is 1.5 s): Muninn's own rename is not
    // another app's delete or change.
    frames(&mut app.rt, 240);
    let open = app.rt.state.board_open.as_ref().unwrap();
    assert_eq!(open.conflict, None, "no 'Removed or renamed on disk'");
    assert_eq!(open_path(&app.rt), Some(new.clone()));

    // Still editable, and it saves to the new path.
    draw_rect(&mut app.rt, 150.0);
    command(&mut app.rt, 's');
    assert_eq!(elements_on_disk(&new), 2, "saved to the new path");
    assert!(!old.exists(), "nothing written back under the old name");
    frames(&mut app.rt, 60);
    assert!(
        !names_under(fx.state.path())
            .iter()
            .any(|n| n.contains("Untitled_board") && !n.starts_with("settings")),
        "the old name's recovery copy is gone: {:?}",
        names_under(fx.state.path())
    );

    // A relaunch offers nothing to recover and reopens the renamed board.
    drop(app);
    let mut again = fx.launch();
    frames(&mut again.rt, 10);
    assert!(
        again.rt.state.recovered.is_empty(),
        "{:?}",
        again.rt.state.recovered
    );
    command(&mut again.rt, '4');
    frames(&mut again.rt, 30);
    assert_eq!(open_path(&again.rt), Some(new));
}

#[test]
fn esc_cancels_a_board_rename_and_changes_nothing() {
    let fx = Fixture::new(&[("Alpha.md", "# Alpha\n")]);
    let mut app = fx.launch();
    new_board(&mut app);
    key(&mut app.rt, Key::Function(2));
    assert!(rename_field_open(&app.rt));
    type_text(&mut app.rt, "Nope");
    key(&mut app.rt, Key::Escape);
    assert!(!rename_field_open(&app.rt), "Esc closes the field");
    assert_eq!(app.rt.state.mode, Mode::Board, "and stays on Board");
    assert!(fx.path("Untitled board.excalidraw").is_file());
    assert!(!fx.path("Nope.excalidraw").exists());
    assert!(!app.rt.state.is_typing());
}

#[test]
fn a_taken_or_invalid_name_is_refused_and_the_field_stays_open() {
    let fx = Fixture::new(&[("Alpha.md", "# Alpha\n"), ("Taken.excalidraw", "keep")]);
    let mut app = fx.launch();
    new_board(&mut app);
    let old = fx.path("Untitled board.excalidraw");
    for bad in ["Taken", "a/b", "   ", ".hidden"] {
        key(&mut app.rt, Key::Function(2));
        assert!(rename_field_open(&app.rt), "{bad:?}: F2 opens the field");
        type_text(&mut app.rt, bad);
        if bad.trim().is_empty() {
            // Typing spaces over a selected name leaves only spaces.
        }
        key(&mut app.rt, Key::Enter);
        assert!(
            rename_field_open(&app.rt),
            "{bad:?}: refused, the field stays for a fix"
        );
        assert!(
            !text_of(&app.rt, "board-rename-error").is_empty(),
            "{bad:?}: and says why"
        );
        assert!(old.is_file(), "{bad:?}: the board is not renamed");
        key(&mut app.rt, Key::Escape);
    }
    assert_eq!(
        fs::read_to_string(fx.path("Taken.excalidraw")).unwrap(),
        "keep"
    );
    assert_eq!(open_path(&app.rt), Some(old));
}

// ---- Right-click Rename, double-click the title ----------------------------

#[test]
fn rename_from_the_board_panel_row_menu() {
    let fx = Fixture::new(&[
        ("Alpha.md", "# Alpha\n"),
        ("A.excalidraw", DRAW),
        ("B.excalidraw", DRAW),
    ]);
    let mut app = fx.launch();
    command(&mut app.rt, '4');
    frames(&mut app.rt, 10);
    press(&mut app.rt, "board-row-0");
    frames(&mut app.rt, 20);
    right_click(&mut app.rt, "board-row-1");
    assert!(
        app.rt.find("menu-rename").is_some(),
        "the row offers Rename"
    );
    press(&mut app.rt, "menu-rename");
    assert!(rename_field_open(&app.rt));
    type_text(&mut app.rt, "Second");
    key(&mut app.rt, Key::Enter);
    assert!(fx.path("Second.excalidraw").is_file());
    assert!(!fx.path("B.excalidraw").exists());
    assert_eq!(
        open_path(&app.rt),
        Some(fx.path("A.excalidraw")),
        "renaming another board leaves the open one open"
    );
    assert!(
        app.rt.state.boards.iter().any(|b| b.file.title == "Second"),
        "listed under the new name"
    );
}

#[test]
fn rename_from_the_notes_sidebar_boards_section() {
    let fx = Fixture::new(&[("Alpha.md", "# Alpha\n"), ("Plan.excalidraw", DRAW)]);
    let mut app = fx.launch();
    frames(&mut app.rt, 10);
    right_click(&mut app.rt, "sidebar-board-0");
    press(&mut app.rt, "menu-rename");
    assert!(rename_field_open(&app.rt));
    type_text(&mut app.rt, "Roadmap");
    key(&mut app.rt, Key::Enter);
    assert!(fx.path("Roadmap.excalidraw").is_file());
    assert!(!fx.path("Plan.excalidraw").exists());
    assert_eq!(text_of(&app.rt, "sidebar-board-title-0"), "Roadmap");
    assert_eq!(
        app.rt.state.mode,
        Mode::Editor,
        "renaming does not switch modes"
    );
}

#[test]
fn double_click_the_board_title_renames_the_open_board() {
    let fx = Fixture::new(&[("Alpha.md", "# Alpha\n")]);
    let mut app = fx.launch();
    new_board(&mut app);
    double_click(&mut app.rt, "board-surface-title");
    assert!(
        rename_field_open(&app.rt),
        "double click opens the name field"
    );
    type_text(&mut app.rt, "Sketch");
    key(&mut app.rt, Key::Enter);
    assert!(fx.path("Sketch.excalidraw").is_file());
    assert_eq!(open_path(&app.rt), Some(fx.path("Sketch.excalidraw")));
    assert_eq!(text_of(&app.rt, "board-surface-title"), "Sketch");
}

// ---- Embeds, and the hide-sidebar step -------------------------------------

#[test]
fn embeds_of_a_renamed_board_follow_it_in_the_open_note_too() {
    let fx = Fixture::new(&[
        ("Home.md", "# Home\n\n![[Plan.excalidraw]]\n"),
        ("Plan.excalidraw", DRAW),
    ]);
    let mut app = fx.launch();
    frames(&mut app.rt, 10);
    assert!(app.rt.state.editor.text.contains("Plan.excalidraw"));
    command(&mut app.rt, '4');
    frames(&mut app.rt, 10);
    press(&mut app.rt, "board-row-0");
    key(&mut app.rt, Key::Function(2));
    type_text(&mut app.rt, "Roadmap");
    key(&mut app.rt, Key::Enter);
    assert_eq!(
        fs::read_to_string(fx.path("Home.md")).unwrap(),
        "# Home\n\n![[Roadmap.excalidraw]]\n"
    );
    command(&mut app.rt, '1');
    frames(&mut app.rt, 10);
    assert!(
        app.rt.state.editor.text.contains("![[Roadmap.excalidraw]]"),
        "the open note shows the new embed: {}",
        app.rt.state.editor.text
    );
}

#[test]
fn hiding_the_sidebar_or_leaving_board_cancels_a_board_rename() {
    let fx = Fixture::new(&[("Alpha.md", "# Alpha\n"), ("Plan.excalidraw", DRAW)]);
    let mut app = fx.launch();
    frames(&mut app.rt, 10);
    // Started in the sidebar, then the sidebar is hidden with its button.
    right_click(&mut app.rt, "sidebar-board-0");
    press(&mut app.rt, "menu-rename");
    assert!(rename_field_open(&app.rt));
    press(&mut app.rt, "sidebar-toggle");
    assert!(!rename_field_open(&app.rt), "the hidden field is cancelled");
    assert!(!app.rt.state.is_typing(), "and lets go of the keyboard");
    press(&mut app.rt, "sidebar-toggle");
    assert!(
        !rename_field_open(&app.rt),
        "not still open when it comes back"
    );
    // Started on Board with F2, then Cmd+1 leaves Board.
    command(&mut app.rt, '4');
    frames(&mut app.rt, 10);
    press(&mut app.rt, "board-row-0");
    key(&mut app.rt, Key::Function(2));
    assert!(rename_field_open(&app.rt));
    command(&mut app.rt, '1');
    assert_eq!(app.rt.state.mode, Mode::Editor);
    assert!(
        app.rt.state.board_rename.is_none(),
        "leaving Board cancels it"
    );
    command(&mut app.rt, '4');
    frames(&mut app.rt, 10);
    assert!(!rename_field_open(&app.rt));
    assert!(fx.path("Plan.excalidraw").is_file());
}

// ---- GK on 9e0a99e ---------------------------------------------------------

/// The rename field is on screen exactly when a rename is under way: never
/// a rename that has the keyboard (`is_typing`) with no field to show it.
fn field_matches_state(rt: &Runtime<State>) -> bool {
    rt.state.board_rename.is_some() == rename_field_open(rt)
}

/// B-1: a multibyte name whose byte length puts the `.excalidraw` check
/// mid-character (`é ünïcode` is 12 bytes) crashed the app on Enter.
#[test]
fn a_multibyte_name_renames_the_board_without_crashing() {
    let fx = Fixture::with_config(
        &[("Alpha.md", "# Alpha\n")],
        &[("muninn/settings.json", r#"{"board_autosave": false}"#)],
    );
    let mut app = fx.launch();
    new_board(&mut app);
    draw_rect(&mut app.rt, 0.0);
    key(&mut app.rt, Key::Function(2));
    assert!(rename_field_open(&app.rt), "F2 opens the name field");
    type_text(&mut app.rt, "é ünïcode");
    key(&mut app.rt, Key::Enter);
    let new = fx.path("é ünïcode.excalidraw");
    assert!(
        new.is_file(),
        "renamed on disk: {:?}",
        names_under(fx.vault.path())
    );
    assert!(!fx.path("Untitled board.excalidraw").exists());
    assert_eq!(
        elements_on_disk(&new),
        1,
        "the unsaved rectangle came along"
    );
    assert_eq!(open_path(&app.rt), Some(new.clone()));
    assert_eq!(text_of(&app.rt, "board-surface-title"), "é ünïcode");
    // Typing the extension after a multibyte name, from the title this time.
    double_click(&mut app.rt, "board-surface-title");
    assert!(
        rename_field_open(&app.rt),
        "double click opens the name field"
    );
    type_text(&mut app.rt, "ünïcode ñé.excalidraw");
    key(&mut app.rt, Key::Enter);
    let again = fx.path("ünïcode ñé.excalidraw");
    assert!(again.is_file(), "{:?}", names_under(fx.vault.path()));
    assert_eq!(open_path(&app.rt), Some(again));
    assert!(!rename_field_open(&app.rt) && app.rt.state.board_rename.is_none());
}

//! Kanban slice 12: the archive on the Kanban page. A card's menu and a
//! `Cmd`/`Ctrl`-click move a card's lines into the archive as one undo step,
//! the header's Archive page lists them and restores or deletes them, a
//! non-English archive word is kept, and viewing writes nothing.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::KanbanDefaults;
use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, PhysicalInput, Runtime};

const BOARD: &str = include_str!("../../muninn-core/tests/fixtures/kanban/edit-board.md");
const GERMAN: &str = include_str!("../../muninn-core/tests/fixtures/kanban/archive-de.md");

/// The unsaved indicator, with the platform's save chord.
const UNSAVED: &str = if cfg!(target_os = "macos") {
    "Unsaved · ⌘S saves"
} else {
    "Unsaved · Ctrl+S saves"
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-archive-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn app(tmp: &TempVault, name: &str, defaults: KanbanDefaults) -> Runtime<State> {
    let config = unique_dir("config");
    let state_home = unique_dir("state");
    let state = State::with_settings(
        tmp.path().to_path_buf(),
        state_home,
        config.clone(),
        // These tests pin explicit saving; slice 7 tests autosave.
        Settings {
            kanban_autosave: false,
            kanban_defaults: defaults,
            ..Settings::load_from(&config)
        },
    );
    let mut rt = Runtime::new(state, view, (1400.0, 900.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    let id = rt.state.vault.resolve(name).expect("the board");
    act(&mut rt, |s| s.open_kanban(id));
    rt
}

fn vault() -> TempVault {
    TempVault::new(&[("Agenda.md", "# Agenda\n"), ("Board.md", BOARD)])
}

fn on_board(tmp: &TempVault) -> Runtime<State> {
    app(tmp, "Board", KanbanDefaults::default())
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

fn act(rt: &mut Runtime<State>, f: impl FnOnce(&mut State)) {
    rt.update(|s, _| f(s));
    frame(rt);
}

fn command(c: char, shift: bool) -> KeyEvent {
    KeyEvent {
        key: Key::Command(c),
        mods: Mods {
            cmd: true,
            shift,
            ..Mods::default()
        },
    }
}

fn click(rt: &mut Runtime<State>, key: &str) {
    for _ in 0..30 {
        frame(rt);
    }
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frame(rt);
}

/// A click with `Cmd` (or `Ctrl`) held, as the checkbox archives.
fn cmd_click(rt: &mut Runtime<State>, key: &str) {
    for _ in 0..30 {
        frame(rt);
    }
    rt.physical_input(PhysicalInput::Modifiers(Mods {
        cmd: true,
        ..Mods::default()
    }));
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    // The press's handlers run on the next frame, while the key is still held.
    frame(rt);
    rt.physical_input(PhysicalInput::Modifiers(Mods::default()));
    frame(rt);
}

fn source(rt: &Runtime<State>) -> String {
    rt.state
        .kanban
        .as_ref()
        .and_then(|k| k.board.as_ref())
        .expect("a board")
        .source()
        .to_owned()
}

fn on_disk(tmp: &TempVault, name: &str) -> String {
    fs::read_to_string(tmp.path().join(name)).expect("the board file")
}

fn text_of(rt: &Runtime<State>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
}

fn has(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn the_card_menu_archives_as_one_undo_step_and_save_writes_it() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-card-0-0-menu");
    click(&mut rt, "menu-archive");
    let archived = source(&rt);
    assert!(
        archived.contains(
            "## To do\n\n- [ ] Review [[Roadmap]] ^card-2a\n  with the team\n\n\
             A paragraph between lanes.\n\n## Doing (2)\n\n- [ ] Fix the sync bug\n\n\n\
             ## Done\n\n**Complete**\n- [x] Ship 0.7.1\n- [ ] Tag the release\n\n\
             ***\n\n## Archive\n\n- [ ] Write the release notes\n"
        ),
        "{archived}"
    );
    assert_eq!(
        rt.state
            .kanban
            .as_ref()
            .unwrap()
            .board
            .as_ref()
            .unwrap()
            .lanes()[0]
            .cards()
            .len(),
        1
    );
    assert_eq!(text_of(&rt, "kanban-status").as_deref(), Some(UNSAVED));
    assert_eq!(on_disk(&tmp, "Board.md"), BOARD, "nothing written yet");

    // One undo puts the exact bytes back; redo archives again.
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), BOARD);
    rt.key(command('z', true));
    frame(&mut rt);
    assert_eq!(source(&rt), archived);

    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp, "Board.md"), archived);
    assert_eq!(text_of(&rt, "kanban-status").as_deref(), Some("Saved"));
}

#[test]
fn cmd_click_on_the_checkbox_archives_and_a_plain_click_still_toggles() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    // Cmd-click an open card: it is archived, not marked done.
    cmd_click(&mut rt, "kanban-card-1-0-mark");
    let archived = source(&rt);
    assert!(
        archived.contains("## Archive\n\n- [ ] Fix the sync bug\n"),
        "{archived}"
    );
    assert!(
        rt.state
            .kanban
            .as_ref()
            .unwrap()
            .board
            .as_ref()
            .unwrap()
            .lanes()[1]
            .cards()
            .is_empty(),
        "the Doing lane lost its only card"
    );
    // The archived card is still open.
    assert!(
        !rt.state
            .kanban
            .as_ref()
            .unwrap()
            .board
            .as_ref()
            .unwrap()
            .archive()
            .expect("archive")
            .cards()[0]
            .is_done()
    );

    // A plain click on another card's mark toggles it, as before.
    let before = source(&rt);
    click(&mut rt, "kanban-card-0-0-mark");
    assert_ne!(source(&rt), before);
    assert!(source(&rt).contains("- [x] Write the release notes\n"));
    assert!(!source(&rt).contains("***\n\n## Archive\n\n- [x] Write the release notes\n"));
}

#[test]
fn the_archive_page_lists_restores_and_deletes_with_undo() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.archive_card(0, 0));
    let archived = source(&rt);
    assert!(archived.contains("- [ ] Write the release notes\n"));
    assert_eq!(on_disk(&tmp, "Board.md"), BOARD);

    // The header button opens the archive page, read from the same file.
    click(&mut rt, "kanban-archive");
    assert!(has(&rt, "kanban-archive-panel"));
    assert_eq!(
        text_of(&rt, "kanban-archive-count").as_deref(),
        Some("1 archived card")
    );
    assert_eq!(
        text_of(&rt, "kanban-archive-text-0").as_deref(),
        Some("Write the release notes")
    );
    assert_eq!(on_disk(&tmp, "Board.md"), BOARD, "display writes nothing");
    assert!(rt.state.kanban.as_ref().unwrap().dirty());

    // Restore: the end of the first lane.
    click(&mut rt, "kanban-archive-restore-0");
    let restored = source(&rt);
    assert!(restored.contains(
        "- [ ] Review [[Roadmap]] ^card-2a\n  with the team\n- [ ] Write the release notes\n"
    ));
    assert!(has(&rt, "kanban-archive-empty"));
    assert_eq!(
        text_of(&rt, "kanban-archive-count").as_deref(),
        Some("0 archived cards")
    );
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), archived);
    assert_eq!(
        text_of(&rt, "kanban-archive-count").as_deref(),
        Some("1 archived card"),
        "undo is shown in the page"
    );

    // Restore to… offers the lanes, and puts it at that lane's end.
    click(&mut rt, "kanban-archive-lanes-0");
    assert!(has(&rt, "kanban-archive-lane-0-1"));
    click(&mut rt, "kanban-archive-lane-0-1");
    assert!(
        source(&rt)
            .contains("## Doing (2)\n\n- [ ] Fix the sync bug\n- [ ] Write the release notes\n"),
        "{}",
        source(&rt)
    );

    // Delete removes only the archived card's lines, and undoes.
    act(&mut rt, |s| s.archive_card(0, 0));
    let before_delete = source(&rt);
    assert!(before_delete.contains("- [ ] Review [[Roadmap]] ^card-2a\n"));
    click(&mut rt, "kanban-archive-delete-0");
    assert!(!source(&rt).contains("Review [[Roadmap]]"));
    assert!(has(&rt, "kanban-archive-empty"));
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), before_delete);

    // Save writes the edits; the file is the source exactly.
    let edited = source(&rt);
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp, "Board.md"), edited);
    assert_eq!(text_of(&rt, "kanban-status").as_deref(), Some("Saved"));
}

#[test]
fn an_existing_archive_word_is_kept_in_the_page_and_the_file() {
    let tmp = TempVault::new(&[("Archiv.md", GERMAN)]);
    let mut rt = app(&tmp, "Archiv", KanbanDefaults::default());
    click(&mut rt, "kanban-card-0-0-menu");
    click(&mut rt, "menu-archive");
    let archived = source(&rt);
    assert!(
        archived.contains("## Archiv\n\n- [x] Alte Aufgabe\n- [x] Versandt\n"),
        "{archived}"
    );
    assert!(!archived.contains("## Archive"), "{archived}");
    assert_eq!(
        rt.state
            .kanban
            .as_ref()
            .unwrap()
            .board
            .as_ref()
            .unwrap()
            .archive()
            .expect("archiv")
            .word(),
        "Archiv"
    );

    click(&mut rt, "kanban-archive");
    assert_eq!(
        text_of(&rt, "kanban-archive-text-1").as_deref(),
        Some("Versandt")
    );
}

#[test]
fn archive_date_settings_come_from_the_footer_then_the_app_defaults() {
    let board = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n\n\
                 %% kanban:settings\n```\n\
                 {\"kanban-plugin\":\"board\",\"archive-with-date\":true,\
                 \"archive-date-format\":\"[footer]\",\"archive-date-position\":\"start\"}\n\
                 ```\n%%\n";
    let tmp = TempVault::new(&[("Dated.md", board)]);
    let mut rt = app(
        &tmp,
        "Dated",
        KanbanDefaults::from_json(Some(&serde_json::json!({
            "archive-with-date": false,
            "archive-date-separator": "/",
        }))),
    );
    let settings = rt.state.kanban_archive_settings();
    assert!(settings.with_date, "the footer wins");
    assert_eq!(settings.date_format, "[footer]");
    assert_eq!(
        settings.date_position,
        muninn_core::ArchiveDatePosition::Start
    );
    assert_eq!(
        settings.date_separator, "/",
        "the app default fills the key the footer leaves out"
    );

    click(&mut rt, "kanban-card-0-0-menu");
    click(&mut rt, "menu-archive");
    assert!(
        source(&rt).contains("- [ ] footer/one\n"),
        "{}",
        source(&rt)
    );

    // An app default supplies the date when the footer says nothing.
    let board = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n";
    let tmp = TempVault::new(&[("Dated.md", board)]);
    let mut rt = app(
        &tmp,
        "Dated",
        KanbanDefaults::from_json(Some(&serde_json::json!({
            "archive-with-date": true,
            "archive-date-format": "[app]",
            "archive-date-position": "end",
            "archive-date-separator": " - ",
        }))),
    );
    act(&mut rt, |s| s.archive_card(0, 0));
    assert!(source(&rt).contains("- [ ] one - app\n"), "{}", source(&rt));
}

#[test]
fn viewing_the_archive_never_writes_or_dirties_the_board() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    rt.key(command('s', false));
    frame(&mut rt);
    click(&mut rt, "kanban-archive");
    let board = rt.state.kanban.as_ref().unwrap();
    assert!(board.board.is_some());
    assert!(!board.dirty());
    assert_eq!(
        text_of(&rt, "kanban-archive-empty").as_deref(),
        Some("No archived cards.")
    );
    for _ in 0..5 {
        frame(&mut rt);
    }
    assert_eq!(on_disk(&tmp, "Board.md"), BOARD);
    assert!(
        text_of(&rt, "kanban-status")
            .as_deref()
            .is_some_and(|status| status.is_empty())
    );
}

#[test]
fn the_close_button_returns_to_the_lanes() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-archive");
    assert!(has(&rt, "kanban-archive-panel"));
    click(&mut rt, "kanban-archive-close");
    assert!(!has(&rt, "kanban-archive-panel"));
    assert!(has(&rt, "kanban-lane-0"));
}

#[test]
fn a_done_lane_card_archives_too() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.archive_card(2, 0));
    let archived = source(&rt);
    assert!(
        archived.contains("## Archive\n\n- [x] Ship 0.7.1\n"),
        "{archived}"
    );
    assert!(!archived.contains("**Complete**\n- [x] Ship 0.7.1\n"));
    assert_eq!(rt.state.mode, Mode::Kanban);
}

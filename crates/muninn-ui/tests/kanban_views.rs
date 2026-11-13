//! Kanban slice 15: list and table views.
//!
//! The file's `kanban-plugin` value picks the view, `basic` and unknown values
//! stay the board, the header's switcher shows any of the three without
//! writing, and both views draw the same parsed cards and lanes as the board.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-views-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

/// A board with cards that exercise text, a link, a date, a tag, a checklist,
/// an empty lane and a long multibyte line.
fn board(value: &str) -> String {
    let long = format!("{} end", "漢字 😀 long line ".repeat(40));
    format!(
        "---\n\nkanban-plugin: {value}\n\n---\n\n\
         ## To do\n\n\
         - [ ] Write [[Roadmap]] #release @{{2026-11-11}}\n  with a second line\n\
         - [ ] {long}\n\n\
         ## Empty\n\n\n\
         ## Done\n\n\
         **Complete**\n\
         - [x] Ship it\n  - [x] one\n  - [ ] two\n"
    )
}

fn views_vault() -> TempVault {
    let board_md = board("board");
    let list_md = board("list");
    let table_md = board("table");
    let basic_md = board("basic");
    let odd_md = board("something-newer");
    TempVault::new(&[
        ("Agenda.md", "# Agenda\n\nA plain note.\n"),
        ("Roadmap.md", "# Roadmap\n\nPlans.\n"),
        ("Board.md", board_md.as_str()),
        ("List.md", list_md.as_str()),
        ("Table.md", table_md.as_str()),
        ("Basic.md", basic_md.as_str()),
        ("Odd.md", odd_md.as_str()),
        (
            "Collapsed.md",
            "---\n\nkanban-plugin: list\n\n---\n\n\
             ## To do\n\n- [ ] one\n\n\
             ## Done\n\n- [x] old\n\n\
             %% kanban:settings\n```\n\
             {\"kanban-plugin\":\"list\",\"list-collapse\":[false,true]}\n\
             ```\n%%\n",
        ),
        ("Empty.md", "---\n\nkanban-plugin: list\n\n---\n\n"),
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

fn fresh(tmp: &TempVault) -> (Runtime<State>, PathBuf, PathBuf) {
    let config = unique_dir("config");
    let state_home = unique_dir("state");
    let rt = runtime(state_at(tmp.path(), &config, &state_home));
    (rt, config, state_home)
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

/// Enter transitions settle before a press is placed on the card text; the
/// other kanban tests do the same.
fn settle(rt: &mut Runtime<State>) {
    for _ in 0..30 {
        frame(rt);
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
}

/// Select `title` in Write, then ⌘5.
fn show(rt: &mut Runtime<State>, title: &str) {
    let id = rt.state.vault.resolve(title).expect(title);
    rt.state.open_from_list(id);
    rt.key(command('5'));
    frame(rt);
}

fn has(rt: &Runtime<State>, key: &str) -> bool {
    rt.find(key).is_some()
}

fn text_of(rt: &Runtime<State>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
}

fn page_text(rt: &Runtime<State>) -> String {
    rt.elements()
        .into_iter()
        .filter_map(|e| e.text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every `.md` file's bytes, by name.
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(root)
        .expect("vault")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read(&path).expect("bytes"))
        })
        .collect();
    files.sort();
    files
}

#[test]
fn the_files_plugin_value_picks_the_view() {
    let tmp = views_vault();
    let (mut rt, _, _) = fresh(&tmp);

    show(&mut rt, "List");
    assert!(has(&rt, "kanban-list"), "list opens as a list");
    assert!(has(&rt, "kanban-list-scroll"));
    assert!(!has(&rt, "kanban-scroll"), "not the board");
    assert!(!has(&rt, "kanban-table-scroll"), "not the table");

    show(&mut rt, "Table");
    assert!(has(&rt, "kanban-table"));
    assert!(has(&rt, "kanban-table-scroll"));
    assert!(!has(&rt, "kanban-scroll"));

    // `basic` and a value with no rule keep slice 1's F-1 decision: the board.
    for title in ["Basic", "Odd"] {
        show(&mut rt, title);
        assert!(has(&rt, "kanban-scroll"), "{title} is the board");
        assert!(!has(&rt, "kanban-list"));
        assert!(!has(&rt, "kanban-table"));
    }
}

#[test]
fn list_and_table_show_the_same_lanes_and_cards() {
    let tmp = views_vault();
    let (mut rt, _, _) = fresh(&tmp);

    show(&mut rt, "List");
    assert_eq!(
        text_of(&rt, "kanban-list-lane-0-title").as_deref(),
        Some("To do")
    );
    assert_eq!(
        text_of(&rt, "kanban-list-lane-1-title").as_deref(),
        Some("Empty")
    );
    assert_eq!(
        text_of(&rt, "kanban-list-lane-2-title").as_deref(),
        Some("Done")
    );
    assert_eq!(
        text_of(&rt, "kanban-list-lane-0-count").as_deref(),
        Some("2")
    );
    assert_eq!(
        text_of(&rt, "kanban-list-lane-1-count").as_deref(),
        Some("0")
    );
    assert_eq!(
        text_of(&rt, "kanban-list-lane-1-empty").as_deref(),
        Some("No cards")
    );
    assert!(has(&rt, "kanban-list-lane-2-complete"), "a done lane");
    let list = page_text(&rt);
    for needle in [
        "Write",
        "with a second line",
        "Ship it",
        "漢字 😀 long line",
    ] {
        assert!(list.contains(needle), "list shows {needle:?}: {list}");
    }

    show(&mut rt, "Table");
    assert_eq!(
        text_of(&rt, "kanban-table-0-0-lane").as_deref(),
        Some("To do")
    );
    assert_eq!(
        text_of(&rt, "kanban-table-2-0-lane").as_deref(),
        Some("Done")
    );
    assert_eq!(
        text_of(&rt, "kanban-table-0-0-date").as_deref(),
        Some("2026-11-11")
    );
    assert_eq!(
        text_of(&rt, "kanban-table-0-0-tags").as_deref(),
        Some("#release")
    );
    assert_eq!(
        text_of(&rt, "kanban-table-2-0-progress").as_deref(),
        Some("1/2"),
        "the checklist's done/total"
    );
    assert_eq!(
        text_of(&rt, "kanban-table-lane-1-title").as_deref(),
        Some("Empty")
    );
    assert_eq!(
        text_of(&rt, "kanban-table-empty-lane").as_deref(),
        Some("No cards")
    );
    let flattened = text_of(&rt, "kanban-table-0-0-text").expect("card text");
    assert!(
        flattened.contains("with a second line"),
        "a multi-line card keeps both lines on one row: {flattened:?}"
    );
    assert!(!flattened.contains('\n'), "the row is one line");
    let long = text_of(&rt, "kanban-table-0-1-text").expect("long card");
    assert!(long.contains("漢字 😀"), "multibyte text is intact");
    assert!(!long.contains('\n'));
    // The table's header names every column.
    for (key, label) in [
        ("kanban-table-head-lane", "Lane"),
        ("kanban-table-head-card", "Card"),
        ("kanban-table-head-date", "Date"),
        ("kanban-table-head-tags", "Tags"),
        ("kanban-table-head-progress", "Progress"),
    ] {
        assert_eq!(text_of(&rt, key).as_deref(), Some(label), "{key}");
    }
}

#[test]
fn switching_views_keeps_every_byte_and_the_boards_cards() {
    let tmp = views_vault();
    let before = snapshot(tmp.path());
    let (mut rt, _, _) = fresh(&tmp);
    show(&mut rt, "Board");
    assert!(has(&rt, "kanban-scroll"));
    settle(&mut rt);

    click(&mut rt, "kanban-view-list");
    frame(&mut rt);
    assert!(has(&rt, "kanban-list"));
    assert!(page_text(&rt).contains("Write"), "the same cards");

    click(&mut rt, "kanban-view-table");
    frame(&mut rt);
    assert!(has(&rt, "kanban-table"));
    assert!(page_text(&rt).contains("Write"));

    click(&mut rt, "kanban-view-board");
    frame(&mut rt);
    assert!(has(&rt, "kanban-scroll"));
    assert!(has(&rt, "kanban-lane-0-title"), "the board is back");

    assert_eq!(
        rt.state.kanban_view(),
        muninn_core::KanbanView::Board,
        "the switcher shows the board"
    );
    assert!(
        !rt.state.kanban.as_ref().unwrap().dirty(),
        "switching views is not an edit"
    );
    // The other header controls are still there.
    for key in [
        "kanban-open-markdown",
        "kanban-archive",
        "kanban-filter-toggle",
    ] {
        assert!(has(&rt, key), "{key}");
    }
    drop(rt);
    assert_eq!(snapshot(tmp.path()), before, "no file changed");
}

#[test]
fn a_collapsed_lane_keeps_its_header_and_hides_its_cards() {
    let tmp = views_vault();
    let (mut rt, _, _) = fresh(&tmp);
    show(&mut rt, "Collapsed");
    assert!(has(&rt, "kanban-list-lane-1-title"));
    assert_eq!(
        text_of(&rt, "kanban-list-lane-1-collapsed").as_deref(),
        Some("Collapsed · 1 cards")
    );
    assert!(
        !has(&rt, "kanban-card-1-0"),
        "a collapsed lane's cards are hidden"
    );
    assert!(has(&rt, "kanban-card-0-0"), "the open lane still shows");

    click(&mut rt, "kanban-view-table");
    frame(&mut rt);
    assert!(has(&rt, "kanban-table-lane-1"));
    assert!(!has(&rt, "kanban-table-1-0"));
    assert!(
        text_of(&rt, "kanban-table-lane-note").is_some_and(|note| note.contains("Collapsed")),
        "the table says the lane is collapsed"
    );
}

#[test]
fn a_table_row_selects_its_card_and_its_link_opens_the_note() {
    let tmp = views_vault();
    let (mut rt, _, _) = fresh(&tmp);
    show(&mut rt, "Table");
    settle(&mut rt);
    click(&mut rt, "kanban-table-0-0");
    frame(&mut rt);
    assert_eq!(rt.state.kanban_selected, Some((0, 0)));
    assert_eq!(rt.state.mode, Mode::Kanban, "selecting stays on the table");

    click(&mut rt, "kanban-table-0-0-open");
    frame(&mut rt);
    assert_eq!(
        rt.state.selected_path,
        Some(tmp.path().join("Roadmap.md")),
        "the row's link opens the note"
    );
    assert_ne!(rt.state.mode, Mode::Kanban);
}

#[test]
fn the_checkbox_still_toggles_done_from_the_table() {
    let tmp = views_vault();
    let before = snapshot(tmp.path());
    let (mut rt, _, _) = fresh(&tmp);
    show(&mut rt, "Table");
    settle(&mut rt);
    click(&mut rt, "kanban-card-0-0-mark");
    frame(&mut rt);
    let board = rt.state.kanban.as_ref().unwrap().board.as_ref().unwrap();
    assert!(board.lanes()[0].cards()[0].is_done());
    assert!(rt.state.kanban.as_ref().unwrap().dirty());
    assert_eq!(snapshot(tmp.path()), before, "nothing is written yet");
}

#[test]
fn an_empty_board_says_so_in_both_views() {
    let tmp = views_vault();
    let (mut rt, _, _) = fresh(&tmp);
    show(&mut rt, "Empty");
    assert_eq!(
        text_of(&rt, "kanban-list-empty-text").as_deref(),
        Some("No lanes on this board")
    );
    click(&mut rt, "kanban-view-table");
    frame(&mut rt);
    assert_eq!(
        text_of(&rt, "kanban-table-empty-text").as_deref(),
        Some("No lanes on this board")
    );
}

#[test]
fn another_board_opens_in_its_own_view_and_the_switcher_is_not_saved() {
    let tmp = views_vault();
    let (mut rt, config, state_home) = fresh(&tmp);
    show(&mut rt, "Board");
    click(&mut rt, "kanban-view-list");
    frame(&mut rt);
    assert!(has(&rt, "kanban-list"));

    // Another kanban opens in its file's view, not the one just picked.
    show(&mut rt, "Table");
    assert!(has(&rt, "kanban-table"));
    show(&mut rt, "Board");
    assert!(has(&rt, "kanban-scroll"));

    // A restart forgets the switch too: Board.md is a board.
    drop(rt);
    let rt = runtime(state_at(tmp.path(), &config, &state_home));
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert!(has(&rt, "kanban-scroll"));
    assert!(!has(&rt, "kanban-list"));
}

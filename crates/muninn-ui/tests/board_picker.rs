//! Issue #188: the Board page's header picker replaced its 240 pt list rail.
//!
//! It opens from `Boards · N`, lists every board with its element counts, marks
//! the open one, selects on click, makes a board from its footer, closes on
//! Escape or a press outside, roves with Up/Down and presses with Enter, and
//! gives the button its focus back. Headless.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};
use serde_json::Value;

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");
const SCENE: &str = include_str!("../../muninn-core/tests/fixtures/board/scene.excalidraw");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-picker-{tag}-{}-{}",
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
        ("Alpha.md", "# Alpha\n"),
        ("A.excalidraw", DRAW),
        ("B.excalidraw", SCENE),
    ])
}

fn launch(root: &Path) -> Runtime<State> {
    let config = unique_dir("config");
    let state_home = unique_dir("state");
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

fn plain(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
}

fn press_key(rt: &mut Runtime<State>, key: Key) {
    rt.key(plain(key));
    frames(rt, 2);
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 3);
}

fn on_board(rt: &mut Runtime<State>) {
    rt.key(KeyEvent {
        key: Key::Command('4'),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    frames(rt, 3);
    assert_eq!(rt.state.mode, Mode::Board);
}

fn element(rt: &Runtime<State>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("missing {key}"))
}

fn focused(rt: &Runtime<State>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused && !e.exiting)
        .and_then(|e| e.key)
}

fn a11y<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    if v["key"] == key {
        return Some(v);
    }
    v["children"]
        .as_array()?
        .iter()
        .find_map(|child| a11y(child, key))
}

fn states(v: &Value) -> Vec<&str> {
    v["states"].as_array().map_or(Vec::new(), |states| {
        states.iter().filter_map(Value::as_str).collect()
    })
}

fn board_index(rt: &Runtime<State>, title: &str) -> usize {
    rt.state
        .boards
        .iter()
        .position(|b| b.file.title == title)
        .unwrap_or_else(|| panic!("no board {title}"))
}

#[test]
fn the_header_button_opens_a_named_list_that_marks_the_open_board() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    on_board(&mut rt);

    // The button replaces the old "Boards · N" text pair: a named, expandable
    // button with the catalog title inside it.
    let button = element(&rt, "board-picker");
    assert_eq!(button.role, "button");
    assert_eq!(button.label.as_deref(), Some("Choose a board"));
    let tree = rt.accessibility_json();
    let node = a11y(&tree, "board-picker").expect("the button in the a11y tree");
    assert!(states(node).contains(&"collapsed"), "{:?}", node["states"]);
    assert!(
        rt.find("board-list-empty").is_none() && rt.find("board-row-0").is_none(),
        "the list is closed at first"
    );

    // Open it: every board is a named option, and the open one is selected.
    rt.state.open_board_at(rt.state.boards[0].file.path.clone());
    frames(&mut rt, 10);
    click(&mut rt, "board-picker");
    let tree = rt.accessibility_json();
    // The open list is a modal, so the button behind it is inert and pruned
    // from the a11y tree; the open flag is the state the button reports.
    assert!(rt.state.board_picker.is_some());
    assert!(element(&rt, "board-picker").label.is_some());
    assert_eq!(element(&rt, "board-picker-list").role, "listbox");
    assert_eq!(element(&rt, "board-picker-rows").role, "group");
    for i in 0..2 {
        let row = element(&rt, &format!("board-row-{i}"));
        assert_eq!(row.role, "option", "row {i}");
        assert!(
            row.label
                .as_deref()
                .is_some_and(|label| label.contains("elements")),
            "row {i} is named with its counts: {:?}",
            row.label
        );
        let node = a11y(&tree, &format!("board-row-{i}")).expect("row in the tree");
        assert_eq!(
            states(node).contains(&"selected"),
            i == 0,
            "row {i}: {:?}",
            node["states"]
        );
    }
    assert!(
        element(&rt, "board-row-0-detail")
            .text
            .is_some_and(|text| text.contains("elements")),
        "the row keeps its detail line"
    );
    // It floats under the header, narrower than the page.
    frames(&mut rt, 30);
    let list = element(&rt, "board-picker-list").rect;
    let button = element(&rt, "board-picker").rect;
    assert_eq!(list[2], 280.0, "{list:?}");
    assert!(
        list[1] >= button[1] + button[3],
        "the list sits under the button: {list:?} {button:?}"
    );
    assert!(
        (list[0] - button[0]).abs() <= 8.0,
        "and starts at its left edge: {list:?} {button:?}"
    );
}

#[test]
fn selecting_a_row_closes_the_list_and_opens_that_board() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    on_board(&mut rt);
    click(&mut rt, "board-picker");
    let i = board_index(&rt, "B");
    click(&mut rt, &format!("board-row-{i}"));
    assert_eq!(rt.state.board_picker, None, "a pick closes it");
    assert!(
        rt.state
            .board_selected
            .as_ref()
            .is_some_and(|p| p.ends_with("B.excalidraw"))
    );
    assert!(rt.find("board-row-0").is_none());
    assert_eq!(
        element(&rt, "board-surface-title").text.as_deref(),
        Some("B")
    );
}

#[test]
fn up_down_and_enter_pick_a_board_and_escape_gives_the_button_its_focus() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    on_board(&mut rt);
    click(&mut rt, "board-picker");
    // The list opens on the first row (the open board, or the first); the
    // panel holds the keyboard and Down moves the highlight to the second.
    assert_eq!(rt.state.board_picker, Some(0));
    assert_eq!(focused(&rt).as_deref(), Some("board-picker-list"));
    press_key(&mut rt, Key::Down);
    assert_eq!(rt.state.board_picker, Some(1));
    press_key(&mut rt, Key::Enter);
    assert_eq!(rt.state.board_picker, None, "Enter closes the list");
    assert!(
        rt.state
            .board_selected
            .as_ref()
            .is_some_and(|p| p.ends_with("B.excalidraw")),
        "Enter opened the focused board"
    );

    // Escape closes without selecting and hands the button the keyboard.
    click(&mut rt, "board-picker");
    press_key(&mut rt, Key::Escape);
    assert_eq!(rt.state.board_picker, None);
    assert_eq!(focused(&rt).as_deref(), Some("board-picker"));

    // A press outside closes it too.
    click(&mut rt, "board-picker");
    rt.pointer_down(600.0, 500.0);
    rt.pointer_up(600.0, 500.0);
    frames(&mut rt, 2);
    assert_eq!(rt.state.board_picker, None, "a press outside closes it");
}

#[test]
fn the_footer_makes_untitled_boards_in_order() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", DRAW)]);
    let mut rt = launch(tmp.path());
    on_board(&mut rt);
    click(&mut rt, "board-picker");
    click(&mut rt, "board-picker-new");
    let first = tmp.path().join("Untitled board.excalidraw");
    assert!(first.is_file(), "{:?}", fs::read_dir(tmp.path()));
    assert_eq!(
        rt.state.board_open.as_ref().map(|o| o.path.clone()),
        Some(first)
    );
    assert_eq!(rt.state.board_picker, None, "making one closes the list");

    click(&mut rt, "board-picker");
    click(&mut rt, "board-picker-new");
    assert!(tmp.path().join("Untitled board 2.excalidraw").is_file());
    assert_eq!(
        rt.state.board_open.as_ref().map(|o| o.path.clone()),
        Some(tmp.path().join("Untitled board 2.excalidraw"))
    );
    assert_eq!(rt.state.boards.len(), 3);
    // Both lists follow the re-list.
    assert_eq!(rt.state.sidebar_boards.len(), 3);
}

#[test]
fn an_empty_vault_says_no_boards_once_and_still_offers_new_board() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut rt = launch(tmp.path());
    on_board(&mut rt);
    assert!(
        rt.find("board-placeholder-title").is_some(),
        "the placeholder says there are no boards"
    );
    assert!(
        rt.find("board-surface-title").is_none(),
        "and no heading repeats it"
    );
    assert!(
        rt.find("board-new").is_some(),
        "one New board in the header"
    );
    assert!(
        rt.find("board-new-empty").is_none(),
        "not a second one in the placeholder"
    );

    click(&mut rt, "board-picker");
    assert!(
        rt.find("board-list-empty").is_some(),
        "the open picker says it once"
    );
    assert!(rt.find("board-row-0").is_none(), "there is no row to list");
    click(&mut rt, "board-picker-new");
    assert!(tmp.path().join("Untitled board.excalidraw").is_file());
    assert!(rt.find("board-canvas").is_some(), "and opens it");
}

/// A board's right-click rename in the notes sidebar still works beside the
/// picker's rows, which intentionally have no menu of their own.
#[test]
fn picker_rows_do_not_float_a_menu_over_the_list() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    on_board(&mut rt);
    click(&mut rt, "board-picker");
    let (x, y) = rt.find("board-row-0").expect("a row");
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    frames(&mut rt, 12);
    assert!(
        rt.state.menu.is_none(),
        "the picker row has no context menu"
    );
    assert_eq!(rt.state.mode, Mode::Board);
}

//! #72: the Board toolbar and canvas accessibility pass.
//!
//! The toolbar is a named `toolbar` of toggle buttons: each has a role, a
//! name, help with its letter, a focus ring and a 30 pt target, so Tab reaches
//! it and Space picks a tool. A press still blurs, which keeps the letter keys
//! on the canvas. The canvas is a named `canvas` whose name carries the
//! element count, the picked tool and the selection.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{BoardTool, Settings, State, on_key, poll, view};
use rune::{ElementInfo, Env, Key, KeyEvent, Mods, Runtime};
use serde_json::Value;

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-a11y-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn vault() -> TempVault {
    TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", DRAW)])
}

fn on_board(tmp: &TempVault) -> Runtime<State> {
    let (config, state_home) = (unique_dir("config"), unique_dir("state"));
    let state = State::with_settings(
        tmp.path().to_path_buf(),
        state_home,
        config.clone(),
        Settings::load_from(&config),
    );
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Command('4'),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    frames(&mut rt, 2);
    let row = board_row(&rt, "Draw");
    rt.pointer_down(row.0, row.1);
    rt.pointer_up(row.0, row.1);
    frames(&mut rt, 60);
    assert_eq!(rt.state.mode, muninn_ui::Mode::Board);
    rt
}

fn board_row(rt: &Runtime<State>, title: &str) -> (f32, f32) {
    let i = rt
        .state
        .boards
        .iter()
        .position(|b| b.file.title == title)
        .unwrap_or_else(|| panic!("no board {title}"));
    rt.find(&format!("board-row-{i}"))
        .unwrap_or_else(|| panic!("no row for {title}"))
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

fn act(rt: &mut Runtime<State>, f: impl FnOnce(&mut State)) {
    rt.update(|s, _| f(s));
    frame(rt);
}

fn press(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    frame(rt);
}

/// Tab until `key` has the keyboard.
fn tab_to(rt: &mut Runtime<State>, key: &str) {
    for _ in 0..160 {
        if focused(rt).as_deref() == Some(key) {
            return;
        }
        press(rt, Key::Tab);
    }
    panic!("Tab never reached {key}; focused {:?}", focused(rt));
}

fn element(rt: &Runtime<State>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("missing element {key}"))
}

fn focused(rt: &Runtime<State>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused && !e.exiting)
        .and_then(|e| e.key)
}

fn a11y(rt: &Runtime<State>) -> Value {
    rt.accessibility_json()
}

fn a11y_find<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    if v["key"] == key {
        return Some(v);
    }
    v["children"]
        .as_array()?
        .iter()
        .find_map(|child| a11y_find(child, key))
}

fn a11y_states(v: &Value) -> Vec<&str> {
    v["states"].as_array().map_or(Vec::new(), |states| {
        states.iter().filter_map(Value::as_str).collect()
    })
}

/// The toolbar buttons, in draw order: key, name, and the letter in help.
const BUTTONS: [(&str, &str, &str); 9] = [
    ("board-tool-hand", "Hand", "H"),
    ("board-tool-select", "Select", "V"),
    ("board-tool-rectangle", "Rectangle", "R"),
    ("board-tool-diamond", "Diamond", "D"),
    ("board-tool-ellipse", "Ellipse", "O"),
    ("board-tool-arrow", "Arrow", "A"),
    ("board-tool-line", "Line", "L"),
    ("board-tool-freedraw", "Freehand", "P"),
    ("board-tool-text", "Text", "T"),
];

#[test]
fn board_a11y_toolbar_buttons_are_named_focusable_targets_with_help() {
    let tmp = vault();
    let rt = on_board(&tmp);

    // The bar is a named toolbar, not an anonymous row of icons.
    let toolbar = element(&rt, "board-toolbar");
    assert_eq!(toolbar.role, "toolbar");
    assert_eq!(toolbar.label.as_deref(), Some("Board tools"));

    for (key, label, letter) in BUTTONS {
        let button = element(&rt, key);
        assert_eq!(button.role, "button", "{key}");
        assert!(button.focusable, "{key} is a Tab stop");
        assert_eq!(button.label.as_deref(), Some(label), "{key}");
        assert!(
            button
                .help
                .as_deref()
                .is_some_and(|help| help.contains(letter)),
            "{key} help names its key: {:?}",
            button.help
        );
        assert!(
            button.rect[2] >= 24.0 && button.rect[3] >= 24.0,
            "{key} is a 24 pt target: {:?}",
            button.rect
        );
    }

    // The picker reports the current tool as a pressed toggle button.
    let tree = a11y(&rt);
    let select = a11y_find(&tree, "board-tool-select").expect("select in the tree");
    assert!(
        a11y_states(select).contains(&"checked"),
        "Select is the default tool: {:?}",
        select["states"]
    );
    let rectangle = a11y_find(&tree, "board-tool-rectangle").expect("rectangle in the tree");
    assert!(a11y_states(rectangle).contains(&"unchecked"));
}

#[test]
fn board_a11y_tab_reaches_a_tool_button_and_space_picks_it_without_losing_the_letter_keys() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    assert_eq!(rt.state.board_tool, None, "Select is picked at first");

    tab_to(&mut rt, "board-tool-rectangle");
    let button = element(&rt, "board-tool-rectangle");
    assert!(button.focus_visible, "the focus ring shows for Tab");

    // Space presses the focused button; the press selects the tool and hands
    // the keyboard back to the canvas (the existing blur).
    press(&mut rt, Key::Text(" ".to_owned()));
    assert_eq!(rt.state.board_tool, Some(BoardTool::Rectangle));
    assert_eq!(focused(&rt), None, "the tool press blurs to the canvas");

    // Letter keys still pick tools after a toolbar press.
    press(&mut rt, Key::Text("d".to_owned()));
    assert_eq!(rt.state.board_tool, Some(BoardTool::Diamond));
}

#[test]
fn board_a11y_canvas_is_named_with_its_tool_and_selection() {
    let tmp = vault();
    let mut rt = on_board(&tmp);

    let canvas = element(&rt, "board-canvas");
    assert_eq!(canvas.role, "canvas");
    let label = canvas.label.expect("the canvas is named");
    assert!(
        label.starts_with("Board canvas,"),
        "a screen reader hears what it is: {label}"
    );
    assert!(label.contains("elements"), "the count is named: {label}");
    assert!(label.contains("select tool"), "the tool is named: {label}");
    assert!(
        !label.contains("selected"),
        "nothing is selected on open: {label}"
    );

    act(&mut rt, |s| s.set_board_tool(Some(BoardTool::Ellipse)));
    let label = element(&rt, "board-canvas").label.expect("named");
    assert!(label.contains("ellipse tool"), "the tool follows: {label}");

    act(&mut rt, State::board_select_all);
    let selected = rt.state.board_selection().len();
    assert!(selected > 0, "the fixture has elements to select");
    let label = element(&rt, "board-canvas").label.expect("named");
    assert!(
        label.contains(&format!("{selected} selected")),
        "the selection is announced: {label}"
    );
}

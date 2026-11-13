//! Kanban slice 16: the page's keyboard and accessibility pass.
//!
//! The controls are Tab stops with roles and names, the cards a list whose
//! rows rove with Up/Down, Space/Enter toggle a card's done mark, Shift+F10
//! opens the focused card's menu, the conflict bar is an alert dialog that
//! Escape dismisses before the page, and a closed editor or menu gives the
//! keyboard back to the card or lane that opened it.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{DocConflict, MenuTarget, Mode, Settings, State, on_key, poll, view};
use rune::{ElementInfo, Env, Key, KeyEvent, Mods, Runtime};
use serde_json::Value;

const BOARD: &str = "\
---
kanban-plugin: board
---

## To do

- [ ] Ship the a11y pass
- [x] Read the audit

## Doing

- [ ] Check the menus
";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-a11y-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn vault() -> TempVault {
    TempVault::new(&[("Agenda.md", "# Agenda\n"), ("Board.md", BOARD)])
}

fn on_board(tmp: &TempVault) -> Runtime<State> {
    let (config, state_home) = (unique_dir("config"), unique_dir("state"));
    let state = State::with_settings(
        tmp.path().to_path_buf(),
        state_home,
        config.clone(),
        Settings {
            kanban_autosave: false,
            ..Settings::load_from(&config)
        },
    );
    let mut rt = Runtime::new(state, view, (1400.0, 900.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    let id = rt.state.vault.resolve("Board").expect("Board");
    rt.update(|s, _| s.open_kanban(id));
    frames(&mut rt, 2);
    rt
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

fn click(rt: &mut Runtime<State>, key: &str) {
    for _ in 0..20 {
        frame(rt);
    }
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frame(rt);
}

fn press(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
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

fn element(rt: &Runtime<State>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("missing element {key}"))
}

fn has(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
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

#[test]
fn main_controls_report_roles_names_focusability_and_state() {
    let tmp = vault();
    let rt = on_board(&tmp);
    let first = "Ship the a11y pass";

    // The page is a main landmark; the lanes are regions and the cards a list.
    assert_eq!(element(&rt, "kanban-page").role, "main");
    let lane = element(&rt, "kanban-lane-0");
    assert_eq!(lane.role, "region");
    assert!(
        lane.label.as_deref().is_some_and(|l| l.contains("To do")),
        "the lane is named: {:?}",
        lane.label
    );
    let cards = element(&rt, "kanban-lane-0-cards");
    assert_eq!(cards.role, "list");

    // A card row is a selectable list item, a Tab stop, and named by its text.
    let card = element(&rt, "kanban-card-0-0");
    assert_eq!(card.role, "listitem");
    assert!(card.focusable);
    assert!(
        card.label.as_deref().is_some_and(|l| l.contains(first)),
        "the card is named by its text: {:?}",
        card.label
    );

    // The checkbox says the card's text, and the state says done.
    let mark = element(&rt, "kanban-card-0-0-mark");
    assert_eq!(mark.role, "checkbox");
    assert!(mark.focusable);
    assert_eq!(mark.label.as_deref(), Some(first));
    assert_eq!(mark.help.as_deref(), Some("Mark done"));
    let tree = a11y(&rt);
    let checked = a11y_find(&tree, "kanban-card-0-1-mark").expect("the done card's mark");
    assert!(
        a11y_states(checked).contains(&"checked"),
        "a done card's box is checked: {:?}",
        checked["states"]
    );
    let unchecked = a11y_find(&tree, "kanban-card-0-0-mark").expect("the open card's mark");
    assert!(a11y_states(unchecked).contains(&"unchecked"));

    // Every icon and header button is focusable, named and has help.
    for (key, label) in [
        ("kanban-card-0-0-menu", "Card actions: Ship the a11y pass"),
        ("kanban-lane-0-menu", "Lane actions: To do"),
        ("kanban-lane-0-collapse", "Collapse lane: To do"),
        ("kanban-filter-toggle", "Filter"),
        ("kanban-archive", "Archive"),
        ("kanban-open-markdown", "Open as markdown"),
        ("kanban-add-lane", "Add lane"),
        ("kanban-lane-0-add", "Add card to To do"),
    ] {
        let button = element(&rt, key);
        assert_eq!(button.role, "button", "{key}");
        assert!(button.focusable, "{key} is a Tab stop");
        assert_eq!(button.label.as_deref(), Some(label), "{key}");
        assert_eq!(button.help.as_deref(), Some(label), "{key} has help");
    }

    // The view switcher is a tab list; the current view is the selected tab.
    let board_tab = element(&rt, "kanban-view-board");
    assert_eq!(board_tab.role, "tab");
    assert!(board_tab.focusable);
    assert!(board_tab.item_selected, "the current view is selected");
    assert!(!element(&rt, "kanban-view-list").item_selected);

    // The save status is a status region, not bare text.
    assert_eq!(element(&rt, "kanban-status").role, "status");
}

#[test]
fn space_and_enter_toggle_the_selected_card() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.select_kanban_card(0, 0));
    assert!(!source(&rt).contains("- [x] Ship the a11y pass"));

    press(&mut rt, Key::Text(" ".to_owned()));
    assert!(
        source(&rt).contains("- [x] Ship the a11y pass"),
        "Space marks the selected card done: {}",
        source(&rt)
    );
    press(&mut rt, Key::Enter);
    assert!(
        source(&rt).contains("- [ ] Ship the a11y pass"),
        "Enter marks it open again: {}",
        source(&rt)
    );

    // A focused card owns the keys itself, after a press focused it.
    click(&mut rt, "kanban-card-0-0");
    assert_eq!(focused(&rt).as_deref(), Some("kanban-card-0-0"));
    press(&mut rt, Key::Text(" ".to_owned()));
    assert!(
        source(&rt).contains("- [x] Ship the a11y pass"),
        "Space on the focused card toggles it: {}",
        source(&rt)
    );
    press(&mut rt, Key::Enter);
    assert!(
        source(&rt).contains("- [ ] Ship the a11y pass"),
        "Enter on the focused card toggles it back: {}",
        source(&rt)
    );
    assert_eq!(rt.state.mode, Mode::Kanban);
}

#[test]
fn the_menu_key_opens_the_focused_cards_menu_and_escape_returns_focus() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-card-0-0");
    assert_eq!(focused(&rt).as_deref(), Some("kanban-card-0-0"));

    rt.key(KeyEvent {
        key: Key::Function(10),
        mods: Mods {
            shift: true,
            ..Mods::default()
        },
    });
    frame(&mut rt);
    assert_eq!(
        rt.state.menu.as_ref().map(|m| m.target.clone()),
        Some(MenuTarget::Card(0, 0)),
        "Shift+F10 opened the focused card's menu"
    );

    // The menu is a menu of menu items, each a Tab stop.
    let menu = element(&rt, "context-menu");
    assert_eq!(menu.role, "menu");
    assert_eq!(menu.label.as_deref(), Some("Card actions"));
    let item = element(&rt, "menu-edit");
    assert_eq!(item.role, "menuitem");
    assert!(item.focusable);
    assert_eq!(item.label.as_deref(), Some("Edit"));
    let cancel = element(&rt, "menu-cancel");
    assert_eq!(cancel.role, "menuitem");
    assert!(cancel.focusable);

    press(&mut rt, Key::Escape);
    assert!(rt.state.menu.is_none(), "Escape closed the menu");
    assert_eq!(rt.state.mode, Mode::Kanban, "the page stays");
    assert_eq!(
        focused(&rt).as_deref(),
        Some("kanban-card-0-0"),
        "the menu gave the keyboard back to its card"
    );
}

#[test]
fn escape_dismisses_the_conflict_bar_before_leaving_the_page() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.toggle_card_done(0, 0));
    fs::write(
        tmp.path().join("Board.md"),
        BOARD.replace("- [ ] Check the menus", "- [ ] Theirs"),
    )
    .unwrap();
    act(&mut rt, |s| s.refresh_kanban());
    assert_eq!(
        rt.state.kanban.as_ref().and_then(|k| k.conflict),
        Some(DocConflict::Changed)
    );
    frame(&mut rt);

    // The bar is an alert dialog whose sentence is an alert.
    let bar = element(&rt, "kanban-conflict");
    assert_eq!(bar.role, "alertdialog");
    assert_eq!(bar.label.as_deref(), Some("Changed on disk by another app"));
    assert_eq!(element(&rt, "kanban-conflict-text").role, "alert");
    for (key, label) in [
        ("kanban-keep-mine", "Keep mine"),
        ("kanban-save-copy", "Save copy"),
        ("kanban-load-theirs", "Load theirs"),
    ] {
        let choice = element(&rt, key);
        assert_eq!(choice.role, "button");
        assert!(choice.focusable, "{key} is reachable");
        assert_eq!(choice.label.as_deref(), Some(label));
    }

    // Escape puts the bar away; the page and the conflict stay.
    press(&mut rt, Key::Escape);
    assert_eq!(rt.state.mode, Mode::Kanban, "Escape did not leave the page");
    assert!(!has(&rt, "kanban-conflict"), "the bar is away");
    assert_eq!(
        rt.state.kanban.as_ref().and_then(|k| k.conflict),
        Some(DocConflict::Changed),
        "the conflict still blocks saves"
    );
    let review = element(&rt, "kanban-conflict-review");
    assert_eq!(review.role, "button");
    assert!(review.focusable);
    assert_eq!(review.label.as_deref(), Some("Resolve conflict"));

    // Resolve conflict brings the bar back; the next Escape leaves the page.
    click(&mut rt, "kanban-conflict-review");
    assert!(has(&rt, "kanban-conflict"), "the bar is back");
    press(&mut rt, Key::Escape);
    assert!(!has(&rt, "kanban-conflict"));
    press(&mut rt, Key::Escape);
    assert_eq!(rt.state.mode, Mode::Editor, "nothing else is left to peel");
}

#[test]
fn closing_the_editor_or_lane_field_returns_focus_to_its_control() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.begin_edit_card(0, 0));
    for _ in 0..2 {
        frame(&mut rt);
    }
    assert!(has(&rt, "kanban-card-editor"));
    // The editor's field is the one text input on the page.
    assert_eq!(
        focused(&rt).as_deref(),
        Some("input"),
        "the editor owns the keyboard"
    );
    press(&mut rt, Key::Escape);
    assert!(!has(&rt, "kanban-card-editor"));
    assert_eq!(
        focused(&rt).as_deref(),
        Some("kanban-card-0-0"),
        "the card has the keyboard back"
    );

    // A lane title field gives the keyboard back to the lane's menu button.
    act(&mut rt, |s| s.begin_rename_lane(0));
    for _ in 0..2 {
        frame(&mut rt);
    }
    assert_eq!(focused(&rt).as_deref(), Some("kanban-lane-field"));
    press(&mut rt, Key::Escape);
    assert!(!has(&rt, "kanban-lane-field"));
    assert_eq!(
        focused(&rt).as_deref(),
        Some("kanban-lane-0-menu"),
        "the lane's control has the keyboard back"
    );
}

#[test]
fn a_press_elsewhere_clears_an_open_fields_keyboard() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.begin_edit_card(0, 0));
    for _ in 0..2 {
        frame(&mut rt);
    }
    assert!(rt.state.is_typing(), "the editor owns the keyboard");
    click(&mut rt, "kanban-card-0-1");
    assert!(
        !rt.state.kanban_card_field.focused,
        "a press on another card cleared the field's focus"
    );
    assert!(
        rt.state.kanban_card.is_some(),
        "the editor stays open with its text"
    );
    assert!(!rt.state.is_typing(), "the page owns the keyboard again");
    assert_eq!(rt.state.kanban_selected, Some((0, 1)), "the press selected");
    press(&mut rt, Key::Up);
    assert_eq!(
        rt.state.kanban_selected,
        Some((0, 0)),
        "the page's arrows work again"
    );
}

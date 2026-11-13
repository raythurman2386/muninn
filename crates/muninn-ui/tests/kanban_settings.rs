//! Kanban slice 14: the Board settings panel and Settings › Kanban.
//!
//! Both surfaces are generated from the same schema, so the same key changes
//! in the app-wide `kanban_defaults` from the modal tab and in the board's
//! footer from the page panel. Reset clears the one place it belongs to. Lane
//! width and the new-line trigger take effect at once, hidden header buttons
//! are gone, and opening or closing a surface writes nothing.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Settings, State, on_key, poll, view};
use rune::{Env, Ime, Key, KeyEvent, Mods, Runtime};

const BOARD: &str = "\
---
kanban-plugin: board
---

## To do

- [ ] first card
";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-settings-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

struct Fixture {
    rt: Runtime<State>,
    config: PathBuf,
    _vault: TempVault,
}

impl Fixture {
    fn settings_json(&self) -> String {
        fs::read_to_string(self.config.join("muninn/settings.json")).unwrap_or_default()
    }

    fn board(&self) -> &muninn_core::Kanban {
        self.rt
            .state
            .kanban
            .as_ref()
            .and_then(|shown| shown.board.as_ref())
            .expect("a board")
    }

    fn source(&self) -> String {
        self.board().source().to_owned()
    }

    fn footer(&self) -> serde_json::Value {
        self.board()
            .footer_json()
            .map(|json| serde_json::from_str(json).expect("footer json"))
            .unwrap_or_else(|| serde_json::json!({}))
    }

    fn disk(&self) -> String {
        fs::read_to_string(self._vault.path().join("Board.md")).expect("Board.md")
    }
}

fn on_board(tag: &str, board: &str) -> Fixture {
    let vault = TempVault::new(&[("Board.md", board)]);
    let config = unique_dir(&format!("{tag}-config"));
    let state_home = unique_dir(&format!("{tag}-state"));
    let state = State::with_settings(
        vault.path().to_path_buf(),
        state_home.clone(),
        config.clone(),
        Settings {
            kanban_autosave: false,
            ..Settings::load_from(&config)
        },
    );
    let mut rt = Runtime::new(state, view, (1600.0, 3200.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    let id = rt.state.vault.resolve("Board").expect("Board");
    rt.update(|s, _| s.open_kanban(id));
    rt.frame(1.0 / 60.0);
    Fixture {
        rt,
        config,
        _vault: vault,
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
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is on screen"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..4 {
        frame(rt);
    }
}

fn key(rt: &mut Runtime<State>, key: Key, shift: bool) {
    rt.key(KeyEvent {
        key,
        mods: Mods {
            shift,
            ..Mods::default()
        },
    });
    frame(rt);
}

fn open_settings(rt: &mut Runtime<State>) {
    rt.key(KeyEvent {
        key: Key::Command(','),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    for _ in 0..8 {
        frame(rt);
    }
    assert!(rt.state.settings_modal);
}

fn has(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|element| element.key.as_deref() == Some(key) && !element.exiting)
}

#[test]
fn the_global_tab_and_the_board_panel_write_the_same_key_in_the_right_place() {
    let mut f = on_board("same-key", BOARD);
    let disk = f.disk();
    open_settings(&mut f.rt);
    click(&mut f.rt, "settings-tabs/segment5");
    assert_eq!(f.rt.state.settings_tab, muninn_ui::SettingsTab::Kanban);

    click(&mut f.rt, "kanban-setting-show-relative-date-switch");
    assert!(
        f.rt.state.kanban_app_effective().dates.show_relative_date,
        "the modal writes the app-wide default"
    );
    assert!(f.rt.state.kanban_app_sets("show-relative-date"));
    assert!(f.settings_json().contains("\"show-relative-date\": true"));
    assert_eq!(f.source(), disk, "the board is not touched");
    assert!(!f.rt.state.kanban_dirty());

    act(&mut f.rt, State::close_settings);
    click(&mut f.rt, "kanban-settings");
    assert!(
        f.rt.state.kanban_settings_open,
        "the page has a Board settings panel"
    );
    assert!(
        f.rt.elements()
            .iter()
            .filter_map(|element| element.text.as_deref())
            .any(|label| label == "App default"),
        "the control says where its value came from"
    );

    click(&mut f.rt, "kanban-setting-show-relative-date-switch");
    assert_eq!(
        f.footer()["show-relative-date"],
        false,
        "toggling the app default true writes a false board override"
    );
    assert!(!f.rt.state.kanban_date_settings().show_relative_date);

    click(&mut f.rt, "kanban-reset-board-show-relative-date");
    assert!(
        f.footer().get("show-relative-date").is_none(),
        "Reset board removes only the footer key"
    );
    assert!(
        f.rt.state.kanban_date_settings().show_relative_date,
        "the app default still applies"
    );

    click(&mut f.rt, "kanban-reset-app-show-relative-date");
    assert!(
        !f.rt.state.kanban_app_sets("show-relative-date"),
        "Reset app default clears the app-wide key"
    );
    assert!(
        !f.rt.state.kanban_date_settings().show_relative_date,
        "the built-in value is back"
    );
}

#[test]
fn lane_width_changes_at_once_and_resets() {
    let mut f = on_board("lane-width", BOARD);
    click(&mut f.rt, "kanban-settings");
    let before = f.board().lanes().len();
    assert!(before > 0);
    click(&mut f.rt, "kanban-setting-lane-width-plus");
    assert_eq!(f.footer()["lane-width"], 280);
    assert_eq!(f.rt.state.kanban_board_settings().lane_width, 280);

    click(&mut f.rt, "kanban-settings-close");
    let lane =
        f.rt.elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some("kanban-lane-0"))
            .expect("lane 0 is drawn");
    assert!(
        (lane.rect[2] - 280.0).abs() < 1.0,
        "the lane is drawn 280 points wide, got {}",
        lane.rect[2]
    );

    click(&mut f.rt, "kanban-settings");
    click(&mut f.rt, "kanban-reset-board-lane-width");
    assert!(f.footer().get("lane-width").is_none());
    assert_eq!(f.rt.state.kanban_board_settings().lane_width, 272);
}

#[test]
fn shift_enter_as_the_new_line_trigger_breaks_the_line_and_shift_enter_commits() {
    let board = "\
---
kanban-plugin: board
---

## To do

- [ ] first

%% kanban:settings
```
{\"kanban-plugin\":\"board\",\"new-line-trigger\":\"shift+enter\"}
```
%%
";
    let mut f = on_board("new-line", board);
    act(&mut f.rt, |s| s.begin_edit_card(0, 0));
    assert!(f.rt.state.kanban_card.is_some(), "the card editor is open");
    act(&mut f.rt, |s| s.kanban_card_field.replace_text("first"));

    key(&mut f.rt, Key::Enter, false);
    assert!(
        f.rt.state.kanban_card.is_some(),
        "a plain Enter breaks the line"
    );
    assert!(
        f.rt.state.kanban_card_field.text.ends_with('\n'),
        "the line break is in the field"
    );
    f.rt.ime(Ime::Commit("second".to_owned()));
    frame(&mut f.rt);

    key(&mut f.rt, Key::Enter, true);
    assert!(
        f.rt.state.kanban_card.is_none(),
        "Shift+Enter commits the card"
    );
    let text = f.board().lanes()[0].cards()[0].text();
    assert_eq!(text, "first\nsecond");
}

#[test]
fn hidden_header_buttons_are_gone_and_board_settings_stays() {
    let board = "\
---
kanban-plugin: board
---

## To do

- [ ] one

%% kanban:settings
```
{\"kanban-plugin\":\"board\",\"show-filter-button\":false,\"show-open-markdown-button\":false}
```
%%
";
    let f = on_board("header-buttons", board);
    assert!(!has(&f.rt, "kanban-filter-toggle"), "Filter is hidden");
    assert!(
        !has(&f.rt, "kanban-open-markdown"),
        "Open as markdown is hidden"
    );
    assert!(has(&f.rt, "kanban-archive"), "Archive keeps its builtin");
    assert!(
        has(&f.rt, "kanban-settings"),
        "Board settings stays reachable"
    );
}

#[test]
fn the_metadata_list_adds_and_removes_rows() {
    let mut f = on_board("metadata-list", BOARD);
    click(&mut f.rt, "kanban-settings");
    click(&mut f.rt, "kanban-setting-linked-page-metadata-new");
    f.rt.ime(Ime::Commit("status".to_owned()));
    frame(&mut f.rt);
    click(&mut f.rt, "kanban-setting-linked-page-metadata-add");
    assert_eq!(
        f.footer()["linked-page-metadata"],
        serde_json::json!([{"key": "status", "containsMarkdown": false}])
    );

    click(
        &mut f.rt,
        "kanban-setting-linked-page-metadata-row-0-remove",
    );
    assert_eq!(f.footer()["linked-page-metadata"], serde_json::json!([]));
}

#[test]
fn opening_and_closing_a_surface_writes_nothing() {
    let mut f = on_board("no-write", BOARD);
    let disk = f.disk();
    let settings = f.settings_json();

    open_settings(&mut f.rt);
    click(&mut f.rt, "settings-tabs/segment5");
    act(&mut f.rt, State::close_settings);

    click(&mut f.rt, "kanban-settings");
    click(&mut f.rt, "kanban-settings-close");

    assert_eq!(f.disk(), disk, "the board file is unchanged");
    assert_eq!(f.source(), disk, "the parsed board is unchanged");
    assert!(!f.rt.state.kanban_dirty());
    assert_eq!(f.settings_json(), settings, "settings.json is unchanged");
}

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
    /// The note the fixture opened first, for `disk`.
    first: String,
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
        fs::read_to_string(self._vault.path().join(&self.first)).expect("the first note")
    }
}

fn on_board(tag: &str, board: &str) -> Fixture {
    on_boards(tag, &[("Board.md", board)])
}

/// A vault with several kanban notes; the first is opened.
fn on_boards(tag: &str, notes: &[(&str, &str)]) -> Fixture {
    fixture(tag, notes, None)
}

/// A vault whose `settings.json` already holds `kanban_defaults`.
fn on_boards_with_defaults(
    tag: &str,
    notes: &[(&str, &str)],
    defaults: &serde_json::Value,
) -> Fixture {
    fixture(tag, notes, Some(defaults))
}

fn fixture(tag: &str, notes: &[(&str, &str)], defaults: Option<&serde_json::Value>) -> Fixture {
    let vault = TempVault::new(notes);
    let config = unique_dir(&format!("{tag}-config"));
    let state_home = unique_dir(&format!("{tag}-state"));
    let settings = Settings {
        kanban_autosave: false,
        kanban_defaults: defaults.map_or_else(muninn_core::KanbanDefaults::default, |value| {
            muninn_core::KanbanDefaults::from_json(Some(value))
        }),
        ..Settings::default()
    };
    settings.save_to(&config).expect("settings file");
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
    let first = notes[0].0.to_owned();
    let stem = first.trim_end_matches(".md");
    let id = rt.state.vault.resolve(stem).expect("the first note");
    rt.update(|s, _| s.open_kanban(id));
    rt.frame(1.0 / 60.0);
    Fixture {
        rt,
        config,
        _vault: vault,
        first,
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
    assert!(rt.state.settings_open);
}

fn has(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|element| element.key.as_deref() == Some(key) && !element.exiting)
}

fn text_of(rt: &Runtime<State>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
        .and_then(|element| element.text.clone())
}

/// Focus a settings text field and select its value.
fn focus_field(rt: &mut Runtime<State>, field: &str) {
    click(rt, field);
    rt.key(KeyEvent {
        key: Key::Command('a'),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    frame(rt);
}

/// Focus a settings text field, select its value, and type `text` over it.
fn type_text(rt: &mut Runtime<State>, field: &str, text: &str) {
    focus_field(rt, field);
    rt.ime(Ime::Commit(text.to_owned()));
    for _ in 0..3 {
        frame(rt);
    }
}

fn advance(rt: &mut Runtime<State>, frames: usize) {
    for _ in 0..frames {
        frame(rt);
    }
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

fn text_board(trigger: &str) -> String {
    format!(
        "\
---
kanban-plugin: board
---

## To do

- [ ] a

%% kanban:settings
```
{{\"kanban-plugin\":\"board\",\"date-trigger\":\"{trigger}\"}}
```
%%
"
    )
}

#[test]
fn switching_boards_closes_the_panel_and_the_next_board_shows_its_own_text() {
    let a = text_board("#");
    let b = text_board("$");
    let mut f = on_boards("switch", &[("Board.md", &a), ("Other.md", &b)]);
    click(&mut f.rt, "kanban-settings");
    assert_eq!(
        f.rt.state.kanban_setting_text(false, "date-trigger"),
        Some("#")
    );

    let other = f.rt.state.vault.resolve("Other").expect("Other");
    act(&mut f.rt, |s| s.open_kanban(other));
    advance(&mut f.rt, 3);
    assert!(
        !f.rt.state.kanban_settings_open,
        "the panel belongs to the board it opened on"
    );

    click(&mut f.rt, "kanban-settings");
    assert_eq!(
        f.rt.state.kanban_setting_text(false, "date-trigger"),
        Some("$"),
        "the panel shows the new board's value, not the old one's"
    );
    assert_eq!(f.footer()["date-trigger"], "$");
}

#[test]
fn a_typed_text_setting_is_one_undo_step_and_does_not_evict_card_edits() {
    let mut f = on_board("typed-text", BOARD);
    // One card edit already in the history: the typing must not evict it.
    act(&mut f.rt, |s| s.begin_edit_card(0, 0));
    act(&mut f.rt, |s| {
        s.kanban_card_field.replace_text("edited card")
    });
    key(&mut f.rt, Key::Enter, false);
    assert_eq!(f.board().lanes()[0].cards()[0].text(), "edited card");

    click(&mut f.rt, "kanban-settings");
    focus_field(&mut f.rt, "kanban-setting-date-trigger-text");
    for chunk in ["X", "Y", "Z"] {
        f.rt.ime(Ime::Commit(chunk.to_owned()));
        frame(&mut f.rt);
    }
    advance(&mut f.rt, 2);
    assert_eq!(f.footer()["date-trigger"], "XYZ");
    assert_eq!(
        f.rt.state.kanban_setting_text(false, "date-trigger"),
        Some("XYZ")
    );

    act(&mut f.rt, State::undo_kanban);
    assert!(
        f.footer().get("date-trigger").is_none(),
        "one undo reverts the whole typed value, not one character"
    );
    assert_eq!(
        f.rt.state.kanban_setting_text(false, "date-trigger"),
        Some("@"),
        "the field shows the builtin again, not the undone text"
    );

    act(&mut f.rt, State::undo_kanban);
    assert_eq!(
        f.board().lanes()[0].cards()[0].text(),
        "first card",
        "the card edit is still the next undo step: typing did not consume the history"
    );

    // The resynced field must not put the undone text back on the next key.
    f.rt.ime(Ime::Commit("Q".to_owned()));
    frame(&mut f.rt);
    assert_eq!(
        f.footer()["date-trigger"],
        "@Q",
        "typing appends to the builtin, not to the undone value"
    );
}

#[test]
fn resetting_the_app_default_from_the_board_panel_refills_both_surfaces() {
    let mut f = on_board("reset-app-panel", BOARD);
    act(&mut f.rt, |s| {
        s.set_kanban_app_value(
            "date-trigger",
            muninn_core::KanbanSettingValue::Text("!".to_owned()),
        );
    });
    assert!(f.rt.state.kanban_app_sets("date-trigger"));

    click(&mut f.rt, "kanban-settings");
    assert_eq!(
        f.rt.state.kanban_setting_text(false, "date-trigger"),
        Some("!"),
        "the board panel shows the app default"
    );
    click(&mut f.rt, "kanban-reset-app-date-trigger");
    assert!(!f.rt.state.kanban_app_sets("date-trigger"));
    assert_eq!(
        f.rt.state.kanban_setting_text(false, "date-trigger"),
        Some("@"),
        "the board panel's field falls back to the builtin, not empty"
    );
    assert_eq!(
        f.rt.state.kanban_setting_text(true, "date-trigger"),
        Some("@"),
        "the app tab's field is refilled too"
    );
}

#[test]
fn a_wrong_typed_stored_value_is_named_ignored_and_can_still_be_reset() {
    let board = "\
---
kanban-plugin: board
---

## To do

- [ ] one

%% kanban:settings
```
{\"kanban-plugin\":\"board\",\"lane-width\":\"wide\"}
```
%%
";
    let mut f = on_boards_with_defaults(
        "wrong-type-label",
        &[("Board.md", board)],
        &serde_json::json!({"lane-width": "wide"}),
    );
    click(&mut f.rt, "kanban-settings");
    assert_eq!(
        text_of(&f.rt, "kanban-setting-lane-width-source").as_deref(),
        Some("Board value ignored"),
        "a present-but-wrong-typed value is not called an override"
    );
    assert!(
        has(&f.rt, "kanban-reset-board-lane-width"),
        "Reset board is still offered for the ignored value"
    );

    click(&mut f.rt, "kanban-reset-board-lane-width");
    assert!(f.footer().get("lane-width").is_none());
    assert_eq!(
        text_of(&f.rt, "kanban-setting-lane-width-source").as_deref(),
        Some("App value ignored"),
        "the wrong-typed app default is named as ignored too"
    );
    assert!(has(&f.rt, "kanban-reset-app-lane-width"));

    click(&mut f.rt, "kanban-reset-app-lane-width");
    assert!(!f.rt.state.kanban_app_sets("lane-width"));
    assert_eq!(
        text_of(&f.rt, "kanban-setting-lane-width-source").as_deref(),
        Some("Default")
    );
    assert_eq!(f.rt.state.kanban_board_settings().lane_width, 272);
}

#[test]
fn opening_the_archive_closes_the_board_settings_panel() {
    let mut f = on_board("archive-closes", BOARD);
    click(&mut f.rt, "kanban-settings");
    assert!(f.rt.state.kanban_settings_open);
    click(&mut f.rt, "kanban-archive");
    assert!(f.rt.state.kanban_archive_open);
    assert!(
        !f.rt.state.kanban_settings_open,
        "the archive takes the body from the panel"
    );
}

#[test]
fn editing_one_metadata_entry_keeps_its_extra_fields_and_malformed_entries() {
    let board = "\
---
kanban-plugin: board
---

## To do

- [ ] one

%% kanban:settings
```
{\"kanban-plugin\":\"board\",\"linked-page-metadata\":[{\"key\":\"status\",\"label\":\"Status\",\"containsMarkdown\":\"yes\"},{\"broken\":1},{\"key\":\"owner\",\"containsMarkdown\":false}]}
```
%%
";
    let mut f = on_board("metadata-raw", board);
    click(&mut f.rt, "kanban-settings");
    click(
        &mut f.rt,
        "kanban-setting-linked-page-metadata-row-1-markdown",
    );
    assert_eq!(
        f.footer()["linked-page-metadata"],
        serde_json::json!([
            {"key": "status", "label": "Status", "containsMarkdown": "yes"},
            {"broken": 1},
            {"key": "owner", "containsMarkdown": true}
        ]),
        "only the toggled entry changes; extras and malformed entries stay"
    );

    click(
        &mut f.rt,
        "kanban-setting-linked-page-metadata-row-0-remove",
    );
    assert_eq!(
        f.footer()["linked-page-metadata"],
        serde_json::json!([
            {"broken": 1},
            {"key": "owner", "containsMarkdown": true}
        ])
    );
}

#[test]
fn a_card_menu_marks_done_when_the_checkbox_is_hidden() {
    let board = "\
---
kanban-plugin: board
---

## To do

- [ ] one

%% kanban:settings
```
{\"kanban-plugin\":\"board\",\"show-checkboxes\":false}
```
%%
";
    let mut f = on_board("card-menu-done", board);
    act(&mut f.rt, |s| s.open_card_menu(0, 0));
    advance(&mut f.rt, 2);
    assert!(
        !has(&f.rt, "kanban-card-0-0-mark"),
        "show-checkboxes:false hides the only visible toggle"
    );
    assert!(
        has(&f.rt, "menu-toggle_done"),
        "the card menu offers the done toggle"
    );
    assert!(
        f.rt.elements()
            .iter()
            .filter_map(|element| element.text.as_deref())
            .any(|label| label == "Mark done"),
        "the menu names what it will do"
    );
    click(&mut f.rt, "menu-toggle_done");
    assert!(f.board().lanes()[0].cards()[0].is_done());
}

#[test]
fn a_number_stepper_clamps_a_stored_value_up_to_its_minimum() {
    let board = "\
---
kanban-plugin: board
---

## To do

- [ ] one

%% kanban:settings
```
{\"kanban-plugin\":\"board\",\"lane-width\":50}
```
%%
";
    let mut f = on_board("number-clamp", board);
    click(&mut f.rt, "kanban-settings");
    assert_eq!(f.footer()["lane-width"], 50);
    click(&mut f.rt, "kanban-setting-lane-width-plus");
    assert_eq!(f.footer()["lane-width"], 120, "clamped up to the minimum");
}

#[test]
fn an_unreadable_board_has_no_settings_button() {
    let board = "\
---
kanban-plugin: board
---

## To do

- [ ] one

%% kanban:settings
```
{not json
```
%%
";
    let mut f = on_board("unreadable-settings", board);
    assert!(
        f.rt.state
            .kanban
            .as_ref()
            .is_some_and(|shown| shown.board.is_none()),
        "the footer does not parse"
    );
    assert!(
        has(&f.rt, "kanban-placeholder"),
        "the unreadable board shows its placeholder"
    );
    assert!(!has(&f.rt, "kanban-settings"), "there is no panel to open");
    act(&mut f.rt, State::toggle_kanban_settings);
    assert!(!f.rt.state.kanban_settings_open);
}

#[test]
fn an_external_reload_refreshes_the_open_panel_text() {
    let mut f = on_board("external-reload", &text_board("#"));
    click(&mut f.rt, "kanban-settings");
    assert_eq!(
        f.rt.state.kanban_setting_text(false, "date-trigger"),
        Some("#")
    );
    // Another app rewrites the board's footer while the panel is open.
    fs::write(f._vault.path().join("Board.md"), text_board("$")).expect("write the board");
    act(&mut f.rt, State::refresh_kanban);
    assert!(
        f.rt.state.kanban_settings_open,
        "the same board stays shown"
    );
    assert_eq!(
        f.rt.state.kanban_setting_text(false, "date-trigger"),
        Some("$"),
        "the field follows the reloaded footer"
    );
}

#[test]
fn resetting_the_autosave_app_default_turns_it_back_on() {
    let mut f = on_board("autosave-reset", BOARD);
    act(&mut f.rt, |s| {
        s.set_kanban_app_value(
            muninn_core::KANBAN_AUTOSAVE,
            muninn_core::KanbanSettingValue::Bool(false),
        );
    });
    assert!(!f.rt.state.kanban_app_effective().autosave);
    assert!(f.settings_json().contains("\"kanban_autosave\": false"));
    assert!(f.rt.state.kanban_app_sets(muninn_core::KANBAN_AUTOSAVE));

    act(&mut f.rt, |s| {
        s.reset_kanban_app_value(muninn_core::KANBAN_AUTOSAVE);
    });
    assert!(
        f.rt.state.kanban_app_effective().autosave,
        "the builtin is on again"
    );
    assert!(!f.rt.state.kanban_app_sets(muninn_core::KANBAN_AUTOSAVE));
    assert!(f.settings_json().contains("\"kanban_autosave\": true"));
}

#[test]
fn opening_settings_closes_the_board_panel() {
    let mut f = on_board("modal-over-panel", BOARD);
    click(&mut f.rt, "kanban-settings");
    assert!(f.rt.state.kanban_settings_open);
    open_settings(&mut f.rt);
    assert!(
        !f.rt.state.kanban_settings_open,
        "the two settings surfaces are never open together"
    );
    click(&mut f.rt, "settings-tabs/segment5");
    type_text(&mut f.rt, "kanban-setting-date-trigger-text", "!!");
    assert_eq!(
        f.rt.state.kanban_setting_text(true, "date-trigger"),
        Some("!!")
    );
    act(&mut f.rt, State::close_settings);
    click(&mut f.rt, "kanban-settings");
    assert_eq!(
        f.rt.state.kanban_setting_text(false, "date-trigger"),
        Some("!!"),
        "reopening the panel shows the app default set above it"
    );
}

#[test]
fn an_app_text_default_is_written_once_typing_rests() {
    let mut f = on_board("app-text-debounce", BOARD);
    open_settings(&mut f.rt);
    click(&mut f.rt, "settings-tabs/segment5");
    type_text(&mut f.rt, "kanban-setting-date-trigger-text", "XY");
    assert_eq!(
        f.rt.state.kanban_setting_text(true, "date-trigger"),
        Some("XY")
    );
    assert!(
        !f.settings_json().contains("\"XY\""),
        "typing does not write settings.json once per character"
    );
    advance(&mut f.rt, 40);
    assert!(
        f.settings_json().contains("\"date-trigger\": \"XY\""),
        "the idle write lands once typing rests: {}",
        f.settings_json()
    );
}

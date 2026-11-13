//! Kanban slice 9: dates and times on the Kanban page. The card menu's
//! Add a date/time writes one token through the normal edit path (one undo
//! step), a typed trigger inserts at the caret, Escape leaves the trigger,
//! display settings resolve from the footer and from app defaults, and a
//! date sort uses the parsed date, not the shown text.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use muninn_core::{CivilDate, KanbanDefaults, Vault, format_date, testing::TempVault};
use muninn_ui::{Mode, PickerKind, Settings, State, on_key, poll, view};
use rune::{Env, Ime, Key, KeyEvent, Mods, Runtime};

const PLAIN: &str = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Plain\n";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-dates-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn vault_with(board: &str) -> TempVault {
    TempVault::new(&[("Agenda.md", "# Agenda\n"), ("Board.md", board)])
}

fn on_board_with(tmp: &TempVault, defaults: KanbanDefaults) -> Runtime<State> {
    let config = unique_dir("config");
    let state_home = unique_dir("state");
    let state = State::with_settings(
        tmp.path().to_path_buf(),
        state_home,
        config.clone(),
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
    let id = rt.state.vault.resolve("Board").expect("Board");
    act(&mut rt, |s| s.open_kanban(id));
    rt
}

fn on_board(tmp: &TempVault) -> Runtime<State> {
    on_board_with(tmp, KanbanDefaults::default())
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

fn press(rt: &mut Runtime<State>, key: Key, shift: bool) {
    rt.key(KeyEvent {
        key,
        mods: Mods {
            shift,
            ..Mods::default()
        },
    });
    frame(rt);
}

fn type_text(rt: &mut Runtime<State>, typed: &str) {
    rt.ime(Ime::Commit(typed.to_owned()));
    frame(rt);
    // The IME event can land after the frame's view build; force the next
    // build so the picker it opens is in the tree.
    rt.update(|_, _| {});
    frame(rt);
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

fn menu(rt: &mut Runtime<State>, owner: &str, action: &str) {
    click(rt, owner);
    click(rt, &format!("menu-{action}"));
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

fn on_disk(tmp: &TempVault) -> String {
    fs::read_to_string(tmp.path().join("Board.md")).expect("Board.md")
}

fn page_text(rt: &Runtime<State>) -> String {
    rt.elements()
        .into_iter()
        .filter_map(|e| e.text)
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_of(rt: &Runtime<State>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
}

fn local_today() -> CivilDate {
    let epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        });
    let local = rune::LocalTime::at(epoch_ms);
    CivilDate::new(i64::from(local.year), local.month, local.day)
        .unwrap_or_else(|| CivilDate::from_days_since_epoch(0))
}

fn today_token() -> String {
    format!("@{{{}}}", format_date(local_today(), "YYYY-MM-DD"))
}

fn today_day(day: u32) -> String {
    let today = local_today();
    format!(
        "@{{{}}}",
        format_date(
            CivilDate::new(today.year(), today.month(), day).expect("day"),
            "YYYY-MM-DD"
        )
    )
}

fn day_in(year: i64, month: u32, day: u32) -> String {
    format!(
        "@{{{}}}",
        format_date(CivilDate::new(year, month, day).expect("day"), "YYYY-MM-DD")
    )
}

#[test]
fn add_a_date_from_the_card_menu_is_one_undo_step_and_saves_exactly() {
    let tmp = vault_with(PLAIN);
    let mut rt = on_board(&tmp);
    menu(&mut rt, "kanban-card-0-0-menu", "add_date");
    assert!(rt.state.kanban_picker.is_some(), "the picker is open");
    assert_eq!(
        rt.state.kanban_picker.as_ref().map(|p| p.kind),
        Some(PickerKind::Date)
    );
    click(&mut rt, "kanban-picker-today");
    click(&mut rt, "kanban-picker-set");
    let edited = source(&rt);
    let want = format!(
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Plain {}\n",
        today_token()
    );
    assert_eq!(edited, want, "only the card line changes");
    assert_eq!(on_disk(&tmp), PLAIN, "nothing is written before ⌘S");
    assert!(text_of(&rt, "kanban-card-0-0-date").is_some());

    // One action: one undo puts back the exact bytes.
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), PLAIN);

    // Escape cancels the menu picker and changes nothing.
    menu(&mut rt, "kanban-card-0-0-menu", "add_date");
    assert!(rt.state.kanban_picker.is_some());
    press(&mut rt, Key::Escape, false);
    assert!(rt.state.kanban_picker.is_none());
    assert_eq!(source(&rt), PLAIN);

    // Do it again and save.
    menu(&mut rt, "kanban-card-0-0-menu", "add_date");
    click(&mut rt, "kanban-picker-today");
    click(&mut rt, "kanban-picker-set");
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), want);

    // Clear asks for nothing and removes the token.
    menu(&mut rt, "kanban-card-0-0-menu", "add_date");
    click(&mut rt, "kanban-picker-clear");
    assert_eq!(source(&rt), PLAIN);
}

#[test]
fn add_a_time_from_the_card_menu_writes_exact_bytes_and_undoes_once() {
    let tmp = vault_with(PLAIN);
    let mut rt = on_board(&tmp);
    menu(&mut rt, "kanban-card-0-0-menu", "add_time");
    assert_eq!(
        rt.state.kanban_picker.as_ref().map(|p| p.kind),
        Some(PickerKind::Time)
    );
    assert_eq!(
        rt.state.kanban_hour_field.selected().chars().count(),
        2,
        "the whole hour is selected, so typing replaces it"
    );
    type_text(&mut rt, "14");
    assert_eq!(rt.state.kanban_hour_field.text, "14");
    act(&mut rt, |s| s.kanban_minute_field.replace_text("05"));
    click(&mut rt, "kanban-picker-set");
    let want = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Plain @@{14:05}\n".to_owned();
    assert_eq!(source(&rt), want);
    assert_eq!(on_disk(&tmp), PLAIN);
    assert_eq!(
        text_of(&rt, "kanban-card-0-0-time").as_deref(),
        Some("14:05")
    );
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), want);
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), PLAIN);
}

#[test]
fn a_typed_trigger_inserts_at_the_caret_and_escape_leaves_it_alone() {
    let tmp = vault_with(PLAIN);
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| {
        s.begin_edit_card(0, 0);
        s.kanban_card_field.set_caret(0);
    });
    type_text(&mut rt, " @");
    assert!(
        rt.state.kanban_picker.is_some(),
        "the date picker opened at the caret"
    );
    // A day click chooses and accepts: it is a write.
    click(&mut rt, "kanban-picker-day-15");
    let field = rt.state.kanban_card_field.text.clone();
    assert_eq!(
        field,
        format!(" {}Plain", today_day(15)),
        "inserted at the caret"
    );
    press(&mut rt, Key::Enter, false);
    assert!(!rt.state.is_typing(), "Enter commits the card");
    assert!(source(&rt).contains(&format!("- [ ] {field}\n")));

    // Escape closes the picker, leaves the typed trigger and the editor open.
    act(&mut rt, |s| s.begin_edit_card(0, 0));
    type_text(&mut rt, " more @");
    assert!(rt.state.kanban_picker.is_some());
    let with_trigger = rt.state.kanban_card_field.text.clone();
    press(&mut rt, Key::Escape, false);
    assert!(rt.state.kanban_picker.is_none(), "the picker closed");
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(rt.state.kanban_card_field.text, with_trigger);
    assert!(rt.state.kanban_card.is_some(), "the editor is still open");
    press(&mut rt, Key::Escape, false);
    assert!(!rt.state.is_typing(), "a second Escape cancels the editor");

    // Typing `@@` opens the time picker.
    act(&mut rt, |s| s.begin_edit_card(0, 0));
    type_text(&mut rt, " at @@");
    assert_eq!(
        rt.state.kanban_picker.as_ref().map(|p| p.kind),
        Some(PickerKind::Time)
    );
    act(&mut rt, |s| {
        s.kanban_hour_field.replace_text("9");
        s.kanban_minute_field.replace_text("30");
    });
    click(&mut rt, "kanban-picker-set");
    assert!(
        rt.state.kanban_card_field.text.ends_with(" at @@{09:30}"),
        "{}",
        rt.state.kanban_card_field.text
    );
}

#[test]
fn display_settings_resolve_from_the_footer_and_from_app_defaults() {
    let dated = "---\nkanban-plugin: board\n---\n\n## To do\n\n\
                 - [ ] Due @{2026-11-12}\n\n%% kanban:settings\n```\n{}\n```\n%%\n";
    // Hide is the built-in default.
    let tmp = vault_with(&dated.replace("{}", r#"{"kanban-plugin":"board"}"#));
    let rt = on_board(&tmp);
    assert!(
        !page_text(&rt).contains("@{2026-11-12}"),
        "the token is hidden in the title"
    );
    assert_eq!(
        text_of(&rt, "kanban-card-0-0-date").as_deref(),
        Some("2026-11-12")
    );

    // The footer can show the token and format the foot.
    let footer = r#"{"kanban-plugin":"board","hide-date-in-title":false,"date-display-format":"D MMMM YYYY","show-relative-date":true}"#;
    let tmp = vault_with(&dated.replace("{}", footer));
    let rt = on_board(&tmp);
    let page = page_text(&rt);
    assert!(
        page.contains("@{2026-11-12}"),
        "the token stays in the title"
    );
    assert!(page.contains("12 November 2026"), "{page}");
    assert!(
        page.contains("today")
            || page.contains("tomorrow")
            || page.contains("yesterday")
            || page.contains("in ")
            || page.contains("days"),
        "{page}"
    );

    // App defaults resolve when the footer has no key …
    let tmp = vault_with(&dated.replace("{}", r#"{"kanban-plugin":"board"}"#));
    let rt = on_board_with(
        &tmp,
        KanbanDefaults {
            hide_date_in_title: Some(false),
            date_display_format: Some("DD.MM.YYYY".to_owned()),
            ..KanbanDefaults::default()
        },
    );
    assert!(page_text(&rt).contains("@{2026-11-12}"));
    assert_eq!(
        text_of(&rt, "kanban-card-0-0-date").as_deref(),
        Some("12.11.2026")
    );

    // … and the footer wins over them.
    let footer =
        r#"{"kanban-plugin":"board","hide-date-in-title":true,"date-display-format":"DD.MM.YYYY"}"#;
    let tmp = vault_with(&dated.replace("{}", footer));
    let rt = on_board_with(
        &tmp,
        KanbanDefaults {
            hide_date_in_title: Some(false),
            date_display_format: Some("YYYY/MM/DD".to_owned()),
            ..KanbanDefaults::default()
        },
    );
    assert!(!page_text(&rt).contains("@{2026-11-12}"));
    assert_eq!(
        text_of(&rt, "kanban-card-0-0-date").as_deref(),
        Some("12.11.2026")
    );
}

#[test]
fn a_dirty_board_keeps_its_bytes_whatever_the_display_setting() {
    let board = "---\nkanban-plugin: board\n---\n\n## To do\n\n\
                 - [ ] Due @{2026-11-12}\n\n%% kanban:settings\n```\n{\"hide-date-in-title\":false}\n```\n%%\n";
    let tmp = vault_with(board);
    let mut rt = on_board(&tmp);
    assert_eq!(source(&rt), board, "display changes no bytes");
    act(&mut rt, |s| s.toggle_card_done(0, 0));
    let edited = source(&rt);
    assert_eq!(edited, board.replacen("- [ ] Due", "- [x] Due", 1));
    assert!(edited.contains("@{2026-11-12}"));
    assert_eq!(on_disk(&tmp), board);
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), board);
}

#[test]
fn sorting_uses_the_parsed_date_and_time_not_the_display_string() {
    let board = "---\nkanban-plugin: board\n---\n\n## To do\n\n\
                 - [ ] Later @{01.12.2026}\n\
                 - [ ] No date\n\
                 - [ ] Earlier @{12.11.2026}\n\
                 - [ ] Same late @{12.11.2026} @@{10:00}\n\
                 - [ ] Same early @{12.11.2026} @@{09:00}\n\n\
                 %% kanban:settings\n```\n{\"date-format\":\"DD.MM.YYYY\",\"time-format\":\"HH:mm\"}\n```\n%%\n";
    let tmp = vault_with(board);
    let mut rt = on_board(&tmp);
    menu(&mut rt, "kanban-lane-0-menu", "sort_date");
    let cards: Vec<String> = rt
        .state
        .kanban
        .as_ref()
        .unwrap()
        .board
        .as_ref()
        .unwrap()
        .lanes()[0]
        .cards()
        .iter()
        .map(|card| card.text().to_owned())
        .collect();
    assert_eq!(
        cards,
        [
            "Earlier @{12.11.2026}",
            "Same early @{12.11.2026} @@{09:00}",
            "Same late @{12.11.2026} @@{10:00}",
            "Later @{01.12.2026}",
            "No date"
        ]
    );
    assert_eq!(on_disk(&tmp), board);
}

#[test]
fn clicking_the_date_opens_its_daily_note() {
    let board = "---\nkanban-plugin: board\n---\n\n## To do\n\n\
                 - [ ] Due @{2026-11-11}\n\n%% kanban:settings\n```\n{\"link-date-to-daily-note\":true}\n```\n%%\n";
    let tmp = TempVault::new(&[
        ("Agenda.md", "# Agenda\n"),
        ("2026-11-11.md", "# The day\n"),
        ("Board.md", board),
    ]);
    let mut rt = on_board(&tmp);
    let date = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("kanban-card-0-0-date"))
        .expect("the date row");
    assert_eq!(date.role, "link");
    assert_eq!(date.text.as_deref(), Some("2026-11-11"));
    click(&mut rt, "kanban-card-0-0-date");
    assert_eq!(
        rt.state.selected_path,
        Some(tmp.path().join("2026-11-11.md")),
        "the date link opens the daily note"
    );
    assert_eq!(on_disk(&tmp), board, "opening a link writes nothing");
}

#[test]
fn a_link_form_alias_strips_the_alias_and_a_missing_note_is_created() {
    let alias = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Due @[[2026-11-11|Nov 11]]\n";
    let tmp = TempVault::new(&[
        ("Agenda.md", "# Agenda\n"),
        ("2026-11-11.md", "# The day\n"),
        ("Board.md", alias),
    ]);
    let mut rt = on_board(&tmp);
    assert_eq!(
        text_of(&rt, "kanban-card-0-0-date").as_deref(),
        Some("2026-11-11"),
        "the alias is display text, not part of the date"
    );
    click(&mut rt, "kanban-card-0-0-date");
    assert_eq!(
        rt.state.selected_path,
        Some(tmp.path().join("2026-11-11.md")),
        "the link resolves to the note before the alias"
    );
    assert!(!tmp.path().join("2026-11-11|Nov 11.md").exists());

    let missing = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Due @[[2099-01-02]]\n";
    let tmp = vault_with(missing);
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-card-0-0-date");
    assert_eq!(
        rt.state.selected_path,
        Some(tmp.path().join("2099-01-02.md")),
        "a missing daily note is created like any missing card link"
    );
    assert!(tmp.path().join("2099-01-02.md").is_file());
    assert_eq!(on_disk(&tmp), missing, "the board itself is untouched");
}

#[test]
fn a_new_card_at_the_lane_end_shows_the_picker_and_enter_accepts_it() {
    let tmp = vault_with(PLAIN);
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.begin_add_card(0));
    type_text(&mut rt, "@");
    assert!(
        rt.find("kanban-picker").is_some(),
        "the picker is drawn for the new card at the lane end"
    );
    press(&mut rt, Key::Enter, false);
    assert!(
        rt.state.kanban_picker.is_none(),
        "Enter accepted the picker"
    );
    assert_eq!(rt.state.kanban_card_field.text, today_token());
    assert!(rt.state.kanban_card.is_some(), "the editor stays open");
    press(&mut rt, Key::Enter, false);
    assert!(!rt.state.is_typing());
    assert_eq!(
        source(&rt),
        format!(
            "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Plain\n- [ ] {}\n",
            today_token()
        )
    );
}

#[test]
fn a_menu_date_replaces_the_existing_token_in_one_undo_step() {
    let board = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Due @{2026-11-11}\n";
    let tmp = vault_with(board);
    let mut rt = on_board(&tmp);
    menu(&mut rt, "kanban-card-0-0-menu", "add_date");
    // A day click chooses and accepts: it is a write.
    click(&mut rt, "kanban-picker-day-15");
    let want = format!(
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Due {}\n",
        day_in(2026, 11, 15)
    );
    assert_eq!(source(&rt), want, "one token, replaced in place");
    assert_eq!(source(&rt).matches("@").count(), 1);
    assert_eq!(on_disk(&tmp), board);
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), board, "one undo step");
}

#[test]
fn a_menu_date_keeps_a_link_form_folder_heading_and_alias() {
    for (old, want) in [
        ("@[[2026-11-11|Nov 11]]", "@[[2026-11-15|Nov 11]]"),
        ("@[[daily/2026-11-11#Today]]", "@[[daily/2026-11-15#Today]]"),
    ] {
        let board = format!("---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Due {old}\n");
        let tmp = vault_with(&board);
        let mut rt = on_board(&tmp);
        menu(&mut rt, "kanban-card-0-0-menu", "add_date");
        click(&mut rt, "kanban-picker-day-15");
        assert_eq!(
            source(&rt),
            format!("---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Due {want}\n"),
            "{old}"
        );
        assert_eq!(on_disk(&tmp), board);
        rt.key(command('z', false));
        frame(&mut rt);
        assert_eq!(source(&rt), board, "{old}: one undo step");
    }
}

#[test]
fn a_menu_date_picker_enter_does_not_write_but_add_date_does() {
    let tmp = vault_with(PLAIN);
    let mut rt = on_board(&tmp);
    menu(&mut rt, "kanban-card-0-0-menu", "add_date");
    assert!(rt.state.kanban_picker.is_some());
    let before = source(&rt);
    press(&mut rt, Key::Enter, false);
    assert!(
        rt.state.kanban_picker.is_some(),
        "Enter on the first control did not accept the picker"
    );
    assert_eq!(source(&rt), before, "and wrote nothing");
    // Enter on the focused Previous month button moved the grid; Today puts
    // the choice back on today, then Add date writes it.
    click(&mut rt, "kanban-picker-today");
    click(&mut rt, "kanban-picker-set");
    assert!(rt.state.kanban_picker.is_none());
    assert_eq!(
        source(&rt),
        format!(
            "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Plain {}\n",
            today_token()
        )
    );
}

#[test]
fn shift_enter_with_a_picker_open_inserts_a_line_break() {
    let tmp = vault_with(PLAIN);
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.begin_edit_card(0, 0));
    type_text(&mut rt, " @");
    assert!(rt.state.kanban_picker.is_some());
    press(&mut rt, Key::Enter, true);
    assert!(
        rt.state.kanban_card_field.text.ends_with('\n'),
        "Shift+Enter kept the line break: {:?}",
        rt.state.kanban_card_field.text
    );
    press(&mut rt, Key::Escape, false);
    assert!(!rt.state.is_typing());
    assert_eq!(source(&rt), PLAIN, "nothing was committed");
}

#[test]
fn a_date_link_resolves_at_press_time() {
    let board = "---\nkanban-plugin: board\n---\n\n## To do\n\n\
                 - [ ] Due @{2099-01-02}\n\n%% kanban:settings\n```\n{\"link-date-to-daily-note\":true}\n```\n%%\n";
    let tmp = vault_with(board);
    let mut rt = on_board(&tmp);
    // Draw the lane first, then create the note behind the running page and
    // rescan the vault. The memoised lane still has its date row.
    fs::write(tmp.path().join("2099-01-02.md"), "# The day\n").unwrap();
    rt.update(|s, _| {
        s.vault = Vault::load(&s.vault.root).expect("reload the vault");
    });
    frame(&mut rt);
    click(&mut rt, "kanban-card-0-0-date");
    assert_eq!(
        rt.state.selected_path,
        Some(tmp.path().join("2099-01-02.md")),
        "the link is resolved when pressed, not when the lane was built"
    );
    assert!(
        !tmp.path().join("2099-01-02 2.md").exists(),
        "a stale missing: handler would create a duplicate"
    );
    assert_eq!(on_disk(&tmp), board);
}

#[test]
fn a_draft_card_picker_offers_no_clear() {
    let tmp = vault_with(PLAIN);
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.begin_add_card(0));
    type_text(&mut rt, "@");
    assert!(rt.state.kanban_picker.is_some());
    assert!(
        rt.find("kanban-picker-clear").is_none(),
        "there is no token to clear on the draft card"
    );
}

#[test]
fn a_menu_time_replaces_and_clear_removes_the_token() {
    let board = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] At @@{09:00}\n";
    let tmp = vault_with(board);
    let mut rt = on_board(&tmp);
    menu(&mut rt, "kanban-card-0-0-menu", "add_time");
    assert_eq!(rt.state.kanban_hour_field.selected().chars().count(), 2);
    type_text(&mut rt, "14");
    act(&mut rt, |s| s.kanban_minute_field.replace_text("05"));
    click(&mut rt, "kanban-picker-set");
    assert_eq!(
        source(&rt),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] At @@{14:05}\n"
    );
    menu(&mut rt, "kanban-card-0-0-menu", "add_time");
    click(&mut rt, "kanban-picker-clear");
    assert_eq!(
        source(&rt),
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] At\n"
    );
    rt.key(command('z', false));
    frame(&mut rt);
    assert!(
        source(&rt).contains("- [ ] At @@{14:05}\n"),
        "the clear is one undo step"
    );
}

#[test]
fn an_editor_clear_removes_the_token_from_the_field() {
    let board = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] Due @{2026-11-11}\n";
    let tmp = vault_with(board);
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.begin_edit_card(0, 0));
    type_text(&mut rt, " @");
    assert!(rt.state.kanban_picker.is_some());
    click(&mut rt, "kanban-picker-clear");
    assert!(rt.state.kanban_picker.is_none(), "Clear closed the picker");
    assert_eq!(rt.state.kanban_card_field.text, "Due @");
    assert_eq!(source(&rt), board, "the field is not committed yet");
    press(&mut rt, Key::Escape, false);
    assert!(!rt.state.is_typing());
    assert_eq!(source(&rt), board);
}

#[test]
fn undo_after_accept_does_not_reopen_the_picker_and_retyping_does() {
    let tmp = vault_with(PLAIN);
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| {
        s.begin_edit_card(0, 0);
        s.kanban_card_field.set_caret(5);
    });
    type_text(&mut rt, " @");
    // A day click chooses and accepts: it is a write.
    click(&mut rt, "kanban-picker-day-15");
    assert!(rt.state.kanban_card_field.text.contains(&today_day(15)));
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(rt.state.kanban_card_field.text, "Plain @", "the field undo");
    assert!(
        rt.state.kanban_picker.is_none(),
        "undoing the insertion does not reopen the picker"
    );

    // A fresh editor can open the picker at the same text again.
    act(&mut rt, |s| s.cancel_card_edit());
    act(&mut rt, |s| s.begin_edit_card(0, 0));
    type_text(&mut rt, " @");
    assert!(rt.state.kanban_picker.is_some());

    // Escape then backspace changes the text, so typing the trigger again
    // opens the picker.
    press(&mut rt, Key::Escape, false);
    assert!(rt.state.kanban_picker.is_none(), "Escape closed the picker");
    assert!(rt.state.kanban_card.is_some(), "the editor stays open");
    press(&mut rt, Key::Backspace, false);
    type_text(&mut rt, " @");
    assert!(
        rt.state.kanban_picker.is_some(),
        "a changed text opens the picker again"
    );
}

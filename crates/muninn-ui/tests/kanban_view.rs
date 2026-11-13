//! Kanban slice 2: `Mode::Kanban` and the read-only kanban page. The mode
//! wiring, which kanban the page shows, restart restore, the lane header,
//! the unreadable state, and that viewing never writes the file.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, PaletteAction, PaletteItem, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const SPRINT: &str = include_str!("../../muninn-core/tests/fixtures/kanban/lanes-and-limits.md");
const BUSY: &str = include_str!("../../muninn-core/tests/fixtures/kanban/over-limit.md");
const TORN: &str = include_str!("../../muninn-core/tests/fixtures/kanban/bad-json.md");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

/// `Agenda` sorts first, so a fresh launch selects a note that is not a kanban.
fn kanban_vault() -> TempVault {
    TempVault::new(&[
        ("Agenda.md", "# Agenda\n\nA plain note.\n"),
        ("Busy.md", BUSY),
        ("Sprint.md", SPRINT),
        ("Torn.md", TORN),
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

fn command(c: char) -> KeyEvent {
    KeyEvent {
        key: Key::Command(c),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    }
}

fn key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
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

fn shown(rt: &Runtime<State>) -> Option<&str> {
    rt.state.kanban.as_ref().map(|k| k.rel.as_str())
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
fn cmd_5_shows_the_kanban_page_and_esc_returns_to_the_last_note_mode() {
    let tmp = kanban_vault();
    let (mut rt, _, _) = fresh(&tmp);
    rt.key(command('2'));
    rt.key(command('5'));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert!(rt.find("kanban-page").is_some());
    assert!(rt.find("note-body").is_none(), "Kanban replaces the note");
    assert_eq!(rt.state.last_note_mode, Mode::Preview, "not a note mode");

    rt.key(key(Key::Escape));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Preview, "Esc leaves Kanban for Read");

    click(&mut rt, "mode-kanban");
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Kanban, "the mode button");
    rt.state.toggle_kanban();
    assert_eq!(rt.state.mode, Mode::Preview);
}

#[test]
fn the_palette_finds_kanban() {
    let tmp = kanban_vault();
    let (mut rt, _, _) = fresh(&tmp);
    rt.key(KeyEvent {
        key: Key::Command('p'),
        mods: Mods {
            cmd: true,
            shift: true,
            ..Mods::default()
        },
    });
    frame(&mut rt);
    for c in "toggle kanban".chars() {
        rt.key(key(Key::Text(c.to_string())));
    }
    frame(&mut rt);
    assert_eq!(
        rt.state.palette_items(),
        vec![PaletteItem::Action(PaletteAction::ToggleKanban)]
    );
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Kanban);
}

#[test]
fn the_page_shows_the_selected_kanban_else_the_last_one_else_nothing() {
    let tmp = kanban_vault();
    let (mut rt, _, _) = fresh(&tmp);
    // Agenda is selected and no kanban was shown yet.
    rt.key(command('5'));
    frame(&mut rt);
    assert_eq!(shown(&rt), None);
    assert_eq!(
        text_of(&rt, "kanban-placeholder-title").as_deref(),
        Some("No kanban open")
    );

    show(&mut rt, "Sprint");
    assert_eq!(shown(&rt), Some("Sprint.md"));
    show(&mut rt, "Busy");
    assert_eq!(shown(&rt), Some("Busy.md"), "a selected kanban replaces it");

    // A selected note that is not a kanban keeps the last kanban.
    show(&mut rt, "Agenda");
    assert_eq!(rt.state.selected_path, Some(tmp.path().join("Agenda.md")));
    assert_eq!(shown(&rt), Some("Busy.md"));
    assert_eq!(rt.state.kanban_rel.as_deref(), Some("Busy.md"));

    // Back on the Kanban page changes the selection; the page follows it.
    rt.key(command('['));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(rt.state.selected_path, Some(tmp.path().join("Busy.md")));
    assert_eq!(shown(&rt), Some("Busy.md"));
    rt.key(command('['));
    frame(&mut rt);
    assert_eq!(rt.state.selected_path, Some(tmp.path().join("Sprint.md")));
    assert_eq!(shown(&rt), Some("Sprint.md"));
}

#[test]
fn lanes_show_counts_limits_over_limit_and_complete() {
    let tmp = kanban_vault();
    let (mut rt, _, _) = fresh(&tmp);
    show(&mut rt, "Busy");
    assert_eq!(text_of(&rt, "kanban-title").as_deref(), Some("Busy"));
    let titles: Vec<_> = (0..3)
        .map(|i| text_of(&rt, &format!("kanban-lane-{i}-title")).unwrap())
        .collect();
    assert_eq!(titles, ["Ideas", "Doing", "Shipped"], "file order");
    assert!(rt.find("kanban-lane-3").is_none());
    assert_eq!(text_of(&rt, "kanban-lane-0-count").as_deref(), Some("1"));
    assert_eq!(text_of(&rt, "kanban-lane-1-count").as_deref(), Some("3/2"));
    assert_eq!(
        text_of(&rt, "kanban-lane-1-over").as_deref(),
        Some("Over limit")
    );
    assert!(rt.find("kanban-lane-0-over").is_none());
    assert!(rt.find("kanban-lane-2-complete").is_some(), "a done lane");
    assert!(rt.find("kanban-lane-1-complete").is_none());
    assert!(rt.find("kanban-card-1-2").is_some());
    assert!(rt.find("kanban-card-1-3").is_none());
    let page = page_text(&rt);
    for needle in [
        "Sketch the lane header",
        "Label the cards",
        "with a second line",
    ] {
        assert!(page.contains(needle), "card text {needle:?} in {page}");
    }
    assert!(
        !page.contains("A paragraph between lanes"),
        "passthrough stays in the file, not on the board"
    );

    // A written (0) and a limit that is not reached are not over.
    show(&mut rt, "Sprint");
    assert_eq!(text_of(&rt, "kanban-lane-1-count").as_deref(), Some("2/3"));
    assert_eq!(text_of(&rt, "kanban-lane-2-count").as_deref(), Some("1"));
    assert!(
        (0..4).all(|i| rt.find(&format!("kanban-lane-{i}-over")).is_none()),
        "no lane is over"
    );
}

#[test]
fn an_unreadable_kanban_offers_open_as_markdown() {
    let tmp = kanban_vault();
    let (mut rt, _, _) = fresh(&tmp);
    show(&mut rt, "Torn");
    assert_eq!(shown(&rt), Some("Torn.md"));
    assert!(rt.state.kanban.as_ref().unwrap().board.is_none());
    assert_eq!(
        text_of(&rt, "kanban-placeholder-title").as_deref(),
        Some("This note can't be shown as a kanban")
    );
    assert!(rt.find("kanban-lanes").is_none());
    click(&mut rt, "kanban-open-markdown");
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Editor);
    assert_eq!(rt.state.selected_path, Some(tmp.path().join("Torn.md")));
    assert_eq!(rt.state.editor.text, TORN);
}

#[test]
fn a_restart_reopens_kanban_on_the_last_kanban() {
    let tmp = kanban_vault();
    let (mut rt, config, state_home) = fresh(&tmp);
    show(&mut rt, "Busy");
    // Leave Agenda selected: the restart must use the saved kanban.
    rt.key(command('1'));
    let agenda = rt.state.vault.resolve("Agenda").unwrap();
    rt.state.open_from_list(agenda);
    rt.key(command('5'));
    frame(&mut rt);
    let saved = Settings::load_from(&config);
    assert_eq!(saved.mode, Mode::Kanban);
    assert_eq!(saved.kanban.as_deref(), Some("Busy.md"));
    assert_eq!(saved.selected_rel.as_deref(), Some("Agenda.md"));
    drop(rt);

    let rt = runtime(state_at(tmp.path(), &config, &state_home));
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), Some("Busy.md"));
    assert_eq!(text_of(&rt, "kanban-lane-1-count").as_deref(), Some("3/2"));
    drop(rt);

    // Restored only while the file is still a kanban.
    fs::write(tmp.path().join("Busy.md"), "# Busy\n\nNot a board now.\n").unwrap();
    let rt = runtime(state_at(tmp.path(), &config, &state_home));
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(shown(&rt), None);
    assert!(rt.find("kanban-placeholder").is_some());
    drop(rt);

    fs::remove_file(tmp.path().join("Busy.md")).unwrap();
    let rt = runtime(state_at(tmp.path(), &config, &state_home));
    assert_eq!(shown(&rt), None, "nor once it is gone");
}

#[test]
fn another_vault_does_not_show_the_kanban() {
    let a = kanban_vault();
    let b = TempVault::new(&[("Agenda.md", "# Agenda\n"), ("Busy.md", BUSY)]);
    let (mut rt, config, state_home) = fresh(&a);
    show(&mut rt, "Busy");
    assert!(rt.state.switch_vault(b.path().to_path_buf()));
    assert_eq!(rt.state.kanban_rel, None, "a switch forgets A's kanban");
    assert_eq!(shown(&rt), None);
    assert!(rt.state.switch_vault(a.path().to_path_buf()));
    show(&mut rt, "Busy");
    drop(rt);
    // The same path in another vault is not the kanban that was shown.
    let rt = runtime(state_at(b.path(), &config, &state_home));
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(rt.state.kanban_rel, None);
    assert_eq!(shown(&rt), None);
}

#[test]
fn viewing_never_writes_a_kanban_file() {
    let tmp = kanban_vault();
    let before = snapshot(tmp.path());
    let (mut rt, config, state_home) = fresh(&tmp);
    for title in ["Sprint", "Busy", "Torn", "Sprint"] {
        show(&mut rt, title);
        for _ in 0..30 {
            frame(&mut rt);
        }
    }
    for c in ['1', '5', '2', '5', '3', '5', '4', '5'] {
        rt.key(command(c));
        frame(&mut rt);
    }
    rt.state.refresh_kanban();
    frame(&mut rt);
    drop(rt);
    let mut rt = runtime(state_at(tmp.path(), &config, &state_home));
    for _ in 0..30 {
        frame(&mut rt);
    }
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(snapshot(tmp.path()), before, "every file is byte-identical");
}

#[test]
fn settings_with_every_mode_key_still_load() {
    for (key, mode) in [
        ("editor", Mode::Editor),
        ("preview", Mode::Preview),
        ("graph", Mode::Graph),
        ("board", Mode::Board),
        ("kanban", Mode::Kanban),
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
        let settings = Settings::load_from(&config);
        assert_eq!(settings.mode, mode, "{key}");
        assert_eq!(settings.kanban, None, "{key}");
    }
}

fn settle(rt: &mut Runtime<State>) {
    for _ in 0..30 {
        frame(rt);
    }
}

fn has(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

// P1-2: a menu or Delete lane confirmation names a card or lane by place;
// switching kanbans replaces the board under it, so it must close.

#[test]
fn switching_kanbans_closes_a_card_menu_and_a_delete_lane_confirmation() {
    let tmp = kanban_vault();
    let before = snapshot(tmp.path());
    let (mut rt, _, _) = fresh(&tmp);
    show(&mut rt, "Busy");
    settle(&mut rt);
    rt.update(|s, _| s.open_card_menu(0, 0));
    settle(&mut rt);
    assert!(has(&rt, "context-menu"), "the card menu is drawn");
    show(&mut rt, "Sprint");
    settle(&mut rt);
    assert_eq!(shown(&rt), Some("Sprint.md"));
    assert!(rt.state.menu.is_none(), "the menu closed with its board");
    assert!(!has(&rt, "context-menu"), "no menu is drawn");
    assert!(!has(&rt, "menu-delete"));

    // Delete lane's confirmation names a lane by place too.
    rt.update(|s, _| s.open_lane_menu(1));
    rt.update(|s, _| s.pick_menu("delete_lane"));
    settle(&mut rt);
    assert!(
        has(&rt, "kanban-delete-lane-question"),
        "the confirmation is drawn"
    );
    show(&mut rt, "Busy");
    settle(&mut rt);
    assert!(rt.state.menu.is_none());
    assert!(!has(&rt, "kanban-delete-lane-question"));
    // The confirmation's Delete cannot reach the other board's lane.
    rt.update(|s, _| s.pick_menu("delete"));
    settle(&mut rt);
    assert_eq!(shown(&rt), Some("Busy.md"));
    assert_eq!(snapshot(tmp.path()), before, "no bytes changed");
}

// P2-3: a board re-read can move cards; the selection is a card, not a place.

#[test]
fn a_re_read_keeps_the_selection_on_its_card_and_drops_it_when_gone() {
    let tmp = kanban_vault();
    let (mut rt, _, _) = fresh(&tmp);
    show(&mut rt, "Sprint");
    settle(&mut rt);
    click(&mut rt, "kanban-card-0-0");
    frame(&mut rt);
    assert_eq!(rt.state.kanban_selected, Some((0, 0)));
    // Another app inserts a card above it.
    let changed = SPRINT.replacen("## Backlog\n\n", "## Backlog\n\n- [ ] Inserted first\n", 1);
    fs::write(tmp.path().join("Sprint.md"), &changed).unwrap();
    rt.state.refresh_kanban();
    frame(&mut rt);
    assert_eq!(rt.state.kanban_selected, Some((0, 1)), "the same card");
    let board = rt.state.kanban.as_ref().unwrap().board.as_ref().unwrap();
    assert_eq!(board.lanes()[0].cards()[1].text(), "Write the threat model");
    // The selected card goes: the selection goes with it.
    let gone = SPRINT.replace("- [ ] Write the threat model\n", "");
    fs::write(tmp.path().join("Sprint.md"), &gone).unwrap();
    rt.state.refresh_kanban();
    frame(&mut rt);
    assert_eq!(rt.state.kanban_selected, None);
    assert_eq!(shown(&rt), Some("Sprint.md"));
}

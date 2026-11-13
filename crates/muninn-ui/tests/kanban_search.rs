//! Kanban slice 13: Board Find, filters and nested checklists.
//!
//! `⌘F` on the Kanban page searches the parsed board (not the Write buffer)
//! and keeps the selection on the current match; the header's Filter hides
//! cards without touching the model or the file; a nested task item's click
//! changes exactly its own character through the core edit path.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const BOARD: &str = "\
---
kanban-plugin: board
---

## To do

- [ ] Ship release #release
  - [ ] write notes
  - [x] read notes
  - [ ] 漢字
- [x] Fix docs #ops @{2026-11-12}
- [ ] Plan party #release @{2026-11-10}
";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-search-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn on_board_with(board: &str) -> (TempVault, Runtime<State>) {
    let tmp = TempVault::new(&[("Agenda.md", "# Agenda\n\nPlain.\n"), ("Board.md", board)]);
    let config = unique_dir("config");
    let state_home = unique_dir("state");
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
    rt.frame(1.0 / 60.0);
    (tmp, rt)
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

fn act(rt: &mut Runtime<State>, f: impl FnOnce(&mut State)) {
    rt.update(|s, _| f(s));
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

fn key(k: Key) -> KeyEvent {
    KeyEvent {
        key: k,
        mods: Mods::default(),
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

fn open_find(rt: &mut Runtime<State>) {
    rt.key(command('f'));
    frame(rt);
}

fn type_query(rt: &mut Runtime<State>, query: &str) {
    rt.update(|s, _| {
        let find = s.kanban_find.as_mut().expect("Board Find");
        find.field.replace_text(query);
        find.field.set_caret(query.len());
        find.field.focused = true;
    });
    frame(rt);
}

#[test]
fn board_find_searches_cards_and_steps_matches_without_writing() {
    let (tmp, mut rt) = on_board_with(BOARD);
    act(&mut rt, |s| s.kanban_selected = Some((0, 2)));
    open_find(&mut rt);
    assert!(rt.state.kanban_find.is_some());
    assert!(rt.state.find.is_none(), "the note Find bar stays closed");
    assert!(has(&rt, "kanban-find-bar"));
    assert!(has(&rt, "kanban-find"));

    type_query(&mut rt, "notes");
    let find = rt.state.kanban_find.as_ref().expect("find");
    assert_eq!(find.matches.len(), 2, "two nested lines match");
    assert_eq!(
        rt.state.kanban_selected,
        Some((0, 0)),
        "the first match's card is selected"
    );
    assert_eq!(text_of(&rt, "kanban-find-count").as_deref(), Some("1/2"));

    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert_eq!(text_of(&rt, "kanban-find-count").as_deref(), Some("2/2"));
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert_eq!(text_of(&rt, "kanban-find-count").as_deref(), Some("1/2"));

    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods {
            shift: true,
            ..Mods::default()
        },
    });
    frame(&mut rt);
    assert_eq!(text_of(&rt, "kanban-find-count").as_deref(), Some("2/2"));

    // Case-insensitive, and a multibyte match is found by the same scan.
    type_query(&mut rt, "SHIP");
    assert_eq!(
        rt.state.kanban_find.as_ref().map(|f| f.matches.len()),
        Some(1)
    );
    assert_eq!(rt.state.kanban_selected, Some((0, 0)));
    type_query(&mut rt, "漢字");
    assert_eq!(
        rt.state.kanban_find.as_ref().map(|f| f.matches.len()),
        Some(1)
    );

    // Escape restores the selection the bar opened on, and gives the page its
    // arrows back.
    rt.key(key(Key::Escape));
    frame(&mut rt);
    assert!(rt.state.kanban_find.is_none());
    assert_eq!(rt.state.kanban_selected, Some((0, 2)));
    assert!(!rt.state.editor.focused && !rt.state.search.focused);
    rt.key(key(Key::Up));
    frame(&mut rt);
    assert_eq!(rt.state.kanban_selected, Some((0, 1)), "arrows work again");

    assert_eq!(source(&rt), BOARD, "Board Find writes nothing");
    assert_eq!(on_disk(&tmp), BOARD);
}

#[test]
fn board_find_outlines_the_current_match_and_the_match_card_can_be_edited() {
    let (_tmp, mut rt) = on_board_with(BOARD);
    open_find(&mut rt);
    type_query(&mut rt, "plan");
    assert_eq!(rt.state.kanban_selected, Some((0, 2)));
    assert!(
        rt.state.kanban_find_mark(0, 2).1,
        "the current match is marked"
    );
    // The board's own Find bar replaces the note bar even while the card
    // editor owns the field: the page is never left keyboard-dead.
    act(&mut rt, |s| s.begin_edit_card(0, 2));
    open_find(&mut rt);
    assert!(rt.state.kanban_find.is_some());
    assert_eq!(rt.state.mode, Mode::Kanban);
}

#[test]
fn a_text_filter_hides_cards_composes_with_tag_and_clears() {
    let (tmp, mut rt) = on_board_with(BOARD);

    click(&mut rt, "kanban-filter-toggle");
    assert!(has(&rt, "kanban-filter-panel"));
    act(&mut rt, |s| {
        s.kanban_filter.text.replace_text("Ship");
        s.kanban_filter.text.set_caret(4);
    });
    frame(&mut rt);
    assert!(has(&rt, "kanban-card-0-0"));
    assert!(
        !has(&rt, "kanban-card-0-1"),
        "a non-matching card is hidden"
    );
    assert!(
        !has(&rt, "kanban-card-0-2"),
        "a non-matching card is hidden"
    );
    assert!(has(&rt, "kanban-filter-summary"));
    let summary = text_of(&rt, "kanban-filter-summary-text").expect("summary");
    assert!(summary.contains("1/3"), "{summary}");
    assert!(summary.contains("Ship"), "{summary}");

    // AND: the tag must match too.
    act(&mut rt, |s| s.kanban_filter.tag.replace_text("#ops"));
    frame(&mut rt);
    assert!(
        !has(&rt, "kanban-card-0-0"),
        "the tag excludes the text match"
    );
    assert!(
        !has(&rt, "kanban-card-0-1"),
        "the text excludes the tag match"
    );
    act(&mut rt, |s| s.kanban_filter.tag.replace_text("#release"));
    frame(&mut rt);
    assert!(has(&rt, "kanban-card-0-0"));

    // Clear all brings every card back and removes the summary.
    click(&mut rt, "kanban-filter-clear");
    assert!(!rt.state.kanban_filter_active());
    assert!(has(&rt, "kanban-card-0-0"));
    assert!(has(&rt, "kanban-card-0-1"));
    assert!(has(&rt, "kanban-card-0-2"));
    assert!(!has(&rt, "kanban-filter-summary"));
    assert_eq!(source(&rt), BOARD, "filtering writes nothing");
    assert_eq!(on_disk(&tmp), BOARD);
}

#[test]
fn done_and_date_filters_compose_and_keep_the_selection_visible() {
    let (_tmp, mut rt) = on_board_with(BOARD);
    click(&mut rt, "kanban-filter-toggle");
    click(&mut rt, "kanban-filter-done");
    frame(&mut rt);
    assert!(has(&rt, "kanban-card-0-1"), "the done card is shown");
    assert!(!has(&rt, "kanban-card-0-0"));
    assert!(!has(&rt, "kanban-card-0-2"));
    assert_eq!(
        rt.state.kanban_selected,
        Some((0, 1)),
        "the selection follows the visible card"
    );

    click(&mut rt, "kanban-filter-any-date");
    act(&mut rt, |s| {
        s.kanban_filter.from.replace_text("2000-01-01");
        s.kanban_filter.to.replace_text("2100-01-01");
    });
    frame(&mut rt);
    // Done AND dated: only the done, dated card remains.
    assert!(has(&rt, "kanban-card-0-1"));
    assert!(!has(&rt, "kanban-card-0-0"), "an undated card is hidden");
    assert!(
        !has(&rt, "kanban-card-0-2"),
        "the open dated card is hidden"
    );

    // A malformed range is not applied and says so.
    act(&mut rt, |s| s.kanban_filter.to.replace_text("soon"));
    frame(&mut rt);
    assert!(has(&rt, "kanban-filter-error"));
    assert_eq!(rt.state.kanban_filter.date_range(), None);

    // Clear the date range, then the done state.
    act(&mut rt, |s| {
        s.kanban_filter.from.replace_text("");
        s.kanban_filter.to.replace_text("");
    });
    click(&mut rt, "kanban-filter-done-any");
    frame(&mut rt);
    assert!(has(&rt, "kanban-card-0-0"));
    assert!(has(&rt, "kanban-card-0-1"));
    assert!(has(&rt, "kanban-card-0-2"));
}

#[test]
fn a_tag_chip_sets_the_tag_filter_and_hides_other_cards() {
    let board = "\
---
kanban-plugin: board
---

## To do

- [ ] Ship #release
- [ ] Fix #ops

%% kanban:settings
```
{\"kanban-plugin\":\"board\",\"show-tags-at-bottom\":true}
```
%%
";
    let (_tmp, mut rt) = on_board_with(board);
    click(&mut rt, "kanban-card-0-0-tag-0");
    assert_eq!(rt.state.kanban_filter.tag_text(), Some("release"));
    assert!(has(&rt, "kanban-card-0-0"), "the tagged card stays");
    assert!(!has(&rt, "kanban-card-0-1"), "the untagged card is hidden");
    assert!(has(&rt, "kanban-filter-summary"));
}

#[test]
fn a_nested_checklist_item_toggles_one_character_and_is_undoable() {
    let (tmp, mut rt) = on_board_with(BOARD);
    assert!(has(&rt, "kanban-card-0-0-checklist"));
    assert_eq!(
        text_of(&rt, "kanban-card-0-0-checklist-progress-text").as_deref(),
        Some("1/3")
    );
    assert!(has(&rt, "kanban-card-0-0-task-0"));
    assert!(has(&rt, "kanban-card-0-0-task-2"));

    click(&mut rt, "kanban-card-0-0-task-0");
    let edited = source(&rt);
    assert_eq!(
        edited,
        BOARD.replacen("  - [ ] write notes", "  - [x] write notes", 1),
        "only the item's character changes"
    );
    assert!(
        edited.contains("- [ ] Ship release"),
        "the outer mark is unchanged"
    );
    assert_eq!(on_disk(&tmp), BOARD, "nothing is written before ⌘S");
    assert_eq!(
        text_of(&rt, "kanban-card-0-0-checklist-progress-text").as_deref(),
        Some("2/3")
    );

    rt.update(|s, _| s.undo_kanban());
    frame(&mut rt);
    assert_eq!(source(&rt), BOARD, "one undo step puts it back");

    // The card's own checkbox stays the outer toggle.
    click(&mut rt, "kanban-card-0-0-mark");
    let toggled = source(&rt);
    assert!(toggled.contains("- [x] Ship release"));
    assert!(
        toggled.contains("  - [ ] write notes"),
        "nested items are unchanged"
    );
}

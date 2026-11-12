//! Kanban slice 5: lanes, card moves, sorting and collapse on the Kanban
//! page. Each action changes only its own lines, is one undo step and is
//! written only by `⌘S`; collapse is the footer's `list-collapse` key.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{CardStep, Settings, State, on_key, poll, view};
use rune::{Env, Ime, Key, KeyEvent, Mods, Runtime};

const BOARD: &str = include_str!("../../muninn-core/tests/fixtures/kanban/lanes-board.md");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-lanes-{tag}-{}-{}",
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

/// The app on `Board.md`'s Kanban page.
fn on_board(tmp: &TempVault) -> Runtime<State> {
    let config = unique_dir("config");
    let state_home = unique_dir("state");
    let state = State::with_settings(
        tmp.path().to_path_buf(),
        state_home,
        config.clone(),
        Settings::load_from(&config),
    );
    let mut rt = Runtime::new(state, view, (1600.0, 900.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    let id = rt.state.vault.resolve("Board").expect("Board");
    act(&mut rt, |s| s.open_kanban(id));
    rt
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

fn press(rt: &mut Runtime<State>, key: Key, alt: bool, shift: bool) {
    rt.key(KeyEvent {
        key,
        mods: Mods {
            alt,
            shift,
            ..Mods::default()
        },
    });
    frame(rt);
}

fn type_text(rt: &mut Runtime<State>, typed: &str) {
    rt.ime(Ime::Commit(typed.to_owned()));
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

/// Each lane's title and its cards' first lines, with their done marks.
fn lanes(rt: &Runtime<State>) -> Vec<(String, Vec<String>)> {
    let board = rt
        .state
        .kanban
        .as_ref()
        .and_then(|k| k.board.as_ref())
        .expect("a board");
    board
        .lanes()
        .iter()
        .map(|lane| {
            let cards = lane
                .cards()
                .iter()
                .map(|c| {
                    let first = c.text().split(' ').take(2).collect::<Vec<_>>().join(" ");
                    format!("{}{first}", if c.is_done() { "x " } else { "" })
                })
                .collect();
            (lane.title().to_owned(), cards)
        })
        .collect()
}

fn text_of(rt: &Runtime<State>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
}

fn titles(rt: &Runtime<State>) -> Vec<String> {
    lanes(rt).into_iter().map(|(title, _)| title).collect()
}

fn cards(rt: &Runtime<State>, lane: usize) -> Vec<String> {
    lanes(rt).swap_remove(lane).1
}

/// Whether `key` is drawn, once what changed has settled.
fn has(rt: &mut Runtime<State>, key: &str) -> bool {
    for _ in 0..30 {
        frame(rt);
    }
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

/// Undo every edit, then save: the file holds the original bytes again.
fn undo_to_start_and_save(rt: &mut Runtime<State>, tmp: &TempVault, edits: usize) {
    for _ in 0..edits {
        rt.key(command('z', false));
        frame(rt);
    }
    assert_eq!(source(rt), BOARD, "undo puts back the exact bytes");
    rt.key(command('s', false));
    frame(rt);
    assert_eq!(on_disk(tmp), BOARD);
}

#[test]
fn lane_actions_change_only_their_lines_and_wait_for_save() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    let mut want = BOARD.to_owned();
    let check = |want: &mut String, rt: &Runtime<State>, from: &str, to: &str, what: &str| {
        assert_eq!(want.matches(from).count(), 1, "{what}: {from:?}");
        *want = want.replacen(from, to, 1);
        assert_eq!(source(rt), *want, "{what}");
        assert_eq!(on_disk(&tmp), BOARD, "{what}: nothing written before ⌘S");
    };

    // + Add lane: `## Title` and blank lines, before the settings footer.
    click(&mut rt, "kanban-add-lane");
    assert!(rt.state.is_typing(), "the lane field has the keyboard");
    type_text(&mut rt, "Later");
    press(&mut rt, Key::Enter, false, false);
    assert!(!rt.state.is_typing());
    check(
        &mut want,
        &rt,
        "\n%% kanban",
        "\n## Later\n\n\n\n%% kanban",
        "add lane",
    );

    // Rename keeps the line otherwise; a limit is `## Title (N)`.
    menu(&mut rt, "kanban-lane-3-menu", "rename_lane");
    type_text(&mut rt, " on");
    press(&mut rt, Key::Enter, false, false);
    check(&mut want, &rt, "## Later\n", "## Later on\n", "rename");
    menu(&mut rt, "kanban-lane-3-menu", "lane_limit");
    type_text(&mut rt, "2");
    press(&mut rt, Key::Enter, false, false);
    check(
        &mut want,
        &rt,
        "## Later on\n",
        "## Later on (2)\n",
        "limit",
    );
    // A limit that is not a whole number is refused and changes nothing;
    // the field keeps the text until Esc.
    menu(&mut rt, "kanban-lane-3-menu", "lane_limit");
    type_text(&mut rt, "x");
    press(&mut rt, Key::Enter, false, false);
    assert_eq!(source(&rt), want, "a refused limit");
    assert_eq!(rt.state.kanban_lane_field.text, "2x");
    assert_eq!(
        text_of(&rt, "kanban-error").as_deref(),
        Some("A card limit is a whole number")
    );
    press(&mut rt, Key::Escape, false, false);
    // Clearing a limit removes the suffix: `(0)` is never written.
    menu(&mut rt, "kanban-lane-1-menu", "lane_limit");
    press(&mut rt, Key::Backspace, false, false);
    press(&mut rt, Key::Enter, false, false);
    check(
        &mut want,
        &rt,
        "## Doing (3)\n",
        "## Doing\n",
        "clear limit",
    );

    // The done-lane marker: one line in, one line out.
    menu(&mut rt, "kanban-lane-3-menu", "done_lane");
    check(
        &mut want,
        &rt,
        "## Later on (2)\n\n",
        "## Later on (2)\n\n**Complete**\n",
        "done lane",
    );
    menu(&mut rt, "kanban-lane-2-menu", "not_done_lane");
    check(
        &mut want,
        &rt,
        "**Complete**\n- [x] Ship",
        "- [x] Ship",
        "not done lane",
    );

    // Insert lane after: the new lane goes before the next lane's title.
    menu(&mut rt, "kanban-lane-0-menu", "insert_lane_after");
    type_text(&mut rt, "Soon");
    press(&mut rt, Key::Enter, false, false);
    check(
        &mut want,
        &rt,
        "## Doing\n",
        "## Soon\n\n\n\n## Doing\n",
        "insert after",
    );
    menu(&mut rt, "kanban-lane-1-menu", "insert_lane_before");
    type_text(&mut rt, "First");
    press(&mut rt, Key::Enter, false, false);
    check(
        &mut want,
        &rt,
        "## Soon\n",
        "## First\n\n\n\n## Soon\n",
        "insert before",
    );
    assert_eq!(
        titles(&rt),
        ["To do", "First", "Soon", "Doing", "Done", "Later on"]
    );

    // Delete: an empty lane goes at once; a lane with cards asks first.
    menu(&mut rt, "kanban-lane-1-menu", "delete_lane");
    check(&mut want, &rt, "## First\n\n\n\n", "", "delete empty lane");
    menu(&mut rt, "kanban-lane-2-menu", "delete_lane");
    assert!(has(&mut rt, "menu-delete"), "the confirmation is up");
    assert_eq!(source(&rt), want, "nothing deleted before the confirmation");
    click(&mut rt, "menu-delete");
    check(
        &mut want,
        &rt,
        "## Doing\n\n- [ ] Review the PR #review\n\n\n",
        "",
        "delete lane with cards",
    );
    assert_eq!(titles(&rt), ["To do", "Soon", "Done", "Later on"]);

    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), want, "⌘S writes every edit");
    undo_to_start_and_save(&mut rt, &tmp, 11);
}

#[test]
fn cards_move_by_keyboard_and_menu() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-card-0-0");
    assert_eq!(rt.state.kanban_selected, Some((0, 0)), "a click selects");
    // The arrow keys move the selection; Alt moves the card.
    press(&mut rt, Key::Down, false, false);
    press(&mut rt, Key::Up, false, false);
    assert_eq!(rt.state.kanban_selected, Some((0, 0)));
    press(&mut rt, Key::Down, true, false);
    assert_eq!(
        cards(&rt, 0),
        ["Fix the", "Write the", "Book the", "Ask about"]
    );
    assert_eq!(
        rt.state.kanban_selected,
        Some((0, 1)),
        "the selection follows"
    );
    press(&mut rt, Key::Down, true, true);
    assert_eq!(
        cards(&rt, 0),
        ["Fix the", "Book the", "Ask about", "Write the"]
    );
    press(&mut rt, Key::Up, true, true);
    assert_eq!(
        cards(&rt, 0),
        ["Write the", "Fix the", "Book the", "Ask about"]
    );
    // The card's own lines move whole, the paragraph after the lane stays.
    press(&mut rt, Key::Down, true, false);
    press(&mut rt, Key::Down, true, false);
    press(&mut rt, Key::Down, true, false);
    assert!(source(&rt).contains(
        "- [ ] Fix the parser #bug @{2026-11-20}\n  with a second line\n- [ ] Book the room\n- [ ] Ask about the budget #admin @{2026-11-20}\n- [ ] Write the docs #docs @{2026-12-01}\n\nA note between lanes.\n"
    ));
    // Alt+Right: the next lane, at the same place or its end.
    press(&mut rt, Key::Right, true, false);
    assert_eq!(cards(&rt, 1), ["Review the", "Write the"]);
    // Into a done lane: the card is marked done; back out, it keeps the mark.
    press(&mut rt, Key::Right, true, false);
    assert_eq!(cards(&rt, 2), ["x Ship 0.7.1", "x Write the"]);
    press(&mut rt, Key::Left, true, false);
    assert_eq!(cards(&rt, 1), ["Review the", "x Write the"]);
    assert_eq!(on_disk(&tmp), BOARD, "nothing written before ⌘S");

    // The card menu: Move to top, and Move to list… picks a lane by name.
    menu(&mut rt, "kanban-card-1-1-menu", "move_top");
    assert_eq!(cards(&rt, 1), ["x Write the", "Review the"]);
    menu(&mut rt, "kanban-card-0-0-menu", "move_to_list");
    assert!(!has(&mut rt, "move-to-lane-0"), "not its own lane");
    click(&mut rt, "move-to-lane-2");
    assert_eq!(cards(&rt, 2), ["x Ship 0.7.1", "x Fix the"]);
    assert!(
        source(&rt).contains("- [x] Fix the parser #bug @{2026-11-20}\n  with a second line\n")
    );
    menu(&mut rt, "kanban-card-1-1-menu", "move_previous");
    assert_eq!(cards(&rt, 0), ["Book the", "Review the", "Ask about"]);
    // Nowhere to go: Alt+Up on the first card does nothing.
    act(&mut rt, |s| s.select_kanban_card(0, 0));
    let before = source(&rt);
    press(&mut rt, Key::Up, true, false);
    assert_eq!(source(&rt), before);

    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), source(&rt));
    undo_to_start_and_save(&mut rt, &tmp, 12);
}

#[test]
fn sorting_is_stable_and_keeps_the_paragraph_in_place() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    menu(&mut rt, "kanban-lane-0-menu", "sort_text");
    assert_eq!(
        cards(&rt, 0),
        ["Ask about", "Book the", "Fix the", "Write the"]
    );
    // Same day: the original order; no date: last.
    menu(&mut rt, "kanban-lane-0-menu", "sort_date");
    assert_eq!(
        cards(&rt, 0),
        ["Ask about", "Fix the", "Write the", "Book the"]
    );
    menu(&mut rt, "kanban-lane-0-menu", "sort_tags");
    assert_eq!(
        cards(&rt, 0),
        ["Ask about", "Fix the", "Write the", "Book the"]
    );
    assert!(source(&rt).contains("- [ ] Book the room\n\nA note between lanes.\n"));
    assert_eq!(on_disk(&tmp), BOARD);
    // Sorting an ordered lane is no edit: two undos reach the start.
    undo_to_start_and_save(&mut rt, &tmp, 2);
}

#[test]
fn collapse_is_a_saved_setting_that_only_changes_its_key() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-lane-1-collapse");
    assert!(has(&mut rt, "kanban-lane-1-expand"), "a narrow strip");
    assert!(!has(&mut rt, "kanban-lane-1-add"), "no cards drawn");
    let collapsed = BOARD.replace(
        "\"numbers\":[1, 2, 3]}",
        "\"numbers\":[1, 2, 3],\"list-collapse\":[false,true,false]}",
    );
    assert_eq!(source(&rt), collapsed, "only the footer key changes");
    assert_eq!(on_disk(&tmp), BOARD);
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), collapsed);

    // Opened again, the lane is still collapsed; expanding writes `false`.
    let mut again = on_board(&tmp);
    assert!(has(&mut again, "kanban-lane-1-expand"));
    click(&mut again, "kanban-lane-1-expand");
    assert!(has(&mut again, "kanban-lane-1-add"));
    assert_eq!(
        source(&again),
        collapsed.replace("[false,true,false]", "[false,false,false]")
    );
    // A new lane keeps the array aligned.
    click(&mut again, "kanban-lane-2-collapse");
    act(&mut again, |s| s.begin_add_lane(0));
    type_text(&mut again, "Inbox");
    press(&mut again, Key::Enter, false, false);
    assert!(source(&again).contains("\"list-collapse\":[false,false,false,true]}"));
    assert!(
        has(&mut again, "kanban-lane-3-expand"),
        "Done is still the collapsed lane"
    );
}

/// Slice 4's rules hold for the lane field: a refused text stays in the open
/// field, with the reason on the page, and changes nothing; actions wait
/// until it is fixed or cancelled.
#[test]
fn a_refused_lane_field_keeps_its_text_and_holds_actions() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    menu(&mut rt, "kanban-lane-0-menu", "lane_limit");
    type_text(&mut rt, "two");
    press(&mut rt, Key::Enter, false, false);
    assert_eq!(source(&rt), BOARD);
    assert!(rt.state.kanban_lane_edit.is_some(), "the field stays open");
    assert_eq!(rt.state.kanban_lane_field.text, "two");
    assert!(has(&mut rt, "kanban-lane-field"));
    assert_eq!(
        text_of(&rt, "kanban-error").as_deref(),
        Some("A card limit is a whole number")
    );
    // A move, a sort or a collapse waits while the field is refused.
    act(&mut rt, |s| s.step_kanban_card(0, 0, CardStep::Down));
    act(&mut rt, |s| s.sort_lane(0, muninn_core::CardOrder::Text));
    act(&mut rt, |s| s.toggle_lane_collapsed(0));
    assert_eq!(source(&rt), BOARD);
    assert_eq!(rt.state.kanban_lane_field.text, "two");
    // ⌘S can't save past it either.
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), BOARD);
    // Esc lets it go and clears the message.
    press(&mut rt, Key::Escape, false, false);
    assert!(rt.state.kanban_lane_edit.is_none());
    assert!(text_of(&rt, "kanban-error").is_none());
    // A title the model can't write is refused the same way.
    menu(&mut rt, "kanban-lane-0-menu", "rename_lane");
    act(&mut rt, |s| {
        s.kanban_lane_field.replace_text("To do\n## Two")
    });
    press(&mut rt, Key::Enter, false, false);
    assert_eq!(source(&rt), BOARD);
    assert_eq!(rt.state.kanban_lane_field.text, "To do\n## Two");
    let error = text_of(&rt, "kanban-error").unwrap_or_default();
    assert!(error.starts_with("Could not rename the lane"), "{error:?}");
    press(&mut rt, Key::Escape, false, false);
    assert!(rt.state.kanban_lane_edit.is_none());
}

/// An action on a card or lane drawn before an open editor committed acts on
/// that same card or lane afterwards: an added card above it moves it down
/// one, and an added lane before it moves it right one.
#[test]
fn moves_sorts_and_collapse_follow_their_target_after_a_commit() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    // Insert card before the first card is open; Alt+Down on "Write the
    // docs" (drawn at 0) commits it, then moves that card.
    act(&mut rt, |s| s.begin_insert_card(0, 0));
    type_text(&mut rt, "New first");
    act(&mut rt, |s| s.step_kanban_card(0, 0, CardStep::Down));
    assert_eq!(
        cards(&rt, 0),
        ["New first", "Fix the", "Write the", "Book the", "Ask about"]
    );
    assert_eq!(rt.state.kanban_selected, Some((0, 2)));
    // A new lane's field is open before Doing; collapsing Doing (drawn at 1)
    // commits it, then collapses Doing, now at 2.
    act(&mut rt, |s| s.begin_add_lane(1));
    type_text(&mut rt, "Soon");
    act(&mut rt, |s| s.toggle_lane_collapsed(1));
    assert_eq!(titles(&rt), ["To do", "Soon", "Doing", "Done"]);
    let collapsed: Vec<bool> = rt
        .state
        .kanban
        .as_ref()
        .and_then(|k| k.board.as_ref())
        .expect("a board")
        .lanes()
        .iter()
        .map(muninn_core::Lane::collapsed)
        .collect();
    assert_eq!(collapsed, [false, false, true, false]);
    // Move to list… picked while a lane field is open: both lanes follow.
    act(&mut rt, |s| s.begin_add_lane(0));
    type_text(&mut rt, "First");
    act(&mut rt, |s| s.move_card_to_lane(0, 0, 3));
    assert_eq!(titles(&rt), ["First", "To do", "Soon", "Doing", "Done"]);
    assert_eq!(
        cards(&rt, 1),
        ["Fix the", "Write the", "Book the", "Ask about"]
    );
    // Done (drawn at 3) is now at 4, and a card entering it becomes done.
    assert_eq!(cards(&rt, 4), ["x Ship 0.7.1", "x New first"]);
    // Sorting a lane drawn after a new lane sorts that lane.
    act(&mut rt, |s| s.begin_add_lane(1));
    type_text(&mut rt, "Zero");
    act(&mut rt, |s| s.sort_lane(1, muninn_core::CardOrder::Text));
    assert_eq!(
        cards(&rt, 2),
        ["Ask about", "Book the", "Fix the", "Write the"]
    );
    // Nothing is written before ⌘S, and undo walks back every edit.
    assert_eq!(on_disk(&tmp), BOARD);
    undo_to_start_and_save(&mut rt, &tmp, 8);
}

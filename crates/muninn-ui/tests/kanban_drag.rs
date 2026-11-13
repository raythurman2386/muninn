//! Kanban slice 6: dragging cards and lanes on the Kanban page, with real
//! pointer events. A drop is one undo step, written only by `⌘S`; Esc puts
//! everything back; the keyboard and the lane menu move lanes too.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Ime, Key, KeyEvent, Mods, Runtime};

const BOARD: &str = include_str!("../../muninn-core/tests/fixtures/kanban/drag-board.md");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-drag-{tag}-{}-{}",
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
        // These tests pin slice 4's explicit save; slice 7 tests autosave.
        Settings {
            kanban_autosave: false,
            ..Settings::load_from(&config)
        },
    );
    let mut rt = Runtime::new(state, view, (1600.0, 900.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    let id = rt.state.vault.resolve("Board").expect("Board");
    rt.update(|s, _| s.open_kanban(id));
    settle(&mut rt);
    rt
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

fn settle(rt: &mut Runtime<State>) {
    for _ in 0..30 {
        frame(rt);
    }
}

fn act(rt: &mut Runtime<State>, f: impl FnOnce(&mut State)) {
    rt.update(|s, _| f(s));
    frame(rt);
}

fn key(rt: &mut Runtime<State>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    frame(rt);
}

fn command(rt: &mut Runtime<State>, c: char) {
    let mods = Mods {
        cmd: true,
        ..Mods::default()
    };
    key(rt, Key::Command(c), mods);
}

fn rect(rt: &mut Runtime<State>, key: &str) -> [f32; 4] {
    settle(rt);
    rt.presented(key)
        .unwrap_or_else(|| panic!("missing {key}"))
        .0
}

fn centre(r: [f32; 4]) -> (f32, f32) {
    (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = centre(rect(rt, key));
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frame(rt);
}

/// Press at `from`, move to `to` in steps, and hold there.
fn hold(rt: &mut Runtime<State>, from: (f32, f32), to: (f32, f32)) {
    rt.pointer_move(from.0, from.1);
    rt.pointer_down(from.0, from.1);
    frame(rt);
    for step in 1..=8 {
        let f = step as f32 / 8.0;
        rt.pointer_move(from.0 + (to.0 - from.0) * f, from.1 + (to.1 - from.1) * f);
        frame(rt);
    }
    settle(rt);
    // Moving the last bit again reads what the frames above laid out.
    rt.pointer_move(to.0, to.1 + 0.5);
    rt.pointer_move(to.0, to.1);
    frame(rt);
}

fn drop_at(rt: &mut Runtime<State>, to: (f32, f32)) {
    rt.pointer_up(to.0, to.1);
    settle(rt);
}

fn drag(rt: &mut Runtime<State>, from: (f32, f32), to: (f32, f32)) {
    hold(rt, from, to);
    drop_at(rt, to);
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

fn dirty(rt: &Runtime<State>) -> bool {
    rt.state.kanban_dirty()
}

/// Each lane's title, and its cards' first words with their done marks.
fn lanes(rt: &Runtime<State>) -> Vec<(String, Vec<String>)> {
    let source = source(rt);
    let board = muninn_core::Kanban::parse(&source).expect("parse");
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

fn cards(rt: &Runtime<State>, lane: usize) -> Vec<String> {
    lanes(rt).swap_remove(lane).1
}

fn titles(rt: &Runtime<State>) -> Vec<String> {
    lanes(rt).into_iter().map(|(title, _)| title).collect()
}

fn has(rt: &mut Runtime<State>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

/// Undo every edit, then save: the file holds the original bytes again.
fn undo_to_start_and_save(rt: &mut Runtime<State>, tmp: &TempVault, edits: usize) {
    for _ in 0..edits {
        command(rt, 'z');
    }
    assert_eq!(source(rt), BOARD, "undo puts back the exact bytes");
    command(rt, 's');
    assert_eq!(on_disk(tmp), BOARD);
}

#[test]
fn a_card_lands_where_the_bar_shows_between_cards_of_any_height() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    let tall = rect(&mut rt, "kanban-card-0-1");
    let short = rect(&mut rt, "kanban-card-0-0");
    assert!(
        tall[3] > 1.5 * short[3],
        "the tall card is drawn taller: {} vs {}",
        tall[3],
        short[3]
    );
    // Low in the tall card, below its middle: after it, before "Another".
    let to = (tall[0] + tall[2] / 2.0, tall[1] + tall[3] - 6.0);
    let last = centre(rect(&mut rt, "kanban-card-0-3"));
    hold(&mut rt, last, to);
    assert!(has(&mut rt, "kanban-drop"), "the bar is drawn mid-drag");
    let bar = rect(&mut rt, "kanban-drop");
    assert!(
        bar[1] >= tall[1] + tall[3],
        "the bar is under the tall card"
    );
    assert!(
        bar[1] < rect(&mut rt, "kanban-card-0-2")[1],
        "and above the next card"
    );
    assert_eq!(source(&rt), BOARD, "nothing moves until the drop");
    drop_at(&mut rt, to);
    assert!(!has(&mut rt, "kanban-drop"));
    assert_eq!(
        cards(&rt, 0),
        ["Short card", "A tall", "Last short", "Another short"]
    );
    // The card's line moved; nothing else changed. Not written before ⌘S.
    let want = BOARD.replacen("- [ ] Last short card\n", "", 1).replacen(
        "- [ ] Another short card\n",
        "- [ ] Last short card\n- [ ] Another short card\n",
        1,
    );
    assert_eq!(source(&rt), want);
    assert_eq!(on_disk(&tmp), BOARD);
    assert_eq!(rt.state.kanban_selected, Some((0, 2)), "the dropped card");
    command(&mut rt, 's');
    assert_eq!(on_disk(&tmp), want);
    undo_to_start_and_save(&mut rt, &tmp, 1);
}

#[test]
fn a_collapsed_lane_takes_a_card_last_and_a_done_lane_marks_it_done() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    let doing = centre(rect(&mut rt, "kanban-card-1-0"));
    let waiting = centre(rect(&mut rt, "kanban-lane-2"));
    hold(&mut rt, doing, waiting);
    assert!(
        !has(&mut rt, "kanban-drop"),
        "a collapsed lane is outlined, not given a bar"
    );
    drop_at(&mut rt, waiting);
    assert_eq!(cards(&rt, 1), Vec::<String>::new());
    assert_eq!(cards(&rt, 2), ["Waiting on", "In progress"]);
    let shipped = rect(&mut rt, "kanban-card-3-0");
    let below = (shipped[0] + 20.0, shipped[1] + shipped[3] + 20.0);
    let short = centre(rect(&mut rt, "kanban-card-0-0"));
    drag(&mut rt, short, below);
    assert_eq!(cards(&rt, 3), ["x Shipped", "x Short card"]);
    assert!(source(&rt).contains("- [x] Shipped\n- [x] Short card\n"));
    assert_eq!(on_disk(&tmp), BOARD);
    undo_to_start_and_save(&mut rt, &tmp, 2);
}

#[test]
fn esc_cancels_a_drag_and_a_drop_in_place_records_nothing() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    let short = centre(rect(&mut rt, "kanban-card-0-0"));
    let done = centre(rect(&mut rt, "kanban-lane-3"));
    hold(&mut rt, short, done);
    assert!(has(&mut rt, "kanban-drop"));
    key(&mut rt, Key::Escape, Mods::default());
    drop_at(&mut rt, done);
    assert_eq!(
        rt.state.mode,
        Mode::Kanban,
        "Esc ended the drag, not the page"
    );
    assert!(!has(&mut rt, "kanban-drop"));
    assert_eq!(source(&rt), BOARD);
    assert!(!dirty(&rt), "nothing to save, nothing in history");
    // A drop where the card already is changes nothing either.
    let tall = rect(&mut rt, "kanban-card-0-1");
    drag(
        &mut rt,
        centre(tall),
        (tall[0] + tall[2] / 2.0, tall[1] + tall[3] - 4.0),
    );
    assert_eq!(source(&rt), BOARD);
    assert!(!dirty(&rt));
    // A press that hardly moves is a click: it selects.
    let another = centre(rect(&mut rt, "kanban-card-0-2"));
    drag(&mut rt, another, (another.0 + 1.0, another.1 + 1.0));
    assert_eq!(rt.state.kanban_selected, Some((0, 2)));
    assert_eq!(source(&rt), BOARD);
    command(&mut rt, 'z');
    assert_eq!(source(&rt), BOARD, "there was nothing to undo");
}

#[test]
fn a_lane_drags_by_its_header_and_keeps_collapse_aligned() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    let head = centre(rect(&mut rt, "kanban-lane-3-title"));
    let doing = rect(&mut rt, "kanban-lane-1");
    let to = (doing[0] + 20.0, doing[1] + 20.0);
    hold(&mut rt, head, to);
    assert!(has(&mut rt, "kanban-lane-drop"));
    assert_eq!(source(&rt), BOARD);
    drop_at(&mut rt, to);
    assert!(!has(&mut rt, "kanban-lane-drop"));
    assert_eq!(titles(&rt), ["To do", "Done", "Doing", "Waiting"]);
    let board = muninn_core::Kanban::parse(&source(&rt)).expect("parse");
    assert_eq!(
        board.footer_json(),
        Some(
            "{\"kanban-plugin\":\"board\",\"list-collapse\":[false,false,false,true],\"note-key\":\"kept as is\"}"
        )
    );
    assert!(board.lanes()[3].collapsed(), "Waiting is still collapsed");
    // A collapsed lane drags by its strip.
    let strip = centre(rect(&mut rt, "kanban-lane-3"));
    let first = rect(&mut rt, "kanban-lane-0");
    drag(&mut rt, strip, (first[0] + 10.0, first[1] + 20.0));
    assert_eq!(titles(&rt), ["Waiting", "To do", "Done", "Doing"]);
    assert!(source(&rt).contains("\"list-collapse\":[true,false,false,false]"));
    assert_eq!(on_disk(&tmp), BOARD);
    undo_to_start_and_save(&mut rt, &tmp, 2);
}

#[test]
fn lanes_move_by_keyboard_and_by_the_lane_menu() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-card-1-0");
    assert_eq!(rt.state.kanban_selected, Some((1, 0)));
    let alt_shift = Mods {
        alt: true,
        shift: true,
        ..Mods::default()
    };
    key(&mut rt, Key::Right, alt_shift);
    assert_eq!(titles(&rt), ["To do", "Waiting", "Doing", "Done"]);
    assert!(source(&rt).contains("\"list-collapse\":[false,true,false,false]"));
    assert_eq!(
        rt.state.kanban_selected,
        Some((2, 0)),
        "the selection follows"
    );
    key(&mut rt, Key::Right, alt_shift);
    key(&mut rt, Key::Right, alt_shift);
    assert_eq!(
        titles(&rt),
        ["To do", "Waiting", "Done", "Doing"],
        "no lane past the last"
    );
    // The lane menu: no Move lane left on the first lane.
    click(&mut rt, "kanban-lane-0-menu");
    settle(&mut rt);
    assert!(!has(&mut rt, "menu-move_lane_left"));
    assert!(has(&mut rt, "menu-move_lane_right"));
    key(&mut rt, Key::Escape, Mods::default());
    click(&mut rt, "kanban-lane-3-menu");
    click(&mut rt, "menu-move_lane_left");
    assert_eq!(titles(&rt), ["To do", "Waiting", "Doing", "Done"]);
    assert_eq!(on_disk(&tmp), BOARD);
    undo_to_start_and_save(&mut rt, &tmp, 3);
}

#[test]
fn a_drag_commits_an_open_editor_first_and_waits_on_a_refused_one() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    // Insert card before the first card is open; the tall card is drawn at 1.
    act(&mut rt, |s| s.begin_insert_card(0, 0));
    rt.ime(Ime::Commit("New first".to_owned()));
    let tall = centre(rect(&mut rt, "kanban-card-0-1"));
    let doing = rect(&mut rt, "kanban-lane-1");
    let to = (doing[0] + 40.0, doing[1] + doing[3] - 10.0);
    drag(&mut rt, tall, to);
    assert_eq!(
        cards(&rt, 0),
        ["New first", "Short card", "Another short", "Last short"]
    );
    assert_eq!(cards(&rt, 1), ["In progress", "A tall"]);
    // A refused editor holds the drag: nothing moves and its text stays. A
    // trailing blank line is not part of a card (the reader ends the card
    // there), so the add is refused.
    act(&mut rt, |s| s.begin_add_card(0));
    rt.ime(Ime::Commit("Call Ana\n\n".to_owned()));
    let before = source(&rt);
    let short = centre(rect(&mut rt, "kanban-card-0-1"));
    drag(&mut rt, short, to);
    assert_eq!(source(&rt), before);
    assert_eq!(rt.state.kanban_card_field.text, "Call Ana\n\n");
    assert!(rt.state.kanban_card.is_some(), "the editor stays open");
    act(&mut rt, State::cancel_card_edit);
    assert_eq!(on_disk(&tmp), BOARD);
    undo_to_start_and_save(&mut rt, &tmp, 2);
}

fn alt(shift: bool) -> Mods {
    Mods {
        alt: true,
        shift,
        ..Mods::default()
    }
}

/// The board after `op` on the original, through the core model.
fn board_after(op: impl FnOnce(&mut muninn_core::Kanban)) -> String {
    let mut board = muninn_core::Kanban::parse(BOARD).expect("parse");
    op(&mut board);
    board.source().to_owned()
}

// GK N1: a key that changes the board while a press is held ends the drag, so
// the release can't move whatever card is now at the pressed place.

#[test]
fn alt_shift_arrows_mid_drag_end_the_drag() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-card-1-0");
    let short = centre(rect(&mut rt, "kanban-card-0-0"));
    let done = centre(rect(&mut rt, "kanban-lane-3"));
    hold(&mut rt, short, done);
    assert!(has(&mut rt, "kanban-drop"));
    key(&mut rt, Key::Right, alt(true));
    assert!(!has(&mut rt, "kanban-drop"), "the drag ended");
    drop_at(&mut rt, done);
    let moved = board_after(|b| b.move_lane(1, 2).expect("move"));
    assert_eq!(source(&rt), moved, "only the lane moved");
    // Lanes move left too, and a fresh press drags again.
    hold(&mut rt, short, done);
    key(&mut rt, Key::Left, alt(true));
    drop_at(&mut rt, done);
    assert_eq!(source(&rt), BOARD);
    drag(&mut rt, short, done);
    assert_eq!(cards(&rt, 3), ["x Short card", "x Shipped"]);
    assert_eq!(on_disk(&tmp), BOARD);
    undo_to_start_and_save(&mut rt, &tmp, 3);
}

#[test]
fn undo_and_redo_mid_drag_end_the_drag() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-card-0-3");
    key(&mut rt, Key::Up, alt(false));
    let moved = source(&rt);
    assert_ne!(moved, BOARD);
    let short = centre(rect(&mut rt, "kanban-card-0-0"));
    let done = centre(rect(&mut rt, "kanban-lane-3"));
    hold(&mut rt, short, done);
    command(&mut rt, 'z');
    drop_at(&mut rt, done);
    assert_eq!(source(&rt), BOARD, "only the undo");
    hold(&mut rt, short, done);
    command(&mut rt, 'y');
    drop_at(&mut rt, done);
    assert_eq!(source(&rt), moved, "only the redo");
    assert_eq!(on_disk(&tmp), BOARD);
    undo_to_start_and_save(&mut rt, &tmp, 1);
}

#[test]
fn alt_arrows_mid_drag_end_the_drag_even_before_it_moves() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-card-0-0");
    let tall = centre(rect(&mut rt, "kanban-card-0-1"));
    let done = centre(rect(&mut rt, "kanban-lane-3"));
    hold(&mut rt, tall, done);
    // Short card swaps with the tall one: the pressed place now holds it.
    key(&mut rt, Key::Down, alt(false));
    drop_at(&mut rt, done);
    assert_eq!(
        cards(&rt, 0),
        ["A tall", "Short card", "Another short", "Last short"]
    );
    assert_eq!(cards(&rt, 3), ["x Shipped"], "nothing was dropped");
    // A press that has not moved yet ends the same way.
    let pressed = centre(rect(&mut rt, "kanban-card-0-1"));
    rt.pointer_move(pressed.0, pressed.1);
    rt.pointer_down(pressed.0, pressed.1);
    frame(&mut rt);
    key(&mut rt, Key::Up, alt(false));
    let after_key = source(&rt);
    rt.pointer_move(done.0, done.1);
    frame(&mut rt);
    assert!(!has(&mut rt, "kanban-drop"));
    drop_at(&mut rt, done);
    assert_eq!(source(&rt), after_key);
    assert_eq!(source(&rt), BOARD, "the two swaps undo each other");
    assert_eq!(cards(&rt, 3), ["x Shipped"]);
}

// GK N2: switching mode mid-drag ends the drag.
#[test]
fn a_mode_switch_mid_drag_ends_the_drag() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    let short = centre(rect(&mut rt, "kanban-card-0-0"));
    let done = centre(rect(&mut rt, "kanban-lane-3"));
    hold(&mut rt, short, done);
    assert!(has(&mut rt, "kanban-drop"));
    command(&mut rt, '1');
    assert_eq!(rt.state.mode, Mode::Editor);
    command(&mut rt, '5');
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert!(!has(&mut rt, "kanban-drop"), "no bar left over");
    drop_at(&mut rt, done);
    assert_eq!(source(&rt), BOARD);
    assert!(!dirty(&rt));
}

// GK N4: the paths the first round's tests left out.

#[test]
fn a_lane_drags_right_and_settles_an_open_lane_field() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.begin_rename_lane(0));
    act(&mut rt, |s| s.kanban_lane_field.replace_text("Next"));
    let head = centre(rect(&mut rt, "kanban-lane-0-title"));
    let doing = rect(&mut rt, "kanban-lane-1");
    let past = (doing[0] + doing[2] - 10.0, head.1);
    hold(&mut rt, head, past);
    assert!(rt.state.kanban_lane_edit.is_none(), "the field committed");
    assert!(has(&mut rt, "kanban-lane-drop"));
    drop_at(&mut rt, past);
    assert_eq!(titles(&rt), ["Doing", "Next", "Waiting", "Done"]);
    let want = board_after(|b| {
        b.rename_lane(0, "Next").expect("rename");
        b.move_lane(0, 1).expect("move");
    });
    assert_eq!(source(&rt), want);
    assert_eq!(on_disk(&tmp), BOARD);
    undo_to_start_and_save(&mut rt, &tmp, 2);
}

#[test]
fn a_press_on_another_card_closes_an_open_menu_before_the_drag() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.open_card_menu(0, 0));
    assert!(rt.state.menu.is_some());
    // Slice 16 fix: the menu is a modal overlay, so the first press outside it
    // dismisses the menu and the drag starts on the next press.
    let doing = centre(rect(&mut rt, "kanban-card-1-0"));
    click(&mut rt, "kanban-card-1-0");
    assert!(rt.state.menu.is_none(), "the press closed the menu");
    let todo = centre(rect(&mut rt, "kanban-lane-0"));
    drag(&mut rt, doing, todo);
    assert_eq!(cards(&rt, 1), Vec::<String>::new());
    undo_to_start_and_save(&mut rt, &tmp, 1);
}

#[test]
fn opening_another_kanban_mid_drag_ends_the_drag() {
    let tmp = TempVault::new(&[("Board.md", BOARD), ("Other.md", BOARD)]);
    let mut rt = on_board(&tmp);
    let short = centre(rect(&mut rt, "kanban-card-0-0"));
    let done = centre(rect(&mut rt, "kanban-lane-3"));
    hold(&mut rt, short, done);
    let other = rt.state.vault.resolve("Other").expect("Other");
    act(&mut rt, |s| s.open_kanban(other));
    assert!(!has(&mut rt, "kanban-drop"));
    drop_at(&mut rt, done);
    assert_eq!(source(&rt), BOARD);
    assert!(!dirty(&rt));
    assert_eq!(on_disk(&tmp), BOARD);
    let other = fs::read_to_string(tmp.path().join("Other.md")).expect("Other.md");
    assert_eq!(other, BOARD);
}

/// P2-6: a refresh that arrives while a press is held does not replace the
/// board under it (the shared disk rule says `Later`); the next refresh after
/// the release applies what was deferred.
#[test]
fn a_vault_refresh_does_not_replace_the_board_under_a_held_drag() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    let card = rect(&mut rt, "kanban-card-0-0");
    let from = centre(card);
    // A real drag that would drop the card back where it is: past the click
    // distance, and the same slot on release, so the board stays clean.
    let to = (from.0, from.1 + 6.0);
    hold(&mut rt, from, to);
    assert!(has(&mut rt, "kanban-drop"), "the drag is held");
    let theirs = BOARD.replace("Short card", "Short card, edited");
    fs::write(tmp.path().join("Board.md"), &theirs).unwrap();
    // The vault watcher's reload asks for a refresh: the press defers it.
    rt.update(|s, _| s.refresh_kanban());
    frame(&mut rt);
    assert_eq!(source(&rt), BOARD, "the board under the drag is kept");
    assert!(has(&mut rt, "kanban-drop"), "and the drag is still held");
    drop_at(&mut rt, to);
    assert_eq!(source(&rt), BOARD, "dropping over itself changes nothing");
    // The next refresh, after the release, applies what is on disk.
    rt.update(|s, _| s.refresh_kanban());
    frame(&mut rt);
    assert_eq!(source(&rt), theirs);
    assert!(!dirty(&rt));
}

// P1-1: a lane menu names a lane by place. A keyboard lane move shifts that
// place, so the menu must close rather than let Delete lane hit another lane.

#[test]
fn a_keyboard_lane_move_closes_an_open_lane_menu_so_delete_cannot_hit_a_neighbour() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-card-0-0");
    click(&mut rt, "kanban-lane-0-menu");
    assert!(has(&mut rt, "menu-delete_lane"), "the lane menu is open");
    key(&mut rt, Key::Right, alt(true));
    let moved = board_after(|b| b.move_lane(0, 1).expect("move"));
    assert_eq!(titles(&rt), ["Doing", "To do", "Waiting", "Done"]);
    assert_eq!(source(&rt), moved);
    assert!(rt.state.menu.is_none(), "the menu closed with the move");
    // Lane 0 is "Doing" now; the menu's Delete for "To do" must not act.
    act(&mut rt, |s| s.pick_menu("delete_lane"));
    assert_eq!(source(&rt), moved, "nothing was deleted without reopening");
    // Reopened on lane 0, Delete takes that lane and its cards, and no other.
    act(&mut rt, |s| s.open_lane_menu(0));
    act(&mut rt, |s| s.pick_menu("delete_lane"));
    act(&mut rt, |s| s.pick_menu("delete"));
    assert_eq!(titles(&rt), ["To do", "Waiting", "Done"]);
    assert_eq!(
        cards(&rt, 0),
        ["Short card", "A tall", "Another short", "Last short"],
        "the other lane's cards are all still there"
    );
    assert_eq!(on_disk(&tmp), BOARD, "nothing written before ⌘S");
}

// P2: a held drag defers the vault watcher's refresh. When the file was
// renamed in the meantime, the page must follow the new name once the press
// ends, without waiting for another vault event.

#[test]
fn a_rename_during_a_held_drag_is_followed_after_release() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    let from = centre(rect(&mut rt, "kanban-card-0-0"));
    let to = (from.0, from.1 + 6.0);
    hold(&mut rt, from, to);
    assert!(has(&mut rt, "kanban-drop"), "the drag is held");
    // Another app renames the file while the press is held.
    fs::rename(tmp.path().join("Board.md"), tmp.path().join("Renamed.md")).unwrap();
    // Long enough for the vault watcher: its reload is deferred by the press.
    for _ in 0..120 {
        frame(&mut rt);
    }
    assert_eq!(
        rt.state.kanban.as_ref().unwrap().rel,
        "Board.md",
        "the press keeps the page where it is"
    );
    assert!(has(&mut rt, "kanban-drop"), "and the drag is still held");
    drop_at(&mut rt, to);
    // No other vault event comes: only the kanban watcher looks again.
    for _ in 0..180 {
        frame(&mut rt);
    }
    assert_eq!(
        rt.state.kanban.as_ref().unwrap().rel,
        "Renamed.md",
        "the page follows the rename once the press ends"
    );
    assert_eq!(source(&rt), BOARD, "with its text");
    assert!(!dirty(&rt));
    assert!(!tmp.path().join("Board.md").exists());
}

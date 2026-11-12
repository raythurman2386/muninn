//! Kanban slice 4: card editing on the Kanban page. Each action changes only
//! its own lines, nothing is written before `⌘S`, undo puts back the exact
//! bytes, done lanes make done cards, multibyte text edits cleanly, a refused
//! edit changes nothing, and unsaved edits survive leaving the page.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Ime, Key, KeyEvent, Mods, Runtime};

const BOARD: &str = include_str!("../../muninn-core/tests/fixtures/kanban/edit-board.md");
const SPRINT: &str = include_str!("../../muninn-core/tests/fixtures/kanban/lanes-and-limits.md");
const CRLF: &str = include_str!("../../muninn-core/tests/fixtures/kanban/crlf.md");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-edit-{tag}-{}-{}",
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
        ("Agenda.md", "# Agenda\n"),
        ("Board.md", BOARD),
        ("Sprint.md", SPRINT),
    ])
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
    let mut rt = Runtime::new(state, view, (1400.0, 900.0), Env::new(true, 2.0));
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

/// A state change from outside a handler, as a click or key would make it.
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
}

fn click(rt: &mut Runtime<State>, key: &str) {
    // Let a menu or editor that just opened finish moving into place.
    for _ in 0..30 {
        frame(rt);
    }
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
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

fn on_disk(tmp: &TempVault) -> String {
    fs::read_to_string(tmp.path().join("Board.md")).expect("Board.md")
}

fn text_of(rt: &Runtime<State>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
}

/// The lines `after` has in place of `before`'s: (removed, added), after the
/// common first and last lines.
fn diff(before: &str, after: &str) -> (Vec<String>, Vec<String>) {
    let a: Vec<&str> = before.split_inclusive('\n').collect();
    let b: Vec<&str> = after.split_inclusive('\n').collect();
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..]
        .iter()
        .rev()
        .zip(b[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let own = |lines: &[&str]| lines.iter().map(|l| (*l).to_owned()).collect();
    (own(&a[head..a.len() - tail]), own(&b[head..b.len() - tail]))
}

fn lines(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| (*l).to_owned()).collect()
}

/// Each action as the page offers it, with the lines it changes. The file
/// is untouched until `⌘S`, then holds exactly the edited text.
#[test]
fn each_action_changes_only_its_lines_and_nothing_is_written_before_save() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    let mut before = source(&rt);
    assert_eq!(before, BOARD);
    let mut step = |rt: &mut Runtime<State>, removed: &[&str], added: &[&str], what: &str| {
        let after = source(rt);
        assert_eq!(
            diff(&before, &after),
            (lines(removed), lines(added)),
            "{what}"
        );
        assert_eq!(on_disk(&tmp), BOARD, "{what}: nothing written yet");
        before = after;
    };

    // + Add card: at the end of the lane, before the paragraph after it.
    click(&mut rt, "kanban-lane-0-add");
    assert!(rt.state.is_typing(), "the editor has the keyboard");
    type_text(&mut rt, "Plan the demo");
    press(&mut rt, Key::Enter, false);
    assert!(!rt.state.is_typing());
    step(&mut rt, &[], &["- [ ] Plan the demo\n"], "add");
    assert_eq!(
        text_of(&rt, "kanban-status").as_deref(),
        Some("Unsaved · ⌘S saves")
    );

    // Edit, with a multibyte line and a Shift+Enter second line.
    act(&mut rt, |s| s.begin_edit_card(0, 2));
    press(&mut rt, Key::Text(" ".to_owned()), false);
    type_text(&mut rt, "漢字 🎉 café");
    press(&mut rt, Key::Enter, true);
    type_text(&mut rt, "second line");
    press(&mut rt, Key::Enter, false);
    step(
        &mut rt,
        &["- [ ] Plan the demo\n"],
        &["- [ ] Plan the demo 漢字 🎉 café\n", "  second line\n"],
        "edit",
    );

    // The card menu: Duplicate places a copy right after.
    click(&mut rt, "kanban-card-0-0-menu");
    click(&mut rt, "menu-duplicate");
    step(
        &mut rt,
        &[],
        &["- [ ] Write the release notes\n"],
        "duplicate",
    );
    // Insert before / after open an editor at that place.
    click(&mut rt, "kanban-card-0-0-menu");
    click(&mut rt, "menu-insert_before");
    type_text(&mut rt, "First");
    press(&mut rt, Key::Enter, false);
    step(&mut rt, &[], &["- [ ] First\n"], "insert before");
    assert_eq!(
        rt.state
            .kanban
            .as_ref()
            .unwrap()
            .board
            .as_ref()
            .unwrap()
            .lanes()[0]
            .cards()[0]
            .text(),
        "First"
    );
    click(&mut rt, "kanban-card-0-0-menu");
    click(&mut rt, "menu-insert_after");
    type_text(&mut rt, "Second");
    press(&mut rt, Key::Enter, false);
    step(&mut rt, &[], &["- [ ] Second\n"], "insert after");
    assert_eq!(
        rt.state
            .kanban
            .as_ref()
            .unwrap()
            .board
            .as_ref()
            .unwrap()
            .lanes()[0]
            .cards()[1]
            .text(),
        "Second"
    );

    // Split: the two-line card becomes two cards; the first keeps its id.
    click(&mut rt, "kanban-card-0-4-menu");
    click(&mut rt, "menu-split");
    step(
        &mut rt,
        &["  with the team\n"],
        &["- [ ] with the team\n"],
        "split",
    );

    // Delete.
    click(&mut rt, "kanban-card-0-1-menu");
    click(&mut rt, "menu-delete");
    step(&mut rt, &["- [ ] Second\n"], &[], "delete");

    // The checkbox toggles one character.
    click(&mut rt, "kanban-card-1-0-mark");
    step(
        &mut rt,
        &["- [ ] Fix the sync bug\n"],
        &["- [x] Fix the sync bug\n"],
        "toggle",
    );

    // ⌘S writes once: the file is the edited text, byte for byte.
    let edited = source(&rt);
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), edited);
    assert_eq!(text_of(&rt, "kanban-status").as_deref(), Some("Saved"));
    assert_eq!(
        edited,
        "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] First\n\
         - [ ] Write the release notes\n- [ ] Write the release notes\n\
         - [ ] Review [[Roadmap]] ^card-2a\n- [ ] with the team\n\
         - [ ] Plan the demo 漢字 🎉 café\n  second line\n\n\
         A paragraph between lanes.\n\n## Doing (2)\n\n- [x] Fix the sync bug\n\n\n\
         ## Done\n\n**Complete**\n- [x] Ship 0.7.1\n- [ ] Tag the release\n"
    );
    // The note's index follows the saved file.
    let id = rt.state.vault.resolve("Board").unwrap();
    assert_eq!(rt.state.vault.notes[id].source, edited);
}

/// Undo puts back the exact bytes before each action, redo the bytes after;
/// undoing to the start and saving writes the original file.
#[test]
fn undo_and_redo_restore_exact_bytes_and_a_save_after_undo_restores_the_file() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    let mut history = vec![source(&rt)];
    let actions: [fn(&mut State); 5] = [
        |s| s.toggle_card_done(0, 0),
        |s| s.duplicate_card(0, 1),
        |s| s.split_card(0, 1),
        |s| s.delete_card(1, 0),
        |s| s.mark_lane_done(2),
    ];
    for action in actions {
        act(&mut rt, action);
        history.push(source(&rt));
    }
    assert!(
        history.windows(2).all(|w| w[0] != w[1]),
        "each action changed the text"
    );
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), history[5]);
    for want in history.iter().rev().skip(1) {
        rt.key(command('z', false));
        frame(&mut rt);
        assert_eq!(&source(&rt), want);
    }
    for want in &history[1..] {
        rt.key(command('z', true));
        frame(&mut rt);
        assert_eq!(&source(&rt), want);
    }
    for _ in 0..5 {
        rt.key(command('z', false));
        frame(&mut rt);
    }
    assert_eq!(source(&rt), BOARD);
    // Ctrl+Y is redo too, and a new edit clears redo.
    rt.key(command('y', false));
    frame(&mut rt);
    assert_eq!(source(&rt), history[1]);
    rt.key(command('z', false));
    frame(&mut rt);
    act(&mut rt, |s| s.toggle_card_done(1, 0));
    rt.key(command('y', false));
    frame(&mut rt);
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), BOARD);
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), BOARD, "the original bytes, exactly");
}

/// A card added to, duplicated in or split inside a done lane is born done,
/// and Mark all cards done marks every open card, one character each.
#[test]
fn done_lanes_make_done_cards_and_mark_all_marks_each_open_card() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-lane-2-add");
    type_text(&mut rt, "Write the notes");
    press(&mut rt, Key::Enter, false);
    let after_add = source(&rt);
    assert_eq!(
        diff(BOARD, &after_add),
        (vec![], lines(&["- [x] Write the notes\n"]))
    );
    act(&mut rt, |s| s.duplicate_card(2, 1));
    let after_dup = source(&rt);
    assert_eq!(
        diff(&after_add, &after_dup),
        (vec![], lines(&["- [x] Tag the release\n"])),
        "an open card duplicated in a done lane is done"
    );
    act(&mut rt, |s| s.begin_edit_card(2, 0));
    press(&mut rt, Key::Enter, true);
    type_text(&mut rt, "and announce it");
    press(&mut rt, Key::Enter, false);
    let edited = source(&rt);
    act(&mut rt, |s| s.split_card(2, 0));
    assert_eq!(
        diff(&edited, &source(&rt)),
        (
            lines(&["  and announce it\n"]),
            lines(&["- [x] and announce it\n"])
        )
    );
    let split = source(&rt);
    click(&mut rt, "kanban-lane-2-menu");
    click(&mut rt, "menu-mark_done");
    assert_eq!(
        diff(&split, &source(&rt)),
        (
            lines(&["- [ ] Tag the release\n"]),
            lines(&["- [x] Tag the release\n"])
        ),
        "only the open card changes"
    );
    // One edit: one undo takes it back.
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), split);
    assert_eq!(on_disk(&tmp), BOARD);
}

/// The editor moves by whole characters through CJK, emoji and combining
/// marks, and Esc cancels without an edit.
#[test]
fn multibyte_text_edits_by_whole_characters_and_esc_cancels() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-lane-1-add");
    type_text(&mut rt, "e\u{301}漢👍🏽z");
    // Back over z and the emoji with its modifier, then type before them.
    press(&mut rt, Key::Left, false);
    press(&mut rt, Key::Left, false);
    type_text(&mut rt, "字");
    press(&mut rt, Key::Enter, false);
    let card = &rt
        .state
        .kanban
        .as_ref()
        .unwrap()
        .board
        .as_ref()
        .unwrap()
        .lanes()[1]
        .cards()[1];
    assert_eq!(card.text(), "e\u{301}漢字👍🏽z");
    let before = source(&rt);
    act(&mut rt, |s| s.begin_edit_card(1, 1));
    type_text(&mut rt, "never kept");
    press(&mut rt, Key::Escape, false);
    assert!(!rt.state.is_typing());
    assert_eq!(
        rt.state.mode,
        Mode::Kanban,
        "Esc in the editor stays on the page"
    );
    assert_eq!(source(&rt), before);
}

/// An edit the model refuses changes nothing, in memory or on disk, and
/// says why without blocking the page.
#[test]
fn a_refused_edit_changes_nothing() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    // A trailing ` ^id` would be read back as a block id, not as the text.
    click(&mut rt, "kanban-lane-0-add");
    type_text(&mut rt, "Call Ana ^x1");
    press(&mut rt, Key::Enter, false);
    assert_eq!(source(&rt), BOARD);
    assert!(
        rt.state
            .notice
            .as_deref()
            .is_some_and(|n| n.starts_with("Could not add the card")),
        "{:?}",
        rt.state.notice
    );
    assert!(!rt.state.kanban_dirty());
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), BOARD, "nothing to undo");
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), BOARD);
}

/// New lines use the file's line ending.
#[test]
fn a_crlf_file_gets_crlf_cards() {
    let tmp = TempVault::new(&[("Board.md", CRLF)]);
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-lane-0-add");
    type_text(&mut rt, "Two");
    press(&mut rt, Key::Enter, true);
    type_text(&mut rt, "lines");
    press(&mut rt, Key::Enter, false);
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(
        on_disk(&tmp),
        "---\r\nkanban-plugin: board\r\n---\r\n\r\n## To do\r\n\r\n- [ ] One card\r\n- [ ] Two\r\n  lines\r\n"
    );
}

/// Board's rule for unsaved work: leaving the page keeps the edits, and
/// picking another kanban saves them first.
#[test]
fn unsaved_edits_survive_leaving_the_page_and_save_before_another_kanban() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.toggle_card_done(0, 0));
    let edited = source(&rt);
    rt.key(command('1', false));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Editor);
    assert_eq!(on_disk(&tmp), BOARD, "leaving does not save");
    rt.key(command('5', false));
    frame(&mut rt);
    assert_eq!(
        source(&rt),
        edited,
        "coming back does not re-read over them"
    );
    // An open editor goes with the page too: leaving commits it.
    click(&mut rt, "kanban-lane-1-add");
    type_text(&mut rt, "Typed");
    rt.key(command('2', false));
    frame(&mut rt);
    assert!(!rt.state.is_typing());
    rt.key(command('5', false));
    frame(&mut rt);
    let edited = source(&rt);
    assert!(edited.contains("- [ ] Typed\n"));

    let sprint = rt.state.vault.resolve("Sprint").unwrap();
    act(&mut rt, |s| s.open_kanban(sprint));
    assert_eq!(on_disk(&tmp), edited, "saved before the switch");
    assert_eq!(rt.state.kanban.as_ref().unwrap().rel, "Sprint.md");
}

/// A file another app changed is not written over: the save is refused, the
/// edits stay, and the page does not switch away from them.
#[test]
fn a_file_changed_on_disk_refuses_the_save_and_keeps_the_edits() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.toggle_card_done(0, 0));
    let edited = source(&rt);
    let theirs = format!("{BOARD}- [ ] Added elsewhere, and longer\n");
    fs::write(tmp.path().join("Board.md"), &theirs).unwrap();
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), theirs);
    assert_eq!(
        text_of(&rt, "kanban-status").as_deref(),
        Some("Not saved: the file changed on disk")
    );
    let sprint = rt.state.vault.resolve("Sprint").unwrap();
    act(&mut rt, |s| s.open_kanban(sprint));
    assert_eq!(rt.state.kanban.as_ref().unwrap().rel, "Board.md");
    assert_eq!(source(&rt), edited);
    assert!(
        !rt.state.flush_before_quit(),
        "the quit is refused, with the reason"
    );
    assert!(rt.state.error.is_some());
    assert_eq!(on_disk(&tmp), theirs);
}

/// The close path saves unsaved kanban edits, as it saves a board.
#[test]
fn quitting_saves_unsaved_edits() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-lane-0-add");
    type_text(&mut rt, "Still typing");
    assert!(rt.state.flush_before_quit());
    assert!(on_disk(&tmp).contains("- [ ] Still typing\n"));
}

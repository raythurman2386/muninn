//! Kanban slice 4: card editing on the Kanban page. Each action changes only
//! its own lines, nothing is written before `⌘S`, undo puts back the exact
//! bytes, done lanes make done cards, multibyte text edits cleanly, a refused
//! edit changes nothing, and unsaved edits survive leaving the page. Esc
//! cancels an open field even after it lost focus.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{CardSlot, Mode, Settings, State, on_key, poll, view};
use rune::{Env, Ime, Key, KeyEvent, Mods, Runtime};

const BOARD: &str = include_str!("../../muninn-core/tests/fixtures/kanban/edit-board.md");
const SPRINT: &str = include_str!("../../muninn-core/tests/fixtures/kanban/lanes-and-limits.md");
const CRLF: &str = include_str!("../../muninn-core/tests/fixtures/kanban/crlf.md");

/// The unsaved indicator, with the platform's save chord.
const UNSAVED: &str = if cfg!(target_os = "macos") {
    "Unsaved · ⌘S saves"
} else {
    "Unsaved · Ctrl+S saves"
};

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
        // These tests pin slice 4's explicit save; slice 7 tests autosave.
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
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
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
    assert_eq!(text_of(&rt, "kanban-status").as_deref(), Some(UNSAVED));

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

/// An edit the model refuses changes nothing, in memory or on disk, says
/// why on the page, and keeps the typed text in the open editor.
#[test]
fn a_refused_edit_changes_nothing_and_keeps_the_typed_text() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    // A line of only spaces cannot be read back as the same card text.
    click(&mut rt, "kanban-lane-0-add");
    type_text(&mut rt, "Call Ana\n ");
    press(&mut rt, Key::Enter, false);
    assert_eq!(source(&rt), BOARD);
    assert!(rt.state.is_typing(), "the editor stays open");
    assert_eq!(rt.state.kanban_card_field.text, "Call Ana\n ");
    let error = text_of(&rt, "kanban-error").unwrap_or_default();
    assert!(error.starts_with("Could not add the card"), "{error:?}");
    // ⌘S can't save text the model refuses: the file and the text stay.
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(on_disk(&tmp), BOARD);
    assert_eq!(rt.state.kanban_card_field.text, "Call Ana\n ");
    assert_eq!(
        text_of(&rt, "kanban-status").as_deref(),
        Some("Not saved: the card being edited was refused")
    );
    // Nor can quitting; the reason shows on the page.
    let mut quits = true;
    act(&mut rt, |s| quits = s.flush_before_quit());
    assert!(!quits);
    let error = text_of(&rt, "kanban-app-error").unwrap_or_default();
    assert!(error.contains("could not be saved"), "{error:?}");
    assert!(
        !error.contains("elsewhere"),
        "Open as markdown shows the file, not the edits"
    );
    assert_eq!(on_disk(&tmp), BOARD);
    // Fixing the text adds the card and clears the message.
    act(&mut rt, |s| s.kanban_card_field.replace_text("Call Ana"));
    press(&mut rt, Key::Enter, false);
    assert!(!rt.state.is_typing());
    assert!(source(&rt).contains("- [ ] Call Ana\n"));
    assert!(text_of(&rt, "kanban-error").is_none());
    // A refused edit of a card keeps its text too, and Esc lets it go.
    act(&mut rt, |s| s.begin_edit_card(0, 0));
    act(&mut rt, |s| s.kanban_card_field.replace_text("Reworded\n "));
    press(&mut rt, Key::Enter, false);
    assert_eq!(rt.state.kanban_card_field.text, "Reworded\n ");
    assert!(text_of(&rt, "kanban-error").is_some());
    press(&mut rt, Key::Escape, false);
    assert!(!rt.state.is_typing());
    assert!(text_of(&rt, "kanban-error").is_none());
    assert!(source(&rt).contains("- [ ] Write the release notes\n"));
}

/// The page draws cards as they were when it was built. With an Insert card
/// editor open above a card, acting on that card commits the editor first,
/// which moves the card down one; each action follows it there.
#[test]
fn actions_on_a_card_follow_it_past_the_card_an_open_editor_adds() {
    let open_insert_before = |rt: &mut Runtime<State>| {
        click(rt, "kanban-card-0-0-menu");
        click(rt, "menu-insert_before");
        type_text(rt, "New first");
        assert!(rt.state.is_typing());
    };
    let first = "- [ ] New first\n";
    let card = "- [ ] Write the release notes\n";

    // The checkbox.
    let tmp = vault();
    let mut rt = on_board(&tmp);
    open_insert_before(&mut rt);
    click(&mut rt, "kanban-card-0-0-mark");
    assert!(
        source(&rt).contains(&format!("{first}- [x] Write the release notes\n")),
        "{}",
        source(&rt)
    );

    // The card's menu button, then Delete.
    let tmp = vault();
    let mut rt = on_board(&tmp);
    open_insert_before(&mut rt);
    click(&mut rt, "kanban-card-0-0-menu");
    click(&mut rt, "menu-delete");
    let after = source(&rt);
    assert!(after.contains(first) && !after.contains(card), "{after}");

    // A right-click on the card, then Duplicate.
    let tmp = vault();
    let mut rt = on_board(&tmp);
    open_insert_before(&mut rt);
    for _ in 0..30 {
        frame(&mut rt);
    }
    let (x, y) = rt.find("kanban-card-0-0").expect("card");
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    frame(&mut rt);
    click(&mut rt, "menu-duplicate");
    assert!(
        source(&rt).contains(&format!("{first}{card}{card}")),
        "{}",
        source(&rt)
    );

    // A double click edits it.
    let tmp = vault();
    let mut rt = on_board(&tmp);
    open_insert_before(&mut rt);
    for _ in 0..30 {
        frame(&mut rt);
    }
    let (x, y) = rt.find("kanban-card-0-0").expect("card");
    for _ in 0..2 {
        rt.pointer_down(x, y);
        frame(&mut rt);
        rt.pointer_up(x, y);
        frame(&mut rt);
    }
    frame(&mut rt);
    assert!(source(&rt).contains(&format!("{first}{card}")));
    assert_eq!(
        rt.state.kanban_card,
        Some(CardSlot {
            lane: 0,
            index: 1,
            new: false
        })
    );
    assert_eq!(rt.state.kanban_card_field.text, "Write the release notes");

    // A card the editor adds below the one clicked leaves it in place.
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-card-0-0-menu");
    click(&mut rt, "menu-insert_after");
    type_text(&mut rt, "Next");
    click(&mut rt, "kanban-card-0-0-mark");
    assert!(
        source(&rt).contains("- [x] Write the release notes\n- [ ] Next\n"),
        "{}",
        source(&rt)
    );
}

/// Switching vaults saves the kanban's edits into the vault they belong to
/// first, and is refused, with the reason on the page, when that fails.
#[test]
fn switching_vaults_saves_the_kanban_first() {
    let tmp = vault();
    let other = TempVault::new(&[("Other.md", "# Other\n")]);
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.toggle_card_done(0, 0));
    let edited = source(&rt);
    assert!(rt.state.switch_vault(other.path().to_path_buf()));
    assert_eq!(on_disk(&tmp), edited, "saved before the switch");

    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.toggle_card_done(0, 0));
    let theirs = format!("{BOARD}- [ ] Added elsewhere, and longer\n");
    fs::write(tmp.path().join("Board.md"), &theirs).unwrap();
    let mut switched = true;
    act(&mut rt, |s| {
        switched = s.switch_vault(other.path().to_path_buf())
    });
    assert!(!switched);
    assert_eq!(on_disk(&tmp), theirs);
    assert_eq!(rt.state.kanban.as_ref().unwrap().rel, "Board.md");
    assert_eq!(
        text_of(&rt, "kanban-app-error").as_deref(),
        Some("Save the current kanban before switching vaults")
    );
}

/// After ⌘S on the selected note's kanban, Write shows the saved text, not
/// the text it read before the edits.
#[test]
fn the_write_editor_shows_the_saved_text() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    let board = rt.state.vault.resolve("Board").unwrap();
    assert_eq!(rt.state.selected, Some(board));
    act(&mut rt, |s| s.toggle_card_done(0, 0));
    rt.key(command('s', false));
    frame(&mut rt);
    let saved = on_disk(&tmp);
    assert_ne!(saved, BOARD);
    rt.key(command('1', false));
    frame(&mut rt);
    assert_eq!(rt.state.mode, Mode::Editor);
    assert_eq!(rt.state.editor.text, saved);
    assert!(!rt.state.dirty(), "the note buffer matches the file");
}

/// An action that changes nothing is not an edit: no history entry, nothing
/// unsaved.
#[test]
fn an_action_that_changes_nothing_is_not_recorded() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    // The Done lane's open card, then Mark all cards done twice: the second
    // finds nothing open.
    act(&mut rt, |s| s.mark_lane_done(2));
    let once = source(&rt);
    assert_ne!(once, BOARD);
    act(&mut rt, |s| s.mark_lane_done(2));
    // Editing a card to its own text.
    act(&mut rt, |s| s.begin_edit_card(0, 0));
    press(&mut rt, Key::Enter, false);
    assert_eq!(source(&rt), once);
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), BOARD, "one undo goes back past the one edit");
    assert!(!rt.state.kanban_dirty());
    rt.key(command('z', false));
    frame(&mut rt);
    assert_eq!(source(&rt), BOARD, "nothing else was recorded");
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
/// edits stay, and the page does not switch away from them. Quitting keeps
/// them in the recovery copy (slice 7).
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
    // Slice 7: the quit goes ahead with the edits in the recovery copy.
    assert!(rt.state.flush_before_quit(), "kept in the recovery copy");
    let copy = rt.state.recovery_path("Board.md");
    assert_eq!(fs::read_to_string(copy).unwrap(), edited);
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

/// A click on the page header takes the keyboard from an open field, as a
/// click on the sidebar or another page would.
fn blur(rt: &mut Runtime<State>) {
    let (x, y) = rt.find("kanban-title").expect("kanban title");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..2 {
        frame(rt);
    }
}

/// Alt with an arrow: a card move on the page.
fn alt_press(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods {
            alt: true,
            ..Mods::default()
        },
    });
    frame(rt);
}

/// Ctrl/Cmd is a command chord on the page.
fn page_command(rt: &mut Runtime<State>, c: char) {
    rt.key(command(c, false));
    frame(rt);
}

/// The board after `op` on `BOARD`, through the core model.
fn board_after(op: impl FnOnce(&mut muninn_core::Kanban)) -> String {
    let mut board = muninn_core::Kanban::parse(BOARD).expect("parse");
    op(&mut board);
    board.source().to_owned()
}

// P1-1: a card menu names (lane, index). A keyboard move shifts that index,
// so the menu must close rather than let Delete act on the card now there.

#[test]
fn a_keyboard_move_closes_an_open_card_menu_so_delete_cannot_hit_a_neighbour() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    // The menu is on "Review [[Roadmap]]", the second card.
    act(&mut rt, |s| s.select_kanban_card(0, 1));
    act(&mut rt, |s| s.open_card_menu(0, 1));
    assert!(rt.state.menu.is_some());
    alt_press(&mut rt, Key::Up);
    let moved = board_after(|b| b.move_card(0, 1, 0, 0).expect("move"));
    assert_eq!(source(&rt), moved, "the selected card moved up");
    assert!(rt.state.menu.is_none(), "the menu closed with the move");
    // The index the menu named now holds the other card. Delete must not act.
    act(&mut rt, |s| s.pick_menu("delete"));
    assert_eq!(source(&rt), moved, "nothing was deleted without reopening");
    // Reopened on the card drawn there, Delete takes that card and no other.
    act(&mut rt, |s| s.open_card_menu(0, 0));
    act(&mut rt, |s| s.pick_menu("delete"));
    assert_eq!(
        source(&rt),
        board_after(|b| {
            b.move_card(0, 1, 0, 0).expect("move");
            b.delete_card(0, 0).expect("delete");
        }),
        "the named card is the one deleted"
    );
    assert_eq!(on_disk(&tmp), BOARD, "nothing written before ⌘S");
}

// P2-1: a card editor or lane field that lost its focus is still open. Esc on
// the page cancels it and stays; ⌘Z is the field's own undo, not a commit.

#[test]
fn esc_cancels_a_blurred_card_editor_and_undo_never_commits_it() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.begin_edit_card(0, 0));
    type_text(&mut rt, " EDITED");
    blur(&mut rt);
    assert!(!rt.state.is_typing(), "the click took the keyboard");
    assert!(rt.state.kanban_card.is_some(), "the editor is still open");
    // ⌘Z undoes the typing in the field; it does not commit it to the board.
    page_command(&mut rt, 'z');
    assert_eq!(rt.state.kanban_card_field.text, "Write the release notes");
    assert!(rt.state.kanban_card.is_some(), "the editor stays open");
    assert_eq!(source(&rt), BOARD, "the typing never reached the board");
    // Esc cancels the field and leaves the page where it is.
    press(&mut rt, Key::Escape, false);
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(rt.state.kanban_card, None);
    assert!(!rt.state.kanban.as_ref().unwrap().dirty());
    assert_eq!(source(&rt), BOARD);
    assert_eq!(on_disk(&tmp), BOARD);
}

#[test]
fn esc_cancels_a_blurred_lane_field_and_undo_never_commits_it() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.begin_rename_lane(1));
    type_text(&mut rt, " renamed");
    blur(&mut rt);
    assert!(!rt.state.is_typing(), "the click took the keyboard");
    assert!(
        rt.state.kanban_lane_edit.is_some(),
        "the field is still open"
    );
    page_command(&mut rt, 'z');
    assert_eq!(rt.state.kanban_lane_field.text, "Doing");
    assert!(rt.state.kanban_lane_edit.is_some(), "the field stays open");
    assert_eq!(source(&rt), BOARD, "the typing never reached the board");
    press(&mut rt, Key::Escape, false);
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert_eq!(rt.state.kanban_lane_edit, None);
    assert!(!rt.state.kanban.as_ref().unwrap().dirty());
    assert_eq!(source(&rt), BOARD);
    assert_eq!(on_disk(&tmp), BOARD);
}

// P2-2: Open as markdown saves first, so Write shows the card; a refused save
// keeps the page, with its reason.

#[test]
fn open_as_markdown_saves_a_new_card_before_showing_the_note() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-lane-0-add");
    type_text(&mut rt, "From the board");
    press(&mut rt, Key::Enter, false);
    let edited = source(&rt);
    assert_ne!(edited, BOARD);
    assert_eq!(on_disk(&tmp), BOARD, "not written before the switch");
    act(&mut rt, |s| s.open_kanban_as_markdown());
    assert_eq!(rt.state.mode, Mode::Editor);
    assert_eq!(on_disk(&tmp), edited, "the card is in the file");
    assert_eq!(rt.state.editor.text, edited, "and in the editor");
    // The board's save is not left refused by the switch.
    rt.key(command('5', false));
    frame(&mut rt);
    assert_eq!(source(&rt), edited);
    act(&mut rt, |s| s.toggle_card_done(0, 0));
    rt.key(command('s', false));
    frame(&mut rt);
    assert_eq!(
        text_of(&rt, "kanban-status").as_deref(),
        Some("Saved"),
        "the board can save again"
    );

    let tmp = vault();
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-lane-0-add");
    type_text(&mut rt, "From the board");
    press(&mut rt, Key::Enter, false);
    let theirs = format!("{BOARD}- [ ] Added elsewhere, and longer\n");
    fs::write(tmp.path().join("Board.md"), &theirs).unwrap();
    act(&mut rt, |s| s.open_kanban_as_markdown());
    assert_eq!(rt.state.mode, Mode::Kanban, "refused: the page stays");
    assert!(
        matches!(
            rt.state.kanban.as_ref().map(|k| k.status.clone()),
            Some(muninn_ui::BoardStatus::Failed(reason))
                if reason == "the file changed on disk"
        ),
        "the page says why"
    );
    assert_eq!(on_disk(&tmp), theirs, "their file is untouched");
}

// P2-3: the selection is a card, not a place: undo, redo and a re-read put it
// back on the same card, or clear it when that card is gone.

#[test]
fn undo_and_redo_follow_the_selected_card_or_drop_it() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.select_kanban_card(0, 1));
    act(&mut rt, |s| s.begin_insert_card(0, 0));
    type_text(&mut rt, "New first");
    press(&mut rt, Key::Enter, false);
    let inserted = board_after(|b| b.add_card(0, 0, "New first").expect("add"));
    assert_eq!(source(&rt), inserted);
    assert_eq!(
        rt.state.kanban_selected,
        Some((0, 2)),
        "the commit moved the selected card down"
    );
    // Undo: the inserted card goes, and the selection is still "Review".
    page_command(&mut rt, 'z');
    assert_eq!(source(&rt), BOARD);
    assert_eq!(rt.state.kanban_selected, Some((0, 1)));
    // Redo: it comes back above, and the selection follows it down again.
    page_command(&mut rt, 'y');
    assert_eq!(source(&rt), inserted);
    assert_eq!(rt.state.kanban_selected, Some((0, 2)));
    // Undo the insertion with that card selected: it no longer exists.
    act(&mut rt, |s| s.select_kanban_card(0, 0));
    page_command(&mut rt, 'z');
    assert_eq!(source(&rt), BOARD);
    assert_eq!(rt.state.kanban_selected, None, "the selected card is gone");
    page_command(&mut rt, 'y');
    assert_eq!(source(&rt), inserted);
    assert_eq!(rt.state.kanban_selected, None, "nothing was selected again");
    assert_eq!(on_disk(&tmp), BOARD);
}

// P2-2: a block id duplicated in an earlier lane must not capture the
// selection: it is counted in the order the restore searches.

#[test]
fn a_duplicate_block_id_in_an_earlier_lane_keeps_the_selection_in_its_lane() {
    let src = "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] One ^dup\n- [ ] Two\n\n## Doing\n\n- [ ] Three ^dup\n- [ ] Four\n";
    let tmp = TempVault::new(&[("Board.md", src)]);
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.select_kanban_card(1, 0));
    act(&mut rt, |s| s.toggle_card_done(1, 0));
    assert_eq!(
        rt.state.kanban_selected,
        Some((1, 0)),
        "the toggle keeps the selected card"
    );
    act(&mut rt, |s| s.undo_kanban());
    assert_eq!(source(&rt), src);
    assert_eq!(
        rt.state.kanban_selected,
        Some((1, 0)),
        "the undo does not move it to lane 0's copy"
    );
}

// P2-3: Copy link gives a text-only card a block id, and its undo takes the
// id away again; the selection stays on the card through both.

#[test]
fn copy_link_and_its_undo_keep_the_selected_card() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.select_kanban_card(0, 1));
    let mut copied = None;
    act(&mut rt, |s| copied = s.copy_card_link(0, 1));
    assert!(copied.is_some(), "a link was copied");
    assert_eq!(
        rt.state.kanban_selected,
        Some((0, 1)),
        "Copy link keeps the selected card"
    );
    act(&mut rt, |s| s.undo_kanban());
    assert_eq!(source(&rt), BOARD, "the id is undone");
    assert_eq!(
        rt.state.kanban_selected,
        Some((0, 1)),
        "and the undo keeps it too"
    );
}

// P2-3: a typo fixed in the selected text-only card elsewhere keeps the
// selection at the same place while the lane still holds the same cards.

#[test]
fn an_external_text_fix_keeps_the_selected_card_in_its_place() {
    let tmp = vault();
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.select_kanban_card(0, 0));
    let fixed = BOARD.replace("Write the release notes", "Write the release note");
    fs::write(tmp.path().join("Board.md"), &fixed).unwrap();
    for _ in 0..120 {
        frame(&mut rt);
    }
    assert_eq!(source(&rt), fixed, "the re-read is shown");
    assert_eq!(
        rt.state.kanban_selected,
        Some((0, 0)),
        "the card keeps its place"
    );
}

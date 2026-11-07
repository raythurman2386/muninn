//! M2 headless tests: in-note find, keyboard search results, Back/Forward note
//! history, per-note undo history (EDT-02) and Markdown comfort (EDT-05).
//!
//! Like `ui.rs`, these drive the real Rune runtime against a temporary vault
//! (no window, no snapshots).

use std::fs;

use muninn_core::testing::TempVault;
use muninn_ui::{State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime, Tx};

fn nav_vault() -> TempVault {
    // Built with `vec!` rather than a tuple array so this fixture stays a
    // distinct helper, not another copy of the suite's `temp_vault`.
    let notes: Vec<(&str, &str)> = vec![
        ("Alpha.md", "A link here.\nAnother link there.\n"),
        ("Beta.md", "Beta also has a link.\n"),
        ("Gamma.md", "Gamma links too.\n"),
    ];
    TempVault::new(&notes)
}

fn runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state")),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

fn key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
}

fn text(s: &str) -> KeyEvent {
    key(Key::Text(s.to_owned()))
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

fn shift(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods {
            shift: true,
            ..Mods::default()
        },
    }
}

/// Type `typed` one character at a time into the focused field, then one
/// settle frame so query-derived state (find matches) is up to date.
fn type_text(rt: &mut Runtime<State>, typed: &str) {
    for ch in typed.chars() {
        rt.key(text(&ch.to_string()));
        frame(rt);
    }
    frame(rt);
}

#[test]
fn find_in_note_counts_wraps_and_escape_closes() {
    let tmp = nav_vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.open_note(alpha);
    frame(&mut rt);

    // Two matches of "link" in Alpha's body.
    let starts: Vec<usize> = rt
        .state
        .editor
        .text
        .match_indices("link")
        .map(|(at, _)| at)
        .collect();
    assert_eq!(starts.len(), 2, "fixture must hold two matches");

    rt.key(command('f'));
    frame(&mut rt);
    assert!(rt.state.find.is_some(), "Cmd/Ctrl+F opens the bar");
    assert!(
        rt.state.find.as_ref().expect("find").field.focused,
        "the find field owns typing"
    );

    type_text(&mut rt, "link");
    let found = rt.state.find.as_ref().expect("find");
    assert_eq!(found.query, "link");
    assert_eq!(found.matches, starts, "match starts are byte offsets");
    assert_eq!(found.index, 0, "the first match is current");
    assert_eq!(
        (rt.state.editor.anchor, rt.state.editor.focus),
        (starts[0], starts[0] + 4),
        "the match is the editor selection"
    );

    // Enter advances; Enter again wraps to the first match.
    rt.key(key(Key::Enter));
    frame(&mut rt);
    let found = rt.state.find.as_ref().expect("find");
    assert_eq!(found.index, 1);
    assert_eq!(rt.state.editor.anchor, starts[1]);
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert_eq!(rt.state.find.as_ref().expect("find").index, 0);
    assert_eq!(rt.state.editor.anchor, starts[0]);

    // Shift+Enter goes back, wrapping to the last match.
    rt.key(shift(Key::Enter));
    frame(&mut rt);
    assert_eq!(rt.state.find.as_ref().expect("find").index, 1);

    // Escape closes and hands typing back to the editor.
    rt.key(key(Key::Escape));
    frame(&mut rt);
    assert!(rt.state.find.is_none(), "Escape closes the bar");
    assert!(rt.state.editor.focused, "focus returns to the editor");
    rt.key(text("x"));
    frame(&mut rt);
    assert!(
        rt.state.editor.text.contains('x'),
        "typing after Escape edits the note"
    );
}

#[test]
fn clearing_the_find_query_closes_the_bar() {
    let tmp = nav_vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.open_note(alpha);
    frame(&mut rt);

    rt.key(command('f'));
    frame(&mut rt);
    type_text(&mut rt, "link");
    assert_eq!(rt.state.find.as_ref().expect("find").matches.len(), 2);

    rt.state.find.as_mut().expect("find").field.replace_text("");
    frame(&mut rt);
    assert!(
        rt.state.find.is_none(),
        "clearing a non-empty query closes the bar"
    );
}

#[test]
fn search_up_down_opens_the_highlighted_result() {
    let tmp = nav_vault();
    let mut rt = runtime(&tmp);
    let beta = rt.state.vault.resolve("Beta").expect("Beta");

    rt.key(command('k'));
    frame(&mut rt);
    assert!(rt.state.search.focused);
    type_text(&mut rt, "a");

    let results = rt.state.search_results();
    assert_eq!(results.len(), 3, "query 'a' matches all three notes");

    // The first Down selects the first row, the second moves to Beta.
    rt.key(key(Key::Down));
    frame(&mut rt);
    assert_eq!(rt.state.search_selected, Some(0));
    rt.key(key(Key::Down));
    frame(&mut rt);
    assert_eq!(rt.state.search_selected, Some(1));

    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert_eq!(
        rt.state.selected,
        Some(beta),
        "Enter opens the highlighted row, not the first"
    );
}

#[test]
fn search_selection_wraps_and_clears_with_the_query() {
    let tmp = nav_vault();
    let mut rt = runtime(&tmp);

    rt.key(command('k'));
    frame(&mut rt);
    type_text(&mut rt, "a");

    // Up from nothing wraps to the last result.
    rt.key(key(Key::Up));
    frame(&mut rt);
    assert_eq!(rt.state.search_selected, Some(2));

    // Down wraps forward to the first.
    rt.key(key(Key::Down));
    frame(&mut rt);
    assert_eq!(rt.state.search_selected, Some(0));

    // Changing the query drops the old highlight.
    rt.state.search.replace_text("");
    frame(&mut rt);
    assert_eq!(
        rt.state.search_selected, None,
        "a query change clears the selection"
    );
}

#[test]
fn history_back_forward_and_truncate_on_new_open() {
    let tmp = nav_vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    let beta = rt.state.vault.resolve("Beta").expect("Beta");
    let gamma = rt.state.vault.resolve("Gamma").expect("Gamma");

    // Startup already opened Alpha; visit Beta then Gamma.
    rt.state.open_note(alpha);
    rt.state.open_note(beta);
    rt.state.open_note(gamma);
    frame(&mut rt);
    assert_eq!(rt.state.history.len(), 3);
    assert_eq!(rt.state.history_index, 2);
    assert!(rt.state.can_back());
    assert!(!rt.state.can_forward());

    // Back via the topbar chip.
    let (x, y) = rt.find("history-back").expect("back chip");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frame(&mut rt);
    assert_eq!(rt.state.selected, Some(beta));
    assert_eq!(rt.state.history_index, 1);

    rt.state.back();
    assert_eq!(rt.state.selected, Some(alpha));
    rt.state.forward();
    assert_eq!(rt.state.selected, Some(beta));
    rt.state.forward();
    assert_eq!(rt.state.selected, Some(gamma));

    // A new open from the middle drops the abandoned forward branch.
    rt.state.back();
    assert_eq!(rt.state.selected, Some(beta));
    rt.state.open_note(alpha);
    assert_eq!(rt.state.selected, Some(alpha));
    assert!(
        !rt.state.can_forward(),
        "opening a new note truncates the forward branch"
    );
    assert!(!rt.state.forward());
}

#[test]
fn history_keeps_the_caret_and_dedupes_consecutive_visits() {
    let tmp = nav_vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    let beta = rt.state.vault.resolve("Beta").expect("Beta");

    rt.state.open_note(alpha);
    let len_before = rt.state.history.len();
    rt.state.load_note(alpha, false);
    assert_eq!(
        rt.state.history.len(),
        len_before,
        "reloading the open note adds no history entry"
    );

    rt.state.editor.anchor = 5;
    rt.state.editor.focus = 5;
    rt.state.open_note(beta);
    assert_eq!(rt.state.history.len(), len_before + 1);
    rt.state.back();
    assert_eq!(rt.state.selected, Some(alpha));
    assert_eq!(
        rt.state.editor.focus, 5,
        "Back restores the caret stored when Alpha was left"
    );
}

// ---- M2 EDT-05: Markdown comfort --------------------------------------------

/// Put `text` in the editor with the caret at its end and typing focused.
fn stage(rt: &mut Runtime<State>, text: &str) {
    rt.state.editor.replace_text(text);
    rt.state.focus_editor();
    frame(rt);
}

fn enter(rt: &mut Runtime<State>) {
    rt.key(key(Key::Enter));
    frame(rt);
}

#[test]
fn enter_continues_bullet_lists_keeping_the_indent() {
    let tmp = nav_vault();
    let mut rt = runtime(&tmp);

    stage(&mut rt, "- one");
    enter(&mut rt);
    assert_eq!(rt.state.editor.text, "- one\n- ");
    assert_eq!(rt.state.editor.focus, "- one\n- ".len());

    // Typing continues the new item, and the next Enter starts another.
    rt.key(text("two"));
    enter(&mut rt);
    assert_eq!(rt.state.editor.text, "- one\n- two\n- ");

    stage(&mut rt, "  * indented");
    enter(&mut rt);
    assert_eq!(
        rt.state.editor.text, "  * indented\n  * ",
        "the new marker keeps the original indentation"
    );
}

#[test]
fn enter_increments_numbered_lists() {
    let tmp = nav_vault();
    let mut rt = runtime(&tmp);

    stage(&mut rt, "1. first");
    enter(&mut rt);
    assert_eq!(rt.state.editor.text, "1. first\n2. ");

    stage(&mut rt, "9. ninth");
    enter(&mut rt);
    assert_eq!(rt.state.editor.text, "9. ninth\n10. ");
}

#[test]
fn enter_on_an_empty_list_item_ends_the_list() {
    let tmp = nav_vault();
    let mut rt = runtime(&tmp);

    stage(&mut rt, "- one\n- ");
    enter(&mut rt);
    assert_eq!(rt.state.editor.text, "- one\n");
    assert_eq!(
        rt.state.editor.focus,
        "- one\n".len(),
        "caret stays on the new empty paragraph"
    );

    stage(&mut rt, "    * ");
    enter(&mut rt);
    assert_eq!(
        rt.state.editor.text, "",
        "the marker and its indent go away"
    );
}

#[test]
fn enter_closes_an_unclosed_wikilink_with_the_caret_inside() {
    let tmp = nav_vault();
    let mut rt = runtime(&tmp);

    // With the autocomplete list open Enter belongs to it (EDT-03), so this
    // case is the list declined: no candidate, or Escape dismissed it.
    stage(&mut rt, "See [[Alpha");
    rt.state.dismiss_complete();
    enter(&mut rt);
    assert_eq!(rt.state.editor.text, "See [[Alpha]]");
    assert_eq!(
        rt.state.editor.focus,
        "See [[Alpha".len(),
        "caret sits between the closing brackets"
    );

    // A query with no matching note leaves the completion closed; Enter still
    // closes the brackets.
    stage(&mut rt, "See [[zzzz");
    enter(&mut rt);
    assert_eq!(rt.state.editor.text, "See [[zzzz]]");

    // A closed link is ordinary text: Enter continues the paragraph.
    stage(&mut rt, "See [[Alpha]]");
    enter(&mut rt);
    assert_eq!(rt.state.editor.text, "See [[Alpha]]\n");
}

#[test]
fn tab_indents_four_spaces_without_leaving_the_editor() {
    let tmp = nav_vault();
    let mut rt = runtime(&tmp);

    stage(&mut rt, "ab");
    rt.state.editor.focus = 1;
    rt.state.editor.anchor = 1;
    rt.key(key(Key::Tab));
    frame(&mut rt);

    assert_eq!(rt.state.editor.text, "a    b");
    assert_eq!(rt.state.editor.focus, 5);
    assert!(rt.state.editor.focused, "Tab stays in the note");
}

// ---- M2 EDT-02: per-note undo history ---------------------------------------

#[test]
fn switching_notes_preserves_each_notes_undo_stack() {
    let tmp = nav_vault();
    let mut s =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    let alpha = s.vault.resolve("Alpha").expect("Alpha");
    let beta = s.vault.resolve("Beta").expect("Beta");
    let original = s.vault.notes[alpha].source.clone();
    let beta_text = s.vault.notes[beta].source.clone();

    s.editor.focus = s.editor.text.len();
    s.editor.anchor = s.editor.focus;
    s.editor.insert("typed");

    // A -> B -> A: the first visit's text and undo stack come back.
    s.open_note(beta);
    assert_eq!(s.editor.text, beta_text);
    s.open_note(alpha);
    assert_eq!(s.editor.text, format!("{original}typed"));
    // Rune's own Cmd+Z handler, the path the runtime reaches for the chord.
    let _ = s.editor.key(&command('z'), None);
    assert_eq!(
        s.editor.text, original,
        "Cmd+Z undoes the edit made on the first visit"
    );
}

#[test]
fn a_never_edited_note_has_no_phantom_history() {
    let tmp = nav_vault();
    let mut s =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    let beta = s.vault.resolve("Beta").expect("Beta");

    s.editor.focus = s.editor.text.len();
    s.editor.insert("typed");
    s.open_note(beta);

    let untouched = s.editor.text.clone();
    s.editor.undo();
    assert_eq!(
        s.editor.text, untouched,
        "Beta was never edited, so it must not borrow Alpha's history"
    );
}

#[test]
fn save_and_mode_switches_keep_the_note_history() {
    let tmp = nav_vault();
    let mut s =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    let alpha = s.vault.resolve("Alpha").expect("Alpha");
    let original = s.vault.notes[alpha].source.clone();

    s.editor.focus = s.editor.text.len();
    s.editor.insert("typed");
    assert!(s.save_now(), "the edit is written");
    s.toggle_preview();
    s.toggle_preview();

    s.editor.undo();
    assert_eq!(s.editor.text, original);
}

#[test]
fn reloading_a_changed_note_does_not_resurrect_the_old_history() {
    let tmp = nav_vault();
    let mut s =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    let alpha = s.vault.resolve("Alpha").expect("Alpha");
    let beta = s.vault.resolve("Beta").expect("Beta");
    let path = s.vault.notes[alpha].path.clone();
    let mut tx = Tx::default();
    assert!(!poll(&mut s, 1.0, &mut tx), "nothing changed yet");

    // Save a typed edit, then let the file change under us while clean.
    s.editor.focus = s.editor.text.len();
    s.editor.insert("typed");
    assert!(s.save_now());
    fs::write(&path, "# Alpha changed\n").expect("external edit");
    assert!(poll(&mut s, 2.7, &mut tx), "the watcher reloads");
    assert_eq!(s.editor.text, "# Alpha changed\n");

    let changed = s.editor.text.clone();
    s.editor.undo();
    assert_eq!(
        s.editor.text, changed,
        "the reload started a fresh, empty undo stack"
    );

    // Leaving and returning must not bring the discarded stack back.
    s.open_note(beta);
    s.open_note(alpha);
    s.editor.undo();
    assert_eq!(s.editor.text, changed);
}

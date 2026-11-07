//! M2 Lane 2a headless tests: wikilink completion, heading deep links, the
//! broken-links dashboard and rename/move link rewriting.
//!
//! These drive the real Rune runtime against a temporary vault, like `ui.rs`
//! and `navigation.rs`.

use std::fs;

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime, Tx};

fn links_vault() -> TempVault {
    TempVault::new(&[
        (
            "Home.md",
            "# Home\n\nA [[Missing One]] and [[gone/Missing Two]] and [[Also#Head]].\n",
        ),
        ("Beta.md", "# First\n\ntext\n\n## Second\n\nend\n"),
        ("a/Index.md", "a index"),
        ("b/Index.md", "b index"),
    ])
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

fn type_text(rt: &mut Runtime<State>, typed: &str) {
    for ch in typed.chars() {
        rt.key(text(&ch.to_string()));
        frame(rt);
    }
    frame(rt);
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frame(rt);
}

#[test]
fn inline_completion_lists_duplicates_and_inserts_the_full_stem() {
    let tmp = links_vault();
    let mut rt = runtime(&tmp);
    let home = rt.state.vault.resolve("Home").expect("Home");
    rt.state.open_note(home);
    frame(&mut rt);
    let end = rt.state.editor.text.len();
    rt.state.editor.set_caret(end);
    frame(&mut rt);

    type_text(&mut rt, "[[ind");
    let complete = rt.state.complete.as_ref().expect("autocomplete opens");
    assert_eq!(complete.query, "ind");
    let labels: Vec<String> = complete
        .items
        .iter()
        .map(|&id| rt.state.vault.display_label(id))
        .collect();
    assert!(labels.contains(&"a/Index".to_owned()), "{labels:?}");
    assert!(labels.contains(&"b/Index".to_owned()), "{labels:?}");
    assert!(
        labels.iter().all(|label| label != "Home"),
        "the query filters: {labels:?}"
    );

    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert!(rt.state.complete.is_none(), "accept closes the list");
    assert!(
        rt.state.editor.text.ends_with("[[a/Index]]"),
        "Enter inserts the full unambiguous stem: {:?}",
        rt.state.editor.text
    );
}

#[test]
fn escape_dismisses_then_typing_reopens_and_pointer_accepts() {
    let tmp = links_vault();
    let mut rt = runtime(&tmp);
    let home = rt.state.vault.resolve("Home").expect("Home");
    rt.state.open_note(home);
    frame(&mut rt);
    let end = rt.state.editor.text.len();
    rt.state.editor.set_caret(end);
    type_text(&mut rt, "\n[[be");
    assert!(rt.state.complete.is_some());

    rt.key(key(Key::Escape));
    frame(&mut rt);
    assert!(rt.state.complete.is_none(), "Escape dismisses the list");
    frame(&mut rt);
    assert!(
        rt.state.complete.is_none(),
        "a dismissed list does not reopen on the next frame"
    );

    // Typing moves the caret, which is a new completion request.
    type_text(&mut rt, "t");
    assert!(rt.state.complete.is_some(), "typing reopens the list");

    let beta = rt.state.vault.resolve("Beta").expect("Beta");
    for _ in 0..6 {
        frame(&mut rt);
    }
    click(&mut rt, &format!("complete-item-{beta}"));
    assert!(
        rt.state.editor.text.contains("[[Beta]]"),
        "pointer click accepts: {:?}",
        rt.state.editor.text
    );
    assert!(rt.state.complete.is_none());
}

#[test]
fn completion_does_not_fire_inside_fenced_or_inline_code() {
    let tmp = links_vault();
    let mut rt = runtime(&tmp);
    let home = rt.state.vault.resolve("Home").expect("Home");
    rt.state.open_note(home);
    frame(&mut rt);

    rt.state.editor.replace_text("```\n[[ind");
    rt.state.focus_editor();
    frame(&mut rt);
    assert!(
        rt.state.complete.is_none(),
        "a fenced code block never triggers completion"
    );

    rt.state.editor.replace_text("`[[ind` and more");
    rt.state.focus_editor();
    rt.state.editor.set_caret(rt.state.editor.text.len());
    frame(&mut rt);
    assert!(
        rt.state.complete.is_none(),
        "inline code is not a link position"
    );
}

#[test]
fn cmd_p_inserts_a_wikilink_without_typing_brackets() {
    let tmp = links_vault();
    let mut rt = runtime(&tmp);
    let home = rt.state.vault.resolve("Home").expect("Home");
    rt.state.open_note(home);
    frame(&mut rt);
    rt.state.editor.set_caret(rt.state.editor.text.len());
    let before = rt.state.editor.text.clone();

    rt.key(command('p'));
    frame(&mut rt);
    let complete = rt.state.complete.as_ref().expect("picker opens");
    assert!(complete.open.is_none(), "this is the explicit picker");
    assert_eq!(complete.query, "");

    // Picker typing filters but never touches the note until accept.
    type_text(&mut rt, "bet");
    assert_eq!(rt.state.editor.text, before, "typing filters the picker");
    let first = rt.state.complete.as_ref().expect("picker").items[0];
    assert_eq!(rt.state.vault.display_label(first), "Beta");

    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert!(
        rt.state.editor.text.ends_with("[[Beta]]"),
        "{:?}",
        rt.state.editor.text
    );
}

#[test]
fn heading_deep_links_move_the_write_caret_and_queue_a_read_scroll() {
    let tmp = links_vault();
    let mut rt = runtime(&tmp);
    let beta = rt.state.vault.resolve("Beta").expect("Beta");
    rt.state.open_note(beta);
    frame(&mut rt);
    let expected = rt.state.editor.text.find("## Second").expect("heading");
    let mut tx = Tx::default();

    rt.state.open_link("note:Beta#second", &mut tx);
    assert_eq!(
        rt.state.editor.focus, expected,
        "Write mode puts the caret at the heading line"
    );

    // A missing heading is not an error: open at the top.
    rt.state.open_link("note:Beta#Nope", &mut tx);
    assert_eq!(rt.state.editor.focus, 0);

    rt.state.set_mode(Mode::Preview);
    frame(&mut rt);
    rt.update(|s, tx| s.open_link("note:Beta#Second", tx));
    assert_eq!(
        rt.state.pending_heading.as_deref(),
        Some("Second"),
        "Read mode reveals the heading"
    );
    frame(&mut rt);

    rt.update(|s, tx| s.open_link("note:Beta#Absent", tx));
    assert!(rt.state.pending_top, "an absent heading scrolls to the top");
}

#[test]
fn broken_links_dashboard_lists_creates_and_opens_referrers() {
    let tmp = links_vault();
    let mut rt = runtime(&tmp);
    let home = rt.state.vault.resolve("Home").expect("Home");

    let broken = rt.state.vault.broken_links();
    assert_eq!(broken.len(), 3, "{broken:?}");
    let missing = broken
        .iter()
        .find(|entry| entry.target == "Missing One")
        .expect("missing one");
    assert_eq!(missing.referrers, vec![home], "home refers to it");

    frame(&mut rt);
    click(&mut rt, "broken-chip");
    assert!(rt.state.broken_panel);
    assert!(rt.find("broken-panel").is_some());
    assert!(rt.find("broken-target-Missing-One").is_some());
    assert!(rt.find(&format!("broken-ref-Missing-One-{home}")).is_some());

    click(&mut rt, "broken-target-Missing-One");
    assert!(!rt.state.broken_panel, "creating leaves the dashboard");
    assert!(
        rt.state
            .vault
            .notes
            .iter()
            .any(|note| note.title == "Missing One"),
        "the missing target became a note"
    );
    assert_eq!(
        rt.state.broken_count(),
        2,
        "the badge/cache updates after the create"
    );

    let home = rt.state.vault.resolve("Home").expect("Home after reload");
    rt.state.open_broken_links();
    frame(&mut rt);
    click(&mut rt, &format!("broken-ref-gone-Missing-Two-{home}"));
    assert_eq!(rt.state.selected, Some(home), "a referrer opens its note");
    assert!(!rt.state.broken_panel);
}

#[test]
fn rename_rewrites_two_referrers_preserving_alias_and_heading() {
    let tmp = TempVault::new(&[
        ("Home.md", "See [[Alpha]] and [[Alpha#Top|the top]]"),
        ("Second.md", "Back to [[alpha|A]]"),
        ("Alpha.md", "# Top\nbody"),
    ]);
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.load_note(alpha, false);
    rt.state.begin_rename_note(alpha);
    rt.state.rename_field.replace_text("Beta");
    rt.state.submit_rename_note();

    let confirm = rt
        .state
        .rename_confirm
        .as_ref()
        .expect("link-impact dialog");
    assert_eq!(confirm.referrers, 2);
    assert!(confirm.rewrite, "rewrite is the default");
    frame(&mut rt);
    assert!(rt.find("rename-confirm").is_some());

    rt.state.confirm_rename_note();
    frame(&mut rt);
    assert_eq!(
        fs::read_to_string(tmp.path().join("Home.md")).unwrap(),
        "See [[Beta]] and [[Beta#Top|the top]]"
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("Second.md")).unwrap(),
        "Back to [[Beta|A]]"
    );
    assert!(tmp.path().join("Beta.md").is_file());
    assert!(!tmp.path().join("Alpha.md").exists());
    assert!(rt.state.vault.broken_links().is_empty());
}

#[test]
fn rename_can_deliberately_leave_links_dangling() {
    let tmp = TempVault::new(&[("Home.md", "See [[Alpha]]"), ("Alpha.md", "body")]);
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.load_note(alpha, false);
    rt.state.begin_rename_note(alpha);
    rt.state.rename_field.replace_text("Beta");
    rt.state.submit_rename_note();
    assert!(rt.state.rename_confirm.is_some());
    rt.state.toggle_rename_rewrite();
    assert!(!rt.state.rename_confirm.as_ref().expect("dialog").rewrite);
    rt.state.confirm_rename_note();
    frame(&mut rt);

    assert_eq!(
        fs::read_to_string(tmp.path().join("Home.md")).unwrap(),
        "See [[Alpha]]",
        "leave-dangling keeps the source bytes"
    );
    let broken = rt.state.vault.broken_links();
    assert_eq!(broken.len(), 1);
    assert_eq!(broken[0].target, "Alpha");
}

#[test]
fn rename_rewrites_a_bare_duplicate_only_when_it_resolved_there() {
    let tmp = TempVault::new(&[
        ("a/Index.md", "a"),
        ("b/Index.md", "b"),
        ("Home.md", "bare [[Index]] path [[b/Index]]"),
    ]);
    let mut rt = runtime(&tmp);
    let b = rt.state.vault.resolve("b/Index").expect("b index");
    rt.state.load_note(b, false);
    rt.state.begin_rename_note(b);
    rt.state.rename_field.replace_text("Bee");
    rt.state.submit_rename_note();
    assert_eq!(
        rt.state.rename_confirm.as_ref().expect("dialog").referrers,
        1
    );
    rt.state.confirm_rename_note();
    frame(&mut rt);

    assert_eq!(
        fs::read_to_string(tmp.path().join("Home.md")).unwrap(),
        "bare [[Index]] path [[b/Bee]]",
        "the bare link resolves to the other duplicate and stays put"
    );
}

#[test]
fn move_rewrites_inbound_links_to_the_new_stem() {
    let tmp = TempVault::new(&[
        ("Home.md", "See [[Alpha]]"),
        ("Alpha.md", "body"),
        ("dest/keep.md", "keep"),
    ]);
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.load_note(alpha, false);
    rt.state.move_note_to(alpha, "dest");
    let confirm = rt.state.move_confirm.as_ref().expect("move dialog");
    assert_eq!(confirm.referrers, 1);
    rt.state.confirm_move_note();
    frame(&mut rt);

    assert!(tmp.path().join("dest/Alpha.md").is_file());
    assert!(!tmp.path().join("Alpha.md").exists());
    assert_eq!(
        fs::read_to_string(tmp.path().join("Home.md")).unwrap(),
        "See [[dest/Alpha]]"
    );
    assert!(rt.state.vault.broken_links().is_empty());
}

// ---- M2 A11Y-01: keyboard-reachable rail and broken-links chip --------------

/// The element the runtime reports as focused, by its stable test key.
fn focused_key(rt: &Runtime<State>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|element| element.focused)
        .and_then(|element| element.key)
}

/// Press Tab until `wanted` owns focus, failing with the last stop reached.
fn tab_until(rt: &mut Runtime<State>, wanted: &str) {
    for _ in 0..16 {
        if focused_key(rt).as_deref() == Some(wanted) {
            return;
        }
        rt.key(key(Key::Tab));
        frame(rt);
    }
    panic!(
        "Tab never reached {wanted}; last focus was {:?}",
        focused_key(rt)
    );
}

#[test]
fn tab_reaches_the_rail_and_enter_opens_a_backlink() {
    let tmp = TempVault::new(&[
        ("Alpha.md", "Alpha body with no outgoing links"),
        ("Hub.md", "[[Alpha]]"),
        ("Gamma.md", "[[Alpha]]"),
    ]);
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.open_note(alpha);
    frame(&mut rt);

    let backlinks = rt.state.vault.notes[alpha].backlinks.clone();
    assert!(rt.state.vault.notes[alpha].links.is_empty());
    assert_eq!(backlinks.len(), 2, "{backlinks:?}");
    let first = backlinks[0];

    // Escape releases the editor (EDT-05 keeps Tab inside it); with nothing
    // focused, Tab starts at the rail because its rows are the first stops.
    rt.key(key(Key::Escape));
    frame(&mut rt);
    tab_until(&mut rt, &format!("rail-link-0-{first}"));

    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert_eq!(
        rt.state.selected,
        Some(first),
        "Enter on a focused backlink opens that note"
    );
    assert!(
        rt.state.editor.focused,
        "opening returns typing to the editor"
    );
}

#[test]
fn tab_reaches_the_broken_chip_and_enter_opens_the_panel() {
    let tmp = TempVault::new(&[("Home.md", "[[Missing On Purpose]]")]);
    let mut rt = runtime(&tmp);
    frame(&mut rt);

    rt.key(key(Key::Escape));
    frame(&mut rt);
    tab_until(&mut rt, "broken-chip");

    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert!(rt.state.broken_panel, "Enter on the focused chip opens it");
    assert!(rt.find("broken-panel").is_some());

    // The dashboard is a list too: Tab reaches its first target and Enter
    // creates the missing note.
    tab_until(&mut rt, "broken-target-Missing-On-Purpose");
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert!(!rt.state.broken_panel);
    assert!(
        rt.state
            .vault
            .notes
            .iter()
            .any(|note| note.title == "Missing On Purpose"),
        "Enter on a focused target creates the note"
    );
}

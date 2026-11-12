//! Kanban slice 10: tags. A card's tags are read from its text, the board
//! footer and `kanban_defaults` decide whether they stay in the title and
//! whether chips sit at the card's foot, a `#` in the card editor completes a
//! vault tag, and a chip's click fills the existing Find bar. Display and
//! filtering never write the board.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::KanbanDefaults;
use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Ime, Key, KeyEvent, Mods, Runtime};

const BOARD: &str = "\
---
kanban-plugin: board
---

## To do

- [ ] Ship #release #ops
";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-tags-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn vault_with(board: &str) -> TempVault {
    TempVault::new(&[
        ("Agenda.md", "# Agenda\n\nPlan #work and #home/garden.\n"),
        ("Board.md", board),
    ])
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
    rt.update(|s, _| s.open_kanban(id));
    rt.frame(1.0 / 60.0);
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

fn key(k: Key) -> KeyEvent {
    KeyEvent {
        key: k,
        mods: Mods::default(),
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

fn page_text(rt: &Runtime<State>) -> String {
    rt.elements()
        .into_iter()
        .filter(|element| !element.exiting)
        .filter_map(|element| element.text)
        .collect::<Vec<_>>()
        .join("\n")
}

fn has(rt: &Runtime<State>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn count(hay: &str, needle: &str) -> usize {
    hay.match_indices(needle).count()
}

/// The board with a settings footer carrying `json`.
fn with_footer(json: &str) -> String {
    format!("{BOARD}\n%% kanban:settings\n```\n{json}\n```\n%%\n")
}

fn edit_card(rt: &mut Runtime<State>, text: &str) {
    let text = text.to_owned();
    rt.update(|s, _| {
        s.begin_edit_card(0, 0);
        s.kanban_card_field.replace_text(&text);
        let end = s.kanban_card_field.text.len();
        s.kanban_card_field.set_caret(end);
        s.kanban_card_field.focused = true;
    });
}

#[test]
fn typing_hash_offers_vault_tags_and_inserts_at_the_caret() {
    let tmp = vault_with(BOARD);
    let mut rt = on_board(&tmp);
    edit_card(&mut rt, "see #wo");
    assert!(
        rt.state.complete_tick(),
        "the card editor opens tag completion"
    );
    let complete = rt.state.tag_complete.as_ref().expect("tag list");
    assert!(complete.card);
    assert_eq!(complete.items.first().map(String::as_str), Some("work"));
    rt.update(|s, _| {
        assert!(s.complete_key(&key(Key::Enter)));
    });
    assert_eq!(rt.state.kanban_card_field.text, "see #work");
    assert!(rt.state.tag_complete.is_none());
    assert_eq!(source(&rt), BOARD, "the card is not written until commit");

    // Escape closes the list and leaves the typed `#`; it does not cancel the
    // editor.
    edit_card(&mut rt, " #");
    assert!(rt.state.complete_tick());
    assert!(rt.state.tag_complete.is_some());
    rt.update(|s, _| {
        assert!(s.complete_key(&key(Key::Escape)));
    });
    assert!(rt.state.tag_complete.is_none());
    assert_eq!(rt.state.kanban_card_field.text, " #");
    assert!(!rt.state.complete_tick(), "the dismissal holds");
    assert_eq!(rt.state.mode, Mode::Kanban);
    assert!(rt.state.kanban_card.is_some(), "the editor stays open");

    // Up/Down and Tab use the same list keys as `[[`.
    edit_card(&mut rt, "#");
    assert!(rt.state.complete_tick());
    let items = rt.state.tag_complete.as_ref().expect("tags").items.clone();
    assert!(items.len() >= 3, "{items:?}");
    rt.update(|s, _| assert!(s.complete_key(&key(Key::Down))));
    assert_eq!(rt.state.tag_complete.as_ref().map(|c| c.index), Some(1));
    rt.update(|s, _| assert!(s.complete_key(&key(Key::Tab))));
    assert_eq!(rt.state.kanban_card_field.text, format!("#{}", items[1]));
}

#[test]
fn tag_completion_stays_out_of_links_urls_emails_and_headings() {
    let tmp = vault_with(BOARD);
    let mut rt = on_board(&tmp);
    let cases = [
        ("see [[Agenda#hea", "a link target"),
        ("https://example.test/#frag", "a URL fragment"),
        ("mail a#b@c", "an email"),
        ("## Heading", "a heading marker"),
    ];
    for (text, why) in cases {
        edit_card(&mut rt, text);
        rt.state.complete_tick();
        assert!(
            rt.state.tag_complete.is_none(),
            "{why} must not offer tags: {text}"
        );
    }

    // Typing plain text with a `#` after whitespace does open the list.
    edit_card(&mut rt, "#rel");
    assert!(rt.state.complete_tick());
    assert_eq!(
        rt.state.tag_complete.as_ref().and_then(|c| c.items.first()),
        Some(&"release".to_owned())
    );
}

#[test]
fn chips_render_and_hide_show_resolve_from_footer_and_app_defaults() {
    // Built-in: the tag stays in the title and no chip is drawn.
    let tmp = vault_with(BOARD);
    let rt = on_board(&tmp);
    assert_eq!(count(&page_text(&rt), "#release"), 1);
    assert!(!has(&rt, "kanban-card-0-0-tag-0"));

    // The footer asks for chips at the foot.
    let footer = with_footer(r#"{"kanban-plugin":"board","show-tags-at-bottom":true}"#);
    let tmp = vault_with(&footer);
    let rt = on_board(&tmp);
    assert!(has(&rt, "kanban-card-0-0-tag-0"));
    assert_eq!(count(&page_text(&rt), "#release"), 2, "title and chip");
    assert!(page_text(&rt).contains("#ops"));

    // Both settings: the token leaves the title and only the chip shows it.
    let footer = with_footer(
        r#"{"kanban-plugin":"board","show-tags-at-bottom":true,"hide-tags-in-title":true}"#,
    );
    let tmp = vault_with(&footer);
    let rt = on_board(&tmp);
    assert_eq!(count(&page_text(&rt), "#release"), 1, "the chip only");
    assert!(has(&rt, "kanban-card-0-0-tag-0"));

    // Hide only: the tag disappears from the card.
    let footer = with_footer(r#"{"kanban-plugin":"board","hide-tags-in-title":true}"#);
    let tmp = vault_with(&footer);
    let rt = on_board(&tmp);
    assert_eq!(count(&page_text(&rt), "#release"), 0);

    // App defaults apply when the footer has no key …
    let tmp = vault_with(BOARD);
    let rt = on_board_with(
        &tmp,
        KanbanDefaults {
            show_tags_at_bottom: Some(true),
            hide_tags_in_title: Some(true),
            ..KanbanDefaults::default()
        },
    );
    assert!(has(&rt, "kanban-card-0-0-tag-0"));
    assert_eq!(count(&page_text(&rt), "#release"), 1);

    // … and the footer wins over them.
    let tmp = vault_with(&footer);
    let rt = on_board_with(
        &tmp,
        KanbanDefaults {
            show_tags_at_bottom: Some(true),
            hide_tags_in_title: Some(false),
            ..KanbanDefaults::default()
        },
    );
    assert!(has(&rt, "kanban-card-0-0-tag-0"), "the footer shows chips");
    assert_eq!(
        count(&page_text(&rt), "#release"),
        1,
        "the footer's hide wins, so only the chip shows the tag"
    );
}

#[test]
fn clicking_a_tag_opens_find_with_the_tag_and_writes_nothing() {
    let footer = with_footer(r#"{"kanban-plugin":"board","show-tags-at-bottom":true}"#);
    let tmp = vault_with(&footer);
    let mut rt = on_board(&tmp);
    click(&mut rt, "kanban-card-0-0-tag-0");
    let find = rt.state.find.as_ref().expect("the Find bar opened");
    assert_eq!(find.field.text, "#release");
    assert_eq!(find.query, "#release");
    assert!(!find.matches.is_empty(), "the board text has the tag");
    assert_eq!(source(&rt), footer, "a click changes no bytes");
    assert_eq!(on_disk(&tmp), footer);
}

#[test]
fn a_board_with_tags_is_byte_identical_after_display_and_filter() {
    let footer = with_footer(
        r#"{"kanban-plugin":"board","show-tags-at-bottom":true,"hide-tags-in-title":true,"tag-colors":[{"tagKey":"release","color":"1"}]}"#,
    );
    let tmp = vault_with(&footer);
    let mut rt = on_board(&tmp);
    assert_eq!(source(&rt), footer);
    assert_eq!(on_disk(&tmp), footer);
    // tag-colors is kept as written, not rewritten by viewing.
    assert!(source(&rt).contains(r#""tag-colors":[{"tagKey":"release","color":"1"}]"#));

    click(&mut rt, "kanban-card-0-0-tag-0");
    frame(&mut rt);
    rt.update(|s, _| s.close_find());
    frame(&mut rt);
    assert_eq!(source(&rt), footer, "display and filtering write nothing");
    assert_eq!(on_disk(&tmp), footer);

    // The only write is the user's own edit, and only on its own line.
    act(&mut rt, |s| s.toggle_card_done(0, 0));
    let edited = source(&rt);
    assert_eq!(edited, footer.replacen("- [ ] Ship", "- [x] Ship", 1));
    assert!(edited.contains("#release #ops"));
    assert_eq!(on_disk(&tmp), footer, "nothing is written before ⌘S");

    rt.update(|s, _| s.undo_kanban());
    frame(&mut rt);
    assert_eq!(source(&rt), footer);
}

#[test]
fn a_typing_ime_commit_reaches_the_card_editor_without_a_write() {
    let tmp = vault_with(BOARD);
    let mut rt = on_board(&tmp);
    edit_card(&mut rt, "");
    // Draw the editor once so the IME event has its field.
    frame(&mut rt);
    type_text(&mut rt, "#rel");
    // The IME event can land after the frame's poll; one more frame lets the
    // committed text reach `complete_tick`.
    frame(&mut rt);
    assert!(rt.state.tag_complete.is_some(), "poll detected the tag");
    assert_eq!(source(&rt), BOARD);
    assert_eq!(on_disk(&tmp), BOARD);
}

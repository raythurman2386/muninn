//! Kanban slice 11: metadata from a card's linked note. A card's first
//! wikilink selects a note, the board footer and `kanban_defaults` name the
//! keys to draw at the card's foot, and a missing key/note draws nothing
//! unless the board asks for `—`. Display is read-only: no view writes a
//! byte.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::KanbanDefaults;
use muninn_core::testing::TempVault;
use muninn_ui::{Settings, State, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const BOARD: &str = "\
---
kanban-plugin: board
---

## To do

- [ ] Ship [[Agenda]]
";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-metadata-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

const AGENDA: &str = "\
---
status: Active
motto: \"**bold**\"
owner: Ada
---
[docs:: [[Guide|the guide]]]
";

fn vault_with(board: &str) -> TempVault {
    TempVault::new(&[
        ("Agenda.md", AGENDA),
        ("Guide.md", "# Guide\n"),
        ("Other.md", "---\nstatus: Queued\n---\n"),
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
    rt.poll = Some(muninn_ui::poll);
    rt.on_key = Some(muninn_ui::on_key);
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

/// The board with a settings footer carrying `json`.
fn with_footer(json: &str) -> String {
    format!("{BOARD}\n%% kanban:settings\n```\n{json}\n```\n%%\n")
}

fn metadata_footer(fields: &str) -> String {
    with_footer(&format!(
        r#"{{"kanban-plugin":"board","linked-page-metadata":[{fields}]}}"#
    ))
}

fn defaults(fields: &str, show_missing: bool) -> KanbanDefaults {
    let value: serde_json::Value = serde_json::from_str(&format!("[{fields}]")).expect("fields");
    KanbanDefaults {
        linked_page_metadata: muninn_core::metadata_fields(Some(&value)),
        show_missing_metadata: Some(show_missing),
        ..KanbanDefaults::default()
    }
}

#[test]
fn configured_keys_render_from_frontmatter_and_inline_fields() {
    let board = metadata_footer(
        r#"{"key":"status"},{"key":"owner"},{"key":"docs","containsMarkdown":true}"#,
    );
    let tmp = vault_with(&board);
    let rt = on_board(&tmp);
    let text = page_text(&rt);
    assert!(text.contains("status:"), "{text}");
    assert!(text.contains("Active"), "{text}");
    assert!(text.contains("owner:"), "{text}");
    assert!(text.contains("Ada"), "{text}");
    assert!(text.contains("docs:"), "{text}");
    assert!(
        text.contains("the guide"),
        "a markdown value renders: {text}"
    );
    assert!(has(&rt, "kanban-card-0-0-metadata-row-0"));
    assert_eq!(source(&rt), board, "viewing writes nothing");
    assert_eq!(on_disk(&tmp), board);
}

#[test]
fn contains_markdown_renders_links_and_plain_text_keeps_markers() {
    let board = metadata_footer(
        r#"{"key":"motto","containsMarkdown":false},{"key":"docs","containsMarkdown":true}"#,
    );
    let tmp = vault_with(&board);
    let rt = on_board(&tmp);
    let text = page_text(&rt);
    assert!(
        text.contains("**bold**"),
        "plain text keeps its markers: {text}"
    );
    assert!(
        !text.contains("**the guide**"),
        "the markdown value is not escaped text: {text}"
    );
    assert!(text.contains("the guide"), "{text}");
    assert!(
        !text.contains("[[Guide|the guide]]"),
        "the reader rendered the link rather than showing its source: {text}"
    );
    // Without containsMarkdown the same value renders as text.
    let board = metadata_footer(r#"{"key":"docs"}"#);
    let tmp = vault_with(&board);
    let rt = on_board(&tmp);
    let text = page_text(&rt);
    assert!(text.contains("[[Guide|the guide]]"), "{text}");
}

#[test]
fn app_defaults_apply_and_the_footer_wins_or_ignores_a_wrong_type() {
    let tmp = vault_with(BOARD);
    let rt = on_board_with(&tmp, defaults(r#"{"key":"status"}"#, false));
    assert!(page_text(&rt).contains("Active"), "the app default applies");

    let board = metadata_footer(r#"{"key":"owner"}"#);
    let tmp = vault_with(&board);
    let rt = on_board_with(&tmp, defaults(r#"{"key":"status"}"#, false));
    let text = page_text(&rt);
    assert!(text.contains("Ada"), "the footer's key wins: {text}");
    assert!(!text.contains("Active"), "{text}");

    let board = metadata_footer("[]");
    let tmp = vault_with(&board);
    let rt = on_board_with(&tmp, defaults(r#"{"key":"status"}"#, false));
    assert!(
        !has(&rt, "kanban-card-0-0-metadata"),
        "an empty footer array means no rows"
    );
    assert!(!page_text(&rt).contains("Active"));

    let board = with_footer(r#"{"kanban-plugin":"board","linked-page-metadata":"not an array"}"#);
    let tmp = vault_with(&board);
    let rt = on_board_with(&tmp, defaults(r#"{"key":"status"}"#, false));
    assert!(
        page_text(&rt).contains("Active"),
        "a wrong footer type is ignored, so the app default still applies"
    );
}

#[test]
fn missing_keys_and_notes_draw_nothing_unless_the_board_asks() {
    let board = metadata_footer(r#"{"key":"status"},{"key":"absent"}"#);
    let tmp = vault_with(&board);
    let rt = on_board(&tmp);
    let text = page_text(&rt);
    assert!(text.contains("Active"), "{text}");
    assert!(!text.contains("absent:"), "{text}");
    assert!(!text.contains('—'), "{text}");

    let board = with_footer(
        r#"{"kanban-plugin":"board","linked-page-metadata":[{"key":"status"},{"key":"absent"}],"show-missing-metadata":true}"#,
    );
    let tmp = vault_with(&board);
    let rt = on_board(&tmp);
    let text = page_text(&rt);
    assert!(text.contains("absent:"), "{text}");
    assert!(text.contains('—'), "{text}");

    let missing = BOARD.replace("[[Agenda]]", "[[Missing]]");
    let tmp = vault_with(&missing);
    let rt = on_board(&tmp);
    assert!(
        !has(&rt, "kanban-card-0-0-metadata"),
        "a missing note draws nothing by default"
    );

    let board = with_footer(
        r#"{"kanban-plugin":"board","linked-page-metadata":[{"key":"status"}],"show-missing-metadata":true}"#,
    );
    let missing = board.replace("[[Agenda]]", "[[Missing]]");
    let tmp = vault_with(&missing);
    let rt = on_board(&tmp);
    assert!(page_text(&rt).contains('—'), "{:?}", page_text(&rt));
}

#[test]
fn a_changed_card_link_updates_the_rows_on_the_next_lane_rebuild() {
    let board = metadata_footer(r#"{"key":"status"}"#);
    let tmp = vault_with(&board);
    let mut rt = on_board(&tmp);
    assert!(page_text(&rt).contains("Active"));

    act(&mut rt, |s| {
        s.begin_edit_card(0, 0);
        s.kanban_card_field.replace_text("Ship [[Other]]");
    });
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    frame(&mut rt);
    let text = page_text(&rt);
    assert!(text.contains("Queued"), "{text}");
    assert!(!text.contains("Active"), "{text}");
}

#[test]
fn a_dirty_board_keeps_its_bytes_and_metadata_never_writes() {
    let board = metadata_footer(
        r#"{"key":"status"},{"key":"owner"},{"key":"docs","containsMarkdown":true}"#,
    );
    let tmp = vault_with(&board);
    let mut rt = on_board(&tmp);
    assert_eq!(source(&rt), board);

    act(&mut rt, |s| s.toggle_card_done(0, 0));
    let edited = source(&rt);
    assert_eq!(edited, board.replacen("- [ ] Ship", "- [x] Ship", 1));
    assert!(page_text(&rt).contains("Active"), "the rows still draw");
    assert_eq!(on_disk(&tmp), board, "nothing is written before ⌘S");

    act(&mut rt, |s| s.undo_kanban());
    assert_eq!(source(&rt), board);
}

#[test]
fn a_long_multibyte_value_renders_without_panicking() {
    let long = "漢👍🏽".repeat(400);
    let agenda = format!("---\nnotes: \"{long}\"\n---\n");
    let board = metadata_footer(r#"{"key":"notes"}"#);
    let tmp = TempVault::new(&[("Agenda.md", &agenda), ("Board.md", &board)]);
    let rt = on_board(&tmp);
    let text = page_text(&rt);
    assert!(text.contains("notes:"), "{text}");
    assert!(text.contains('漢'), "the multibyte value renders");
}

#[test]
fn typing_with_an_open_editor_does_not_change_the_metadata_bytes() {
    let board = metadata_footer(r#"{"key":"status"}"#);
    let tmp = vault_with(&board);
    let mut rt = on_board(&tmp);
    act(&mut rt, |s| s.begin_edit_card(0, 0));
    frame(&mut rt);
    let event = KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    };
    rt.key(event);
    frame(&mut rt);
    assert_eq!(source(&rt), board);
    assert_eq!(on_disk(&tmp), board);
}

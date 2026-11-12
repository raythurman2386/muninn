//! Kanban slice 8: a card's link, its block id, a note made from the card,
//! autocomplete, the block jump, and a rename that keeps unsaved edits.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Runtime, Tx};

const BOARD: &str = "\
---
kanban-plugin: board
---

## To do

- [ ] Write the release notes
- [ ] Review [[Roadmap]] ^card-7f2
  with the team

## Doing

- [ ] Fix the sync bug
";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-links-{tag}-{}-{}",
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
        ("Roadmap.md", "# Roadmap\n\nThe plan.\n"),
    ])
}

fn runtime(tmp: &TempVault) -> (Runtime<State>, PathBuf) {
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
    rt.update(|s, _| s.open_kanban(id));
    rt.frame(1.0 / 60.0);
    (rt, config)
}

fn board_text(rt: &Runtime<State>) -> String {
    rt.state
        .kanban
        .as_ref()
        .and_then(|shown| shown.board.as_ref())
        .expect("board")
        .source()
        .to_owned()
}

fn on_disk(tmp: &TempVault) -> String {
    fs::read_to_string(tmp.path().join("Board.md")).expect("Board.md")
}

#[test]
fn copying_a_card_link_writes_the_block_id_once() {
    let tmp = vault();
    let (mut rt, _) = runtime(&tmp);
    assert_eq!(on_disk(&tmp), BOARD, "opening the page writes nothing");

    let mut first = None;
    rt.update(|s, _| first = s.copy_card_link(1, 0));
    rt.frame(1.0 / 60.0);
    let first = first.expect("link");
    assert_eq!(first, "[[Board#^card-1]]");
    let written = board_text(&rt);
    assert!(
        written.contains("- [ ] Fix the sync bug ^card-1\n"),
        "{written}"
    );
    assert!(
        written.contains("- [ ] Review [[Roadmap]] ^card-7f2\n"),
        "the existing id stays"
    );
    assert_eq!(on_disk(&tmp), BOARD, "the id is not on disk until save");

    let mut again = None;
    rt.update(|s, _| again = s.copy_card_link(1, 0));
    rt.frame(1.0 / 60.0);
    assert_eq!(again.as_deref(), Some(first.as_str()));
    assert_eq!(
        board_text(&rt),
        written,
        "a second copy does not rewrite the id"
    );

    // The card that already has an id is not given another.
    let before = board_text(&rt);
    let mut kept = None;
    rt.update(|s, _| kept = s.copy_card_link(0, 1));
    rt.frame(1.0 / 60.0);
    assert_eq!(kept.as_deref(), Some("[[Board#^card-7f2]]"));
    assert_eq!(board_text(&rt), before);
}

#[test]
fn typing_a_link_in_a_card_offers_notes_and_inserts_one() {
    let tmp = vault();
    let (mut rt, _) = runtime(&tmp);
    rt.update(|s, _| {
        s.begin_edit_card(0, 0);
        s.kanban_card_field.replace_text("see [[Ro");
        let end = s.kanban_card_field.text.len();
        s.kanban_card_field.set_caret(end);
        s.kanban_card_field.focused = true;
    });
    assert!(rt.state.complete_tick(), "the card editor opens completion");
    let complete = rt.state.complete.as_ref().expect("list");
    assert!(complete.card);
    let roadmap = rt.state.vault.resolve("Roadmap").expect("Roadmap");
    assert_eq!(complete.items.first().copied(), Some(roadmap));
    rt.update(|s, _| {
        s.complete_choose(roadmap);
    });
    assert_eq!(rt.state.kanban_card_field.text, "see [[Roadmap]]");
    assert!(rt.state.complete.is_none());
    // The board is unchanged until the editor commits.
    assert_eq!(board_text(&rt), BOARD);
}

#[test]
fn a_card_link_opens_the_note_and_a_block_id_moves_the_caret() {
    let tmp = vault();
    let (mut rt, _) = runtime(&tmp);
    rt.update(|s, tx| s.open_card_link("note:Roadmap", tx));
    assert_eq!(rt.state.mode, Mode::Editor);
    let selected = rt.state.selected.expect("selected");
    assert_eq!(rt.state.vault.notes[selected].title, "Roadmap");

    let board = rt.state.vault.resolve("Board").expect("Board");
    rt.update(|s, _| s.open_note(board));
    rt.frame(1.0 / 60.0);
    let expected = rt.state.editor.text.find("- [ ] Review").expect("card");
    let mut tx = Tx::default();
    rt.state.open_link("note:Board#^card-7f2", &mut tx);
    assert_eq!(
        rt.state.editor.focus, expected,
        "the caret sits on the card"
    );
    assert_eq!(rt.state.editor.anchor, expected);
}

#[test]
fn a_new_note_from_a_card_uses_the_template_and_replaces_the_card() {
    let tmp = vault();
    let config = unique_dir("config");
    fs::create_dir_all(config.join("muninn")).expect("config");
    fs::write(
        config.join("muninn/settings.json"),
        "{\"note_template\":\"tags: work\\n\"}\n",
    )
    .expect("settings");
    let state_home = unique_dir("state");
    let settings = Settings::load_from(&config);
    let state = State::with_settings(tmp.path().to_path_buf(), state_home, config, settings);
    let mut rt = Runtime::new(state, view, (1400.0, 900.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    let id = rt.state.vault.resolve("Board").expect("Board");
    rt.update(|s, _| s.open_kanban(id));
    rt.frame(1.0 / 60.0);

    rt.update(|s, _| s.new_note_from_card(0, 0));
    rt.frame(1.0 / 60.0);
    let note = fs::read_to_string(tmp.path().join("Write the release notes.md")).expect("note");
    assert_eq!(note, "tags: work\nWrite the release notes\n");
    let shown = board_text(&rt);
    assert!(
        shown.contains("- [ ] [[Write the release notes]]\n"),
        "{shown}"
    );
    assert!(
        shown.contains("^card-7f2"),
        "the other card's id is untouched"
    );
    assert_eq!(on_disk(&tmp), BOARD, "the kanban is not written until save");
}

#[test]
fn renaming_a_linked_note_reloads_a_clean_kanban_and_spares_a_dirty_one() {
    let tmp = vault();
    let (mut rt, config) = runtime(&tmp);

    rename_note(&mut rt, "Roadmap", "Plan");
    let disk = on_disk(&tmp);
    assert!(disk.contains("[[Plan]]"), "{disk}");
    assert!(!disk.contains("[[Roadmap]]"), "{disk}");
    assert!(board_text(&rt).contains("[[Plan]]"), "the page reloaded");
    assert!(rt.state.notice.is_none());
    assert_eq!(rt.state.mode, Mode::Kanban);

    // Unsaved edits: the file and the edits both stay, and the page says so.
    rt.update(|s, _| s.toggle_card_done(1, 0));
    rt.frame(1.0 / 60.0);
    let edited = board_text(&rt);
    assert!(edited.contains("- [x] Fix the sync bug\n"), "{edited}");
    let disk_before = on_disk(&tmp);
    rename_note(&mut rt, "Plan", "Later");
    assert_eq!(
        on_disk(&tmp),
        disk_before,
        "a dirty kanban is not rewritten"
    );
    let kept = board_text(&rt);
    assert!(kept.contains("- [x] Fix the sync bug\n"), "{kept}");
    assert!(kept.contains("[[Plan]]"), "the edit still names Plan");
    assert!(!kept.contains("[[Later]]"));
    assert_eq!(
        rt.state.notice.as_deref(),
        Some("Links in Board were not updated")
    );

    // Renaming the kanban itself follows in the saved path.
    let board = rt
        .state
        .vault
        .resolve("Board")
        .expect("still Board on disk");
    rt.update(|s, _| {
        s.begin_rename_note(board);
        s.rename_field.replace_text("Sprint");
        s.submit_rename_note();
        s.confirm_rename_note();
    });
    rt.frame(1.0 / 60.0);
    assert!(tmp.path().join("Sprint.md").is_file());
    assert!(!tmp.path().join("Board.md").is_file());
    assert_eq!(rt.state.kanban_rel.as_deref(), Some("Sprint.md"));
    let settings = fs::read_to_string(config.join("muninn/settings.json")).expect("settings");
    assert!(settings.contains("\"kanban\": \"Sprint.md\""), "{settings}");
    assert!(
        board_text(&rt).contains("- [x] Fix the sync bug\n"),
        "the unsaved edit survived the kanban's own rename"
    );
}

fn rename_note(rt: &mut Runtime<State>, title: &str, name: &str) {
    let id = rt.state.vault.resolve(title).expect(title);
    let name = name.to_owned();
    rt.update(move |s, _| {
        s.begin_rename_note(id);
        s.rename_field.replace_text(&name);
        s.submit_rename_note();
        s.confirm_rename_note();
    });
    rt.frame(1.0 / 60.0);
}

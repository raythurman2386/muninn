//! Kanban chrome extraction guard (#157): the Kanban page and its archive page
//! draw their static copy from the `muninn-i18n` catalog.
//!
//! The catalog pins the English (`en_us.rs`); these tests pin that the drawn
//! page really reads it, so a string that quietly goes back to a literal is a
//! failure here. The exact English is asserted on both sides: the key resolves
//! to it, and the element carries it. The two parameterised sentences are
//! typed helpers, not keys, so their exact English is asserted directly.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_i18n::{Key, filter_by_tag, restore_to, t};
use muninn_ui::{Settings, State, on_key, poll, view};
use rune::{Env, Runtime};

const BOARD: &str = "\
---
kanban-plugin: board
---

## Doing (1)

- [ ] one
- [ ] two

## Shipped

**Complete**
- [x] three
";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-copy-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

/// The texts the frame draws, in draw order.
fn drawn(rt: &Runtime<State>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter_map(|element| element.text)
        .collect()
}

fn has_text(rt: &Runtime<State>, words: &str) -> bool {
    drawn(rt).iter().any(|drawn| drawn == words)
}

fn on_board(tag: &str) -> Runtime<State> {
    let vault = TempVault::new(&[("Board.md", BOARD), ("Alpha.md", "# Alpha\n")]);
    let config = unique_dir(tag);
    let state_home = unique_dir(tag);
    let state = State::with_settings(
        vault.path().to_path_buf(),
        state_home,
        config,
        Settings::default(),
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

#[test]
fn the_kanban_page_draws_the_catalog_chrome() {
    let mut rt = on_board("page");

    // The page's landmarks and the lane header's state words.
    for key in [
        Key::KanbanPageTitle,
        Key::OverLimit,
        Key::KanbanLaneComplete,
        Key::AddCard,
        Key::KanbanLaneAdd,
    ] {
        let words = t(key);
        assert!(has_text(&rt, words), "{} ({words:?}) is drawn", key.id());
    }
    assert_eq!(t(Key::KanbanPageTitle), "Kanban");
    assert_eq!(t(Key::OverLimit), "Over limit");
    assert_eq!(t(Key::KanbanLaneComplete), "Complete");
    assert_eq!(t(Key::AddCard), "Add card");

    // The view switcher's tabs and the header buttons are catalog copy too.
    assert_eq!(t(Key::KanbanViewBoard), "Board");
    assert!(has_text(&rt, "Board"), "the Board tab is drawn");
    rt.update(|s, _| s.toggle_kanban_filter());
    rt.frame(1.0 / 60.0);
    assert_eq!(t(Key::KanbanFilterTitle), "Filter cards");
    assert!(
        has_text(&rt, "Filter cards"),
        "the filter panel's title is drawn"
    );
    assert_eq!(t(Key::KanbanFilterAnyDate), "Any date");
    assert!(has_text(&rt, "Any date"), "the Any date chip is drawn");
}

#[test]
fn the_archive_page_draws_the_catalog_chrome() {
    let mut rt = on_board("archive");
    rt.update(|s, _| s.toggle_kanban_archive());
    rt.frame(1.0 / 60.0);

    assert_eq!(t(Key::Archive), "Archive");
    assert_eq!(t(Key::KanbanArchiveEmpty), "No archived cards.");
    assert_eq!(t(Key::Close), "Close");
    for key in [Key::Archive, Key::KanbanArchiveEmpty, Key::Close] {
        let words = t(key);
        assert!(
            has_text(&rt, words),
            "{} ({words:?}) is drawn on the archive page",
            key.id()
        );
    }
}

#[test]
fn the_parameterised_kanban_sentences_keep_their_english() {
    // The archive lane picker and a card's tag chip build their sentence in a
    // typed helper, because the lane title and the tag are file data.
    assert_eq!(restore_to("To do"), "Restore to To do");
    assert_eq!(restore_to("Shipped"), "Restore to Shipped");
    assert_eq!(filter_by_tag("x"), "Filter by #x");
    assert_eq!(filter_by_tag("release"), "Filter by #release");
    // The static half of those controls is still a key.
    assert_eq!(t(Key::KanbanArchiveRestore), "Restore");
    assert_eq!(t(Key::KanbanArchiveRestoreTo), "Restore to…");
    assert_eq!(t(Key::KanbanArchiveDelete), "Delete");
}

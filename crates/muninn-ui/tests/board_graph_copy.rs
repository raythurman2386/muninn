//! Board and Graph chrome extraction guard (#157): both pages draw their static
//! copy from the `muninn-i18n` catalog.
//!
//! The catalog pins the English (`en_us.rs`); these tests pin that the drawn
//! Board and Graph pages really read it, so a string that quietly goes back to
//! a literal is a failure here. The exact English is asserted on both sides:
//! the key resolves to it, and the element carries it (as text or as its
//! accessible name). The three parameterised sentences are typed helpers, not
//! keys, so their exact English is asserted directly.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_i18n::{Key, board_name_taken, linked_note_hint, no_note_named, t};
use muninn_ui::{Mode, Settings, State, on_key, poll, view};
use rune::{Env, Runtime};

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-graph-copy-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

/// A headless app over `vault`, drawing `mode` after the first frames.
fn runtime(tag: &str, vault: &TempVault, mode: Mode) -> Runtime<State> {
    let config = unique_dir(tag);
    let state_home = unique_dir(tag);
    let state = State::with_settings(
        vault.path().to_path_buf(),
        state_home,
        config.clone(),
        Settings::load_from(&config),
    );
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt.update(|s, _| s.set_mode(mode));
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    rt
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

/// Whether some element is named `words` for assistive technology.
fn has_label(rt: &Runtime<State>, words: &str) -> bool {
    rt.elements()
        .iter()
        .any(|element| element.label.as_deref() == Some(words))
}

#[test]
fn the_board_page_draws_the_catalog_chrome() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", DRAW)]);
    let mut rt = runtime("board", &vault, Mode::Board);
    // Open the board: the canvas (and its hint) draws only with one open.
    let path = rt.state.boards[0].file.path.clone();
    rt.update(|s, _| s.select_board(path));
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }

    // The page's landmarks and the New board button (visible text and name).
    assert_eq!(t(Key::BoardPageTitle), "Boards");
    assert_eq!(t(Key::BoardPageNew), "New board");
    for key in [Key::BoardPageTitle, Key::BoardPageNew, Key::BoardPageHint] {
        let words = t(key);
        assert!(
            has_text(&rt, words),
            "{} ({words:?}) is drawn on the Board page",
            key.id()
        );
    }
    assert!(
        has_label(&rt, t(Key::BoardPageNew)),
        "the New board button is named from the catalog"
    );

    // The toolbar is named, and every tool button carries its catalog label.
    assert_eq!(t(Key::BoardToolToolbar), "Board tools");
    assert!(has_label(&rt, t(Key::BoardToolToolbar)));
    assert_eq!(t(Key::BoardToolHand), "Hand");
    assert_eq!(t(Key::BoardToolRectangle), "Rectangle");
    assert_eq!(t(Key::BoardToolFreedraw), "Freehand");
    assert_eq!(t(Key::BoardToolText), "Text");
    for key in [
        Key::BoardToolHand,
        Key::BoardToolRectangle,
        Key::BoardToolFreedraw,
        Key::BoardToolText,
    ] {
        assert!(
            has_label(&rt, t(key)),
            "{} ({:?}) names a tool button",
            key.id(),
            t(key)
        );
    }
    assert!(
        rt.elements().iter().any(|element| element
            .help
            .as_deref()
            .is_some_and(|help| help == t(Key::BoardToolRectangleHelp))),
        "the rectangle button's help is the catalog sentence"
    );

    // The palette's panel name and a swatch name are catalog copy too.
    rt.update(|s, _| s.toggle_board_palette());
    rt.frame(1.0 / 60.0);
    assert_eq!(t(Key::BoardPaletteTitle), "Board colours");
    assert_eq!(t(Key::BoardPaletteStroke), "Stroke");
    assert_eq!(t(Key::BoardPaletteSwatchBlack), "black");
    assert!(has_label(&rt, t(Key::BoardPaletteTitle)));
    assert!(has_label(&rt, "Stroke black"));
}

#[test]
fn the_empty_board_page_draws_the_catalog_empty_states() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let rt = runtime("board-empty", &vault, Mode::Board);

    // No board on disk: the panel line, the placeholder and its way in.
    assert_eq!(t(Key::NoBoards), "No boards in this vault yet");
    assert_eq!(t(Key::BoardPageEmptyTitle), "No boards yet");
    assert_eq!(t(Key::BoardPageEmptyDetail), "Make one with New board");
    for key in [
        Key::NoBoards,
        Key::BoardPageEmptyTitle,
        Key::BoardPageEmptyDetail,
        Key::BoardPageEmptyHint,
    ] {
        assert!(
            has_text(&rt, t(key)),
            "{} ({:?}) is drawn on the empty Board page",
            key.id(),
            t(key)
        );
    }
}

#[test]
fn the_graph_page_draws_the_catalog_chrome() {
    // A vault with no notes: the canvas says so, and the controls are up.
    let vault = TempVault::new(&[("Draw.excalidraw", DRAW)]);
    let rt = runtime("graph-empty", &vault, Mode::Graph);

    assert_eq!(t(Key::GraphPageEmpty), "No notes to draw");
    assert_eq!(
        t(Key::GraphPageHint),
        "Arrows focus · Enter open · +/- zoom · 0/F fit · I isolate · Drag pan · Scroll zoom"
    );
    for key in [
        Key::GraphPageEmpty,
        Key::GraphPageHint,
        Key::GraphScopeGlobal,
        Key::GraphScopeLocal,
        Key::GraphActionFit,
        Key::GraphActionReset,
        Key::GraphActionIsolate,
    ] {
        assert!(
            has_text(&rt, t(key)),
            "{} ({:?}) is drawn on the Graph page",
            key.id(),
            t(key)
        );
    }
    assert_eq!(t(Key::GraphPageFindNode), "Find graph node");
    assert_eq!(t(Key::GraphPageFindPlaceholder), "Find node · Enter");
    assert!(
        has_label(&rt, t(Key::GraphPageFindNode)),
        "the Find field is named from the catalog"
    );
}

#[test]
fn the_graph_info_card_draws_the_catalog_chrome() {
    let vault = TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
        ("Beta.md", "# Beta\n"),
    ]);
    let mut rt = runtime("graph-selected", &vault, Mode::Graph);
    // Select a note: the info card names the selection and its actions.
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.update(|s, _| s.selected = Some(alpha));
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }

    assert_eq!(t(Key::GraphPageSelected), "SELECTED");
    assert_eq!(t(Key::GraphActionOpenNote), "Open note");
    for key in [
        Key::GraphPageSelected,
        Key::GraphActionOpenNote,
        Key::GraphScopeGlobal,
    ] {
        assert!(
            has_text(&rt, t(key)),
            "{} ({:?}) is drawn on the Graph info card",
            key.id(),
            t(key)
        );
    }
}

#[test]
fn the_parameterised_board_sentences_keep_their_english() {
    // A rename conflict, an unknown note link and the linked-note hint name
    // file data inside a fixed sentence, so they are typed helpers.
    assert_eq!(
        board_name_taken("Plan"),
        "A board named “Plan” is already in this folder"
    );
    assert_eq!(no_note_named("Plan"), "No note named “Plan” in this vault");
    assert_eq!(
        linked_note_hint("Plan"),
        "Linked note: Plan · \u{2318}Enter opens"
    );
    // The static half of those controls is still a key.
    assert_eq!(t(Key::BoardRenameName), "Board name");
    assert_eq!(t(Key::BoardLinkTitle), "Link note");
    assert_eq!(t(Key::BoardLinkName), "Note name or path");
    assert_eq!(t(Key::BoardLinkHint), "a name or vault path · empty clears");
}

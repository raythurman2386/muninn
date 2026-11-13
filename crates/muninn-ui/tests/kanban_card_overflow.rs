//! Issue #66: one unbroken run of card text stays inside the card.
//!
//! The reader's Markdown has no wrap-anywhere fallback, so card text joined
//! by `=`, `.`, `/` or `_` used to paint past the card and into the next
//! lane. These tests read the laid-out text elements: a wrapped body is more
//! than one line tall, a link is no wider than the body, and a chip is no
//! wider than the card. The stored board and the editor keep the original
//! text; only the drawing gains break opportunities.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::KanbanView;
use muninn_core::testing::TempVault;
use muninn_ui::{Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const ENV: &str = "NEXT_PUBLIC_CONSTRUCTION_MODE=true";
const URL: &str = "https://www.sistersbythemoon.com/api/webhooks/stripe/events";
const TAG: &str = "#this_is_a_very_long_tag_that_should_wrap_inside_the_card";
const WIKI: &str = "THE_LONG_ROADMAP_LABEL_THAT_WRAPS_INSIDE_THE_CARD";

const BOARD: &str = "\
---
kanban-plugin: board
---

## To do

- [ ] NEXT_PUBLIC_CONSTRUCTION_MODE=true
- [ ] https://www.sistersbythemoon.com/api/webhooks/stripe/events
- [ ] #this_is_a_very_long_tag_that_should_wrap_inside_the_card
- [ ] [[Roadmap|THE_LONG_ROADMAP_LABEL_THAT_WRAPS_INSIDE_THE_CARD]]

%% kanban:settings
```
{\"show-tags-at-bottom\":true}
```
%%
";

/// The zero-width space the card's display source gains: a break
/// opportunity with no ink.
const BREAK: char = '\u{200B}';

/// A line of body text at the kit's caption size is about 22.5 points tall.
const ONE_LINE: f32 = 30.0;

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-kanban-overflow-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn board_runtime() -> (Runtime<State>, TempVault) {
    board_runtime_sized(1400.0)
}

fn board_runtime_sized(width: f32) -> (Runtime<State>, TempVault) {
    let tmp = TempVault::new(&[
        ("Agenda.md", "# Agenda\n\nA plain note.\n"),
        ("Board.md", BOARD),
        ("Roadmap.md", "# Roadmap\n\nPlans.\n"),
    ]);
    let config = unique_dir("config");
    let state_home = unique_dir("state");
    let state = State::with_settings(
        tmp.path().to_path_buf(),
        state_home,
        config.clone(),
        Settings::load_from(&config),
    );
    let mut rt = Runtime::new(state, view, (width, 900.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    let id = rt.state.vault.resolve("Board").expect("Board");
    rt.update(|s, _| s.open_kanban(id));
    settle(&mut rt);
    (rt, tmp)
}

fn settle(rt: &mut Runtime<State>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn live(rt: &Runtime<State>) -> Vec<rune::ElementInfo> {
    rt.elements().into_iter().filter(|e| !e.exiting).collect()
}

fn element<'a>(elements: &'a [rune::ElementInfo], key: &str) -> Option<&'a rune::ElementInfo> {
    elements.iter().find(|e| e.key.as_deref() == Some(key))
}

/// The body `b0` whose displayed text is `want`.
fn body<'a>(elements: &'a [rune::ElementInfo], want: &str) -> &'a rune::ElementInfo {
    elements
        .iter()
        .find(|e| {
            e.key.as_deref() == Some("b0")
                && e.text
                    .as_deref()
                    .is_some_and(|text| text.replace(BREAK, "") == want)
        })
        .unwrap_or_else(|| panic!("no card body for {want}"))
}

fn right(rect: [f32; 4]) -> f32 {
    rect[0] + rect[2]
}

fn assert_inside(inner: [f32; 4], outer: [f32; 4], what: &str) {
    assert!(
        inner[2] <= outer[2] + 0.5 && right(inner) <= right(outer) + 0.5,
        "{what} overflows: {:?} is not inside {:?}",
        inner,
        outer
    );
}

#[test]
fn long_card_text_wraps_inside_the_board_lane() {
    let (mut rt, tmp) = board_runtime();
    let elements = live(&rt);
    let lane = element(&elements, "kanban-lane-0").expect("the lane");
    assert_eq!(lane.rect[2], 272.0, "the default lane width");

    for token in [ENV, URL, TAG, WIKI] {
        let text = body(&elements, token);
        assert!(
            text.text
                .as_deref()
                .is_some_and(|shown| shown.contains(BREAK)),
            "{token} gained no break opportunity: {:?}",
            text.text
        );
        assert!(
            text.rect[3] > ONE_LINE,
            "{token} stayed on one line: {:?}",
            text.rect
        );
        assert_inside(text.rect, lane.rect, token);
    }

    // The URL is still one link over the exact address, now no wider than
    // the body it sits in.
    let body = body(&elements, URL);
    let link = elements
        .iter()
        .find(|e| {
            e.role == "link"
                && e.text
                    .as_deref()
                    .is_some_and(|t| t.replace(BREAK, "") == URL)
        })
        .expect("the URL link");
    assert_inside(link.rect, body.rect, "the URL link");

    // One long chip shrinks and wraps instead of blowing the card's width.
    let chip = element(&elements, "kanban-card-0-2-tag-0").expect("the tag chip");
    let card = element(&elements, "kanban-card-0-2").expect("the tag card");
    assert_inside(chip.rect, card.rect, "the tag chip");
    assert!(
        chip.rect[2] <= body.rect[2] + 0.5,
        "the chip is wider than the card's text column: {:?} vs {:?}",
        chip.rect,
        body.rect
    );
    assert!(
        chip.rect[3] > 18.0,
        "the chip did not grow to wrap its long tag: {:?}",
        chip.rect
    );
    let chip_text = elements
        .iter()
        .find(|e| {
            e.role == "text"
                && e.text.as_deref() == Some(TAG)
                && e.rect[1] >= chip.rect[1] - 0.5
                && e.rect[1] <= chip.rect[1] + chip.rect[3]
        })
        .expect("the chip's text");
    assert_inside(chip_text.rect, chip.rect, "the chip text");

    // A wikilink's long label breaks too, and its note still opens. Do this
    // before an external link: opening one leaves Kanban for the note mode.
    let wiki = elements
        .iter()
        .find(|e| e.role == "link" && e.key.as_deref() == Some("note:Roadmap"))
        .expect("the wikilink");
    rt.pointer_down(wiki.rect[0] + 4.0, wiki.rect[1] + 4.0);
    rt.pointer_up(wiki.rect[0] + 4.0, wiki.rect[1] + 4.0);
    settle(&mut rt);
    assert_eq!(
        rt.state.selected_path,
        Some(tmp.path().join("Roadmap.md")),
        "the broken wikilink label still opens its note"
    );

    // The visible URL is still one clickable link to the exact address.
    let board = rt.state.vault.resolve("Board").expect("Board");
    rt.update(|s, _| s.open_kanban(board));
    settle(&mut rt);
    let (x, y) = rt.find(URL).expect("the URL link is clickable");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.take_url_requests(), [URL.to_owned()]);

    let on_disk = fs::read_to_string(tmp.path().join("Board.md")).expect("Board.md");
    assert_eq!(
        on_disk, BOARD,
        "viewing writes nothing and the text is unchanged"
    );
}

#[test]
fn list_and_table_keep_the_same_text_inside() {
    // A narrow List card: the 60-character URL is wider than the row, so it
    // only stays inside when the display text can break.
    let (mut rt, tmp) = board_runtime_sized(600.0);

    rt.update(|s, _| s.set_kanban_view(KanbanView::List));
    settle(&mut rt);
    let elements = live(&rt);
    for (index, token) in [ENV, URL, TAG, WIKI].into_iter().enumerate() {
        let text = body(&elements, token);
        let card = element(&elements, &format!("kanban-card-0-{index}")).expect("the list card");
        assert!(
            text.rect[3] > ONE_LINE,
            "{token} stayed on one line in List: {:?}",
            text.rect
        );
        assert_inside(text.rect, card.rect, token);
    }

    rt.update(|s, _| s.set_kanban_view(KanbanView::Table));
    settle(&mut rt);
    let elements = live(&rt);
    for index in 0..4 {
        let row = element(&elements, &format!("kanban-table-0-{index}")).expect("the table row");
        let text =
            element(&elements, &format!("kanban-table-0-{index}-text")).expect("the cell text");
        assert_inside(text.rect, row.rect, "the table card text");
    }
    let tags = element(&elements, "kanban-table-0-2-tags").expect("the Tags column");
    let chip = element(&elements, "kanban-table-0-2-tag-0").expect("the table tag chip");
    assert_inside(chip.rect, tags.rect, "the table tag chip");

    let on_disk = fs::read_to_string(tmp.path().join("Board.md")).expect("Board.md");
    assert_eq!(on_disk, BOARD, "switching views writes nothing");
}

#[test]
fn the_editor_shows_and_commits_the_original_text() {
    let (mut rt, tmp) = board_runtime();
    rt.update(|s, _| s.begin_edit_card(0, 0));
    settle(&mut rt);
    let shown = rt.state.kanban_card_field.text.clone();
    assert_eq!(
        shown, ENV,
        "the editor gets the stored bytes, not the display copy"
    );
    assert!(
        !shown.contains(BREAK),
        "no break character reaches the editor"
    );

    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    settle(&mut rt);
    let source = rt
        .state
        .kanban
        .as_ref()
        .and_then(|k| k.board.as_ref())
        .expect("the board")
        .source()
        .to_owned();
    assert!(source.contains(ENV), "{source}");
    assert!(
        !source.contains(BREAK),
        "nothing soft is written to the board"
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("Board.md")).expect("Board.md"),
        BOARD,
        "committing unchanged text writes nothing"
    );
}

//! The Kanban page (`Mode::Kanban`): one kanban note shown as a read-only
//! board.
//!
//! The note is read from disk and parsed with [`Kanban`] when Kanban mode is
//! entered, and again when the selection or the vault changes while the page
//! is up ([`State::refresh_kanban`]). Frames draw that parse. Nothing here
//! writes the file. The page shows:
//!
//! - the selected note, when it is a kanban note;
//! - else the kanban shown last ([`State::kanban_rel`], restored from
//!   `settings.json`), while that note is in the vault and still a kanban;
//! - else an empty state.
//!
//! Lanes are columns in file order. A lane header has the title and the card
//! count, `n/N` against a lane limit with an over-limit flag, and Complete on
//! a done lane. Cards show their done mark and their markdown through the
//! reader's renderer. Passthrough text and the archive are not drawn. A note
//! that detects but does not parse offers Open as markdown instead.

use std::fs;
use std::path::Path;

use muninn_core::{Card, Kanban, Lane, is_kanban};
use rune::{El, col, el, icon, row, text};
use rune_kit::{Kit, Markdown, MarkdownStyle};
use rune_theme_opencode::icons as ic;

use crate::{Mode, State};

/// Width of a lane column, in points.
const LANE_W: f32 = 272.0;

/// The kanban note on the Kanban page, as read from disk.
#[derive(Clone, Debug)]
pub struct ShownKanban {
    /// Vault-relative path with `/` separators.
    pub rel: String,
    /// The parsed board. `None` when the note is a kanban note whose file
    /// can't be read as a board (the parse was `InvalidData`).
    pub board: Option<Kanban>,
}

impl State {
    /// Pick and parse the kanban the page shows (see the module docs). The
    /// saved kanban follows a selected kanban note; a selected note that is
    /// not one leaves it as it is.
    pub fn refresh_kanban(&mut self) {
        let selected = self.selected_rel().map(str::to_owned);
        let shown = selected
            .and_then(|rel| self.read_kanban(rel))
            .or_else(|| self.read_kanban(self.kanban_rel.clone()?));
        if let Some(shown) = &shown {
            self.kanban_rel = Some(shown.rel.clone());
        }
        self.kanban = shown;
    }

    /// `rel` read from disk, when it is a note in this vault and a kanban.
    fn read_kanban(&self, rel: String) -> Option<ShownKanban> {
        let note = self.vault.notes.iter().find(|note| note.rel == rel)?;
        let source = fs::read_to_string(&note.path).ok()?;
        is_kanban(&source).then(|| ShownKanban {
            board: Kanban::parse(&source).ok(),
            rel,
        })
    }

    /// A kanban belongs to its vault: forget it on a vault switch.
    pub(crate) fn clear_kanban(&mut self) {
        self.kanban_rel = None;
        self.kanban = None;
    }

    /// Open as markdown: the shown note in Write.
    pub fn open_kanban_as_markdown(&mut self) {
        let Some(id) = self
            .kanban
            .as_ref()
            .and_then(|shown| self.note_id(&shown.rel))
        else {
            return;
        };
        self.set_mode(Mode::Editor);
        self.load_note(id, true);
    }

    fn note_id(&self, rel: &str) -> Option<usize> {
        self.vault
            .notes
            .iter()
            .find(|note| note.rel == rel)
            .map(|note| note.id)
    }
}

/// The lane header's count, `n` or `n/N` against the lane limit, and whether
/// the lane holds more cards than its limit.
fn lane_load(lane: &Lane) -> (String, bool) {
    let n = lane.cards().len();
    match lane.limit() {
        Some(limit) => (format!("{n}/{limit}"), n > limit as usize),
        None => (n.to_string(), false),
    }
}

pub fn kanban(s: &State, k: &Kit) -> El {
    let body = match &s.kanban {
        None => placeholder(
            k,
            "No kanban open",
            "Select a kanban note, then press ⌘5 to see it as a board.",
        ),
        Some(ShownKanban {
            rel,
            board: Some(board),
        }) => lanes(s, k, rel, board),
        Some(ShownKanban { board: None, .. }) => placeholder(
            k,
            "This note can't be shown as a kanban",
            "Its settings footer is unfinished or is not valid JSON. The file is left as it is.",
        )
        .child(open_as_markdown(k)),
    };
    col()
        .key("kanban-page")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(header(s, k))
        .child(
            col()
                .key("kanban-body")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .px(28.0)
                .pb(24.0)
                .child(body),
        )
}

fn header(s: &State, k: &Kit) -> El {
    let head = row().h(58.0).px(28.0).gap(10.0).center_y();
    let Some(shown) = &s.kanban else {
        return head.child(
            text("Kanban")
                .key("kanban-title")
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        );
    };
    let title = Path::new(&shown.rel)
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    head.child(
        text(title)
            .key("kanban-title")
            .font(k.ty.ui_medium)
            .color(k.c.text_base),
    )
    .child(
        text(format!("{} · Read only", shown.rel))
            .key("kanban-detail")
            .font(k.ty.caption())
            .color(k.c.text_faint)
            .truncate(),
    )
}

/// The lanes, side by side, scrolling both ways. Built again only when the
/// file's text or the theme changes.
fn lanes(s: &State, k: &Kit, rel: &str, board: &Kanban) -> El {
    let note = s.note_id(rel);
    let body = rune::memo(
        "kanban-lanes",
        (rel, board.source(), crate::omarchy::kit_id(s, k.dark)),
        move || {
            row().gap(12.0).items_start().children(
                board
                    .lanes()
                    .iter()
                    .enumerate()
                    .map(|(i, lane)| lane_column(s, k, note, i, lane)),
            )
        },
    );
    col()
        .key("kanban-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll_xy()
        .child(body)
}

fn lane_column(s: &State, k: &Kit, note: Option<usize>, i: usize, lane: &Lane) -> El {
    let (count, over) = lane_load(lane);
    let count_color = if over { k.c.warning.fg } else { k.c.text_faint };
    let mut head = row()
        .key(format!("kanban-lane-{i}-head"))
        .w_full()
        .gap(8.0)
        .center_y()
        .child(
            text(lane.title().to_owned())
                .key(format!("kanban-lane-{i}-title"))
                .font(k.ty.ui_medium)
                .color(k.c.text_base)
                .grow(1.0)
                .truncate(),
        )
        .child(
            text(count.clone())
                .key(format!("kanban-lane-{i}-count"))
                .font(k.ty.caption())
                .color(count_color),
        );
    if over {
        head = head.child(
            text("Over limit")
                .key(format!("kanban-lane-{i}-over"))
                .font(k.ty.caption())
                .color(k.c.warning.fg),
        );
    }
    let mut label = format!("{}, {count} cards", lane.title());
    if over {
        label.push_str(", over limit");
    }
    let mut column = col()
        .key(format!("kanban-lane-{i}"))
        .w(LANE_W)
        .padding(10.0)
        .gap(8.0)
        .radius(10.0)
        .fill(k.c.layer_01)
        .border(
            1.0,
            if over {
                k.c.warning.fg
            } else {
                k.c.border_muted
            },
        )
        .role("list")
        .label(label)
        .child(head);
    if lane.done_word().is_some() {
        column = column.child(
            row()
                .key(format!("kanban-lane-{i}-complete"))
                .gap(6.0)
                .center_y()
                .child(icon(&ic::CHECK, 12.0).color(k.c.success.fg))
                .child(text("Complete").font(k.ty.caption()).color(k.c.success.fg)),
        );
    }
    column.children(
        lane.cards()
            .iter()
            .enumerate()
            .map(|(j, card)| card_view(s, k, note, i, j, card)),
    )
}

fn card_view(s: &State, k: &Kit, note: Option<usize>, i: usize, j: usize, card: &Card) -> El {
    let done = card.is_done();
    // Wikilinks and embeds read as the reader shows them; following them
    // from a card is slice 8.
    let source = match note {
        Some(id) => s.vault.preview_with_attachments(card.text(), id),
        None => card.text().to_owned(),
    };
    let first = card.text().lines().next().unwrap_or_default();
    let mark = el()
        .key(format!("kanban-card-{i}-{j}-mark"))
        .size(14.0, 14.0)
        // Level with the first line of text.
        .mt(5.0)
        .radius(4.0)
        .center()
        .border(
            1.0,
            if done {
                k.c.text_accent
            } else {
                k.c.border_base
            },
        )
        .role("checkbox")
        .checked(done)
        .label(if done { "Done" } else { "Open" });
    let mark = if done {
        mark.child(icon(&ic::CHECK, 10.0).color(k.c.text_accent))
    } else {
        mark
    };
    row()
        .key(format!("kanban-card-{i}-{j}"))
        .w_full()
        .padding(10.0)
        .gap(8.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(1.0, k.c.border_muted)
        .role("listitem")
        .label(format!("{}: {first}", if done { "Done" } else { "Open" }))
        .child(mark)
        .child(
            col().grow(1.0).min_w(0.0).child(
                Markdown::new(source)
                    .style(MarkdownStyle::from_kit(k).link(k.c.text_accent))
                    .tasks_read_only()
                    .build(k),
            ),
        )
}

fn open_as_markdown(k: &Kit) -> El {
    row()
        .key("kanban-open-markdown")
        .h(28.0)
        .px(10.0)
        .gap(6.0)
        .radius(7.0)
        .center_y()
        .border(1.0, k.c.border_muted)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label("Open as markdown")
        .on_press(State::open_kanban_as_markdown)
        .child(icon(&ic::EDIT, 12.0).color(k.c.text_accent))
        .child(
            text("Open as markdown")
                .font(k.ty.ui_compact)
                .color(k.c.text_base),
        )
}

fn placeholder(k: &Kit, title: &str, detail: &str) -> El {
    col()
        .key("kanban-placeholder")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .radius(10.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
        .center()
        .gap(8.0)
        .child(icon(&ic::CHECKLIST, 22.0).color(k.c.icon_muted))
        .child(
            text(title.to_owned())
                .key("kanban-placeholder-title")
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(
            text(detail.to_owned())
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loads(source: &str) -> Vec<(String, bool)> {
        Kanban::parse(source)
            .expect("parse")
            .lanes()
            .iter()
            .map(lane_load)
            .collect()
    }

    #[test]
    fn a_lane_is_over_its_limit_only_past_it() {
        let source = "---\nkanban-plugin: board\n---\n\n## Free\n\n- [ ] a\n\n\
                      ## Full (2)\n\n- [ ] a\n- [ ] b\n\n## Over (1)\n\n- [ ] a\n- [x] b\n\n\
                      ## Zero (0)\n\n- [ ] a\n";
        assert_eq!(
            loads(source),
            [
                ("1".to_owned(), false),
                ("2/2".to_owned(), false),
                ("2/1".to_owned(), true),
                // A written (0) is no limit (spec section 4).
                ("1".to_owned(), false),
            ]
        );
    }
}

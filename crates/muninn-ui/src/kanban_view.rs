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
//! reader's renderer, broken wikilinks dimmed as Read dims them. Passthrough
//! text and the archive are not drawn. The header's Open as markdown returns
//! to Write on the shown note; a note that detects but does not parse says
//! so instead of drawing lanes.
//!
//! Slice 3 adds the ways in: the sidebar's Kanbans section and the palette's
//! kanban entries ([`State::open_kanban`]), New Kanban
//! ([`State::new_kanban`]) and Convert empty note to Kanban
//! ([`State::convert_to_kanban`]). Picking a kanban note in the notes list
//! while the page is up keeps the page (`State::open_from_list`).

use std::fs;
use std::path::Path;

use muninn_core::{Card, Kanban, Lane, is_kanban};
use rune::{El, col, el, icon, row, text};
use rune_kit::{Kit, Markdown, MarkdownStyle};
use rune_theme_opencode::icons as ic;

use crate::board_view::placeholder;
use crate::{Mode, State};

/// Width of a lane column, in points: the notes sidebar's width, so a lane
/// reads like the note list beside it.
const LANE_W: f32 = crate::view::SIDEBAR;

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

    /// Open note `id` on the Kanban page (the sidebar's Kanbans section, a
    /// kanban in the palette). The keyboard goes to the page: no text field
    /// keeps it (closing the palette hands back the field it took it from),
    /// so the page's own keys (Esc) reach it.
    pub fn open_kanban(&mut self, id: usize) {
        self.load_note(id, false);
        self.set_mode(Mode::Kanban);
        self.set_text_focus(false, false);
    }

    /// New Kanban: write `Untitled Kanban.md` (never over an existing file)
    /// in the folder New note would use, and open it on the Kanban page.
    pub fn new_kanban(&mut self) {
        self.flush();
        let folder = self.current_folder();
        match self.vault.create_kanban_in(&folder) {
            Ok(path) => {
                let rel = path
                    .strip_prefix(&self.vault.root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                self.log_event("kanban", &format!("created {rel}"));
                self.reload(Some(path));
                self.set_mode(Mode::Kanban);
                self.set_text_focus(false, false);
            }
            Err(e) => {
                self.log_event("kanban", &format!("new kanban not created: {e}"));
                self.notice = Some(format!("Could not create a kanban: {e}"));
            }
        }
    }

    /// The palette offers Convert empty note to Kanban only while the
    /// selected note holds nothing but whitespace.
    #[must_use]
    pub fn can_convert_to_kanban(&self) -> bool {
        self.selected.is_some() && self.editor.text.trim().is_empty()
    }

    /// Convert empty note to Kanban: write the New Kanban layout into the
    /// selected note and show it on the Kanban page. A note with text is
    /// refused and nothing is written.
    pub fn convert_to_kanban(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        self.flush();
        if let Err(e) = self
            .vault
            .convert_to_kanban(id)
            .and_then(|()| self.load_disk_into_editor(id))
        {
            self.log_event("kanban", &format!("not converted: {e}"));
            self.notice = Some(format!("Could not convert to a kanban: {e}"));
            return;
        }
        if self.mode == Mode::Kanban {
            self.refresh_kanban();
        } else {
            self.set_mode(Mode::Kanban);
        }
        self.set_text_focus(false, false);
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

    pub(crate) fn note_id(&self, rel: &str) -> Option<usize> {
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
            "kanban-placeholder",
            &ic::CHECKLIST,
            "No kanban open",
            "Select a kanban note, then press ⌘5 to see it as a board.",
        ),
        Some(ShownKanban {
            rel,
            board: Some(board),
        }) => lanes(s, k, rel, board),
        Some(ShownKanban { board: None, .. }) => placeholder(
            k,
            "kanban-placeholder",
            &ic::CHECKLIST,
            "This note can't be shown as a kanban",
            "Its settings footer is unfinished or is not valid JSON. The file is left as it is.",
        ),
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
            .grow(1.0)
            .truncate(),
    )
    .child(header_button(
        k,
        "kanban-open-markdown",
        &ic::EDIT,
        "Open as markdown",
        State::open_kanban_as_markdown,
    ))
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
                    .link_style(crate::view::missing_link_style(k))
                    .tasks_read_only()
                    .build(k),
            ),
        )
}

/// A small bordered button in a page header: Open as markdown here, Open as
/// kanban in a kanban note's title bar.
pub(crate) fn header_button(
    k: &Kit,
    key: &'static str,
    glyph: &'static rune::Icon,
    label: &'static str,
    action: fn(&mut State),
) -> El {
    row()
        .key(key)
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
        .label(label)
        .on_press(action)
        .child(icon(glyph, 12.0).color(k.c.text_accent))
        .child(text(label).font(k.ty.ui_compact).color(k.c.text_base))
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

//! Kanban slice 15: the List and Table views.
//!
//! The file's `kanban-plugin` value asks for one of them ([`Kanban::view`]);
//! the header's switcher can show any of the three for the session. Both views
//! read the same parsed board and the same card text scanner as the board, so
//! the cards, lanes, dates, tags, metadata, checklists and filters agree.
//!
//! - **List** draws each lane as a section in one vertical scroll: the lane
//!   title, its count (`n/N`, over-limit and Complete), then its cards as
//!   rows. A collapsed lane keeps its header and hides its cards; an empty
//!   lane says so.
//! - **Table** draws a header row (Lane, Card, Date, Tags, Progress) and one
//!   row per card, grouped by lane in file order, in a viewport that scrolls
//!   both ways. A card's multi-line text is flattened to one line.
//!
//! Neither view writes: the card's checkbox (toggle done, Cmd/Ctrl-click to
//! archive), its menu, its links and a tag chip's filter work as on the board,
//! and editing stays on the board view. Switching views never changes the
//! frontmatter or any other byte.

use muninn_core::{
    Card, CardText, CivilDate, DateSettings, Kanban, Lane, MetadataCache, MetadataSettings,
    TagSettings, card_checklist, format_date, format_time, parse_links, scan_card_text,
};
use rune::{El, ScrollSpec, Tx, col, icon, row, text};
use rune_kit::{Kit, Markdown, MarkdownStyle};
use rune_theme_opencode::icons as ic;

use crate::State;
use crate::kanban_view::{
    card_checkbox, card_markdown, checklist_section, is_menu, lane_load, menu_button,
    without_checklist_lines,
};

/// Table column widths, in points. The card column takes the rest.
const LANE_COL: f32 = 150.0;
const DATE_COL: f32 = 150.0;
const TAGS_COL: f32 = 180.0;
const PROGRESS_COL: f32 = 76.0;
/// The table's width, so a narrow window scrolls sideways instead of
/// squeezing the card column away. A horizontal viewport lays content at its
/// natural width, so every row names this width rather than `w_full`.
const TABLE_W: f32 = 1080.0;

/// The settings and the day one frame draws with, resolved once.
struct Context {
    note: Option<usize>,
    dates: DateSettings,
    today: CivilDate,
    tags: TagSettings,
    metadata: MetadataSettings,
}

impl Context {
    fn new(s: &State, rel: &str) -> Self {
        Self {
            note: s.note_id(rel),
            dates: s.kanban_date_settings(),
            today: crate::kanban_dates::local_today(),
            tags: s.kanban_tag_settings(),
            metadata: s.kanban_metadata_settings(),
        }
    }
}

/// The board as one scrollable list of lane sections.
pub(crate) fn list(s: &State, k: &Kit, rel: &str, board: &Kanban) -> El {
    let ctx = Context::new(s, rel);
    let mut sections = Vec::new();
    for (i, lane) in board.lanes().iter().enumerate() {
        sections.push(list_lane(s, k, &ctx, i, lane));
    }
    let content = if sections.is_empty() {
        empty_state(k, "kanban-list-empty", "No lanes on this board")
    } else {
        col()
            .key("kanban-list")
            .w_full()
            .gap(14.0)
            .children(sections)
    };
    col()
        .key("kanban-list-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(content)
}

/// One lane section of the list: its header, then its cards as rows.
fn list_lane(s: &State, k: &Kit, ctx: &Context, i: usize, lane: &Lane) -> El {
    let filtering = s.kanban_filter_active();
    let cards: Vec<(usize, &Card)> = lane
        .cards()
        .iter()
        .enumerate()
        .filter(|(_, card)| s.kanban_filter_matches_card(card, &ctx.dates, ctx.today))
        .collect();
    let (base_count, over) = lane_load(lane);
    let count = if filtering {
        format!("{}/{}", cards.len(), lane.cards().len())
    } else {
        base_count
    };
    let mut section = col()
        .key(format!("kanban-list-lane-{i}"))
        .w_full()
        .gap(6.0)
        .padding(12.0)
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
        .label(format!("{}, {count} cards", lane.title()))
        .child(list_head(k, i, lane, &count, over));
    if is_menu(s, &crate::MenuTarget::Lane(i)) || is_menu(s, &crate::MenuTarget::DeleteLane(i)) {
        section = section.child(crate::view::context_menu(s, k));
    }
    if lane.collapsed() {
        return section.child(
            text(format!("Collapsed · {} cards", lane.cards().len()))
                .key(format!("kanban-list-lane-{i}-collapsed"))
                .font(k.ty.caption())
                .color(k.c.text_faint),
        );
    }
    if cards.is_empty() {
        return section.child(empty_lane(k, i, filtering && !lane.cards().is_empty()));
    }
    // One cache for this lane: a distinct linked note is parsed once.
    let metadata = MetadataCache::for_lane(&s.vault, lane, ctx.note, ctx.metadata.clone());
    for &(j, card) in &cards {
        let (rows, value_note) = metadata.rows(&s.vault, ctx.note, card.text());
        section = section.child(list_card(s, k, ctx, i, j, lane, card, &rows, value_note));
        if is_menu(s, &crate::MenuTarget::Card(i, j))
            || is_menu(s, &crate::MenuTarget::MoveCard(i, j))
        {
            section = section.child(crate::view::context_menu(s, k));
        }
    }
    section
}

/// One lane's header in the list: title, count, over-limit, Complete, menu.
fn list_head(k: &Kit, i: usize, lane: &Lane, count: &str, over: bool) -> El {
    let mut head = row()
        .key(format!("kanban-list-lane-{i}-head"))
        .w_full()
        .gap(8.0)
        .center_y()
        .child(
            text(lane.title().to_owned())
                .key(format!("kanban-list-lane-{i}-title"))
                .font(k.ty.ui_medium)
                .color(k.c.text_base)
                .grow(1.0)
                .truncate(),
        )
        .child(
            text(count.to_owned())
                .key(format!("kanban-list-lane-{i}-count"))
                .font(k.ty.caption())
                .color(if over { k.c.warning.fg } else { k.c.text_faint }),
        );
    if over {
        head = head.child(
            text("Over limit")
                .key(format!("kanban-list-lane-{i}-over"))
                .font(k.ty.caption())
                .color(k.c.warning.fg),
        );
    }
    if lane.done_word().is_some() {
        head = head
            .child(icon(&ic::CHECK, 12.0).color(k.c.success.fg))
            .child(
                text("Complete")
                    .key(format!("kanban-list-lane-{i}-complete"))
                    .font(k.ty.caption())
                    .color(k.c.success.fg),
            );
    }
    head.child(
        menu_button(k, format!("kanban-list-lane-{i}-menu"), "Lane actions")
            .on_press(move |s: &mut State| s.open_lane_menu(i)),
    )
}

/// One card row in the list: the same text, dates, tags, metadata and
/// checklist as the board card, without drag or editing.
#[allow(clippy::too_many_arguments)]
fn list_card(
    s: &State,
    k: &Kit,
    ctx: &Context,
    i: usize,
    j: usize,
    lane: &Lane,
    card: &Card,
    rows: &[muninn_core::MetadataRow],
    value_note: Option<usize>,
) -> El {
    let done = card.is_done();
    let found = scan_card_text(card.text(), &ctx.dates);
    let title_text = found.without(
        card.text(),
        ctx.dates.hide_date_in_title,
        ctx.tags.hide_tags_in_title,
    );
    let checklist = card_checklist(card.text());
    let display_text = without_checklist_lines(&title_text, &checklist);
    let source = card_markdown(&s.vault, ctx.note, &display_text);
    let first = display_text.lines().next().unwrap_or_default();
    let selected = s.kanban_selected == Some((i, j));
    row()
        .key(format!("kanban-card-{i}-{j}"))
        .w_full()
        .padding(10.0)
        .gap(8.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(
            1.0,
            if selected {
                k.c.text_accent
            } else {
                k.c.border_muted
            },
        )
        .role("listitem")
        .label(format!(
            "{} · {}: {first}",
            lane.title(),
            if done { "Done" } else { "Open" }
        ))
        .on_press(move |s: &mut State| s.select_kanban_card(i, j))
        .on_secondary_press(move |s: &mut State, _, _| s.open_card_menu(i, j))
        .child(card_checkbox(k, i, j, done))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(
                    Markdown::new(source)
                        .style(MarkdownStyle::from_kit(k).link(k.c.text_accent))
                        .link_style(crate::view::missing_link_style(k))
                        .on_link_tx(|s: &mut State, url: &str, tx: &mut Tx| {
                            s.open_card_link(url, tx)
                        })
                        .tasks_read_only()
                        .build(k),
                )
                .child(crate::kanban_dates::card_when(
                    k, &ctx.dates, &found, ctx.today, i, j,
                ))
                .child(crate::kanban_tags::card_tags(k, &ctx.tags, &found, i, j))
                .child(crate::kanban_metadata::card_metadata_rows(
                    s, k, rows, value_note, i, j,
                ))
                .child(checklist_section(k, i, j, &checklist)),
        )
        .child(
            menu_button(k, format!("kanban-card-{i}-{j}-menu"), "Card actions")
                .on_press(move |s: &mut State| s.open_card_menu(i, j)),
        )
}

/// The board as a table of cards, grouped by lane in file order.
pub(crate) fn table(s: &State, k: &Kit, rel: &str, board: &Kanban) -> El {
    let ctx = Context::new(s, rel);
    let mut rows = vec![table_head(k)];
    for (i, lane) in board.lanes().iter().enumerate() {
        table_lane_rows(s, k, &ctx, i, lane, &mut rows);
    }
    let content = if board.lanes().is_empty() {
        empty_state(k, "kanban-table-empty-board", "No lanes on this board")
    } else {
        col().key("kanban-table").w(TABLE_W).gap(0.0).children(rows)
    };
    col()
        .key("kanban-table-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll_xy()
        .child(content)
}

/// One lane's table rows: a collapsed or empty note, or its cards.
fn table_lane_rows(s: &State, k: &Kit, ctx: &Context, i: usize, lane: &Lane, rows: &mut Vec<El>) {
    let filtering = s.kanban_filter_active();
    let cards: Vec<(usize, &Card)> = lane
        .cards()
        .iter()
        .enumerate()
        .filter(|(_, card)| s.kanban_filter_matches_card(card, &ctx.dates, ctx.today))
        .collect();
    if lane.collapsed() {
        rows.push(table_lane_note(
            k,
            i,
            lane,
            format!("Collapsed · {} cards", lane.cards().len()),
            None,
        ));
        return;
    }
    if cards.is_empty() {
        let no_match = filtering && !lane.cards().is_empty();
        rows.push(table_lane_note(
            k,
            i,
            lane,
            if no_match {
                "No matching cards".to_owned()
            } else {
                "No cards".to_owned()
            },
            Some(if no_match {
                "kanban-table-empty"
            } else {
                "kanban-table-empty-lane"
            }),
        ));
        return;
    }
    for &(j, card) in &cards {
        rows.push(table_row(s, k, ctx, i, j, lane, card));
        if is_menu(s, &crate::MenuTarget::Card(i, j))
            || is_menu(s, &crate::MenuTarget::MoveCard(i, j))
        {
            rows.push(crate::view::context_menu(s, k));
        }
    }
}

/// The table's header: one label per column.
fn table_head(k: &Kit) -> El {
    row()
        .key("kanban-table-head")
        .w(TABLE_W)
        .h(32.0)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .fill(k.c.layer_01)
        .role("row")
        .label("Lane, Card, Date, Tags, Progress")
        .child(cell(k, "kanban-table-head-lane", "Lane", LANE_COL))
        .child(
            text("Card")
                .key("kanban-table-head-card")
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .grow(1.0)
                .min_w(0.0)
                .selectable(true),
        )
        .child(cell(k, "kanban-table-head-date", "Date", DATE_COL))
        .child(cell(k, "kanban-table-head-tags", "Tags", TAGS_COL))
        .child(cell(
            k,
            "kanban-table-head-progress",
            "Progress",
            PROGRESS_COL,
        ))
}

/// A fixed-width header label.
fn cell(k: &Kit, key: &'static str, label: &'static str, width: f32) -> El {
    text(label)
        .key(key)
        .font(k.ty.caption())
        .color(k.c.text_muted)
        .w(width)
        .truncate()
        .selectable(true)
}

/// One table row for a card.
fn table_row(
    s: &State,
    k: &Kit,
    ctx: &Context,
    i: usize,
    j: usize,
    lane: &Lane,
    card: &Card,
) -> El {
    let done = card.is_done();
    let found = scan_card_text(card.text(), &ctx.dates);
    let title_text = found.without(
        card.text(),
        ctx.dates.hide_date_in_title,
        ctx.tags.hide_tags_in_title,
    );
    let checklist = card_checklist(card.text());
    let display_text = without_checklist_lines(&title_text, &checklist);
    let flat = flatten_card_text(&display_text);
    let tags = crate::kanban_tags::unique_tags(&found)
        .into_iter()
        .map(|tag| format!("#{tag}"))
        .collect::<Vec<_>>()
        .join(" ");
    let date = table_date(&ctx.dates, &found);
    let progress = table_progress(card.text());
    let selected = s.kanban_selected == Some((i, j));
    let first = flat.lines().next().unwrap_or_default().to_owned();
    let mut card_cell = row()
        .key(format!("kanban-table-{i}-{j}-card"))
        .gap(6.0)
        .center_y()
        .grow(1.0)
        .min_w(0.0)
        .child(card_checkbox(k, i, j, done))
        .child(
            text(flat)
                .key(format!("kanban-table-{i}-{j}-text"))
                .font(k.ty.ui_compact)
                .color(if done { k.c.text_faint } else { k.c.text_base })
                .grow(1.0)
                .min_w(0.0)
                .truncate()
                // Selectable text bubbles a press to the row; plain text would
                // take the press itself and the row would never hear it.
                .selectable(true),
        );
    if let Some(link) = parse_links(card.text())
        .into_iter()
        .find(|l| !l.target.is_empty())
    {
        let target = link.target.clone();
        let heading = link.heading.clone();
        card_cell = card_cell.child(
            crate::kanban_view::icon_button(
                k,
                format!("kanban-table-{i}-{j}-open"),
                &ic::LINK,
                12.0,
                "Open link",
            )
            .on_press_tx(move |s: &mut State, tx: &mut Tx| {
                // Resolve at press time: the note may have appeared or been
                // renamed since this row was built.
                let path = match &heading {
                    Some(heading) => format!("{target}#{heading}"),
                    None => target.clone(),
                };
                let url = if s.vault.resolve(&target).is_some() {
                    format!("note:{path}")
                } else {
                    format!("missing:{target}")
                };
                s.open_card_link(&url, tx);
            }),
        );
    }
    row()
        .key(format!("kanban-table-{i}-{j}"))
        .w(TABLE_W)
        .min_h(38.0)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .fill(if j.is_multiple_of(2) {
            k.c.layer_01
        } else {
            k.c.layer_02
        })
        .border(
            1.0,
            if selected {
                k.c.text_accent
            } else {
                k.c.border_muted
            },
        )
        .role("row")
        .label(format!(
            "{} · {}: {first}",
            lane.title(),
            if done { "Done" } else { "Open" }
        ))
        .on_press(move |s: &mut State| s.select_kanban_card(i, j))
        .on_secondary_press(move |s: &mut State, _, _| s.open_card_menu(i, j))
        .child(
            text(lane.title().to_owned())
                .key(format!("kanban-table-{i}-{j}-lane"))
                .font(k.ty.ui_compact)
                .color(k.c.text_muted)
                .w(LANE_COL)
                .truncate()
                .selectable(true),
        )
        .child(card_cell)
        .child(
            text(date)
                .key(format!("kanban-table-{i}-{j}-date"))
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .w(DATE_COL)
                .truncate()
                .selectable(true),
        )
        .child(
            text(tags)
                .key(format!("kanban-table-{i}-{j}-tags"))
                .font(k.ty.caption())
                .color(k.c.text_accent)
                .w(TAGS_COL)
                .truncate()
                .selectable(true),
        )
        .child(
            text(progress)
                .key(format!("kanban-table-{i}-{j}-progress"))
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .w(PROGRESS_COL)
                .truncate()
                .selectable(true),
        )
}

/// A lane with no rows: its name and a reason, in the table's columns.
fn table_lane_note(k: &Kit, i: usize, lane: &Lane, words: String, key: Option<&'static str>) -> El {
    row()
        .key(format!("kanban-table-lane-{i}"))
        .w(TABLE_W)
        .min_h(32.0)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .fill(k.c.layer_01)
        .child(
            text(lane.title().to_owned())
                .key(format!("kanban-table-lane-{i}-title"))
                .font(k.ty.ui_compact)
                .color(k.c.text_muted)
                .w(LANE_COL)
                .truncate()
                .selectable(true),
        )
        .child(
            text(words)
                .key(key.unwrap_or("kanban-table-lane-note"))
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .grow(1.0)
                .truncate()
                .selectable(true),
        )
}

/// A lane's cards are empty (or the filter hides them all).
fn empty_lane(k: &Kit, i: usize, filtered: bool) -> El {
    text(if filtered {
        "No matching cards"
    } else {
        "No cards"
    })
    .key(format!("kanban-list-lane-{i}-empty"))
    .font(k.ty.caption())
    .color(k.c.text_faint)
}

/// An empty board's one line.
fn empty_state(k: &Kit, key: &'static str, words: &'static str) -> El {
    let text_key = if key == "kanban-list-empty" {
        "kanban-list-empty-text"
    } else {
        "kanban-table-empty-text"
    };
    col().key(key).w_full().padding(24.0).items_center().child(
        text(words)
            .key(text_key)
            .font(k.ty.ui_compact)
            .color(k.c.text_faint),
    )
}

/// `text` flattened to one safe line. `split('\n')` and `trim` work on char
/// boundaries, so multibyte text is never cut mid-character.
#[must_use]
pub(crate) fn flatten_card_text(text: &str) -> String {
    let mut flat = String::with_capacity(text.len());
    for line in text.split('\n') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !flat.is_empty() {
            flat.push_str(" · ");
        }
        flat.push_str(line);
    }
    flat
}

/// The first date and first time, in the board's display formats.
fn table_date(settings: &DateSettings, found: &CardText) -> String {
    let mut parts = Vec::new();
    if let Some(token) = &found.date {
        let raw = crate::kanban_dates::link_target(&token.value);
        parts.push(settings.parse_date(raw).map_or_else(
            || raw.to_owned(),
            |date| format_date(date, &settings.date_display_format),
        ));
    }
    if let Some(token) = &found.time {
        parts.push(settings.parse_time(&token.value).map_or_else(
            || token.value.clone(),
            |time| format_time(time, &settings.time_format),
        ));
    }
    parts.join(" ")
}

/// The card's checklist `done/total`, or nothing for a card without one.
fn table_progress(text: &str) -> String {
    let items = card_checklist(text);
    if items.is_empty() {
        return String::new();
    }
    let done = items.iter().filter(|item| item.is_done()).count();
    format!("{done}/{}", items.len())
}

#[cfg(test)]
mod tests {
    use super::flatten_card_text;

    #[test]
    fn multi_line_and_multibyte_text_flattens_to_one_line() {
        assert_eq!(flatten_card_text("one\ntwo"), "one · two");
        assert_eq!(
            flatten_card_text("one\r\n  two  \n\nthree"),
            "one · two · three"
        );
        assert_eq!(flatten_card_text("漢字 é 😀\n次の行"), "漢字 é 😀 · 次の行");
        assert_eq!(flatten_card_text(""), "");
        assert_eq!(flatten_card_text("   \n\t\n"), "");
        // A boundary is never cut: every byte stays valid UTF-8.
        let text = "é".repeat(200);
        assert_eq!(flatten_card_text(&text), text);
    }
}

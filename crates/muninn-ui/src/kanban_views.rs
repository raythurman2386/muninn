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
//! and editing stays on the board view. A card or lane menu item that opens
//! one of the board's in-place fields (Edit, Insert card before / after, Add a
//! date or time, Add card, Insert/Rename lane, Card limit) switches to the
//! Board first (`State::pick_menu`), where the field is drawn; the list and
//! table do not draw those fields. Finding marks every matching card with the
//! board's outline precedence, and the table's Tags column draws the same chip
//! rows as the card's foot. Switching views never changes the frontmatter or
//! any other byte.
//!
//! A lane's section or row group is built again only when what it draws
//! changes: its cards and header (the board's [`lane_version`]), the selected
//! card, the date, tag and metadata settings and the vault revision, the
//! filter and Board Find. A lane with its menu open is built every frame, as
//! on the board. [`lane_version`]: crate::kanban_view::lane_version

use muninn_core::{
    Card, CardText, CivilDate, DateSettings, Kanban, Lane, MetadataCache, MetadataSettings,
    TagSettings, card_checklist, format_date, format_time, parse_links, scan_card_text,
};
use rune::{El, Key, KeyEvent, ScrollSpec, Tx, col, icon, row, text};
use rune_kit::{Kit, Markdown, MarkdownStyle};
use rune_theme_opencode::icons as ic;

use crate::State;
use crate::kanban_view::{
    card_checkbox, card_markdown, checklist_section, lane_load, menu_button,
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

/// How a card row is marked: Find's current or other match, the selection, or
/// nothing. The board's precedence: the current match, then any match, then
/// the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardMark {
    Current,
    Matching,
    Selected,
    Plain,
}

/// The mark for a card, from Find's two flags and the selection. The list and
/// the table draw with the board's precedence.
fn card_mark(current: bool, matching: bool, selected: bool) -> CardMark {
    if current {
        CardMark::Current
    } else if matching {
        CardMark::Matching
    } else if selected {
        CardMark::Selected
    } else {
        CardMark::Plain
    }
}

/// A card row's border, from its mark: width and colour.
fn mark_border(k: &Kit, mark: CardMark) -> (f32, rune::Color) {
    match mark {
        CardMark::Current => (2.0, k.c.text_accent),
        CardMark::Matching => (1.0, k.c.warning.fg),
        CardMark::Selected => (1.0, k.c.text_accent),
        CardMark::Plain => (1.0, k.c.border_muted),
    }
}

/// The settings and the day one frame draws with, resolved once.
#[derive(Clone)]
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
    let layout = s.kanban_board_settings();
    let card_key =
        crate::kanban_view::card_settings_version(s, &ctx.dates, ctx.today, &ctx.metadata, &layout);
    let filter_hash = s.kanban_filter_hash();
    let find_hash = s.kanban_find_hash();
    let kit = crate::omarchy::kit_id(s, k.dark);
    let mut sections = Vec::new();
    for (i, lane) in board.lanes().iter().enumerate() {
        // A lane with its menu open is built every frame: the menu is part of
        // what it draws and names a card by place.
        if s.menu.as_ref().and_then(|menu| menu.target.lane()) == Some(i) {
            sections.push(list_lane(s, k, &ctx, i, lane));
            continue;
        }
        let selected = s.kanban_selected.filter(|(l, _)| *l == i).map(|(_, j)| j);
        let lane_ctx = ctx.clone();
        sections.push(rune::memo(
            format!("kanban-list-lane-{i}"),
            (
                rel,
                kit,
                crate::kanban_view::lane_version(lane),
                selected,
                card_key,
                filter_hash,
                find_hash,
            ),
            move || list_lane(s, k, &lane_ctx, i, lane),
        ));
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
    let section = col()
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
        // Slice 16: the lane is a region; its cards are the list.
        .role("region")
        .label(format!(
            "{} lane, {count} cards{}{}",
            lane.title(),
            if over { ", over limit" } else { "" },
            if lane.done_word().is_some() {
                ", done lane"
            } else {
                ""
            }
        ))
        .child(list_head(k, i, lane, &count, over));
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
    let mut list = col()
        .key(format!("kanban-list-lane-{i}-cards"))
        .w_full()
        .gap(6.0)
        .role("list")
        .label(format!("{} cards", lane.title()))
        .focus_group()
        .focus_follows();
    for &(j, card) in &cards {
        let (rows, value_note) = metadata.rows(&s.vault, ctx.note, card.text());
        list = list.child(list_card(s, k, ctx, i, j, lane, card, &rows, value_note));
    }
    section.child(list)
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
    head.child(menu_button(
        k,
        format!("kanban-list-lane-{i}-menu"),
        format!("Lane actions: {}", lane.title()),
        move |s: &mut State, _: &mut Tx| s.open_lane_menu(i),
    ))
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
    let first = display_text.lines().next().unwrap_or_default().to_owned();
    let selected = s.kanban_selected == Some((i, j));
    let (matching, current) = s.kanban_find_mark(i, j);
    let (width, border) = mark_border(k, card_mark(current, matching, selected));
    let key = format!("kanban-card-{i}-{j}");
    let focused_key = key.clone();
    let state_word = if done { "Done" } else { "Open" };
    let menu_label = if first.trim().is_empty() {
        format!("Card actions: empty card in {}", lane.title())
    } else {
        format!("Card actions: {first}")
    };
    row()
        .key(key)
        .w_full()
        .padding(10.0)
        .gap(8.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(width, border)
        .role("listitem")
        .label(format!("{} · {state_word}: {first}", lane.title()))
        .focusable(true)
        // Slice 16 fix: a nested group puts the row's controls within Tab and
        // arrow reach (checkbox first, then the menu and checklist rows).
        .focus_group()
        .focus_selected(selected)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        // Slice 16: Space and Enter flip the focused card's done mark.
        .on_key(move |s: &mut State, e: &KeyEvent, tx: &mut Tx| {
            if tx.focused() != Some(focused_key.as_str()) {
                return false;
            }
            if crate::kanban_view::card_row_arrow(s, e, tx) {
                return true;
            }
            if e.mods != rune::Mods::default() || s.topmost_layer().is_some() {
                return false;
            }
            match &e.key {
                Key::Enter => {
                    s.toggle_card_done(i, j);
                    true
                }
                Key::Text(typed) if typed == " " => {
                    s.toggle_card_done(i, j);
                    true
                }
                _ => false,
            }
        })
        .on_press(move |s: &mut State| s.select_kanban_card(i, j))
        .on_secondary_press(move |s: &mut State, _, _| s.open_card_menu(i, j))
        .child(card_checkbox(k, i, j, done, &first))
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
        .child(menu_button(
            k,
            format!("kanban-card-{i}-{j}-menu"),
            menu_label,
            move |s: &mut State, _: &mut Tx| s.open_card_menu(i, j),
        ))
}

/// The board as a table of cards, grouped by lane in file order.
pub(crate) fn table(s: &State, k: &Kit, rel: &str, board: &Kanban) -> El {
    let ctx = Context::new(s, rel);
    let layout = s.kanban_board_settings();
    let card_key =
        crate::kanban_view::card_settings_version(s, &ctx.dates, ctx.today, &ctx.metadata, &layout);
    let filter_hash = s.kanban_filter_hash();
    let find_hash = s.kanban_find_hash();
    let kit = crate::omarchy::kit_id(s, k.dark);
    let mut rows = vec![table_head(k)];
    for (i, lane) in board.lanes().iter().enumerate() {
        // A lane with its card menu open is built every frame, as on the board.
        if s.menu.as_ref().and_then(|menu| menu.target.lane()) == Some(i) {
            rows.push(table_lane_group(s, k, &ctx, i, lane));
            continue;
        }
        let selected = s.kanban_selected.filter(|(l, _)| *l == i).map(|(_, j)| j);
        let lane_ctx = ctx.clone();
        rows.push(rune::memo(
            format!("kanban-table-group-{i}"),
            (
                rel,
                kit,
                crate::kanban_view::lane_version(lane),
                selected,
                card_key,
                filter_hash,
                find_hash,
            ),
            move || table_lane_group(s, k, &lane_ctx, i, lane),
        ));
    }
    let content = if board.lanes().is_empty() {
        empty_state(k, "kanban-table-empty-board", "No lanes on this board")
    } else {
        col()
            .key("kanban-table")
            .w(TABLE_W)
            .gap(0.0)
            .role("table")
            .label("Cards by lane")
            // Slice 16 fix: the rows are one group, so Tab reaches the table
            // and the arrows rove its rows; the selection follows focus.
            .focus_group()
            .focus_follows()
            .children(rows)
    };
    col()
        .key("kanban-table-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll_xy()
        .child(content)
}

/// One lane's rows in the table, grouped so the lane is built again only when
/// what it draws changes (its cards, the selected card, the settings, the
/// filter and Find).
fn table_lane_group(s: &State, k: &Kit, ctx: &Context, i: usize, lane: &Lane) -> El {
    let mut rows = Vec::new();
    table_lane_rows(s, k, ctx, i, lane, &mut rows);
    col()
        .key(format!("kanban-table-group-{i}"))
        .w(TABLE_W)
        .gap(0.0)
        .children(rows)
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
            Some("empty"),
        ));
        return;
    }
    for &(j, card) in &cards {
        rows.push(table_row(s, k, ctx, i, j, lane, card));
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
                .role("columnheader")
                .label("Card")
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
        .role("columnheader")
        .label(label)
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
    let date = table_date(&ctx.dates, &found);
    let progress = table_progress(card.text());
    let selected = s.kanban_selected == Some((i, j));
    let (matching, current) = s.kanban_find_mark(i, j);
    let (width, border) = mark_border(k, card_mark(current, matching, selected));
    let first = flat.lines().next().unwrap_or_default().to_owned();
    let focused_key = format!("kanban-table-{i}-{j}");
    let mut card_cell = row()
        .key(format!("kanban-table-{i}-{j}-card"))
        .gap(6.0)
        .center_y()
        .grow(1.0)
        .min_w(0.0)
        .child(card_checkbox(k, i, j, done, &first))
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
        card_cell = card_cell.child(crate::kanban_view::icon_button(
            k,
            format!("kanban-table-{i}-{j}-open"),
            &ic::LINK,
            12.0,
            "Open link",
            move |s: &mut State, tx: &mut Tx| {
                // Resolve at press time: the note may have appeared or
                // been renamed since this row was built.
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
            },
        ));
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
        .border(width, border)
        .role("row")
        .label(format!(
            "{} · {}: {first}",
            lane.title(),
            if done { "Done" } else { "Open" }
        ))
        .focusable(true)
        // Slice 16 fix: a nested group puts the row's controls (checkbox and
        // Open link) within Tab and arrow reach.
        .focus_group()
        .focus_selected(selected)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .on_key(move |s: &mut State, e: &KeyEvent, tx: &mut Tx| {
            if tx.focused() != Some(focused_key.as_str()) {
                return false;
            }
            if crate::kanban_view::card_row_arrow(s, e, tx) {
                return true;
            }
            if e.mods != rune::Mods::default() || s.topmost_layer().is_some() {
                return false;
            }
            match &e.key {
                Key::Enter => {
                    s.toggle_card_done(i, j);
                    true
                }
                Key::Text(typed) if typed == " " => {
                    s.toggle_card_done(i, j);
                    true
                }
                _ => false,
            }
        })
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
        .child(crate::kanban_tags::tag_chips(
            k,
            &found,
            i,
            j,
            "kanban-table",
            Some(TAGS_COL),
        ))
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
fn table_lane_note(
    k: &Kit,
    i: usize,
    lane: &Lane,
    words: String,
    suffix: Option<&'static str>,
) -> El {
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
                // A lane's own key: two empty lanes are two rows.
                .key(format!(
                    "kanban-table-lane-{i}-{}",
                    suffix.unwrap_or("note")
                ))
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
    use super::{CardMark, card_mark, flatten_card_text};

    #[test]
    fn find_marks_take_precedence_over_the_selection() {
        assert_eq!(card_mark(true, true, true), CardMark::Current);
        assert_eq!(card_mark(false, true, true), CardMark::Matching);
        assert_eq!(card_mark(false, false, true), CardMark::Selected);
        assert_eq!(card_mark(false, false, false), CardMark::Plain);
    }

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

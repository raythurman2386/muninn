//! The Kanban page (`Mode::Kanban`): one kanban note shown as a board.
//!
//! The note is read from disk and parsed with [`Kanban`] when Kanban mode is
//! entered, and again when the selection or the vault changes while the page
//! is up ([`State::refresh_kanban`]), unless it holds unsaved edits. Frames
//! draw that parse. Editing, undo and saving are `kanban_edit.rs` (slice 4).
//! The page shows:
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
//! so instead of drawing lanes. Slice 4 adds a checkbox that toggles, a menu
//! on each card and lane, the card editor and **+ Add card** at each lane's
//! foot, and the header's save status. Slice 5 (`kanban_lanes.rs`) adds the
//! lane field, **+ Add lane**, the collapse control and collapsed strips, the
//! selected card's outline, and the card and lane menus' new actions. Slice 6
//! (`kanban_drag.rs`) makes cards and lane headers draggable and draws where
//! a drag would drop. Slice 8 (`kanban_links.rs`) follows a card's wikilink,
//! completes `[[` in the card editor, copies a stable block id, makes a note
//! from a card, and keeps a dirty kanban's bytes when a linked note is renamed.
//! Slice 9 (`kanban_dates.rs`) reads card dates and times, hides them from the
//! title when the board asks, shows them at the card's foot, and opens the
//! date/time picker from a card menu or a typed trigger.
//!
//! Slice 3 adds the ways in: the sidebar's Kanbans section and the palette's
//! kanban entries ([`State::open_kanban`]), New Kanban
//! ([`State::new_kanban`]) and Convert empty note to Kanban
//! ([`State::convert_to_kanban`]). Picking a kanban note in the notes list
//! while the page is up keeps the page (`State::open_from_list`), and one
//! picked from the sidebar search (⌘K) opens here from any mode
//! ([`State::open_from_search`]).

use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

use muninn_core::{Card, Kanban, Lane, is_kanban, parse_links, scan_card_text};
use rune::{Drag, El, Key, KeyEvent, Tx, col, el, icon, row, text};
use rune_kit::{Kit, Markdown, MarkdownStyle, TextArea};
use rune_theme_opencode::icons as ic;

use crate::board_view::{BoardStatus, UNSAVED, placeholder};
use crate::doc_sync::{DocConflict, conflict_bar, disk_conflict, file_state};
use crate::kanban_drag::Dragged;
use crate::kanban_edit::{CardCommit, CardSlot, KanbanHistory};
use crate::kanban_lanes::{LaneField, lane_field};
use crate::{DiskState, MenuTarget, Mode, State};

/// Width of a lane column, in points: the notes sidebar's width, so a lane
/// reads like the note list beside it.
const LANE_W: f32 = crate::view::SIDEBAR;
/// Width of a collapsed lane's strip.
const STRIP_W: f32 = 52.0;
/// Opacity of a card or lane while it is dragged (slice 6).
const DRAGGED: f32 = 0.45;

/// The kanban note on the Kanban page, as read from disk.
#[derive(Clone, Debug)]
pub struct ShownKanban {
    /// Vault-relative path with `/` separators.
    pub rel: String,
    /// The parsed board, with any unsaved edits. `None` when the note is a
    /// kanban note whose file can't be read as a board (the parse was
    /// `InvalidData`); then it is never written.
    pub board: Option<Kanban>,
    /// The file's text as last read or written.
    pub saved: String,
    /// The file's identity as last read or written: a save is refused when
    /// the file no longer matches it.
    pub disk: Option<DiskState>,
    pub status: BoardStatus,
    pub history: KanbanHistory,
    /// Why the last card action was refused, until the next one works.
    pub error: Option<String>,
    /// Slice 7: set while the kanban has unsaved edits and its file changed
    /// or vanished; the conflict bar decides.
    pub conflict: Option<DocConflict>,
    /// Bumped on every edit, undo and redo, so autosave and the recovery
    /// copy wait for the edits to rest.
    pub(crate) generation: u64,
    /// What the recovery copy holds, when one was written: the change count
    /// and the open field's text at the time.
    pub(crate) recovered: Option<(u64, Option<String>)>,
}

impl ShownKanban {
    /// Edits that are not on disk: the board's text differs from the file's.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.board
            .as_ref()
            .is_some_and(|board| board.source() != self.saved)
    }

    /// After an edit, undo or redo.
    pub(crate) fn mark_changed(&mut self) {
        self.generation += 1;
        self.status = if self.dirty() {
            BoardStatus::Unsaved
        } else {
            BoardStatus::Clean
        };
    }

    /// Slice 7: compare the file at `path` with the text last read or
    /// written, and flag the unsaved edits when it changed or vanished
    /// (cleared when it holds that text again). With no known file state (a
    /// kanban restored from its recovery copy over a file that holds
    /// something else) it stays flagged, so only Keep mine writes over it.
    pub(crate) fn check_disk(&mut self, path: &Path) {
        let (known, saved) = (self.disk.is_some(), &self.saved);
        self.conflict = disk_conflict(path, |text| known && text == saved, self.conflict);
        if self.conflict.is_none() {
            // Their write put back the same bytes, or only touched the file.
            self.disk = file_state(path);
        }
    }
}

impl State {
    /// Pick and parse the kanban the page shows (see the module docs). The
    /// saved kanban follows a selected kanban note; a selected note that is
    /// not one leaves it as it is.
    ///
    /// Slice 4, as Board: unsaved edits (or an open card editor) are never
    /// read over. The page keeps them unless another kanban note was
    /// selected; that saves them first and, when the save fails, keeps the
    /// page on them.
    pub fn refresh_kanban(&mut self) {
        let selected = self.selected_rel().map(str::to_owned);
        // Slice 7's shared disk rule says `Later` while a press is held on a
        // card or lane: a reload now would replace the board under the drag
        // and end it. The release ends the press; the next refresh applies
        // what was deferred. Picking another kanban is not a disk refresh and
        // still switches the page (the replacement ends the drag).
        if self.kanban_drag.is_some() {
            let shown = self.kanban.as_ref().map(|shown| shown.rel.as_str());
            let other = selected.as_deref().is_some_and(|rel| {
                Some(rel) != shown
                    && self
                        .note_id(rel)
                        .is_some_and(|id| self.vault.notes[id].is_kanban())
            });
            if !other {
                return;
            }
        }
        if self.kanban_dirty() {
            let shown = self.kanban.as_ref().map(|shown| shown.rel.as_str());
            let other = selected.as_deref().is_some_and(|rel| {
                Some(rel) != shown
                    && self
                        .note_id(rel)
                        .is_some_and(|id| self.vault.notes[id].is_kanban())
            });
            if !other {
                // Slice 7: the same rule as the kanban watcher's.
                self.kanban_follow_disk();
                return;
            }
            if !self.save_kanban() {
                return;
            }
        }
        // Slice 7: a kanban renamed outside Muninn is followed to its new name.
        if let Some(rel) = self.kanban_renamed() {
            self.kanban_rel = Some(rel);
        }
        let shown = selected
            .and_then(|rel| self.read_kanban(rel))
            .or_else(|| self.read_kanban(self.kanban_rel.clone()?));
        if let Some(shown) = &shown {
            self.kanban_rel = Some(shown.rel.clone());
        }
        // Slice 7: the same kanban with the same text keeps the page as it
        // is (its undo history and save state); only the file's identity is
        // taken.
        if let (Some(old), Some(new)) = (&mut self.kanban, &shown)
            && old.rel == new.rel
            && old.saved == new.saved
            && !new.dirty()
        {
            old.disk = new.disk;
            old.conflict = None;
            return;
        }
        let same_rel = self.kanban.as_ref().map(|k| &k.rel) == shown.as_ref().map(|k| &k.rel);
        // A re-read can move the selected card down its lane; another
        // kanban's selection goes with that kanban.
        let selected_card = if same_rel { self.selected_card() } else { None };
        let reloaded = same_rel && shown.is_some();
        self.kanban = shown;
        // A menu names a place on the board that a re-read can move; a press
        // held on it ends with it. The selected card is found again by its
        // content, or kept in place when its lane still holds the same cards.
        self.kanban_board_installed(selected_card);
        // Slice 9: a picker names a card by place, and the board just changed.
        self.close_picker();
        if reloaded {
            self.log_event("watch", "reloaded the shown kanban");
        }
    }

    /// `rel` read from disk, when it is a note in this vault and a kanban.
    ///
    /// Slice 7, the Write hand-off: when `rel` is the note open in Write and
    /// its text there could not be saved, the kanban takes that text as
    /// unsaved edits (the editor gives it up), so nothing typed is dropped.
    pub(crate) fn read_kanban(&mut self, rel: String) -> Option<ShownKanban> {
        let note = self.vault.notes.iter().find(|note| note.rel == rel)?;
        let id = note.id;
        let source = fs::read_to_string(&note.path).ok()?;
        let disk = self.vault.disk_state(id).ok();
        let typed = (self.selected == Some(id) && self.editor.text != self.saved)
            .then(|| Kanban::parse(&self.editor.text).ok())
            .flatten()
            .filter(|board| is_kanban(board.source()));
        if !is_kanban(&source) && typed.is_none() {
            return None;
        }
        let mut shown = ShownKanban {
            board: Kanban::parse(&source).ok(),
            rel,
            saved: source,
            disk,
            status: BoardStatus::Clean,
            history: KanbanHistory::default(),
            error: None,
            conflict: None,
            generation: 0,
            recovered: None,
        };
        if let Some(board) = typed {
            shown.board = Some(board);
            shown.saved = self.saved.clone();
            shown.disk = self.expected_disk;
            shown.mark_changed();
            self.give_editor_text_to_kanban();
            let detail = format!("took the text typed in Write for {}", shown.rel);
            self.log_event("kanban", &detail);
        }
        Some(shown)
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

    /// A kanban belongs to its vault: forget it on a vault switch (which
    /// saves it first).
    pub(crate) fn clear_kanban(&mut self) {
        self.kanban_rel = None;
        self.kanban = None;
        self.cancel_card_edit();
        self.cancel_lane_edit();
        self.close_picker();
        self.kanban_selected = None;
        self.kanban_drag = None;
        self.kanban_autosave_wait = None;
        self.kanban_recovery_wait = None;
    }

    /// A card's menu (its `⋯`, or a right-click on it), drawn under the card.
    pub fn open_card_menu(&mut self, lane: usize, index: usize) {
        if let Some((lane, index)) = self.settle_card_edit(lane, index) {
            self.close_picker();
            self.open_kanban_menu(MenuTarget::Card(lane, index));
        }
    }

    /// A lane's menu (the `⋯` in its header), drawn under the header.
    pub fn open_lane_menu(&mut self, lane: usize) {
        if let Some(lane) = self.settle_lane_edit(lane) {
            self.close_picker();
            self.open_kanban_menu(MenuTarget::Lane(lane));
        }
    }

    fn open_kanban_menu(&mut self, target: MenuTarget) {
        // Drawn in place, like the notes list's menus: no anchor point.
        self.menu = Some(crate::ContextMenu {
            target,
            x: 0.0,
            y: 0.0,
        });
    }

    /// Open as markdown: the shown note in Write. Slice 7: unsaved kanban
    /// edits are saved first, so Write shows them and typing there can't
    /// make a later kanban save refuse; when that save is refused the page
    /// stays, with the reason and the conflict bar.
    pub fn open_kanban_as_markdown(&mut self) {
        let Some(id) = self
            .kanban
            .as_ref()
            .and_then(|shown| self.note_id(&shown.rel))
        else {
            return;
        };
        if !self.save_kanban() {
            return;
        }
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
            ..
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
        .child(message(s, k))
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

/// Why the last card action was refused, and why a quit or vault switch
/// was: the Kanban page has no status bar, so it says so under its header.
fn message(s: &State, k: &Kit) -> El {
    let card = s.kanban.as_ref().and_then(|shown| shown.error.as_deref());
    let notice = s.notice.as_deref();
    if card.is_none() && s.error.is_none() && notice.is_none() {
        return el();
    }
    let line = |key: &'static str, error: Option<&str>| match error {
        Some(error) => text(error.to_owned())
            .key(key)
            .role("alert")
            .font(k.ty.caption())
            .color(k.c.danger.fg)
            .wrap_anywhere()
            .w_full(),
        None => el(),
    };
    let notice = match notice {
        Some(notice) => text(notice.to_owned())
            .key("kanban-notice")
            .role("status")
            .font(k.ty.caption())
            .color(k.c.text_accent)
            .wrap_anywhere()
            .w_full(),
        None => el(),
    };
    col()
        .px(28.0)
        .pb(8.0)
        .gap(4.0)
        .w_full()
        .child(line("kanban-error", card))
        .child(line("kanban-app-error", s.error.as_deref()))
        .child(notice)
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
    // Board's words for the same states.
    let (status, status_color) = match &shown.status {
        BoardStatus::Clean => (String::new(), k.c.text_faint),
        BoardStatus::Unsaved => (UNSAVED.to_owned(), k.c.text_accent),
        BoardStatus::Saved => ("Saved".to_owned(), k.c.text_faint),
        BoardStatus::Failed(e) => (format!("Not saved: {e}"), k.c.text_accent),
    };
    head.child(
        text(title)
            .key("kanban-title")
            .font(k.ty.ui_medium)
            .color(k.c.text_base),
    )
    .child(
        text(shown.rel.clone())
            .key("kanban-detail")
            .font(k.ty.caption())
            .color(k.c.text_faint)
            .shrink()
            .truncate(),
    )
    .child(
        text(status)
            .key("kanban-status")
            .font(k.ty.caption())
            .color(status_color)
            .grow(1.0)
            .truncate(),
    )
    .child(match shown.conflict {
        Some(conflict) => conflict_bar(
            k,
            "kanban",
            conflict,
            [
                State::kanban_keep_mine,
                State::kanban_save_copy,
                State::kanban_discard,
            ],
        ),
        None => el(),
    })
    .child(header_button(
        k,
        "kanban-open-markdown",
        &ic::EDIT,
        "Open as markdown",
        State::open_kanban_as_markdown,
    ))
}

/// The lanes, side by side, scrolling both ways. Each lane is built again
/// only when what it draws changes (its cards, its selected card, the theme);
/// a lane with the card editor, the lane field, a menu or a drag in it is
/// built every frame, as those change.
fn lanes(s: &State, k: &Kit, rel: &str, board: &Kanban) -> El {
    let note = s.note_id(rel);
    let kit = crate::omarchy::kit_id(s, k.dark);
    let new_lane = s
        .kanban_lane_edit
        .filter(|slot| slot.field == LaneField::New)
        .map(|slot| slot.lane);
    let lane_drop = s.kanban_lane_drop();
    let mut columns = Vec::new();
    for (i, lane) in board.lanes().iter().enumerate() {
        if new_lane == Some(i) {
            columns.push(new_lane_column(s, k));
        }
        if lane_drop.is_some_and(|(_, slot)| slot == i) {
            columns.push(lane_drop_bar(s, k));
        }
        let live = s.kanban_card.is_some_and(|slot| slot.lane == i)
            || s.kanban_picker
                .as_ref()
                .is_some_and(|picker| picker.card.0 == i)
            || s.kanban_drag_in(i) != (None, None, false)
            || s.kanban_lane_edit.is_some_and(|slot| slot.lane == i)
            || s.menu.as_ref().is_some_and(|m| match m.target {
                MenuTarget::Lane(l)
                | MenuTarget::DeleteLane(l)
                | MenuTarget::Card(l, _)
                | MenuTarget::MoveCard(l, _) => l == i,
                _ => false,
            });
        columns.push(if live {
            lane_column(s, k, note, i, lane)
        } else {
            let selected = s.kanban_selected.filter(|(l, _)| *l == i).map(|(_, j)| j);
            rune::memo(
                format!("kanban-lane-{i}"),
                (rel, kit, lane_version(lane), selected, date_version(s)),
                move || lane_column(s, k, note, i, lane),
            )
        });
    }
    let count = board.lanes().len();
    if lane_drop.is_some_and(|(_, slot)| slot == count) {
        columns.push(lane_drop_bar(s, k));
    }
    columns.push(if new_lane == Some(count) {
        new_lane_column(s, k)
    } else {
        add_lane_button(k, count)
    });
    let body = row().gap(12.0).items_start().children(columns);
    col()
        .key("kanban-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll_xy()
        .child(body)
}

/// A hash of what a lane draws: its header and its cards' marks and text.
fn lane_version(lane: &Lane) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (
        lane.title(),
        lane.written_limit(),
        lane.done_word(),
        lane.collapsed(),
    )
        .hash(&mut h);
    for card in lane.cards() {
        (card.mark(), card.text()).hash(&mut h);
    }
    h.finish()
}

/// A hash of the date settings a card's text and foot depend on.
fn date_version(s: &State) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.kanban_date_settings().hash(&mut h);
    h.finish()
}

fn lane_column(s: &State, k: &Kit, note: Option<usize>, i: usize, lane: &Lane) -> El {
    let (drop, _, dragged) = s.kanban_drag_in(i);
    if lane.collapsed() {
        return collapsed_lane(k, i, lane, drop.is_some(), dragged);
    }
    let (count, over) = lane_load(lane);
    let count_color = if over { k.c.warning.fg } else { k.c.text_faint };
    let mut head = row()
        .key(format!("kanban-lane-{i}-head"))
        .w_full()
        .gap(8.0)
        .center_y()
        .cursor(rune::Cursor::Grab)
        .on_drag(move |s: &mut State, d: &mut Drag, tx: &mut Tx| {
            s.drag_kanban(Dragged::Lane(i), d, tx);
        })
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
    head = head
        .child(
            icon_button(
                k,
                format!("kanban-lane-{i}-collapse"),
                &ic::CHEVRON_LEFT,
                12.0,
                "Collapse lane",
            )
            .on_press(move |s: &mut State| s.toggle_lane_collapsed(i)),
        )
        .child(
            menu_button(k, format!("kanban-lane-{i}-menu"), "Lane actions")
                .on_press(move |s: &mut State| s.open_lane_menu(i)),
        );
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
        .opacity(if dragged { DRAGGED } else { 1.0 })
        .on_layout_tx(move |s: &mut State, rect, tx: &mut Tx| {
            s.kanban_rects.set_lane(i, rect);
            tx.live_only();
        })
        .child(head);
    if s.kanban_lane_edit
        .is_some_and(|slot| slot.lane == i && slot.field != LaneField::New)
    {
        column = column.child(lane_field(s, k));
    }
    if is_menu(s, &MenuTarget::Lane(i)) || is_menu(s, &MenuTarget::DeleteLane(i)) {
        column = column.child(crate::view::context_menu(s, k));
    }
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
    let new_at = s
        .kanban_card
        .filter(|slot| slot.lane == i && slot.new)
        .map(|slot| slot.index);
    for (j, card) in lane.cards().iter().enumerate() {
        if new_at == Some(j) {
            column = column.child(card_editor(s, k));
        }
        if drop == Some(j) {
            column = column.child(card_drop_bar(k));
        }
        column = column.child(card_view(s, k, note, i, j, card));
        if s.kanban_picker
            .as_ref()
            .is_some_and(|picker| picker.card == (i, j))
        {
            column = column.child(crate::kanban_dates::picker(s, k));
        }
        if is_menu(s, &MenuTarget::Card(i, j)) || is_menu(s, &MenuTarget::MoveCard(i, j)) {
            column = column.child(crate::view::context_menu(s, k));
        }
    }
    if drop == Some(lane.cards().len()) {
        column = column.child(card_drop_bar(k));
    }
    if new_at == Some(lane.cards().len()) {
        column = column.child(card_editor(s, k));
    }
    column.child(
        row()
            .key(format!("kanban-lane-{i}-add"))
            .w_full()
            .h(30.0)
            .px(8.0)
            .gap(6.0)
            .radius(7.0)
            .center_y()
            .hover(k.c.hover)
            .press(k.c.pressed)
            .cursor(rune::Cursor::Pointer)
            .role("button")
            .label("Add card")
            .on_press(move |s: &mut State| s.begin_add_card(i))
            .child(icon(&ic::PLUS, 12.0).color(k.c.text_faint))
            .child(text("Add card").font(k.ty.ui_compact).color(k.c.text_faint)),
    )
}

/// A collapsed lane: a narrow strip with the expand control, the card count
/// and the title. Slice 6: it drags as its lane's header, and is outlined
/// while a card dragged over it would drop at its end.
fn collapsed_lane(k: &Kit, i: usize, lane: &Lane, drop: bool, dragged: bool) -> El {
    let (count, over) = lane_load(lane);
    col()
        .key(format!("kanban-lane-{i}"))
        .w(STRIP_W)
        .padding(8.0)
        .gap(8.0)
        .radius(10.0)
        .items_center()
        .fill(k.c.layer_01)
        .border(
            1.0,
            if drop {
                k.c.text_accent
            } else if over {
                k.c.warning.fg
            } else {
                k.c.border_muted
            },
        )
        .role("list")
        .label(format!("{}, {count} cards, collapsed", lane.title()))
        .opacity(if dragged { DRAGGED } else { 1.0 })
        .cursor(rune::Cursor::Grab)
        .on_drag(move |s: &mut State, d: &mut Drag, tx: &mut Tx| {
            s.drag_kanban(Dragged::Lane(i), d, tx);
        })
        .on_layout_tx(move |s: &mut State, rect, tx: &mut Tx| {
            s.kanban_rects.set_lane(i, rect);
            tx.live_only();
        })
        .child(
            icon_button(
                k,
                format!("kanban-lane-{i}-expand"),
                &ic::CHEVRON_RIGHT,
                12.0,
                "Expand lane",
            )
            .on_press(move |s: &mut State| s.toggle_lane_collapsed(i)),
        )
        .child(
            text(count)
                .key(format!("kanban-lane-{i}-count"))
                .font(k.ty.caption())
                .color(if over { k.c.warning.fg } else { k.c.text_faint }),
        )
        .child(
            text(lane.title().to_owned())
                .key(format!("kanban-lane-{i}-title"))
                .font(k.ty.ui_compact)
                .color(k.c.text_base)
                .w_full()
                .wrap_anywhere(),
        )
}

/// Where a dragged card would land in a lane.
fn card_drop_bar(k: &Kit) -> El {
    el().key("kanban-drop")
        .w_full()
        .h(3.0)
        .radius(2.0)
        .fill(k.c.text_accent)
}

/// Where a dragged lane would go, as tall as that lane.
fn lane_drop_bar(s: &State, k: &Kit) -> El {
    let h = s
        .kanban_lane_drop()
        .and_then(|(lane, _)| s.kanban_rects.lane_height(lane))
        .unwrap_or(LANE_W);
    el().key("kanban-lane-drop")
        .w(3.0)
        .h(h)
        .radius(2.0)
        .fill(k.c.text_accent)
}

/// A new lane's title field, where the lane will go.
fn new_lane_column(s: &State, k: &Kit) -> El {
    col()
        .key("kanban-new-lane")
        .w(LANE_W)
        .padding(10.0)
        .radius(10.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .child(lane_field(s, k))
}

/// **+ Add lane**, after the last lane.
fn add_lane_button(k: &Kit, count: usize) -> El {
    row()
        .key("kanban-add-lane")
        .h(34.0)
        .px(12.0)
        .gap(6.0)
        .radius(10.0)
        .center_y()
        .border(1.0, k.c.border_muted)
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label("Add lane")
        .on_press(move |s: &mut State| s.begin_add_lane(count))
        .child(icon(&ic::PLUS, 12.0).color(k.c.text_faint))
        .child(text("Add lane").font(k.ty.ui_compact).color(k.c.text_faint))
}

fn is_menu(s: &State, target: &MenuTarget) -> bool {
    s.menu.as_ref().is_some_and(|m| m.target == *target)
}

/// The `⋯` that opens a card's or a lane's menu.
fn menu_button(k: &Kit, key: String, label: &'static str) -> El {
    icon_button(k, key, &ic::OUTLINE_DOTS, 14.0, label)
}

/// A small icon button: a menu's `⋯`, a lane's collapse and expand.
fn icon_button(
    k: &Kit,
    key: String,
    glyph: &'static rune::Icon,
    size: f32,
    label: &'static str,
) -> El {
    el().key(key)
        .size(22.0, 22.0)
        .radius(6.0)
        .center()
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(label)
        .child(icon(glyph, size).color(k.c.icon_muted))
}

fn card_view(s: &State, k: &Kit, note: Option<usize>, i: usize, j: usize, card: &Card) -> El {
    let editing = s.kanban_card
        == Some(CardSlot {
            lane: i,
            index: j,
            new: false,
        });
    if editing {
        return card_editor(s, k);
    }
    let done = card.is_done();
    let settings = s.kanban_date_settings();
    let found = scan_card_text(card.text(), &settings);
    let title_text = if settings.hide_date_in_title {
        found.without_dates(card.text())
    } else {
        card.text().to_owned()
    };
    // Wikilinks open the note. A note embed stays a link: the spec allows
    // `![[Note]]` in a card and does not say to inline the note. An attachment
    // embed keeps the reader's filename placeholder.
    let source = card_markdown(&s.vault, note, &title_text);
    let first = title_text.lines().next().unwrap_or_default();
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
        .cursor(rune::Cursor::Pointer)
        .role("checkbox")
        .checked(done)
        .label(if done { "Done" } else { "Open" })
        .on_press(move |s: &mut State| s.toggle_card_done(i, j));
    let mark = if done {
        mark.child(icon(&ic::CHECK, 10.0).color(k.c.text_accent))
    } else {
        mark
    };
    let selected = s.kanban_selected == Some((i, j));
    let dragged = s.kanban_drag_in(i).1 == Some(j);
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
        .label(format!("{}: {first}", if done { "Done" } else { "Open" }))
        .on_press(move |s: &mut State| s.select_kanban_card(i, j))
        // As Board edits text: a double click opens the editor.
        .on_double_press(move |s: &mut State| s.begin_edit_card(i, j))
        .on_secondary_press(move |s: &mut State, _, _| s.open_card_menu(i, j))
        .opacity(if dragged { DRAGGED } else { 1.0 })
        .on_drag(move |s: &mut State, d: &mut Drag, tx: &mut Tx| {
            s.drag_kanban(Dragged::Card(i, j), d, tx);
        })
        .on_layout_tx(move |s: &mut State, rect, tx: &mut Tx| {
            s.kanban_rects.set_card(i, j, rect);
            tx.live_only();
        })
        .child(mark)
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
                .child(crate::kanban_dates::card_when(k, &settings, &found, i, j)),
        )
        .child(
            menu_button(k, format!("kanban-card-{i}-{j}-menu"), "Card actions")
                .on_press(move |s: &mut State| s.open_card_menu(i, j)),
        )
}

/// The card editor: Enter commits, Shift+Enter breaks the line, Esc cancels.
fn card_editor(s: &State, k: &Kit) -> El {
    let mut editor = col().key("kanban-card-editor").w_full().child(
        TextArea::new(&s.kanban_card_field)
            .label("Card text")
            .placeholder("Card text")
            .width(None)
            .auto_grow(2, 10)
            .wrap_anywhere()
            .on_edit(|s: &mut State| {
                s.detect_card_trigger();
                &mut s.kanban_card_field
            })
            .on_submit(|s: &mut State, tx: &mut Tx| {
                // Enter is the completion's accept key while the list is open
                // (`on_key` claims it). A refused text stays in the field.
                if s.complete.is_some() {
                    return;
                }
                if s.commit_card_edit() != CardCommit::Refused {
                    tx.blur();
                }
            })
            .on_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                if s.complete_key(e) {
                    return true;
                }
                if e.key == Key::Escape {
                    // Slice 9: Escape closes the picker and leaves the typed
                    // trigger; a second Escape cancels the editor.
                    if s.kanban_picker.is_some() {
                        s.dismiss_picker();
                        return true;
                    }
                    s.cancel_card_edit();
                    tx.blur();
                    return true;
                }
                false
            })
            .build(k),
    );
    if let Some(complete) = &s.complete
        && complete.card
        && !complete.items.is_empty()
    {
        editor = editor.child(crate::view::link_complete_list(s, k, complete));
    }
    editor
}

/// Card markdown for the reader. Note embeds lose the `!` so they render as
/// links; attachment and board embeds keep it, and the reader's rewrite turns
/// those into a filename placeholder. Dates, times and tags are left as text.
fn card_markdown(vault: &muninn_core::Vault, note: Option<usize>, text: &str) -> String {
    let text = plain_note_embeds(text);
    match note {
        Some(id) => vault.preview_with_attachments(&text, id),
        None => text,
    }
}

/// Drop the embed mark on `![[Note]]`. A target with a non-Markdown extension
/// is a file embed and keeps the mark.
fn plain_note_embeds(text: &str) -> String {
    let links = parse_links(text);
    if links.is_empty() {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for link in links {
        let bang = link.start > 0
            && bytes[link.start - 1] == b'!'
            && (link.start < 2 || bytes[link.start - 2] != b'\\');
        if bang && !embed_is_file(&link.target) {
            out.push_str(&text[last..link.start - 1]);
            last = link.start;
        }
    }
    out.push_str(&text[last..]);
    out
}

fn embed_is_file(target: &str) -> bool {
    let name = target.rsplit(['/', '\\']).next().unwrap_or(target);
    match name.rsplit_once('.') {
        Some((stem, ext)) => !stem.is_empty() && !ext.is_empty() && !ext.eq_ignore_ascii_case("md"),
        None => false,
    }
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
    fn a_note_embed_becomes_a_link_and_a_file_embed_keeps_its_mark() {
        let text = "see ![[Roadmap]] and ![[pic.png]] and \\![[Roadmap]] and @{2026-11-11}";
        assert_eq!(
            plain_note_embeds(text),
            "see [[Roadmap]] and ![[pic.png]] and \\![[Roadmap]] and @{2026-11-11}"
        );
        assert_eq!(plain_note_embeds("plain [[Roadmap]]"), "plain [[Roadmap]]");
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

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
//! date/time picker from a card menu or a typed trigger. Slice 10
//! (`kanban_tags.rs`) hides or chips a card's tags, completes `#` from the
//! vault's tag index, and fills the Find bar from a chip. Slice 11
//! (`kanban_metadata.rs`) draws configured metadata keys from the note a
//! card's first wikilink names, at the card's foot. Slice 12
//! (`kanban_archive.rs`) moves cards into the archive from the card menu or a
//! `Cmd`/`Ctrl`-click, and draws the header's Archive page with Restore and
//! Delete. Slice 13 (`kanban_search.rs`) adds Board Find and the ephemeral
//! filter, and draws a card's nested task items as its checklist at the card's
//! foot (plain-text rows; the display gaps are documented at
//! `without_checklist_lines`). Slice 14 (`kanban_settings.rs`) draws the
//! header's Board settings panel from the shared key schema, and its lane
//! width, new-line trigger and header-button switches take effect on the next
//! frame. Slice 15 (`kanban_views.rs`) draws the list and table views a
//! `kanban-plugin: list` / `table` value asks for, and the header's Board /
//! List / Table switcher chooses one for the session. Slice 16 (fixed in
//! review) adds the page's keyboard and accessibility pass: the lane cards are
//! a list with roving Up/Down whose selection follows focus, every control is
//! a Tab stop with a focus ring and Enter, Space/Enter toggle the focused or
//! selected card's done mark, the menus are focus-taking modal overlays of
//! menu/menuitem rows, the menu-opened date picker is a real modal overlay (a
//! typed-trigger picker stays inline with its field), and the conflict bar is
//! an alert dialog that gives the keyboard to **Resolve conflict** when Escape
//! puts it away.
//!
//! Known limits of that pass (kept rather than faked): a card row is a nested
//! focus group, so Tab reaches the row and then its first control (the
//! checkbox) while the arrows rove the rest; the date picker's day grid is a
//! grid, so Tab reaches it and the arrows move among days without writing; a
//! menu-opened time picker keeps its hour/minute fields and so stays an inline
//! group that Tab can leave; and a Kanban menu floats under its lane header,
//! clamped to stay inside a short window.
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

use muninn_core::{
    Card, ChecklistItem, CivilDate, DateSettings, Kanban, KanbanView, Lane, MetadataCache,
    MetadataSettings, card_checklist, is_kanban, parse_links, scan_card_text,
};
use rune::{Drag, El, Key, KeyEvent, Tx, col, el, icon, row, text};
use rune_kit::{Kit, Markdown, MarkdownStyle, TextArea};
use rune_theme_opencode::icons as ic;

use crate::board_view::{BoardStatus, UNSAVED, placeholder};
use crate::doc_sync::{DocConflict, conflict_bar, disk_conflict, file_state};
use crate::kanban_drag::Dragged;
use crate::kanban_edit::{CardCommit, CardSlot, KanbanHistory};
use crate::kanban_lanes::{LaneField, lane_field};
use crate::{DiskState, MenuTarget, Mode, State};

/// Width of a collapsed lane's strip.
const STRIP_W: f32 = 52.0;
/// Opacity of a card or lane while it is dragged (slice 6).
const DRAGGED: f32 = 0.45;
/// The Kanban page's empty state, with the mode chord this platform uses
/// (slice 16: the words said ⌘5 everywhere).
const NO_KANBAN_DETAIL: &str = if cfg!(target_os = "macos") {
    "Select a kanban note, then press ⌘5 to see it as a board."
} else {
    "Select a kanban note, then press Ctrl+5 to see it as a board."
};

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
    /// Slice 16: Escape dismissed the conflict bar for now (it is an
    /// `alertdialog`), so the header shows **Resolve conflict** instead. A
    /// new or changed conflict shows the bar again; the flag itself stays,
    /// so every save stays blocked until one of the three choices is taken.
    pub(crate) conflict_hidden: bool,
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
        let was = self.conflict;
        self.conflict = disk_conflict(path, |text| known && text == saved, self.conflict);
        if self.conflict.is_none() || self.conflict != was {
            // A conflict that changed (or went away) speaks again even when
            // Escape had put the bar away.
            self.conflict_hidden = false;
        }
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
        // Slice 13: Board Find and the filter belong to one board. Another
        // board starts with both clear; a re-read of the same board keeps them.
        if !same_rel {
            self.reset_kanban_search();
            // Slice 14: the Board settings panel and its text fields belong to
            // the board they opened on, so switching boards closes it rather
            // than leaving one board's text over another's values.
            self.close_kanban_settings();
        } else if shown.as_ref().is_some_and(|shown| shown.board.is_none()) {
            // The file stopped parsing: there is no board to configure.
            self.close_kanban_settings();
        }
        // A re-read can move the selected card down its lane; another
        // kanban's selection goes with that kanban.
        let selected_card = if same_rel { self.selected_card() } else { None };
        let reloaded = same_rel && shown.is_some();
        self.kanban = shown;
        // Slice 15: a view the switcher picked belongs to the board it was
        // picked on; another kanban opens in its own file's view.
        if !same_rel {
            self.kanban_view = None;
        }
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
            conflict_hidden: false,
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
        self.close_kanban_find();
        self.kanban_filter.clear();
        self.kanban_filter_saved = None;
        self.close_kanban_filter();
        self.kanban_selected = None;
        self.kanban_archive_open = false;
        self.close_kanban_settings();
        self.kanban_view = None;
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

    /// Slice 16: Escape with the conflict bar up puts it away (the bar is an
    /// `alertdialog`) without deciding anything. The conflict stays, so saves
    /// stay blocked and the header offers **Resolve conflict**; a new or
    /// changed conflict brings the bar back ([`ShownKanban::check_disk`]).
    pub(crate) fn dismiss_kanban_conflict(&mut self) {
        if let Some(shown) = &mut self.kanban
            && shown.conflict.is_some()
        {
            shown.conflict_hidden = true;
        }
    }

    /// **Resolve conflict**: show the bar again for the pending conflict.
    pub fn show_kanban_conflict(&mut self) {
        self.menu = None;
        if let Some(shown) = &mut self.kanban {
            shown.conflict_hidden = false;
        }
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

    /// Slice 15: the view on screen. The header switcher's choice when there
    /// is one, else the shown file's `kanban-plugin` value, else the board.
    #[must_use]
    pub fn kanban_view(&self) -> KanbanView {
        self.kanban_view
            .unwrap_or_else(|| self.kanban_board().map_or(KanbanView::Board, Kanban::view))
    }

    /// Slice 15: the header's Board / List / Table switcher. It changes only
    /// what the frame draws: the board, its undo history, the file and
    /// `settings.json` are untouched.
    pub fn set_kanban_view(&mut self, view: KanbanView) {
        if self.kanban_view() == view {
            return;
        }
        // An open card or lane editor commits as when the page is left; a
        // refused one keeps the board and the typed text on screen.
        if self.commit_card_edit() == CardCommit::Refused {
            return;
        }
        // A date picker belongs to the board's card, and a press held on a
        // card names a place in a layout that is about to go.
        self.close_picker();
        self.kanban_drag = None;
        self.menu = None;
        self.kanban_view = Some(view);
    }
}

/// The lane header's count, `n` or `n/N` against the lane limit, and whether
/// the lane holds more cards than its limit.
pub(crate) fn lane_load(lane: &Lane) -> (String, bool) {
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
            NO_KANBAN_DETAIL,
        ),
        Some(ShownKanban {
            rel,
            board: Some(board),
            ..
        }) if s.kanban_settings_open => crate::kanban_settings::board_panel(s, k),
        Some(ShownKanban {
            rel,
            board: Some(board),
            ..
        }) if s.kanban_archive_open => crate::kanban_archive::archive_panel(s, k, board),
        Some(ShownKanban {
            rel,
            board: Some(board),
            ..
        }) => match s.kanban_view() {
            KanbanView::Board => lanes(s, k, rel, board),
            KanbanView::List => crate::kanban_views::list(s, k, rel, board),
            KanbanView::Table => crate::kanban_views::table(s, k, rel, board),
        },
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
        .role("main")
        .label("Kanban")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(header(s, k))
        .child(crate::kanban_search::find_bar(s, k))
        .child(if s.kanban_filter.open {
            crate::kanban_search::filter_panel(s, k)
        } else {
            el()
        })
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
    let head = head
        .child(
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
            text(status.clone())
                .key("kanban-status")
                .role("status")
                .font(k.ty.caption())
                .color(status_color)
                .grow(1.0)
                .truncate()
                .help_when_truncated(status),
        )
        .child(match shown.conflict {
            // Slice 16: Escape put the bar away; the conflict and its save
            // block stay, and this brings the bar back.
            Some(_) if shown.conflict_hidden => header_button(
                k,
                "kanban-conflict-review",
                &ic::CIRCLE_EXCLAMATION,
                "Resolve conflict",
                State::show_kanban_conflict,
            ),
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
        .child(filter_summary(s, k));
    // Resolve the board's layout once for the header's three switches: each
    // call would parse the footer JSON again.
    let layout = s.kanban_board_settings();
    let mut head = head;
    if layout.show_filter_button {
        head = head.child(header_button(
            k,
            "kanban-filter-toggle",
            &ic::OUTLINE_SLIDERS,
            "Filter",
            State::toggle_kanban_filter,
        ));
    }
    if layout.show_archive_button {
        head = head.child(header_button(
            k,
            "kanban-archive",
            &ic::ARCHIVE,
            "Archive",
            State::toggle_kanban_archive,
        ));
    }
    if shown.board.is_some() {
        head = head.child(view_switch(s, k));
        // An unreadable board has no settings panel to open.
        head = head.child(header_button(
            k,
            "kanban-settings",
            &ic::SETTINGS_GEAR,
            "Board settings",
            State::toggle_kanban_settings,
        ));
    }
    if layout.show_open_markdown_button {
        head = head.child(header_button(
            k,
            "kanban-open-markdown",
            &ic::EDIT,
            "Open as markdown",
            State::open_kanban_as_markdown,
        ));
    }
    head
}
/// The active filter in the header: how many cards it shows, what it asks
/// for, and Clear. Nothing is drawn when no filter is active.
fn filter_summary(s: &State, k: &Kit) -> El {
    let Some(summary) = s.kanban_filter.summary() else {
        return el();
    };
    let (shown, total) = s.kanban_filter_counts();
    row()
        .key("kanban-filter-summary")
        .gap(6.0)
        .center_y()
        .child(
            text(format!("{shown}/{total} · {summary}"))
                .key("kanban-filter-summary-text")
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .shrink()
                .truncate(),
        )
        .child(
            text("Clear")
                .key("kanban-filter-clear")
                .font(k.ty.caption())
                .color(k.c.text_accent)
                .cursor(rune::Cursor::Pointer)
                .role("button")
                .label("Clear filters")
                .help("Clear filters")
                .focusable(true)
                .focus_ring(2.0, 1.0, k.c.text_accent)
                .on_key(|s: &mut State, e: &KeyEvent, _: &mut Tx| {
                    if e.key == Key::Enter {
                        s.clear_kanban_filter();
                        true
                    } else {
                        false
                    }
                })
                .on_press(|s: &mut State| s.clear_kanban_filter()),
        )
}

/// Slice 15: the header's Board / List / Table switcher. The choice is
/// ephemeral and never changes the frontmatter, so a `kanban-plugin: list`
/// board opens as a list but can still be shown as a board. Slice 16: it is a
/// tab list, so the chosen view reports its selected state.
fn view_switch(s: &State, k: &Kit) -> El {
    let current = s.kanban_view();
    row()
        .key("kanban-view-switch")
        .h(28.0)
        .gap(2.0)
        .px(2.0)
        .radius(7.0)
        .center_y()
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
        .role("tablist")
        .label("Kanban view")
        .focus_group()
        .children(
            [KanbanView::Board, KanbanView::List, KanbanView::Table]
                .into_iter()
                .map(|view| view_tab(k, view, view == current)),
        )
}

/// One tab of [`view_switch`].
fn view_tab(k: &Kit, view: KanbanView, active: bool) -> El {
    let label = match view {
        KanbanView::Board => "Board view",
        KanbanView::List => "List view",
        KanbanView::Table => "Table view",
    };
    row()
        .key(match view {
            KanbanView::Board => "kanban-view-board",
            KanbanView::List => "kanban-view-list",
            KanbanView::Table => "kanban-view-table",
        })
        .h(24.0)
        .px(8.0)
        .radius(6.0)
        .center_y()
        .fill(if active {
            k.alpha(k.c.text_accent, 0.14)
        } else {
            k.c.layer_01
        })
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(rune::Cursor::Pointer)
        .role("tab")
        .label(label)
        .help(label)
        .focusable(true)
        .focus_selected(active)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .on_key(move |s: &mut State, e: &KeyEvent, _: &mut Tx| {
            if e.key == Key::Enter {
                s.set_kanban_view(view);
                true
            } else {
                false
            }
        })
        .on_press(move |s: &mut State| s.set_kanban_view(view))
        .child(text(view.label()).font(k.ty.ui_compact).color(if active {
            k.c.text_accent
        } else {
            k.c.text_muted
        }))
}

/// The lanes, side by side, scrolling both ways. Each lane is built again
/// only when what it draws changes (its cards, its selected card, the theme);
/// a lane with the card editor, the lane field, a menu or a drag in it is
/// built every frame, as those change.
fn lanes(s: &State, k: &Kit, rel: &str, board: &Kanban) -> El {
    let note = s.note_id(rel);
    let dates = s.kanban_date_settings();
    let today = crate::kanban_dates::local_today();
    let metadata_settings = s.kanban_metadata_settings();
    let layout = s.kanban_board_settings();
    let card_key = card_settings_version(s, &dates, today, &metadata_settings, &layout);
    let filter_hash = s.kanban_filter_hash();
    let find_hash = s.kanban_find_hash();
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
            // A live lane is rebuilt every frame; the lane still parses each
            // of its distinct linked notes once for that rebuild.
            lane_column(
                s,
                k,
                i,
                lane,
                &DateContext {
                    dates: &dates,
                    today,
                    note,
                    metadata: &metadata_settings,
                    layout: &layout,
                },
            )
        } else {
            let selected = s.kanban_selected.filter(|(l, _)| *l == i).map(|(_, j)| j);
            let dates = dates.clone();
            let metadata_settings = metadata_settings.clone();
            let layout = layout.clone();
            rune::memo(
                format!("kanban-lane-{i}"),
                (
                    rel,
                    kit,
                    lane_version(lane),
                    selected,
                    card_key,
                    filter_hash,
                    find_hash,
                ),
                move || {
                    // Parsed only when this memo is rebuilt, never on a hit.
                    lane_column(
                        s,
                        k,
                        i,
                        lane,
                        &DateContext {
                            dates: &dates,
                            today,
                            note,
                            metadata: &metadata_settings,
                            layout: &layout,
                        },
                    )
                },
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
pub(crate) fn lane_version(lane: &Lane) -> u64 {
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

/// A hash of the date, tag and metadata settings a card's text and foot
/// depend on, plus the vault revision so a changed linked note's metadata is
/// read again on the next lane rebuild. The List and Table views use the same
/// key, so their lane groups rebuild on the same changes as the board's.
pub(crate) fn card_settings_version(
    s: &State,
    dates: &DateSettings,
    today: CivilDate,
    metadata: &MetadataSettings,
    layout: &muninn_core::KanbanBoardSettings,
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    date_version(dates, today).hash(&mut h);
    s.kanban_tag_settings().hash(&mut h);
    metadata.hash(&mut h);
    layout.hash(&mut h);
    s.vault.revision().hash(&mut h);
    h.finish()
}

/// The date part of a card's memo key: the settings and the local day.
fn date_version(dates: &DateSettings, today: CivilDate) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (dates, today).hash(&mut h);
    h.finish()
}

/// The date settings and the local day a card draws with, resolved once per
/// frame instead of once per card.
struct DateContext<'a> {
    dates: &'a DateSettings,
    today: CivilDate,
    /// The board's own note, for a self-link and for markdown embeds.
    note: Option<usize>,
    metadata: &'a MetadataSettings,
    layout: &'a muninn_core::KanbanBoardSettings,
}

/// The metadata rows a card's foot draws, resolved once by its lane.
struct MetadataRows<'a> {
    rows: &'a [muninn_core::MetadataRow],
    /// The linked note a markdown value's relative embeds resolve against.
    value_note: Option<usize>,
}

fn lane_column(s: &State, k: &Kit, i: usize, lane: &Lane, when: &DateContext<'_>) -> El {
    let (drop, _, dragged) = s.kanban_drag_in(i);
    let filtering = s.kanban_filter_active();
    let cards: Vec<(usize, &Card)> = lane
        .cards()
        .iter()
        .enumerate()
        .filter(|&(j, _)| s.kanban_card_shown_at(when.dates, when.today, i, j))
        .collect();
    if lane.collapsed() {
        return collapsed_lane(k, i, lane, cards.len(), filtering, drop.is_some(), dragged);
    }
    // One cache for this lane rebuild: each distinct linked note is parsed
    // once, not once per card.
    let metadata = MetadataCache::for_lane(&s.vault, lane, when.note, when.metadata.clone());
    let (base_count, over) = lane_load(lane);
    let count = if filtering {
        format!("{}/{}", cards.len(), lane.cards().len())
    } else {
        base_count
    };
    let count_color = if over { k.c.warning.fg } else { k.c.text_faint };
    let mut head = row()
        .key(format!("kanban-lane-{i}-head"))
        .w_full()
        .gap(8.0)
        .center_y()
        .role("group")
        .label(format!("{} controls", lane.title()))
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
                format!("Collapse lane: {}", lane.title()),
                move |s: &mut State, _: &mut Tx| s.toggle_lane_collapsed(i),
            )
            // The button is the disclosure control; the lane region itself is
            // not one.
            .expanded(true),
        )
        .child(menu_button(
            k,
            format!("kanban-lane-{i}-menu"),
            format!("Lane actions: {}", lane.title()),
            move |s: &mut State, _: &mut Tx| s.open_lane_menu(i),
        ));
    let mut label = format!("{}, {count} cards", lane.title());
    if over {
        label.push_str(", over limit");
    }
    if lane.done_word().is_some() {
        label.push_str(", done lane");
    }
    let mut column = col()
        .key(format!("kanban-lane-{i}"))
        .w(lane_width(when.layout))
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
        .role("region")
        .label(label)
        .opacity(if dragged { DRAGGED } else { 1.0 })
        .on_layout_tx(move |s: &mut State, rect, tx: &mut Tx| {
            s.kanban_rects.set_lane(i, rect);
            tx.live_only();
        })
        // Slice 16: the Menu key and Shift+F10 on anything in the lane open
        // its menu, as right-click does (a card's own press wins first).
        .on_secondary_press(move |s: &mut State, _, _| s.open_lane_menu(i))
        .child(head);
    if s.kanban_lane_edit
        .is_some_and(|slot| slot.lane == i && slot.field != LaneField::New)
    {
        column = column.child(lane_field(s, k));
    }
    // Slice 16 fix: the lane and card menus are root overlays
    // (`view::kanban_menu_layer`), not children of the lane: an inline menu
    // left the rest of the board inside its modal.
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
    // Slice 16: the cards are a list of their own, so the lane is a region
    // while its buttons and field live outside it; Up and Down rove among the
    // cards, and selection follows focus so Space/Enter act on the card the
    // keyboard is on (and a click or a shortcut that moves the selection moves
    // focus with it).
    let mut list = col()
        .key(format!("kanban-lane-{i}-cards"))
        .w_full()
        .gap(8.0)
        .role("list")
        .label(format!("{} cards", lane.title()))
        .focus_group()
        .focus_follows();
    let new_at = s
        .kanban_card
        .filter(|slot| slot.lane == i && slot.new)
        .map(|slot| slot.index);
    for &(j, card) in &cards {
        let picker_here = s
            .kanban_picker
            .as_ref()
            .is_some_and(|picker| picker.card == (i, j));
        let new_here = new_at == Some(j);
        if new_here {
            list = list.child(card_editor(s, k));
            if picker_here {
                list = list.child(crate::kanban_dates::picker(s, k));
            }
        }
        if drop == Some(j) {
            list = list.child(card_drop_bar(k));
        }
        let (metadata_rows, value_note) = metadata.rows(&s.vault, when.note, card.text());
        let metadata_rows = MetadataRows {
            rows: &metadata_rows,
            value_note,
        };
        list = list.child(card_view(s, k, i, j, card, when, &metadata_rows));
        if picker_here && !new_here {
            list = list.child(crate::kanban_dates::picker(s, k));
        }
    }
    if drop == Some(lane.cards().len()) {
        list = list.child(card_drop_bar(k));
    }
    if new_at == Some(lane.cards().len()) {
        list = list.child(card_editor(s, k));
        if s.kanban_picker
            .as_ref()
            .is_some_and(|picker| picker.card == (i, lane.cards().len()))
        {
            list = list.child(crate::kanban_dates::picker(s, k));
        }
    }
    column = column.child(list);
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
            .label(format!("Add card to {}", lane.title()))
            .help(format!("Add card to {}", lane.title()))
            .focusable(true)
            .focus_ring(2.0, 1.0, k.c.text_accent)
            .on_key(move |s: &mut State, e: &KeyEvent, _: &mut Tx| {
                if e.key == Key::Enter {
                    s.begin_add_card(i);
                    true
                } else {
                    false
                }
            })
            .on_press(move |s: &mut State| s.begin_add_card(i))
            .child(icon(&ic::PLUS, 12.0).color(k.c.text_faint))
            .child(text("Add card").font(k.ty.ui_compact).color(k.c.text_faint)),
    )
}

/// A collapsed lane: a narrow strip with the expand control, the card count
/// and the title. Slice 6: it drags as its lane's header, and is outlined
/// while a card dragged over it would drop at its end.
fn collapsed_lane(
    k: &Kit,
    i: usize,
    lane: &Lane,
    visible: usize,
    filtering: bool,
    drop: bool,
    dragged: bool,
) -> El {
    let (count, over) = lane_load(lane);
    let count = if filtering {
        format!("{visible}/{}", lane.cards().len())
    } else {
        count
    };
    let mut label = format!("{}, {count} cards, collapsed", lane.title());
    if lane.done_word().is_some() {
        label.push_str(", done lane");
    }
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
        .role("region")
        .label(label)
        .opacity(if dragged { DRAGGED } else { 1.0 })
        .cursor(rune::Cursor::Grab)
        .on_drag(move |s: &mut State, d: &mut Drag, tx: &mut Tx| {
            s.drag_kanban(Dragged::Lane(i), d, tx);
        })
        .on_secondary_press(move |s: &mut State, _, _| s.open_lane_menu(i))
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
                format!("Expand lane: {}", lane.title()),
                move |s: &mut State, _: &mut Tx| s.toggle_lane_collapsed(i),
            )
            .expanded(false),
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

/// A lane's drawn width: the board's `lane-width`, clamped to the range the
/// settings stepper writes.
fn lane_width(layout: &muninn_core::KanbanBoardSettings) -> f32 {
    layout
        .lane_width
        .clamp(muninn_core::MIN_LANE_WIDTH, muninn_core::MAX_LANE_WIDTH) as f32
}

/// Where a dragged card would land in a lane.
fn card_drop_bar(k: &Kit) -> El {
    el().key("kanban-drop")
        .w_full()
        .h(3.0)
        .radius(2.0)
        .fill(k.c.text_accent)
        .a11y_hidden()
}

/// Where a dragged lane would go, as tall as that lane.
fn lane_drop_bar(s: &State, k: &Kit) -> El {
    let h = s
        .kanban_lane_drop()
        .and_then(|(lane, _)| s.kanban_rects.lane_height(lane))
        .unwrap_or_else(|| lane_width(&s.kanban_board_settings()));
    el().key("kanban-lane-drop")
        .w(3.0)
        .h(h)
        .radius(2.0)
        .fill(k.c.text_accent)
        .a11y_hidden()
}

/// A new lane's title field, where the lane will go.
fn new_lane_column(s: &State, k: &Kit) -> El {
    col()
        .key("kanban-new-lane")
        .w(lane_width(&s.kanban_board_settings()))
        .padding(10.0)
        .radius(10.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .role("group")
        .label("New lane")
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
        .help("Add lane")
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .on_key(move |s: &mut State, e: &KeyEvent, _: &mut Tx| {
            if e.key == Key::Enter {
                s.begin_add_lane(count);
                true
            } else {
                false
            }
        })
        .on_press(move |s: &mut State| s.begin_add_lane(count))
        .child(icon(&ic::PLUS, 12.0).color(k.c.text_faint))
        .child(text("Add lane").font(k.ty.ui_compact).color(k.c.text_faint))
}

/// The `⋯` that opens a card's or a lane's menu.
pub(crate) fn menu_button(
    k: &Kit,
    key: String,
    label: impl Into<String>,
    action: impl Fn(&mut State, &mut Tx) + 'static,
) -> El {
    icon_button(k, key, &ic::OUTLINE_DOTS, 14.0, label, action)
}

/// The action a shared icon button runs on Space and Enter.
type IconAction = std::rc::Rc<dyn Fn(&mut State, &mut Tx)>;

/// A small icon button: a menu's `⋯`, a lane's collapse and expand. Slice
/// 16: focusable with a ring, Enter activates it as Space does, and its label
/// is also its tooltip.
pub(crate) fn icon_button(
    k: &Kit,
    key: String,
    glyph: &'static rune::Icon,
    size: f32,
    label: impl Into<String>,
    action: impl Fn(&mut State, &mut Tx) + 'static,
) -> El {
    let label = label.into();
    let action: IconAction = std::rc::Rc::new(action);
    let enter = action.clone();
    el().key(key)
        // WCAG 2.5.8: a target at least 24 × 24 points.
        .size(24.0, 24.0)
        .radius(6.0)
        .center()
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(label.clone())
        .help(label)
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        // Slice 16 fix: a plain Enter activates the focused icon button (the
        // page's own Enter flip only runs with nothing focused).
        .on_key(move |s: &mut State, e: &KeyEvent, tx: &mut Tx| {
            if e.key == Key::Enter && e.mods == rune::Mods::default() {
                enter(s, tx);
                true
            } else {
                false
            }
        })
        .on_press_tx(move |s: &mut State, tx: &mut Tx| action(s, tx))
        .child(icon(glyph, size).color(k.c.icon_muted))
}

/// The card's checkbox on any view: a plain click toggles its done mark and
/// `Cmd`/`Ctrl`-click archives it (slice 12). The list and table views reuse
/// it so the same actions read the same in every layout. Slice 16: its
/// accessible name is the card's text, its checked state says done, and it is
/// a keyboard stop (Space presses it, Enter toggles it).
pub(crate) fn card_checkbox(k: &Kit, i: usize, j: usize, done: bool, card: &str) -> El {
    let name = if card.trim().is_empty() {
        "Card".to_owned()
    } else {
        card.trim().to_owned()
    };
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
        .label(name)
        .help(if done { "Mark not done" } else { "Mark done" })
        .focusable(true)
        .focus_ring(2.0, -1.0, k.c.text_accent)
        .on_key(move |s: &mut State, e: &KeyEvent, _: &mut Tx| {
            // A focused button presses on Space for free; Enter needs its own
            // handler (slice 16). An open picker or bar keeps Enter.
            if e.key == Key::Enter && s.topmost_layer().is_none() {
                s.toggle_card_done(i, j);
                true
            } else {
                false
            }
        })
        .on_press_tx(move |s: &mut State, tx: &mut Tx| {
            // Slice 12: Cmd/Ctrl-click archives; a plain click still toggles.
            let mods = tx.mods();
            if mods.cmd || mods.ctrl {
                s.archive_card(i, j);
            } else {
                s.toggle_card_done(i, j);
            }
        });
    if done {
        mark.child(icon(&ic::CHECK, 10.0).color(k.c.text_accent))
    } else {
        mark
    }
}

fn card_view(
    s: &State,
    k: &Kit,
    i: usize,
    j: usize,
    card: &Card,
    when: &DateContext<'_>,
    metadata: &MetadataRows<'_>,
) -> El {
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
    let tag_settings = s.kanban_tag_settings();
    let found = scan_card_text(card.text(), when.dates);
    let title_text = found.without(
        card.text(),
        when.dates.hide_date_in_title,
        tag_settings.hide_tags_in_title,
    );
    // Slice 13: nested task items are the card's checklist, drawn as
    // interactive rows at the foot; they are removed from the main markdown
    // so one item is not shown twice.
    let checklist = card_checklist(card.text());
    let display_text = without_checklist_lines(&title_text, &checklist);
    // Wikilinks open the note. A note embed stays a link: the spec allows
    // `![[Note]]` in a card and does not say to inline the note. An attachment
    // embed keeps the reader's filename placeholder.
    let source = card_markdown(&s.vault, when.note, &display_text);
    let first = display_text.lines().next().unwrap_or_default().to_owned();
    let mark = card_checkbox(k, i, j, done, &first);
    let selected = s.kanban_selected == Some((i, j));
    let dragged = s.kanban_drag_in(i).1 == Some(j);
    let (matching, current) = s.kanban_find_mark(i, j);
    let border = if current {
        k.c.text_accent
    } else if matching {
        k.c.warning.fg
    } else if selected {
        k.c.text_accent
    } else {
        k.c.border_muted
    };
    let state_word = if done { "Done" } else { "Open" };
    let card_label = if first.trim().is_empty() {
        format!("{state_word}, empty card")
    } else {
        format!("{state_word}: {first}")
    };
    let menu_label = if first.trim().is_empty() {
        "Card actions: empty card".to_owned()
    } else {
        format!("Card actions: {first}")
    };
    let key = format!("kanban-card-{i}-{j}");
    let focused_key = key.clone();
    row()
        .key(key)
        .w_full()
        .padding(10.0)
        .gap(8.0)
        .radius(8.0)
        .fill(k.c.layer_02)
        .border(if current { 2.0 } else { 1.0 }, border)
        .role("listitem")
        .label(card_label)
        .focusable(true)
        // Slice 16 fix: the row is a nested focus group, so Tab reaches its
        // controls (the checkbox first) and the arrows rove the checkbox,
        // menu and checklist rows once focus is on one of them.
        .focus_group()
        .focus_selected(selected)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        // Slice 16: Space and Enter on the focused card flip its done mark,
        // the same edit its checkbox makes. A child control keeps its keys.
        .on_key(move |s: &mut State, e: &KeyEvent, tx: &mut Tx| {
            if tx.focused() != Some(focused_key.as_str()) {
                return false;
            }
            if card_row_arrow(s, e, tx) {
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
        .child(if when.layout.show_checkboxes {
            mark
        } else {
            el()
        })
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
                    k, when.dates, &found, when.today, i, j,
                ))
                .child(crate::kanban_tags::card_tags(
                    k,
                    &tag_settings,
                    &found,
                    i,
                    j,
                ))
                .child(crate::kanban_metadata::card_metadata_rows(
                    s,
                    k,
                    metadata.rows,
                    metadata.value_note,
                    i,
                    j,
                ))
                .child(checklist_section(k, i, j, &checklist)),
        )
        .child(menu_button(
            k,
            format!("kanban-card-{i}-{j}-menu"),
            menu_label,
            move |s: &mut State, _: &mut Tx| {
                s.open_card_menu(i, j);
            },
        ))
}

/// Slice 16: plain arrows on a focused card row move the page's selection,
/// as the page's own keys do, instead of roving the card's list group into
/// the row's checkbox and menu buttons. The card's border shows the
/// selection (a list item draws no focus ring of its own).
pub(crate) fn card_row_arrow(s: &mut State, e: &KeyEvent, tx: &mut Tx) -> bool {
    if e.mods != rune::Mods::default() {
        return false;
    }
    matches!(e.key, Key::Up | Key::Down | Key::Left | Key::Right)
        && crate::kanban_lanes::kanban_key(s, e, tx)
}

/// The card's markdown without the lines that make up its checklist. The
/// checklist rows at the foot are the one interactive copy.
///
/// Only the item lines are dropped. A continuation line under an item stays
/// in the main markdown, and the row shows just the item's own text (plain
/// text, not markdown); a blank continuation or an empty card first line can
/// leave this display text empty or blank, so it is not byte-for-byte the
/// card text. `docs/kanban-format.md` section 5 records the gap; toggling uses
/// the card's exact bytes.
pub(crate) fn without_checklist_lines(text: &str, items: &[ChecklistItem]) -> String {
    if items.is_empty() {
        return text.to_owned();
    }
    let hidden: Vec<usize> = items.iter().map(ChecklistItem::line).collect();
    text.split('\n')
        .enumerate()
        .filter(|(line, _)| !hidden.contains(line))
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The card's nested task items: a progress label and one interactive row per
/// item. Each row toggles only its own character through the core edit path.
pub(crate) fn checklist_section(k: &Kit, i: usize, j: usize, items: &[ChecklistItem]) -> El {
    if items.is_empty() {
        return el();
    }
    let done = items.iter().filter(|item| item.is_done()).count();
    let mut section = col()
        .key(format!("kanban-card-{i}-{j}-checklist"))
        .w_full()
        .gap(2.0)
        .pt(4.0)
        .child(
            row()
                .key(format!("kanban-card-{i}-{j}-checklist-progress"))
                .gap(5.0)
                .center_y()
                .child(icon(&ic::CHECKLIST, 11.0).color(k.c.text_faint))
                .child(
                    text(format!("{done}/{}", items.len()))
                        .key(format!("kanban-card-{i}-{j}-checklist-progress-text"))
                        .font(k.ty.caption())
                        .color(k.c.text_faint),
                ),
        );
    for (n, item) in items.iter().enumerate() {
        let line = item.line();
        let checked = item.is_done();
        let words = if item.text().is_empty() {
            "Checklist item"
        } else {
            item.text()
        };
        section = section.child(
            row()
                .key(format!("kanban-card-{i}-{j}-task-{n}"))
                .w_full()
                .min_h(24.0)
                .gap(6.0)
                // The card indent is already stripped; deeper items keep
                // their extra columns.
                .pl((item.indent() as f32 * 4.0).min(32.0))
                .center_y()
                .radius(4.0)
                .hover(k.c.hover)
                .press(k.c.pressed)
                .cursor(rune::Cursor::Pointer)
                .role("checkbox")
                .checked(checked)
                .label(words.to_owned())
                .focusable(true)
                .focus_ring(2.0, 1.0, k.c.text_accent)
                .on_key(move |s: &mut State, e: &KeyEvent, _: &mut Tx| {
                    if e.key == Key::Enter && s.topmost_layer().is_none() {
                        s.toggle_card_task(i, j, line);
                        true
                    } else {
                        false
                    }
                })
                .on_press(move |s: &mut State| s.toggle_card_task(i, j, line))
                .child(
                    el().size(13.0, 13.0)
                        .radius(3.0)
                        .center()
                        .border(
                            1.0,
                            if checked {
                                k.c.text_accent
                            } else {
                                k.c.border_base
                            },
                        )
                        .child(if checked {
                            icon(&ic::CHECK, 9.0).color(k.c.text_accent)
                        } else {
                            el()
                        }),
                )
                .child(
                    text(item.text().to_owned())
                        .font(k.ty.caption())
                        .color(if checked {
                            k.c.text_faint
                        } else {
                            k.c.text_muted
                        })
                        .wrap_anywhere()
                        .grow(1.0),
                ),
        );
    }
    section
}

/// The card editor: Enter commits, Shift+Enter breaks the line, Esc cancels.
fn card_editor(s: &State, k: &Kit) -> El {
    // Slice 16: closing the editor gives the keyboard back to the card it
    // edited (or to its lane's Add card, for a card not in the file yet).
    let mut editor = col().key("kanban-card-editor").w_full();
    if let Some(slot) = s.kanban_card {
        editor = editor.focus_return(if slot.new {
            format!("kanban-lane-{}-add", slot.lane)
        } else {
            format!("kanban-card-{}-{}", slot.lane, slot.index)
        });
    }
    // Slice 16: the open completion list names the row the field points at.
    let active = s
        .complete
        .as_ref()
        .filter(|complete| complete.card && !complete.items.is_empty())
        .map(|complete| {
            let index = complete.index.min(complete.items.len() - 1);
            format!("complete-item-{}", complete.items[index])
        })
        .or_else(|| {
            s.tag_complete
                .as_ref()
                .filter(|complete| complete.card && !complete.items.is_empty())
                .map(|complete| {
                    format!(
                        "complete-tag-{}",
                        complete.index.min(complete.items.len() - 1)
                    )
                })
        });
    let field = TextArea::new(&s.kanban_card_field)
        .label("Card text")
        .placeholder("Card text")
        .width(None)
        .auto_grow(2, 10)
        .wrap_anywhere()
        .on_edit(|s: &mut State| {
            s.detect_card_trigger();
            &mut s.kanban_card_field
        })
        .on_submit(|s: &mut State, _: &mut Tx| {
            // Enter is the completion's accept key while the list is open
            // (`on_key` claims it). A refused text stays in the field; a
            // committed one leaves the tree and its `focus_return` hands the
            // keyboard back to the card (an explicit blur would erase it).
            if s.complete.is_some() {
                return;
            }
            let _ = s.commit_card_edit();
        })
        .on_key(|s: &mut State, e: &KeyEvent, _: &mut Tx| {
            if s.complete_key(e) {
                return true;
            }
            if e.key == Key::Enter
                && s.kanban_picker.is_some()
                && !e.mods.shift
                && !e.mods.cmd
                && !e.mods.ctrl
            {
                // Slice 9: a plain Enter accepts the open picker instead
                // of committing the bare trigger. Shift+Enter keeps the
                // line break and Cmd/Ctrl+Enter keeps its meaning.
                s.picker_accept();
                return true;
            }
            // Slice 14: `new-line-trigger` swaps what Enter does. With
            // Shift+Enter, a plain Enter breaks the line and Shift+Enter
            // commits the card; Cmd/Ctrl+Enter still commits.
            if e.key == Key::Enter
                && !e.mods.cmd
                && !e.mods.ctrl
                && !e.mods.alt
                && s.kanban_board_settings().new_line_trigger
                    == muninn_core::NewLineTrigger::ShiftEnter
            {
                if e.mods.shift {
                    let _ = s.commit_card_edit();
                } else {
                    s.kanban_card_field.insert("\n");
                }
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
                return true;
            }
            false
        })
        .build(k);
    editor = editor.child(match active {
        Some(active) => field.active_descendant(active),
        None => field,
    });
    if let Some(complete) = &s.complete
        && complete.card
        && !complete.items.is_empty()
    {
        let rows = complete
            .items
            .iter()
            .map(|&id| {
                (
                    format!("complete-item-{id}"),
                    s.vault.display_label(id),
                    crate::view::CompletePick::Note(id),
                )
            })
            .collect();
        editor = editor.child(crate::view::complete_list(
            k,
            "link-complete",
            rows,
            complete.index,
        ));
    }
    if let Some(complete) = &s.tag_complete
        && complete.card
        && !complete.items.is_empty()
    {
        let rows = complete
            .items
            .iter()
            .enumerate()
            .map(|(index, tag)| {
                (
                    format!("complete-tag-{index}"),
                    format!("#{tag}"),
                    crate::view::CompletePick::Tag(tag.clone()),
                )
            })
            .collect();
        editor = editor.child(crate::view::complete_list(
            k,
            "tag-complete",
            rows,
            complete.index,
        ));
    }
    editor
}

/// Card markdown for the reader. Note embeds lose the `!` so they render as
/// links; attachment and board embeds keep it, and the reader's rewrite turns
/// those into a filename placeholder. `text` is already what the card shows:
/// `hide-date-in-title` removed the date and time tokens before this point.
pub(crate) fn card_markdown(vault: &muninn_core::Vault, note: Option<usize>, text: &str) -> String {
    let text = plain_note_embeds(text);
    let preview = match note {
        Some(id) => vault.preview_with_attachments(&text, id),
        None => text,
    };
    // Issue #66: the reader has no wrap-anywhere fallback, so give long
    // unbroken runs break opportunities here, after wikilinks are links.
    crate::kanban_wrap::card_markdown(&preview)
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
/// kanban in a kanban note's title bar. Slice 16: focusable with a ring,
/// Enter activates it, and its label is its tooltip.
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
        .help(label)
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .on_key(move |s: &mut State, e: &KeyEvent, _: &mut Tx| {
            if e.key == Key::Enter {
                action(s);
                true
            } else {
                false
            }
        })
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

    #[test]
    fn a_lane_memo_key_changes_with_the_local_day() {
        let dates = DateSettings::default();
        let today = CivilDate::new(2026, 11, 12).expect("today");
        assert_ne!(
            date_version(&dates, today),
            date_version(&dates, today.add_days(1)),
            "relative labels change at midnight"
        );
    }
}

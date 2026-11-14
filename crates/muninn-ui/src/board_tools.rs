//! Board slice 4: drawing tools and saving.
//!
//! A tool is picked from the toolbar in the canvas's top-left corner or with
//! Excalidraw's keys: `R` rectangle, `D` diamond, `O` ellipse, `A` arrow, `L`
//! line, `P` or `X` freehand, `T` text (also `2`–`8`, Excalidraw's number
//! row; text is slice 6, `board_text`), and `V`
//! or `1` for no tool. With no tool, a drag selects or moves (slice 5,
//! `board_edit`) and the hand tool `H` pans. With a tool, a
//! drag draws: the element is previewed while the pointer moves and added to
//! the in-memory board ([`muninn_core::Board::add_drawn`], which writes it in
//! Excalidraw's own shape) on release. The tool stays picked so several
//! shapes can be drawn in a row (Excalidraw's tool lock); `Esc` drops it, and
//! a second `Esc` leaves Board. The wheel, pinch, arrows and `+`/`-`/`0`
//! still move the camera while a tool is picked.
//!
//! Saving is explicit and small, ahead of slice 7's autosave and board
//! watcher (today a vault reload while Board shows re-reads a clean board):
//!
//! - `⌘S` / `Ctrl+S` on Board saves the open board.
//! - So does any step that would otherwise drop unsaved drawing: picking
//!   another board, switching vaults (which is refused, like a dirty note,
//!   when the save fails) and closing the app. Leaving Board for a note mode
//!   keeps the drawing in memory, unsaved, and coming back does not re-read
//!   the file over it.
//! - A save writes only through [`muninn_core::Board::save`] (hidden temp
//!   file, fsync, rename), only to the path the board was opened from, and
//!   only while that path is inside the vault that is open now, so a board
//!   is never written into another vault.
//! - Before writing, the file is read again; if its bytes are not the ones
//!   Muninn loaded or last wrote (another app changed or removed it), the
//!   save is refused and the drawing stays in memory, marked not saved, and
//!   the heading offers Keep mine, Save copy and Load theirs / Discard
//!   (`board_safety.rs`, which also keeps the dirty board's crash-recovery
//!   copy and guards quitting).
//! - Board keys (tool letters, `+`/`-`/`0`, arrows) only act while no text
//!   field has focus, so typing in the sidebar search stays in the search.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use muninn_core::{Board, IdSource, Ink, ShapeKind};
use rune::{Color, Cursor, Drag, El, Icon, Phase, Shape, Tx, canvas, icon, row};
use rune_kit::Kit;

use crate::board_scene::paint_item;
use crate::board_view::{BoardStatus, OpenBoard};
use crate::{BoardView, Mode, State};

/// A drawing tool. `None` on [`State::board_tool`] is "no tool" (a drag
/// pans; selection arrives in slice 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoardTool {
    Rectangle,
    Diamond,
    Ellipse,
    Arrow,
    Line,
    Freedraw,
    /// Slice 6: a click places text (`board_text`).
    Text,
}

impl BoardTool {
    /// Toolbar order, as in Excalidraw.
    pub const ALL: [BoardTool; 7] = [
        BoardTool::Rectangle,
        BoardTool::Diamond,
        BoardTool::Ellipse,
        BoardTool::Arrow,
        BoardTool::Line,
        BoardTool::Freedraw,
        BoardTool::Text,
    ];

    /// The element kind the tool draws.
    #[must_use]
    pub fn kind(self) -> ShapeKind {
        match self {
            BoardTool::Rectangle => ShapeKind::Rectangle,
            BoardTool::Diamond => ShapeKind::Diamond,
            BoardTool::Ellipse => ShapeKind::Ellipse,
            BoardTool::Arrow => ShapeKind::Arrow,
            BoardTool::Line => ShapeKind::Line,
            BoardTool::Freedraw => ShapeKind::Freedraw,
            BoardTool::Text => ShapeKind::Text,
        }
    }

    /// Stable name for keys and tests (`rectangle`, …, `freedraw`).
    #[must_use]
    pub fn name(self) -> &'static str {
        self.kind().as_str()
    }

    /// The letter shown on the toolbar button.
    #[must_use]
    pub fn letter(self) -> &'static str {
        match self {
            BoardTool::Rectangle => "R",
            BoardTool::Diamond => "D",
            BoardTool::Ellipse => "O",
            BoardTool::Arrow => "A",
            BoardTool::Line => "L",
            BoardTool::Freedraw => "P",
            BoardTool::Text => "T",
        }
    }

    fn glyph(self) -> &'static Icon {
        match self {
            BoardTool::Rectangle => &RECTANGLE,
            BoardTool::Diamond => &DIAMOND,
            BoardTool::Ellipse => &ELLIPSE,
            BoardTool::Arrow => &ARROW,
            BoardTool::Line => &LINE,
            BoardTool::Freedraw => &PEN,
            BoardTool::Text => &TEXT,
        }
    }

    /// The tool's accessible name (#72): a title, not the key-style `name`.
    fn label(self) -> &'static str {
        match self {
            BoardTool::Rectangle => "Rectangle",
            BoardTool::Diamond => "Diamond",
            BoardTool::Ellipse => "Ellipse",
            BoardTool::Arrow => "Arrow",
            BoardTool::Line => "Line",
            BoardTool::Freedraw => "Freehand",
            BoardTool::Text => "Text",
        }
    }

    /// What the tool does, with its key, for the button's help (#72).
    fn help(self) -> &'static str {
        match self {
            BoardTool::Rectangle => "Draw a rectangle (R)",
            BoardTool::Diamond => "Draw a diamond (D)",
            BoardTool::Ellipse => "Draw an ellipse (O)",
            BoardTool::Arrow => "Draw an arrow (A)",
            BoardTool::Line => "Draw a line (L)",
            BoardTool::Freedraw => "Draw freehand (P or X)",
            BoardTool::Text => "Place text, then type (T)",
        }
    }
}

/// What a plain key does to the tool: `Some(Some(t))` picks `t`,
/// `Some(None)` drops the tool, `None` is not a tool key.
#[must_use]
pub fn tool_for_key(text: &str) -> Option<Option<BoardTool>> {
    Some(match text {
        "v" | "V" | "1" => None,
        "r" | "R" | "2" => Some(BoardTool::Rectangle),
        "d" | "D" | "3" => Some(BoardTool::Diamond),
        "o" | "O" | "4" => Some(BoardTool::Ellipse),
        "a" | "A" | "5" => Some(BoardTool::Arrow),
        "l" | "L" | "6" => Some(BoardTool::Line),
        "p" | "P" | "x" | "X" | "7" => Some(BoardTool::Freedraw),
        "t" | "T" | "8" => Some(BoardTool::Text),
        _ => return None,
    })
}

static SELECT: Icon = Icon {
    name: "board-select",
    view: 20.0,
    body: r#"<path d="M5 3L15 10.5L10.4 11.3L13 16.6L11.2 17.5L8.6 12.2L5 15.2Z" stroke="currentColor" stroke-width="1.4" fill="none"/>"#,
};
static HAND: Icon = Icon {
    name: "board-hand",
    view: 20.0,
    body: r#"<path d="M7 10V5.2a1.2 1.2 0 0 1 2.4 0V9.5M9.4 9V4.2a1.2 1.2 0 0 1 2.4 0V9M11.8 9V5.2a1.2 1.2 0 0 1 2.4 0V11c0 3.3-2 5.5-5 5.5c-2.2 0-3.4-1-4.6-3L3.8 11a1.2 1.2 0 0 1 2-1.3L7 11.2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" fill="none"/>"#,
};
static RECTANGLE: Icon = Icon {
    name: "board-rectangle",
    view: 20.0,
    body: r#"<rect x="3.5" y="5" width="13" height="10" rx="2" stroke="currentColor" stroke-width="1.5" fill="none"/>"#,
};
static DIAMOND: Icon = Icon {
    name: "board-diamond",
    view: 20.0,
    body: r#"<path d="M10 3L17 10L10 17L3 10Z" stroke="currentColor" stroke-width="1.5" fill="none"/>"#,
};
static ELLIPSE: Icon = Icon {
    name: "board-ellipse",
    view: 20.0,
    body: r#"<ellipse cx="10" cy="10" rx="7" ry="6" stroke="currentColor" stroke-width="1.5" fill="none"/>"#,
};
static ARROW: Icon = Icon {
    name: "board-arrow",
    view: 20.0,
    body: r#"<path d="M3.5 16.5L16 4M16 4H9.5M16 4V10.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" fill="none"/>"#,
};
static LINE: Icon = Icon {
    name: "board-line",
    view: 20.0,
    body: r#"<line x1="3.5" y1="16.5" x2="16.5" y2="3.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
};
static PEN: Icon = Icon {
    name: "board-freedraw",
    view: 20.0,
    body: r#"<path d="M3 14C5 9 7 8 8.5 10.5C10 13 11.5 13 13 9.5C14 7 15.5 6 17 6.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" fill="none"/>"#,
};

static TEXT: Icon = Icon {
    name: "board-text",
    view: 20.0,
    body: r#"<path d="M4.5 5.5V4H15.5V5.5M10 4V16M7.5 16H12.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" fill="none"/>"#,
};

/// An element being drawn: its tool and the pointer's scene positions so far.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub tool: BoardTool,
    pub points: Vec<[f64; 2]>,
    /// Screen travel since the press, to ignore a click that never moved.
    moved: f32,
}

/// The pointer has to travel this far (in points) before a press draws.
const MIN_TRAVEL: f32 = 2.0;

/// A stable fingerprint of a board file's bytes, to notice another writer.
#[must_use]
pub fn fingerprint(bytes: &str) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// A seed for new ids: the clock and the process, so two Muninns drawing on
/// one board do not draw the same ids.
#[must_use]
pub fn id_seed() -> u64 {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    t ^ (u64::from(std::process::id()) << 32)
}

impl State {
    /// Pick a tool, or none. Dropping the tool also drops a half-drawn draft.
    pub fn set_board_tool(&mut self, tool: Option<BoardTool>) {
        self.board_tool = tool;
        // #69: the palette is a picker for the canvas, so picking a tool puts
        // it away (the chosen colours stay).
        self.board_palette = false;
        if tool.is_some() {
            // Picking a drawing tool leaves selection and panning behind,
            // as in Excalidraw.
            self.board_hand = false;
            self.board_clear_selection();
        }
        if let Some(open) = &mut self.board_open {
            open.draft = None;
        }
    }

    /// Pick the hand tool (`H`) or put it down. The hand drops the drawing
    /// tool; a drag then pans.
    pub fn set_board_hand(&mut self, on: bool) {
        if on {
            self.set_board_tool(None);
            self.board_clear_selection();
        }
        self.board_hand = on;
    }

    /// `Esc` on Board: clear the selection first, then drop the tool or the
    /// hand. `false` when there was nothing to drop (Esc then leaves Board).
    pub fn board_escape(&mut self) -> bool {
        // #69: the palette is layered over the pointer, so it goes first.
        if self.board_palette {
            self.board_palette = false;
            return true;
        }
        if self.board_clear_selection() {
            return true;
        }
        if self.board_tool.is_none() && !self.board_hand {
            return false;
        }
        self.board_hand = false;
        self.set_board_tool(None);
        true
    }

    /// One phase of a drag on the canvas with a tool picked.
    pub(crate) fn draw_board(&mut self, d: &Drag) {
        let Some(tool) = self.board_tool else {
            return;
        };
        if tool == BoardTool::Text {
            // Slice 6: a click (on release) opens the text editor there.
            if d.phase == Phase::Up
                && !d.cancelled()
                && let Some(open) = &self.board_open
            {
                let at = open.cam.view().scene([d.x, d.y]);
                self.begin_board_text([f64::from(at[0]), f64::from(at[1])]);
            }
            return;
        }
        // #72: the ink for the element this drag will make, resolved before
        // the board is borrowed mutably (a dark paper swaps the default).
        let ink = self.new_board_ink();
        let Some(open) = &mut self.board_open else {
            return;
        };
        if open.model.is_none() {
            return;
        }
        let view = open.cam.view();
        let at = view.scene([d.x, d.y]);
        let at = [f64::from(at[0]), f64::from(at[1])];
        match d.phase {
            Phase::Down => {
                open.draft = Some(Draft {
                    tool,
                    points: vec![at],
                    moved: 0.0,
                });
            }
            Phase::Move => {
                if let Some(draft) = &mut open.draft {
                    draft.moved = draft.moved.max(d.dx.abs().max(d.dy.abs()));
                    if tool == BoardTool::Freedraw || draft.points.len() < 2 {
                        draft.points.push(at);
                    } else if let Some(last) = draft.points.last_mut() {
                        *last = at;
                    }
                }
            }
            Phase::Up => {
                let Some(mut draft) = open.draft.take() else {
                    return;
                };
                if d.cancelled() || draft.moved.max(d.dx.abs().max(d.dy.abs())) < MIN_TRAVEL {
                    return;
                }
                if tool == BoardTool::Freedraw || draft.points.len() < 2 {
                    draft.points.push(at);
                } else if let Some(last) = draft.points.last_mut() {
                    *last = at;
                }
                let Some(model) = &mut open.model else { return };
                if let Some(made) = model.add_drawn_with(
                    tool.kind(),
                    &draft.points,
                    ink,
                    &mut self.board_ids,
                    now_ms(),
                ) {
                    // Undo takes a drawn element back as a tombstone.
                    let made = muninn_core::Change::created(made);
                    open.record(vec![made]);
                    open.auto_fit = false;
                    open.needs_fit = false;
                    // The view is pinned now (no refit to the new extent), so
                    // a restart should come back to it (written once the
                    // view rests, slice 7).
                    self.save_settings_soon();
                }
            }
        }
    }

    /// Save the open board if it has unsaved drawing (`⌘S`). `true` when the
    /// file on disk now holds the drawing (or there was nothing to save).
    pub fn save_board(&mut self) -> bool {
        // Slice 6: text being typed is part of what is saved (quit, switch).
        self.commit_board_text();
        let root = self.vault.root.clone();
        let Some(open) = &mut self.board_open else {
            return true;
        };
        if open.status != BoardStatus::Unsaved && !matches!(open.status, BoardStatus::Failed(_)) {
            return true;
        }
        let result = write_board(open, &root);
        let saved = result.is_ok();
        open.status = match result {
            Ok(()) => BoardStatus::Saved,
            Err(e) => BoardStatus::Failed(e),
        };
        if saved {
            open.conflict = None;
            let path = open.path.clone();
            self.board_written(&path);
            self.summarise_open_board();
        } else {
            // A removed or changed file: offer Keep mine / Save copy.
            open.check_disk();
        }
        // Saved: the recovery copy goes; not saved: it holds the drawing.
        let _ = self.board_recovery_tick();
        saved
    }

    /// Whether the open board holds drawing that is not on disk.
    #[must_use]
    pub fn board_dirty(&self) -> bool {
        // #40 review M1: text typed but not committed is unsaved too.
        self.board_text_pending().is_some()
            || self.board_open.as_ref().is_some_and(|open| {
                matches!(open.status, BoardStatus::Unsaved | BoardStatus::Failed(_))
            })
    }
}

/// Write `open`'s board to its own file, refusing a path outside `root`, a
/// board opened from another vault, and a file someone else changed.
fn write_board(open: &mut OpenBoard, root: &Path) -> Result<(), String> {
    let Some(model) = &open.model else {
        return Err("the board could not be read, so it is not written".to_owned());
    };
    if open.vault_root != root {
        return Err("the board belongs to another vault".to_owned());
    }
    if !crate::doc_sync::inside_vault(root, &open.path) {
        return Err("the board is not inside the open vault".to_owned());
    }
    let on_disk = std::fs::read_to_string(&open.path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "the file was removed".to_owned(),
        _ => e.to_string(),
    })?;
    if Some(fingerprint(&on_disk)) != open.disk {
        return Err("the file changed on disk".to_owned());
    }
    let json = model.to_json();
    model.save(&open.path).map_err(|e| e.to_string())?;
    open.disk = Some(fingerprint(&json));
    Ok(())
}

/// The draft as a stroke in canvas coordinates, for the live preview.
pub(crate) fn draft_shape(draft: &Draft, view: BoardView, ink: Ink<'_>, paper: Color) -> Shape {
    let mut scratch = Board::new();
    let mut ids = IdSource::new(0);
    let Some(item) = scratch.add_drawn_with(draft.tool.kind(), &draft.points, ink, &mut ids, 0)
    else {
        return Shape::new();
    };
    paint_item(item, paper, |p| view.screen(p), view.zoom)
}

/// The toolbar pinned in the canvas's top-left corner.
pub(crate) fn toolbar(s: &State, k: &Kit) -> El {
    let mut bar = row()
        .key("board-toolbar")
        // #72: a named toolbar, so assistive technology announces the group
        // before reading its buttons.
        .role("toolbar")
        .label("Board tools")
        .pin(Some(12.0), Some(12.0), None, None)
        .h(36.0)
        .padding(3.0)
        .gap(2.0)
        .radius(8.0)
        .center_y()
        .fill(k.alpha(k.c.layer_02, 0.96))
        .border(1.0, k.c.border_base);
    bar = bar.child(
        tool_button(k, s.board_hand, &HAND, "hand", None).on_press_tx(
            |s: &mut State, tx: &mut Tx| {
                s.set_board_hand(!s.board_hand);
                tx.blur();
            },
        ),
    );
    bar = bar.child(tool_button(
        k,
        s.board_tool.is_none() && !s.board_hand,
        &SELECT,
        "select",
        None,
    ));
    for tool in BoardTool::ALL {
        bar = bar.child(tool_button(
            k,
            s.board_tool == Some(tool),
            tool.glyph(),
            tool.name(),
            Some(tool),
        ));
    }
    bar.child(crate::board_palette::toggle(s, k))
}

fn tool_button(k: &Kit, on: bool, glyph: &'static Icon, name: &str, tool: Option<BoardTool>) -> El {
    // #72: hand and select are not `BoardTool`s, so name them here; the drawn
    // tools carry their own label and help.
    let (label, help) = match tool {
        Some(tool) => (tool.label(), tool.help()),
        None if name == "hand" => ("Hand", "Pan the board (H)"),
        None => ("Select", "Select and move elements (V)"),
    };
    row()
        .key(format!("board-tool-{name}"))
        // #72: the 30 pt target is a named, focusable toggle button with a
        // ring; Space presses it and the press still returns the keyboard to
        // the canvas for the tool letters.
        .role("button")
        .label(label)
        .help(help)
        .checked(on)
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.text_accent)
        .size(30.0, 30.0)
        .radius(6.0)
        .center()
        .fill(if on {
            k.c.layer_03
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .on_press_tx(move |s: &mut State, tx: &mut Tx| {
            s.board_hand = false;
            s.set_board_tool(tool);
            // Keep the keyboard on the canvas for tool letters and arrows.
            tx.blur();
        })
        .child(icon(glyph, 16.0).color(if on { k.c.text_accent } else { k.c.icon_muted }))
}

/// The live preview layer over the canvas, empty without a draft.
pub(crate) fn preview(open: &OpenBoard, ink: Ink<'_>) -> Option<El> {
    let draft = open.draft.as_ref()?;
    // The picked ink is painted through the same legibility check as a
    // committed element (#88), so a dark pick stays visible on a dark paper.
    let paper = open
        .scene
        .as_ref()
        .map_or(rune::Color::WHITE, |scene| scene.paper);
    let shape = draft_shape(draft, open.cam.view(), ink, paper);
    if shape.is_empty() {
        return None;
    }
    Some(
        canvas(shape)
            .key("board-draft")
            .size(open.canvas[0].max(1.0), open.canvas[1].max(1.0))
            .pin(Some(0.0), Some(0.0), None, None),
    )
}

/// Keys only Board reads: tools. `true` when the key was a tool key.
pub(crate) fn tool_key(s: &mut State, text: &str) -> bool {
    if s.mode != Mode::Board {
        return false;
    }
    match tool_for_key(text) {
        Some(tool) => {
            s.board_hand = false;
            s.set_board_tool(tool);
            true
        }
        None => false,
    }
}

//! Board slice 4: drawing tools and saving.
//!
//! A tool is picked from the toolbar in the canvas's top-left corner or with
//! Excalidraw's keys: `R` rectangle, `D` diamond, `O` ellipse, `A` arrow, `L`
//! line, `P` or `X` freehand (also `2`–`7`, Excalidraw's number row), and `V`
//! or `1` for no tool. With no tool, a drag pans as before. With a tool, a
//! drag draws: the element is previewed while the pointer moves and added to
//! the in-memory board ([`muninn_core::Board::add_drawn`], which writes it in
//! Excalidraw's own shape) on release. The tool stays picked so several
//! shapes can be drawn in a row (Excalidraw's tool lock); `Esc` drops it, and
//! a second `Esc` leaves Board. The wheel, pinch, arrows and `+`/`-`/`0`
//! still move the camera while a tool is picked.
//!
//! Saving is explicit and small, ahead of slice 7's autosave and watcher:
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
//!   save is refused and the drawing stays in memory, marked not saved.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use muninn_core::{Board, IdSource, ShapeKind};
use rune::{Cursor, Drag, El, Icon, Phase, Shape, Tx, canvas, icon, row};
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
}

impl BoardTool {
    /// Toolbar order, as in Excalidraw.
    pub const ALL: [BoardTool; 6] = [
        BoardTool::Rectangle,
        BoardTool::Diamond,
        BoardTool::Ellipse,
        BoardTool::Arrow,
        BoardTool::Line,
        BoardTool::Freedraw,
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
        _ => return None,
    })
}

static SELECT: Icon = Icon {
    name: "board-select",
    view: 20.0,
    body: r#"<path d="M5 3L15 10.5L10.4 11.3L13 16.6L11.2 17.5L8.6 12.2L5 15.2Z" stroke="currentColor" stroke-width="1.4" fill="none"/>"#,
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

fn now_ms() -> i64 {
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
        if let Some(open) = &mut self.board_open {
            open.draft = None;
        }
    }

    /// `Esc` on Board: drop the tool first. `false` when there was none.
    pub fn board_escape(&mut self) -> bool {
        if self.board_tool.is_none() {
            return false;
        }
        self.set_board_tool(None);
        true
    }

    /// One phase of a drag on the canvas with a tool picked.
    pub(crate) fn draw_board(&mut self, d: &Drag) {
        let Some(tool) = self.board_tool else {
            return;
        };
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
                if model
                    .add_drawn(tool.kind(), &draft.points, &mut self.board_ids, now_ms())
                    .is_some()
                {
                    open.rebuild();
                    open.status = BoardStatus::Unsaved;
                    open.auto_fit = false;
                    open.needs_fit = false;
                    // The view is pinned now (no refit to the new extent), so
                    // a restart should come back to it.
                    self.save_settings();
                }
            }
        }
    }

    /// Save the open board if it has unsaved drawing (`⌘S`). `true` when the
    /// file on disk now holds the drawing (or there was nothing to save).
    pub fn save_board(&mut self) -> bool {
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
            let path = open.path.clone();
            self.refresh_board_summary(&path);
        }
        saved
    }

    /// Whether the open board holds drawing that is not on disk.
    #[must_use]
    pub fn board_dirty(&self) -> bool {
        self.board_open.as_ref().is_some_and(|open| {
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
    if open.vault_root != root || !open.path.starts_with(root) {
        return Err("the board belongs to another vault".to_owned());
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
pub(crate) fn draft_shape(draft: &Draft, view: BoardView) -> Shape {
    let mut scratch = Board::new();
    let mut ids = IdSource::new(0);
    let Some(item) = scratch.add_drawn(draft.tool.kind(), &draft.points, &mut ids, 0) else {
        return Shape::new();
    };
    paint_item(item, |p| view.screen(p), view.zoom)
}

/// The toolbar pinned in the canvas's top-left corner.
pub(crate) fn toolbar(s: &State, k: &Kit) -> El {
    let mut bar = row()
        .key("board-toolbar")
        .pin(Some(12.0), Some(12.0), None, None)
        .h(36.0)
        .padding(3.0)
        .gap(2.0)
        .radius(8.0)
        .center_y()
        .fill(k.alpha(k.c.layer_02, 0.96))
        .border(1.0, k.c.border_base);
    bar = bar.child(tool_button(
        k,
        s.board_tool.is_none(),
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
    bar
}

fn tool_button(k: &Kit, on: bool, glyph: &'static Icon, name: &str, tool: Option<BoardTool>) -> El {
    row()
        .key(format!("board-tool-{name}"))
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
            s.set_board_tool(tool);
            // Keep the keyboard on the canvas for tool letters and arrows.
            tx.blur();
        })
        .child(icon(glyph, 16.0).color(if on { k.c.text_accent } else { k.c.icon_muted }))
}

/// The live preview layer over the canvas, empty without a draft.
pub(crate) fn preview(open: &OpenBoard) -> Option<El> {
    let draft = open.draft.as_ref()?;
    let shape = draft_shape(draft, open.cam.view());
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
            s.set_board_tool(tool);
            true
        }
        None => false,
    }
}

//! Board slice 5: selecting, moving and deleting elements, with undo.
//!
//! With no drawing tool picked (the select tool, `V`):
//!
//! - A click on an element selects it (its outermost group with it);
//!   Shift+click adds it to the selection, or takes it out again. A click on
//!   empty canvas clears the selection.
//! - A drag that starts on an element moves the selection (the element is
//!   selected first if it was not). A drag that starts on empty canvas draws
//!   a marquee and selects every element wholly inside it; with Shift it
//!   adds to the selection. Elements Muninn keeps raw (frames, images) and
//!   locked ones are never picked (`muninn_core` `board::edit`).
//! - Arrow keys move the selection 1 scene unit, Shift+arrow 5 (Excalidraw's
//!   steps); with nothing selected they pan, as before. `Delete` or
//!   `Backspace` deletes the selection. `⌘A` / `Ctrl+A` selects every element.
//! - `Esc` clears the selection first, then drops the tool, then leaves
//!   Board.
//! - The canvas pans with the hand tool (`H`, or the toolbar's hand), the
//!   wheel with Shift, a trackpad and the arrow keys: a plain drag on empty
//!   canvas is the marquee now, as in Excalidraw.
//!
//! Undo (`⌘Z` / `Ctrl+Z`) and redo (`⌘⇧Z` / `Ctrl+Shift+Z` / `Ctrl+Y`) cover
//! drawing (slice 4), moves and deletes, up to
//! [`muninn_core::HISTORY_LIMIT`] edits per open board. Every edit, undo
//! and redo included, bumps the touched elements' `version`, `versionNonce`
//! and `updated` as Excalidraw does, and marks the board unsaved; saving is
//! still the explicit slice 4 save with all its guards. A delete is an
//! `isDeleted: true` tombstone kept in the file (see `board::edit` for why),
//! and undoing a drawn element leaves its tombstone too.
//!
//! The selection is drawn as an outline round each selected element and a
//! box with corner and edge handles round all of them. The handles only
//! show the selection; resizing is not part of this slice, so a press on a
//! handle does nothing (#39 review M2) instead of moving what is under it.
//! A text label bound to a shape (`containerId`) goes with it: clicking the
//! label selects the shape, and moving, deleting, undoing and redoing the
//! shape carry the label too (#39 review M1, `Board::with_labels`).

use muninn_core::{Change, History, Shape as Element};
use rune::{Color, Drag, LineCap, Phase, Shape, canvas};

use crate::board_view::{BoardStatus, OpenBoard};
use crate::{BoardView, Mode, State};

/// Excalidraw's selection colour.
const SELECTION: &str = "#6965db";
/// How near (in screen points) a click must be to an outline to hit it.
const HIT_SLOP: f32 = 8.0;
/// Arrow-key steps in scene units: plain and with Shift (Excalidraw's).
pub const NUDGE: f64 = 1.0;
pub const NUDGE_BIG: f64 = 5.0;
/// Screen travel under which a press is a click, not a drag.
const CLICK_TRAVEL: f32 = 2.0;
/// The outline sits this far (screen points) outside the element.
const OUTLINE_GAP: f32 = 4.0;
/// Handle side, in screen points.
const HANDLE: f32 = 8.0;

/// A pointer gesture on the canvas with no drawing tool.
#[derive(Clone, Debug)]
pub enum Gesture {
    /// Moving the selection: where it was when the drag began, and the
    /// scene point the drag began at.
    Move {
        start: Vec<Element>,
        from: [f64; 2],
        travel: f32,
    },
    /// A marquee from `from` to `to` (scene), adding to `base`.
    Marquee {
        from: [f64; 2],
        to: [f64; 2],
        base: Vec<String>,
        travel: f32,
    },
}

/// The edit state of one open board: what is selected, the gesture in
/// progress and its undo history.
#[derive(Clone, Debug, Default)]
pub struct BoardEdit {
    /// Selected element ids, in z-order.
    pub selection: Vec<String>,
    pub gesture: Option<Gesture>,
    pub history: History,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn scene_at(view: BoardView, x: f32, y: f32) -> [f64; 2] {
    let p = view.scene([x, y]);
    [f64::from(p[0]), f64::from(p[1])]
}

fn rect(a: [f64; 2], b: [f64; 2]) -> [f64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[0].max(b[0]),
        a[1].max(b[1]),
    ]
}

impl OpenBoard {
    /// Record a finished edit: remember it for undo, redraw and mark the
    /// board unsaved. Nothing happens for an edit that changed nothing.
    pub(crate) fn record(&mut self, edit: Vec<Change>) -> bool {
        if edit.is_empty() {
            return false;
        }
        self.edit.history.record(edit);
        self.rebuild();
        // The user is working on the drawing now: keep the view where it is
        // rather than refitting on the next resize or reload (as drawing does).
        self.auto_fit = false;
        self.status = BoardStatus::Unsaved;
        true
    }

    /// Drop selected ids that are no longer pickable (deleted, locked,
    /// gone after a reload).
    pub(crate) fn prune_selection(&mut self) {
        if let Some(model) = &self.model {
            let live = model.pickable_ids();
            self.edit.selection.retain(|id| live.contains(id));
        } else {
            self.edit.selection.clear();
        }
    }
}

impl State {
    /// The open board's selected element ids.
    #[must_use]
    pub fn board_selection(&self) -> &[String] {
        self.board_open
            .as_ref()
            .map_or(&[], |open| open.edit.selection.as_slice())
    }

    /// Replace the selection (ids that cannot be picked are dropped).
    pub fn set_board_selection(&mut self, ids: Vec<String>) {
        if let Some(open) = &mut self.board_open
            && let Some(model) = &open.model
        {
            open.edit.selection = model.with_groups(&ids);
        }
    }

    /// `⌘A`: select every pickable element (and drop the drawing tool).
    pub fn board_select_all(&mut self) {
        self.set_board_tool(None);
        self.board_hand = false;
        if let Some(open) = &mut self.board_open
            && let Some(model) = &open.model
        {
            open.edit.selection = model.pickable_ids();
        }
    }

    /// Clear the selection; `false` when nothing was selected.
    pub fn board_clear_selection(&mut self) -> bool {
        let Some(open) = &mut self.board_open else {
            return false;
        };
        open.edit.gesture = None;
        !std::mem::take(&mut open.edit.selection).is_empty()
    }

    /// Arrow keys with a selection: move it by (`dx`, `dy`) scene units.
    /// `false` when nothing is selected (the arrows pan then).
    pub fn nudge_board(&mut self, dx: f64, dy: f64) -> bool {
        let Some(open) = &mut self.board_open else {
            return false;
        };
        if open.edit.selection.is_empty() {
            return false;
        }
        let Some(model) = &mut open.model else {
            return false;
        };
        let edit = model.move_by(&open.edit.selection, dx, dy, &mut self.board_ids, now_ms());
        open.record(edit);
        true
    }

    /// `Delete` / `Backspace`: delete the selection (tombstones).
    pub fn delete_board_selection(&mut self) -> bool {
        let Some(open) = &mut self.board_open else {
            return false;
        };
        let Some(model) = &mut open.model else {
            return false;
        };
        let ids = std::mem::take(&mut open.edit.selection);
        let edit = model.delete(&ids, &mut self.board_ids, now_ms());
        open.record(edit)
    }

    /// `⌘Z`: take back the newest edit on the open board.
    pub fn undo_board(&mut self) -> bool {
        self.board_history_step(true)
    }

    /// `⌘⇧Z` / `Ctrl+Y`: apply the newest undone edit again.
    pub fn redo_board(&mut self) -> bool {
        self.board_history_step(false)
    }

    fn board_history_step(&mut self, undo: bool) -> bool {
        let Some(open) = &mut self.board_open else {
            return false;
        };
        let Some(model) = &mut open.model else {
            return false;
        };
        open.edit.gesture = None;
        let now = now_ms();
        let touched = if undo {
            open.edit.history.undo(model, &mut self.board_ids, now)
        } else {
            open.edit.history.redo(model, &mut self.board_ids, now)
        };
        let Some(touched) = touched else {
            return false;
        };
        open.edit.selection = touched;
        open.rebuild();
        open.auto_fit = false;
        open.prune_selection();
        open.status = BoardStatus::Unsaved;
        true
    }

    /// One phase of a drag on the canvas with no drawing tool: select,
    /// move or marquee.
    pub(crate) fn edit_board(&mut self, d: &Drag, shift: bool) {
        let Some(open) = &mut self.board_open else {
            return;
        };
        let view = open.cam.view();
        let at = scene_at(view, d.x, d.y);
        let travel = d.dx.abs().max(d.dy.abs());
        let Some(model) = &mut open.model else {
            return;
        };
        match d.phase {
            Phase::Down => {
                let tol = f64::from(HIT_SLOP / view.zoom.max(f32::MIN_POSITIVE));
                let hit = model.hit_test(at, tol).map(|s| s.id.clone());
                // What a hit picks: its outermost group, a bound label its
                // container (#39 recheck M2), so it is compared with the
                // selection as the selection holds it.
                let group = hit
                    .as_ref()
                    .map(|id| model.with_groups(std::slice::from_ref(id)))
                    .unwrap_or_default();
                let in_sel = group.iter().any(|g| open.edit.selection.contains(g));
                // A press on a handle does nothing (no resize yet), unless it
                // is on a selected element itself (inside its own bounds): a
                // thin line's midpoint or a small shape lies under its handles
                // (#39 recheck M1). Handles sit outside the bounds, so a corner
                // grab still never moves anything.
                let on_selected = in_sel
                    && open.edit.selection.iter().any(|id| {
                        model.shape(id).is_some_and(|e| {
                            let pad = (e.stroke_width / 2.0)
                                .max(1.0 / f64::from(view.zoom.max(f32::MIN_POSITIVE)));
                            e.bounds().is_some_and(|b| {
                                at[0] >= b[0] - pad
                                    && at[0] <= b[2] + pad
                                    && at[1] >= b[1] - pad
                                    && at[1] <= b[3] + pad
                            })
                        })
                    });
                if !shift
                    && !on_selected
                    && on_handle(model, &open.edit.selection, view, [d.x, d.y])
                {
                    open.edit.gesture = None;
                    return;
                }
                open.edit.gesture = match hit {
                    Some(_) => {
                        let sel = &mut open.edit.selection;
                        if shift && in_sel {
                            sel.retain(|s| !group.contains(s));
                            None
                        } else {
                            if shift {
                                let mut all = sel.clone();
                                all.extend(group);
                                *sel = model.with_groups(&all);
                            } else if !in_sel {
                                *sel = group;
                            }
                            Some(Gesture::Move {
                                start: model.snapshot(sel),
                                from: at,
                                travel: 0.0,
                            })
                        }
                    }
                    None => {
                        if !shift {
                            open.edit.selection.clear();
                        }
                        Some(Gesture::Marquee {
                            from: at,
                            to: at,
                            base: open.edit.selection.clone(),
                            travel: 0.0,
                        })
                    }
                };
            }
            Phase::Move | Phase::Up => {
                let up = d.phase == Phase::Up;
                match &mut open.edit.gesture {
                    Some(Gesture::Move {
                        start,
                        from,
                        travel: t,
                    }) => {
                        *t = t.max(travel);
                        let moved = *t >= CLICK_TRAVEL && !d.cancelled();
                        let (dx, dy) = if moved {
                            (at[0] - from[0], at[1] - from[1])
                        } else {
                            (0.0, 0.0)
                        };
                        model.place(start, dx, dy);
                        if up {
                            let start = std::mem::take(start);
                            open.edit.gesture = None;
                            let edit = model.commit(&start, &mut self.board_ids, now_ms());
                            if !open.record(edit) {
                                open.rebuild();
                            }
                        } else {
                            open.rebuild();
                        }
                    }
                    Some(Gesture::Marquee {
                        from,
                        to,
                        base,
                        travel: t,
                    }) => {
                        *t = t.max(travel);
                        *to = at;
                        let mut sel = base.clone();
                        if *t >= CLICK_TRAVEL && !d.cancelled() {
                            sel.extend(model.in_rect(rect(*from, *to)));
                        }
                        open.edit.selection = model.with_groups(&sel);
                        if up {
                            open.edit.gesture = None;
                        }
                    }
                    None => {}
                }
            }
        }
    }
}

/// The selection box (canvas points) the handles sit on: the selection's
/// bounds plus the outline gap, and a second gap round several elements.
fn selection_box(
    model: &muninn_core::Board,
    sel: &[String],
    view: crate::BoardView,
) -> Option<[f32; 4]> {
    let all = model.bounds_of(sel)?;
    let a = view.screen([all[0] as f32, all[1] as f32]);
    let c = view.screen([all[2] as f32, all[3] as f32]);
    let g = if sel.len() > 1 {
        2.0 * OUTLINE_GAP
    } else {
        OUTLINE_GAP
    };
    Some([a[0] - g, a[1] - g, c[0] + g, c[1] + g])
}

/// The centres of the eight handles on box `r`.
fn handle_centres(r: [f32; 4]) -> [[f32; 2]; 8] {
    let (mx, my) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    [
        [r[0], r[1]],
        [mx, r[1]],
        [r[2], r[1]],
        [r[2], my],
        [r[2], r[3]],
        [mx, r[3]],
        [r[0], r[3]],
        [r[0], my],
    ]
}

/// #39 review M2: whether canvas point `p` is on a selection handle. The
/// handles do not resize yet, so a press on one does nothing (rather than
/// grabbing the element under it and moving it).
fn on_handle(
    model: &muninn_core::Board,
    sel: &[String],
    view: crate::BoardView,
    p: [f32; 2],
) -> bool {
    if sel.is_empty() {
        return false;
    }
    let Some(r) = selection_box(model, sel, view) else {
        return false;
    };
    let reach = HANDLE / 2.0 + 2.0;
    handle_centres(r)
        .iter()
        .any(|h| (p[0] - h[0]).abs() <= reach && (p[1] - h[1]).abs() <= reach)
}

/// The selection outlines, handles and marquee over the canvas, in canvas
/// points; `None` when there is nothing to draw.
pub(crate) fn overlay(open: &OpenBoard) -> Option<rune::El> {
    let model = open.model.as_ref()?;
    let view = open.cam.view();
    let ink = rune::color::hex(SELECTION).unwrap_or(Color::TRANSPARENT);
    let mut shape = Shape::new();
    let screen = |b: [f64; 4]| -> [f32; 4] {
        let a = view.screen([b[0] as f32, b[1] as f32]);
        let c = view.screen([b[2] as f32, b[3] as f32]);
        [
            a[0] - OUTLINE_GAP,
            a[1] - OUTLINE_GAP,
            c[0] + OUTLINE_GAP,
            c[1] + OUTLINE_GAP,
        ]
    };
    let boxed = |s: Shape, r: [f32; 4], width: f32| {
        s.stroke_with(
            vec![[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]],
            true,
            width,
            LineCap::Butt,
            ink,
        )
    };
    let sel = &open.edit.selection;
    for id in sel {
        if let Some(b) = model.shape(id).and_then(Element::bounds) {
            shape = boxed(shape, screen(b), 1.0);
        }
    }
    if let Some(r) = selection_box(model, sel, view) {
        if sel.len() > 1 {
            shape = boxed(shape, r, 1.0);
        }
        for [hx, hy] in handle_centres(r) {
            let h = HANDLE / 2.0;
            let sq = vec![
                [hx - h, hy - h],
                [hx + h, hy - h],
                [hx + h, hy + h],
                [hx - h, hy + h],
            ];
            shape = shape.fill(sq.clone(), Color::WHITE);
            shape = shape.stroke_with(sq, true, 1.0, LineCap::Butt, ink);
        }
    }
    if let Some(Gesture::Marquee {
        from, to, travel, ..
    }) = &open.edit.gesture
        && *travel >= CLICK_TRAVEL
    {
        let a = view.screen([from[0] as f32, from[1] as f32]);
        let b = view.screen([to[0] as f32, to[1] as f32]);
        let r = [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[0].max(b[0]),
            a[1].max(b[1]),
        ];
        let quad = vec![[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]];
        shape = shape.fill(quad.clone(), ink.alpha(0.08));
        shape = shape.stroke_with(quad, true, 1.0, LineCap::Butt, ink);
    }
    if shape.is_empty() {
        return None;
    }
    Some(
        canvas(shape)
            .key("board-selection")
            .size(open.canvas[0].max(1.0), open.canvas[1].max(1.0))
            .pin(Some(0.0), Some(0.0), None, None),
    )
}

/// `⌘`-chords Board reads: undo, redo, select all. `true` when handled.
pub(crate) fn board_command(s: &mut State, e: &rune::KeyEvent) -> bool {
    if s.mode != Mode::Board || s.board_open.is_none() || !e.mods.cmd {
        return false;
    }
    let rune::Key::Command(c) = &e.key else {
        return false;
    };
    match c.to_ascii_lowercase() {
        'z' if e.mods.shift => {
            s.redo_board();
        }
        'z' => {
            s.undo_board();
        }
        'y' => {
            s.redo_board();
        }
        'a' => s.board_select_all(),
        _ => return false,
    }
    true
}

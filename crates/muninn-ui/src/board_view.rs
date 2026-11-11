//! The Board page (`Mode::Board`): the vault's `.excalidraw` boards and the
//! board canvas.
//!
//! The page lists the boards found on disk with their element counts (read
//! through `muninn_core::Board`). Picking one opens it on a read-only canvas
//! ([`BoardScene`]) with a pan and zoom camera ([`BoardCam`]). Nothing here
//! writes a board; drawing and saving come in later slices.
//!
//! Camera behaviour:
//!
//! - A board opens fitted to its content, unless it was open earlier in the
//!   session (its view is remembered per board) or it is the board restored
//!   from the last session (its view is in `settings.json`, never in the file).
//! - Switching modes and coming back keeps the open board and its view. The
//!   file is re-read from disk on the way back in, and the view stays put.
//! - Restarting reopens the last board at the view it had when settings were
//!   last saved: on a board pick, a drag or key gesture, a fit, a mode switch
//!   and on close. A plain wheel zoom is saved by the next of those.
//!   A board is only remembered once a frame has drawn it, so one that
//!   cannot be drawn is not reopened on the next launch.
//! - While the view has never been touched, a canvas resize refits it.

use std::path::{Path, PathBuf};

use muninn_core::{Board, BoardFile};
use rune::{
    Color, Cursor, Drag, El, KeyEvent, MagnifyEvent, Phase, PointerButton, TextStyle, Tx,
    WheelEvent, canvas, col, el, icon, row, spacer, text,
};
use rune_kit::Kit;
use rune_theme_opencode::icons as ic;

use crate::board_camera::{BoardCam, BoardView, KEY_PAN, KEY_ZOOM, WHEEL_IN, WHEEL_OUT};
use crate::{BoardScene, Mode, State};

/// The board on the canvas: its drawing and camera.
pub struct OpenBoard {
    pub path: PathBuf,
    /// The drawing, or why the file could not be read.
    pub scene: Result<BoardScene, String>,
    pub cam: BoardCam,
    /// The canvas size in points, from the last layout.
    pub canvas: [f32; 2],
    /// Fit once the canvas has a size (a first open with no stored view).
    pub needs_fit: bool,
    /// The view has not been moved by the user since its fit, so a resize
    /// refits it.
    pub auto_fit: bool,
    view_at_press: BoardView,
    /// How far the board is from being remembered for the next launch (see
    /// [`Shown`]). A board is only written to `settings.json` once a frame
    /// has drawn it, so a board that crashes on its first draw is not
    /// reopened on every launch.
    shown: Shown,
    /// The session this board was restored from, kept while it is not shown
    /// yet (the startup scan still runs) so a quit then does not forget it.
    restored: Option<BoardSession>,
}

/// Steps from opening a board to remembering it for the next launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Shown {
    /// No frame has drawn it yet.
    Not,
    /// A frame drew it (its canvas reported where it was drawn).
    Drawn,
    /// The frame after that one started: the draw finished without a panic.
    Survived,
    /// Written to `settings.json`.
    Remembered,
}

impl OpenBoard {
    fn load(path: PathBuf, view: Option<BoardView>) -> OpenBoard {
        let scene = read_scene(&path);
        let (origin, size) = world_of(&scene);
        OpenBoard {
            path,
            scene,
            cam: BoardCam::new(view.unwrap_or_default(), origin, size),
            canvas: [0.0; 2],
            needs_fit: view.is_none(),
            auto_fit: view.is_none(),
            view_at_press: view.unwrap_or_default(),
            shown: Shown::Not,
            restored: None,
        }
    }

    /// Re-read the file and keep the view where it is.
    fn reload(&mut self) {
        self.scene = read_scene(&self.path);
        let (origin, size) = world_of(&self.scene);
        self.cam.rebase(origin, size);
        if self.auto_fit {
            self.fit();
        }
    }

    /// Centre the content in the canvas (deferred until the canvas has a size).
    pub fn fit(&mut self) {
        if self.canvas[0] < 1.0 || self.canvas[1] < 1.0 {
            self.needs_fit = true;
            return;
        }
        let bounds = self.scene.as_ref().ok().and_then(|s| s.bounds);
        self.cam.set(BoardView::fitted(bounds, self.canvas));
        self.needs_fit = false;
    }

    fn centre(&self) -> [f32; 2] {
        [self.canvas[0] / 2.0, self.canvas[1] / 2.0]
    }
}

fn read_scene(path: &Path) -> Result<BoardScene, String> {
    Board::load(path)
        .map(|board| BoardScene::build(&board))
        .map_err(|e| e.to_string())
}

fn world_of(scene: &Result<BoardScene, String>) -> ([f32; 2], [f32; 2]) {
    scene
        .as_ref()
        .map_or(([0.0; 2], [1.0; 2]), |s| (s.origin, s.size))
}

/// The board to reopen after a restart, as saved in `settings.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardSession {
    /// Vault-relative path with `/` separators.
    pub rel: String,
    pub view: Option<BoardView>,
}

/// One board in the vault, as last read from disk.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardSummary {
    pub file: BoardFile,
    /// Live (drawn) elements and how many of all elements Muninn keeps as-is
    /// because it does not model their type, or why the file could not be read.
    pub counts: Result<BoardCounts, String>,
}

/// Element counts for [`BoardSummary`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardCounts {
    pub live: usize,
    pub raw: usize,
}

impl BoardSummary {
    fn read(file: BoardFile) -> BoardSummary {
        let counts = Board::load(&file.path)
            .map(|board| BoardCounts {
                live: board.live_elements().count(),
                raw: board
                    .elements
                    .iter()
                    .filter(|e| e.shape().is_none())
                    .count(),
            })
            .map_err(|e| e.to_string());
        BoardSummary { file, counts }
    }

    /// `12 elements`, `1 element · 2 kept as-is`, or the read error.
    #[must_use]
    pub fn detail(&self) -> String {
        match &self.counts {
            Ok(c) if c.raw == 0 => plural(c.live, "element", "elements"),
            Ok(c) => format!(
                "{} · {} kept as-is",
                plural(c.live, "element", "elements"),
                c.raw
            ),
            Err(e) => format!("Could not read: {e}"),
        }
    }
}

impl State {
    /// Re-read the vault's boards from disk. The file is the source of truth,
    /// so this runs on entering Board mode and after a vault reload rather
    /// than caching across sessions. A selection whose file is gone clears.
    pub fn refresh_boards(&mut self) {
        self.boards = self
            .vault
            .boards()
            .into_iter()
            .map(BoardSummary::read)
            .collect();
        if let Some(path) = &self.board_selected
            && !self.boards.iter().any(|b| &b.file.path == path)
        {
            self.board_selected = None;
        }
        match (&self.board_selected, &mut self.board_open) {
            (Some(path), Some(open)) if &open.path == path => open.reload(),
            (Some(path), _) => {
                let path = path.clone();
                let view = self.board_views.get(&path).copied();
                self.board_open = Some(OpenBoard::load(path, view));
            }
            (None, _) => self.board_open = None,
        }
        if self.board_selected.is_none()
            && let Some(session) = self.board_restore.take()
            && let Some(b) = self.boards.iter().find(|b| b.file.rel == session.rel)
        {
            let path = b.file.path.clone();
            self.board_selected = Some(path.clone());
            let mut open = OpenBoard::load(path, session.view);
            open.restored = Some(session);
            self.board_open = Some(open);
        }
    }

    /// Per frame: remember the open board for the next launch once a frame
    /// has drawn it and the next frame has begun, so the draw finished.
    /// Returns true when settings were written (nothing visible changed).
    pub(crate) fn board_shown_tick(&mut self, tx: &mut Tx) -> bool {
        let Some(open) = &mut self.board_open else {
            return false;
        };
        match open.shown {
            Shown::Drawn => {
                open.shown = Shown::Survived;
                tx.wake_after(std::time::Duration::from_millis(16));
                false
            }
            Shown::Survived => {
                open.shown = Shown::Remembered;
                open.restored = None;
                self.save_settings();
                true
            }
            Shown::Not | Shown::Remembered => false,
        }
    }

    /// Select a listed board by its absolute path (`BoardFile::path`) and open it on the canvas. Picking the
    /// board that is already open keeps its view.
    pub fn select_board(&mut self, path: PathBuf) {
        if self
            .board_open
            .as_ref()
            .is_some_and(|open| open.path == path)
        {
            self.board_selected = Some(path);
            return;
        }
        self.stash_board_view();
        let view = self.board_views.get(&path).copied();
        // The canvas keeps its size from board to board, so a new board can
        // fit now instead of waiting for a layout change that never comes.
        let canvas = self.board_open.as_ref().map(|open| open.canvas);
        self.board_selected = Some(path.clone());
        let mut open = OpenBoard::load(path, view);
        if let Some(canvas) = canvas {
            open.canvas = canvas;
            if open.needs_fit {
                open.fit();
            }
        }
        self.board_open = Some(open);
        self.save_settings();
    }

    /// Remember the open board's view for this session.
    fn stash_board_view(&mut self) {
        if let Some(open) = &self.board_open
            && !open.needs_fit
        {
            self.board_views.insert(open.path.clone(), open.cam.view());
        }
    }

    /// What `settings.json` keeps for the next launch: the open board and its
    /// view, or a restore that has not been applied yet.
    #[must_use]
    ///
    /// A board no frame has drawn yet is not remembered: if drawing it
    /// crashed, the next launch must not reopen it. While the startup scan
    /// runs (nothing can be drawn) the session it was restored from is kept.
    pub fn board_session(&self) -> Option<BoardSession> {
        let Some(open) = &self.board_open else {
            return self.board_restore.clone();
        };
        if open.shown < Shown::Remembered {
            return if self.loading() {
                open.restored.clone()
            } else {
                None
            };
        }
        let rel = self
            .boards
            .iter()
            .find(|b| b.file.path == open.path)?
            .file
            .rel
            .clone();
        Some(BoardSession {
            rel,
            view: (!open.needs_fit).then(|| open.cam.view()),
        })
    }

    /// Forget every open board and remembered view (another vault opened).
    pub fn clear_boards(&mut self) {
        self.boards.clear();
        self.board_selected = None;
        self.board_open = None;
        self.board_views.clear();
        self.board_restore = None;
    }

    /// Fit the open board's content to the canvas.
    pub fn fit_board(&mut self) {
        if let Some(open) = &mut self.board_open {
            open.fit();
            open.auto_fit = false;
        }
        self.save_settings();
    }

    /// Zoom the open board about the canvas centre.
    pub fn zoom_board(&mut self, factor: f32) {
        if let Some(open) = &mut self.board_open {
            let centre = open.centre();
            open.cam.zoom_at(centre, factor);
            open.auto_fit = false;
            open.needs_fit = false;
        }
        self.save_settings();
    }

    /// Pan the open board by screen points.
    pub fn pan_board(&mut self, dx: f32, dy: f32) {
        if let Some(open) = &mut self.board_open {
            open.cam.pan_by(dx, dy);
            open.auto_fit = false;
            open.needs_fit = false;
        }
        self.save_settings();
    }

    /// The open board's view, for tests and the zoom readout.
    #[must_use]
    pub fn board_view(&self) -> Option<BoardView> {
        self.board_open.as_ref().map(|open| open.cam.view())
    }

    /// Board-only keys: `+`/`-` zoom, `0`/`F` fit, arrows pan. Kept on
    /// `State`, like the graph's, so they work without a window.
    pub fn board_key(&mut self, e: &KeyEvent) -> bool {
        if self.mode != Mode::Board
            || self.board_open.is_none()
            || e.mods.cmd
            || e.mods.ctrl
            || e.mods.alt
        {
            return false;
        }
        match &e.key {
            rune::Key::Left => self.pan_board(KEY_PAN, 0.0),
            rune::Key::Right => self.pan_board(-KEY_PAN, 0.0),
            rune::Key::Up => self.pan_board(0.0, KEY_PAN),
            rune::Key::Down => self.pan_board(0.0, -KEY_PAN),
            rune::Key::Text(t) => match t.as_str() {
                "+" | "=" => self.zoom_board(KEY_ZOOM),
                "-" | "_" => self.zoom_board(1.0 / KEY_ZOOM),
                "0" | "f" | "F" => self.fit_board(),
                _ => return false,
            },
            _ => return false,
        }
        true
    }

    fn drag_board(&mut self, drag: &Drag) {
        let Some(open) = &mut self.board_open else {
            return;
        };
        if drag.phase == Phase::Down {
            open.view_at_press = open.cam.view();
            open.auto_fit = false;
            open.needs_fit = false;
        }
        let mut view = open.view_at_press;
        if !drag.cancelled() {
            view.pan_by(drag.dx, drag.dy);
        }
        open.cam.set(view);
        if drag.phase == Phase::Up {
            self.save_settings();
        }
    }

    /// The selected board's summary, if it is still listed.
    #[must_use]
    pub fn selected_board(&self) -> Option<&BoardSummary> {
        let path = self.board_selected.as_ref()?;
        self.boards.iter().find(|b| &b.file.path == path)
    }
}

pub fn board(s: &State, k: &Kit) -> El {
    col()
        .key("board-page")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(header(s, k))
        .child(
            row()
                .key("board-body")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .gap(16.0)
                .px(28.0)
                .pb(24.0)
                .child(board_list(s, k))
                .child(surface(s, k)),
        )
}

fn header(s: &State, k: &Kit) -> El {
    row()
        .h(58.0)
        .px(28.0)
        .gap(10.0)
        .center_y()
        .child(text("Boards").font(k.ty.ui_medium).color(k.c.text_base))
        .child(
            text(plural(s.boards.len(), "board", "boards"))
                .key("board-count")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
        .child(spacer())
}

fn board_list(s: &State, k: &Kit) -> El {
    let mut list = col()
        .key("board-list")
        .w(240.0)
        .gap(4.0)
        .focus_group()
        .role("list");
    if s.boards.is_empty() {
        list = list.child(
            text("No boards in this vault yet")
                .key("board-list-empty")
                .font(k.ty.caption())
                .color(k.c.text_faint)
                .padding(8.0),
        );
    }
    for (i, b) in s.boards.iter().enumerate() {
        let on = s.board_selected.as_ref() == Some(&b.file.path);
        let path = b.file.path.clone();
        list = list.child(
            col()
                .key(format!("board-row-{i}"))
                .w_full()
                .px(10.0)
                .py(7.0)
                .gap(2.0)
                .radius(7.0)
                .fill(if on {
                    k.c.layer_03
                } else {
                    rune::Color::TRANSPARENT
                })
                .hover(k.c.hover)
                .cursor(Cursor::Pointer)
                .focusable(true)
                .focus_ring(2.0, 1.0, k.c.text_accent)
                .on_press_tx(move |s: &mut State, tx: &mut Tx| {
                    s.select_board(path.clone());
                    // Hand the keyboard to the canvas so arrows pan the board
                    // instead of moving through the list.
                    tx.blur();
                })
                .child(
                    text(b.file.title.clone())
                        .font(k.ty.ui_compact)
                        .color(k.c.text_base)
                        .truncate(),
                )
                .child(
                    text(b.detail())
                        .font(k.ty.caption())
                        .color(k.c.text_faint)
                        .truncate(),
                ),
        );
    }
    list
}

fn surface(s: &State, k: &Kit) -> El {
    let (title, detail) = match s.selected_board() {
        Some(b) => (
            b.file.title.clone(),
            format!("{} · {}", b.file.rel, b.detail()),
        ),
        None => (
            "Empty board".to_owned(),
            "Select a board to see what it holds".to_owned(),
        ),
    };
    let heading = row()
        .h(36.0)
        .gap(10.0)
        .center_y()
        .child(
            text(title)
                .key("board-surface-title")
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(
            text(detail)
                .key("board-surface-detail")
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .truncate(),
        );
    let body = match s.board_open.as_ref() {
        Some(open) => match &open.scene {
            Ok(scene) => board_canvas(s, k, open, scene),
            Err(e) => placeholder(k, &ic::DOT_GRID, "This board could not be read", e),
        },
        None => placeholder(
            k,
            &ic::DOT_GRID,
            "No board open",
            "Pick a board on the left. Boards are Excalidraw files in your vault.",
        ),
    };
    col()
        .key("board-surface")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .gap(6.0)
        .child(heading)
        .child(body)
}

fn placeholder(k: &Kit, glyph: &'static rune::Icon, title: &str, detail: &str) -> El {
    col()
        .key("board-placeholder")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .radius(10.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
        .center()
        .gap(8.0)
        .child(icon(glyph, 22.0).color(k.c.icon_muted))
        .child(
            text(title.to_owned())
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(
            text(detail.to_owned())
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
}

/// The read-only drawing: a world element sized to the board's content that
/// the camera offsets and scales, as the graph canvas does.
fn board_canvas(s: &State, k: &Kit, open: &OpenBoard, scene: &BoardScene) -> El {
    let mut world = el()
        .key("board-world")
        .size(scene.size[0], scene.size[1])
        .pin(Some(0.0), Some(0.0), None, None)
        .live_offset(&open.cam.live_offset)
        .live_scale(&open.cam.live_zoom)
        // Until the first fit lands the camera is not placed yet: draw nothing
        // rather than flash the board at the wrong spot.
        .opacity(if open.needs_fit { 0.0 } else { 1.0 })
        .child(
            canvas(scene.shape.clone())
                .size(scene.size[0], scene.size[1])
                .pin(Some(0.0), Some(0.0), None, None),
        );
    // Text is rasterized at its on-screen size; keep that bounded however far
    // the camera zooms in (see `MAX_SCREEN_FONT`).
    let zoom = open.cam.view().zoom.max(f32::MIN_POSITIVE);
    for (i, t) in scene.texts.iter().enumerate() {
        let font_size = t.font_size.min(crate::board_scene::MAX_SCREEN_FONT / zoom);
        let mut block =
            col()
                .key(format!("board-text-{i}"))
                .pin(Some(t.at[0]), Some(t.at[1]), None, None);
        for line in &t.lines {
            block = block.child(
                text(line.clone())
                    .font(TextStyle {
                        line_height: font_size * t.line_height,
                        ..TextStyle::new(font_size, 400.0)
                    })
                    .color(t.color),
            );
        }
        world = world.child(block);
    }
    let mut canvas_el = el()
        .key("board-canvas")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .radius(10.0)
        .clip_soft(10.0)
        .fill(scene.paper)
        .border(1.0, k.c.border_muted)
        .cursor(Cursor::Grab)
        .on_drawn_layout_tx(|s: &mut State, r: [f32; 4], tx: &mut Tx| {
            let Some(open) = &mut s.board_open else {
                return;
            };
            if open.shown == Shown::Not {
                open.shown = Shown::Drawn;
                tx.wake_after(std::time::Duration::from_millis(16));
            }
            let size = [r[2], r[3]];
            let resized =
                (open.canvas[0] - size[0]).abs() > 0.5 || (open.canvas[1] - size[1]).abs() > 0.5;
            if resized || open.needs_fit {
                open.canvas = size;
                if open.needs_fit || open.auto_fit {
                    open.fit();
                }
                tx.instant();
            }
        })
        .on_wheel(|s: &mut State, e: WheelEvent, _: &mut Tx| {
            let Some(open) = &mut s.board_open else {
                return false;
            };
            open.auto_fit = false;
            open.needs_fit = false;
            if e.mods.shift {
                // Rune's delta already moves content the fingers' way.
                open.cam.pan_by(e.dx, e.dy);
            } else {
                let delta = if e.dy.abs() > f32::EPSILON {
                    e.dy
                } else {
                    e.dx
                };
                let factor = if delta < 0.0 { WHEEL_IN } else { WHEEL_OUT };
                open.cam.zoom_at([e.x, e.y], factor);
            }
            // Not live-only: the zoom readout in the corner changes with it.
            true
        })
        .on_magnify(|s: &mut State, e: MagnifyEvent, _: &mut Tx| {
            let Some(open) = &mut s.board_open else {
                return false;
            };
            open.auto_fit = false;
            open.needs_fit = false;
            open.cam.zoom_at([e.x, e.y], 1.0 + e.delta);
            true
        })
        .pointer_buttons([PointerButton::Primary])
        .on_drag(|s: &mut State, d: &mut Drag, tx: &mut Tx| {
            s.drag_board(d);
            tx.live_only();
        })
        .on_double_press(|s: &mut State| s.fit_board())
        .child(world);
    if scene.bounds.is_none() {
        canvas_el = canvas_el.child(
            col()
                .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .center()
                .child(
                    text("Nothing drawn on this board yet")
                        .key("board-empty")
                        .font(k.ty.ui)
                        .color(Color {
                            r: 0.4,
                            g: 0.4,
                            b: 0.4,
                            a: 1.0,
                        }),
                ),
        );
    }
    canvas_el.child(board_controls(s, k)).child(board_hint(k))
}

fn board_controls(s: &State, k: &Kit) -> El {
    let zoom = s.board_view().map_or(1.0, |v| v.zoom);
    row()
        .pin(None, None, Some(12.0), Some(12.0))
        .h(30.0)
        .padding(3.0)
        .gap(2.0)
        .radius(7.0)
        .center_y()
        .fill(k.alpha(k.c.layer_02, 0.96))
        .border(1.0, k.c.border_base)
        .child(control(k, &ic::MINUS, "board-zoom-out", |s: &mut State| {
            s.zoom_board(1.0 / KEY_ZOOM);
        }))
        .child(
            text(format!("{:.0}%", zoom * 100.0))
                .key("board-zoom")
                .font(k.ty.small_medium)
                .color(k.c.text_muted)
                .w(46.0),
        )
        .child(control(k, &ic::PLUS, "board-zoom-in", |s: &mut State| {
            s.zoom_board(KEY_ZOOM);
        }))
        .child(control(k, &ic::EXPAND, "board-fit", |s: &mut State| {
            s.fit_board()
        }))
}

fn control(
    k: &Kit,
    glyph: &'static rune::Icon,
    key: &str,
    press: impl Fn(&mut State) + 'static,
) -> El {
    row()
        .key(key)
        .size(24.0, 24.0)
        .radius(5.0)
        .center()
        .hover(k.c.hover)
        .press(k.c.pressed)
        .cursor(Cursor::Pointer)
        .on_press(press)
        .child(icon(glyph, 12.0).color(k.c.icon_muted))
}

fn board_hint(k: &Kit) -> El {
    row()
        .pin(Some(12.0), None, None, Some(12.0))
        .h(26.0)
        .px(9.0)
        .radius(6.0)
        .center_y()
        .fill(k.alpha(k.c.bg_deep, 0.78))
        .child(
            text("Drag pan · Scroll zoom · +/- zoom · 0/F fit · Arrows pan · Read-only")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

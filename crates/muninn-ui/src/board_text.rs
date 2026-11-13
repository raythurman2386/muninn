//! Board slice 6: text on the Board canvas.
//!
//! - The text tool (`T`, `8`, or the toolbar's `T`) places text: a click on
//!   the canvas opens an editor there, and the tool goes back to selecting,
//!   as Excalidraw does when its tool lock is off.
//! - A double click on a text element (the first click selects it) edits it
//!   in place; the element is hidden while its editor is open.
//! - In the editor, Enter (and Shift+Enter) adds a line break. `Esc`, a click
//!   anywhere outside the editor, the editor losing focus (the sidebar, a
//!   dialog), switching mode or board, saving and quitting all commit. Committed empty text adds nothing, or deletes the element it
//!   was editing (a tombstone).
//! - A commit is one edit in the slice 5 history (`Board::add_text` /
//!   `Board::set_text`): it bumps `version`, `versionNonce` and `updated` and
//!   undoes and redoes like a move or a delete. The size written to the file
//!   is measured with the font the canvas draws text with: the widest line by
//!   `lines × fontSize × lineHeight`.
//! - While the editor has focus it owns the keyboard: [`State::is_typing`]
//!   is true, so no Board or Graph key and no Board chord fires, and `⌘Z` /
//!   `⌘⇧Z` inside it are the text field's own undo and redo of the typing
//!   (Rune's `TextEdit` history). Board undo applies to whole commits once the
//!   editor is closed.
//! - Text bound to a container (`containerId`, a label inside a shape):
//!   double-clicking the shape, or its label (whose click selects the
//!   shape), edits that label in place, as in Excalidraw. The label keeps
//!   its `containerId` and the shape its `boundElements`, and on commit the
//!   label is re-centred in the shape. A label inside a locked shape is
//!   locked with it. New text is always free-standing; creating a label for
//!   a shape that has none is not part of this slice.
//! - A crash while typing loses nothing: text typed but not committed counts
//!   as unsaved, and the board's recovery copy includes it (written once the
//!   typing rests), so Restore brings it back.
//! - The text size of a new element is Excalidraw's default 20; existing text
//!   keeps its `fontSize`, drawn and edited within the s3 clamp.

use muninn_core::{DEFAULT_BACKGROUND, Ink, ShapeKind, TEXT_FONT_SIZE, TEXT_LINE_HEIGHT};
use rune::{El, Key, KeyEvent, TextEdit, TextStyle, Tx, text_input};

use crate::State;
use crate::board_scene::{MAX_FONT_SIZE, MAX_SCREEN_FONT, MIN_FONT_SIZE};

/// The text element being written or edited.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardTextTarget {
    /// The element edited, `None` for new text.
    pub id: Option<String>,
    /// Scene position of the top-left corner.
    pub at: [f64; 2],
    pub font_size: f64,
    pub line_height: f64,
    pub color: rune::Color,
    /// The Excalidraw colour a new text element is written with (#69: the
    /// palette's stroke); existing text keeps its own.
    pub stroke: String,
    /// The text when the editor opened (empty for new text), so text being
    /// typed is known to be unsaved (#40 review M1).
    pub original: String,
    /// The shape this text is the bound label of (#40 review M2): the label
    /// is re-centred in it on commit.
    pub container: Option<String>,
}

/// Excalidraw's default stroke colour.
fn default_ink() -> rune::Color {
    crate::board_scene::color("#1e1e1e").unwrap_or(rune::Color::BLACK)
}

fn drawn_size(font_size: f64) -> f32 {
    let fs = font_size as f32;
    if fs.is_finite() && fs > 0.0 {
        fs.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
    } else {
        TEXT_FONT_SIZE as f32
    }
}

/// The laid-out width and height of `text` at `font_size` × `line_height`,
/// in scene units: the widest line, and one line height per line.
#[must_use]
pub fn measure_text(text: &str, font_size: f64, line_height: f64) -> [f64; 2] {
    let fs = drawn_size(font_size);
    let style = TextStyle {
        line_height: fs * line_height as f32,
        ..TextStyle::new(fs, 400.0)
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|l| rune::text::measure(l, &[], style, None).0)
        .fold(0.0f32, f32::max);
    let height = lines.len() as f64 * f64::from(fs) * line_height;
    [f64::from(width), height]
}

impl State {
    /// Open the editor for new text at scene point `at`.
    pub fn begin_board_text(&mut self, at: [f64; 2]) {
        self.commit_board_text();
        let Some(open) = &self.board_open else { return };
        if open.model.is_none() {
            return;
        }
        // #69: new text is written in the palette's stroke colour.
        let stroke = self.board_stroke;
        self.board_clear_selection();
        self.board_text = Some(BoardTextTarget {
            id: None,
            at,
            font_size: TEXT_FONT_SIZE,
            line_height: TEXT_LINE_HEIGHT,
            color: crate::board_scene::color(stroke).unwrap_or_else(default_ink),
            stroke: stroke.to_owned(),
            original: String::new(),
            container: None,
        });
        self.open_text_field(String::new());
    }

    /// Open the editor on the existing text element `id`. `false` when it
    /// is not an editable text element.
    pub fn edit_board_text(&mut self, id: &str) -> bool {
        self.commit_board_text();
        let Some(shape) = self
            .board_open
            .as_ref()
            .and_then(|o| o.model.as_ref())
            .and_then(|m| m.shape(id))
        else {
            return false;
        };
        if shape.kind != ShapeKind::Text || shape.is_deleted || shape.is_locked() {
            return false;
        }
        let body = shape.text.clone().unwrap_or(muninn_core::TextBody {
            text: String::new(),
            font_size: TEXT_FONT_SIZE,
        });
        let line_height = shape
            .extra
            .get("lineHeight")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(TEXT_LINE_HEIGHT);
        let color = crate::board_scene::color(&shape.stroke_color).unwrap_or_else(default_ink);
        self.board_text = Some(BoardTextTarget {
            id: Some(id.to_owned()),
            at: [shape.x, shape.y],
            font_size: body.font_size,
            line_height,
            color,
            stroke: shape.stroke_color.clone(),
            original: body.text.clone(),
            container: None,
        });
        self.board_clear_selection();
        self.open_text_field(body.text);
        true
    }

    fn open_text_field(&mut self, text: String) {
        self.board_tool = None;
        self.board_hand = false;
        if let Some(open) = &mut self.board_open {
            open.draft = None;
        }
        self.set_text_focus(false, false);
        let mut field = TextEdit::new(text);
        field.set_caret(field.text.len());
        field.focused = true;
        self.board_text_field = field;
    }

    /// Close the editor and apply its text as one undoable edit. `true`
    /// when an editor was open.
    pub fn commit_board_text(&mut self) -> bool {
        let Some(target) = self.board_text.take() else {
            return false;
        };
        let text = std::mem::take(&mut self.board_text_field.text);
        self.board_text_field = TextEdit::default();
        let size = measure_text(&text, target.font_size, target.line_height);
        let Some(open) = &mut self.board_open else {
            return true;
        };
        let Some(model) = &mut open.model else {
            return true;
        };
        let now = crate::board_tools::now_ms();
        let edit = apply_text(model, &target, &text, size, &mut self.board_ids, now);
        if !open.record(edit) {
            // Nothing changed: show the hidden element again.
            open.rebuild();
        }
        open.prune_selection();
        true
    }

    /// A double click with one element selected: a text element is edited,
    /// and a shape with a bound label (the box, or the label itself, which
    /// selects its box) edits that label, as in Excalidraw (#40 review M2).
    pub(crate) fn board_double_press(&mut self) -> bool {
        let one = match self.board_selection() {
            [id] => id.clone(),
            _ => return false,
        };
        let label = self
            .board_open
            .as_ref()
            .and_then(|o| o.model.as_ref())
            .and_then(|m| m.bound_label(&one))
            .map(|s| s.id.clone());
        match label {
            Some(label) => {
                if !self.edit_board_text(&label) {
                    return false;
                }
                if let Some(t) = &mut self.board_text {
                    t.container = Some(one);
                }
                true
            }
            None => self.edit_board_text(&one),
        }
    }

    /// The editor's text when it differs from what the element held: typed
    /// but not committed, so it is unsaved (#40 review M1).
    pub(crate) fn board_text_pending(&self) -> Option<String> {
        let target = self.board_text.as_ref()?;
        let text = &self.board_text_field.text;
        (*text != target.original).then(|| text.clone())
    }

    /// The open board's JSON with the text being typed applied, as a commit
    /// would apply it (on a copy: the drawing and its history are unchanged).
    pub(crate) fn board_json_with_pending_text(&self) -> Option<String> {
        let model = self.board_open.as_ref()?.model.as_ref()?;
        let (Some(target), Some(text)) = (self.board_text.as_ref(), self.board_text_pending())
        else {
            return Some(model.to_json());
        };
        let mut copy = model.clone();
        let mut ids = self.board_ids.clone();
        let size = measure_text(&text, target.font_size, target.line_height);
        let _ = apply_text(
            &mut copy,
            target,
            &text,
            size,
            &mut ids,
            crate::board_tools::now_ms(),
        );
        Some(copy.to_json())
    }
}

/// Apply editor text to `model` as one edit: new text, an edited element,
/// or a bound label re-centred in its container.
fn apply_text(
    model: &mut muninn_core::Board,
    target: &BoardTextTarget,
    text: &str,
    size: [f64; 2],
    ids: &mut muninn_core::IdSource,
    now: i64,
) -> Vec<muninn_core::Change> {
    match &target.id {
        None => model
            .add_text_with(
                target.at,
                text,
                size,
                Ink {
                    stroke: &target.stroke,
                    background: DEFAULT_BACKGROUND,
                },
                ids,
                now,
            )
            .into_iter()
            .collect(),
        Some(id) => {
            let centre = target
                .container
                .as_ref()
                .and_then(|c| model.shape(c))
                .map(|c| {
                    let b = c.own_box();
                    [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0]
                });
            match centre {
                Some(centre) => model.set_label_text(id, text, size, centre, ids, now),
                None => model.set_text(id, text, size, ids, now),
            }
        }
    }
}

/// The in-canvas editor, pinned over where the text goes.
pub(crate) fn editor(s: &State, view: crate::BoardView) -> Option<El> {
    let t = s.board_text.as_ref()?;
    let zoom = view.zoom.max(f32::MIN_POSITIVE);
    let fs = (drawn_size(t.font_size) * zoom).min(MAX_SCREEN_FONT);
    let p = view.screen([t.at[0] as f32, t.at[1] as f32]);
    let size = measure_text(&s.board_text_field.text, t.font_size, t.line_height);
    let style = TextStyle {
        line_height: fs * t.line_height as f32,
        ..TextStyle::new(fs, 400.0)
    };
    Some(
        text_input(&s.board_text_field, style)
            .key("board-text-editor")
            .pin(Some(p[0]), Some(p[1]), None, None)
            .w(size[0] as f32 * zoom + fs)
            .min_h(fs * t.line_height as f32)
            .color(t.color)
            .caret(
                t.color,
                crate::board_scene::color("#6965db")
                    .unwrap_or(t.color)
                    .alpha(0.25),
            )
            .on_edit(|s: &mut State| &mut s.board_text_field)
            // Enter breaks the line; Esc (or a click outside) commits.
            .on_submit(|s: &mut State, _| s.board_text_field.insert("\n"))
            .on_input_key(|s: &mut State, e: &KeyEvent, tx: &mut Tx| {
                if e.key == Key::Escape {
                    s.commit_board_text();
                    tx.blur();
                    return true;
                }
                false
            }),
    )
}

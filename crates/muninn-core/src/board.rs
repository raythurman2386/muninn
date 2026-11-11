//! Whiteboards stored as Excalidraw `.excalidraw` files.
//!
//! A board is a vault file, not a note: only `.md` files are notes, so boards
//! never enter the note index, the link graph or search. The file is the
//! source of truth. [`Board::load`] parses it and [`Board::save`] writes it
//! back through the same hidden temp + fsync + rename path as note saves, so a
//! crash leaves either the old board or the new one, never a torn file.
//!
//! # Format
//!
//! The file is Excalidraw's v2 scene JSON:
//!
//! ```json
//! { "type": "excalidraw", "version": 2, "source": "…",
//!   "elements": [ … ], "appState": { … }, "files": { … } }
//! ```
//!
//! Muninn models a pragmatic subset. The element kinds in [`ShapeKind`]
//! (rectangle, ellipse, diamond, line, arrow, freedraw, text) get typed
//! fields: `id`, `x`, `y`, `width`, `height`, `angle`, `strokeColor`,
//! `backgroundColor`, `strokeWidth`, `opacity`, `isDeleted`, `version`,
//! `versionNonce`, `seed`, plus `points` for line/arrow/freedraw and `text`
//! and `fontSize` for text.
//!
//! # Preservation
//!
//! Nothing Excalidraw wrote is dropped on a round-trip:
//!
//! - Every element field Muninn does not model (`roughness`, `groupIds`,
//!   `boundElements`, `roundness`, `pressures`, `fontFamily`, …) is kept in
//!   [`Shape::extra`] and written back.
//! - An element of an unknown type (`image`, `frame`, `embeddable`, a future
//!   kind) is kept verbatim as [`Element::Raw`]. So is a known-type element
//!   whose modelled fields are missing (`id`, `x`, `y`) or have an unexpected
//!   JSON type: Muninn will not rewrite a value it cannot represent.
//! - `appState`, `files` and unknown top-level keys are kept as JSON.
//!
//! Two normalisations remain, both data-preserving. Object keys are written in
//! sorted order (JSON objects are unordered; Excalidraw does not care), and a
//! modelled field that was absent is written with Excalidraw's default (for
//! example `"angle": 0`), as Excalidraw's own `restore` fills it on load.
//! Integral numbers are written as integers (`20`, not `20.0`), the way
//! JavaScript's `JSON.stringify` writes them.
//!
//! Obsidian's Excalidraw plugin uses `.excalidraw.md` (Markdown wrapping
//! compressed JSON). Those files end in `.md`, so they stay notes and are not
//! boards.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Number, Value};

use crate::vault::{Vault, atomic_write, sort_paths, walk};

mod create;
mod edit;
mod text;
pub use create::{IdSource, index_after};
pub use edit::{Change, HISTORY_BYTES, HISTORY_LIMIT, History};
pub use text::{TEXT_FONT_FAMILY, TEXT_FONT_SIZE, TEXT_LINE_HEIGHT};

/// File extension of a board, without the dot.
pub const BOARD_EXTENSION: &str = "excalidraw";

/// The only scene `type` Muninn reads or writes.
const SCENE_TYPE: &str = "excalidraw";

/// The scene schema version Muninn writes for a new board.
pub const SCENE_VERSION: u64 = 2;

/// `source` written into a board Muninn creates. Excalidraw writes its page
/// origin here; Muninn has none, and the value is informational only.
pub const MUNINN_SOURCE: &str = "muninn";

/// One `.excalidraw` scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    /// Scene schema version (`2` for every current Excalidraw export).
    pub version: u64,
    /// The app or page that wrote the scene.
    pub source: String,
    /// Elements in z-order (first is drawn first), including deleted ones.
    pub elements: Vec<Element>,
    /// Excalidraw's saved view state (`viewBackgroundColor`, `gridSize`, …).
    pub app_state: Map<String, Value>,
    /// Binary files (images) by file id, as Excalidraw stores them.
    pub files: Map<String, Value>,
    /// Top-level keys Muninn does not model, written back unchanged.
    pub extra: Map<String, Value>,
}

/// One element of a scene.
#[derive(Clone, Debug, PartialEq)]
pub enum Element {
    /// A kind Muninn models, with typed fields. Boxed: a shape is several
    /// times the size of a raw JSON value.
    Shape(Box<Shape>),
    /// Any other element, kept as the exact JSON value that was read.
    Raw(Value),
}

/// The element kinds Muninn draws and edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Diamond,
    Line,
    Arrow,
    Freedraw,
    Text,
}

impl ShapeKind {
    /// Every modelled kind.
    pub const ALL: [ShapeKind; 7] = [
        ShapeKind::Rectangle,
        ShapeKind::Ellipse,
        ShapeKind::Diamond,
        ShapeKind::Line,
        ShapeKind::Arrow,
        ShapeKind::Freedraw,
        ShapeKind::Text,
    ];

    /// The Excalidraw `type` string.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ShapeKind::Rectangle => "rectangle",
            ShapeKind::Ellipse => "ellipse",
            ShapeKind::Diamond => "diamond",
            ShapeKind::Line => "line",
            ShapeKind::Arrow => "arrow",
            ShapeKind::Freedraw => "freedraw",
            ShapeKind::Text => "text",
        }
    }

    /// The kind for an Excalidraw `type`, or `None` for an unmodelled one.
    #[must_use]
    pub fn parse(name: &str) -> Option<ShapeKind> {
        ShapeKind::ALL.into_iter().find(|k| k.as_str() == name)
    }

    /// Line, arrow and freedraw carry `points`.
    #[must_use]
    pub fn has_points(self) -> bool {
        matches!(
            self,
            ShapeKind::Line | ShapeKind::Arrow | ShapeKind::Freedraw
        )
    }
}

/// A modelled element. Coordinates are Excalidraw scene units; `points` are
/// relative to (`x`, `y`).
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub kind: ShapeKind,
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Rotation in radians, clockwise, about the element's centre.
    pub angle: f64,
    pub stroke_color: String,
    pub background_color: String,
    pub stroke_width: f64,
    /// 0–100.
    pub opacity: f64,
    /// Polyline vertices for line, arrow and freedraw. Empty (and not
    /// written) for the other kinds.
    pub points: Vec<[f64; 2]>,
    /// The text and font size of a text element; `None` for other kinds.
    pub text: Option<TextBody>,
    /// Excalidraw keeps deleted elements (tombstones) for collaboration.
    pub is_deleted: bool,
    pub version: i64,
    pub version_nonce: i64,
    pub seed: i64,
    /// Every other field of the element, written back unchanged.
    pub extra: Map<String, Value>,
}

/// The modelled part of a text element.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBody {
    pub text: String,
    pub font_size: f64,
}

// Defaults match Excalidraw's `restore` for a missing field.
const DEFAULT_STROKE: &str = "#1e1e1e";
const DEFAULT_BACKGROUND: &str = "transparent";
const DEFAULT_STROKE_WIDTH: f64 = 2.0;
const DEFAULT_OPACITY: f64 = 100.0;
const DEFAULT_FONT_SIZE: f64 = 20.0;

/// Element keys with typed fields on [`Shape`]; everything else is `extra`.
const MODELLED: [&str; 18] = [
    "type",
    "id",
    "x",
    "y",
    "width",
    "height",
    "angle",
    "strokeColor",
    "backgroundColor",
    "strokeWidth",
    "opacity",
    "points",
    "text",
    "fontSize",
    "isDeleted",
    "version",
    "versionNonce",
    "seed",
];

impl Shape {
    /// A new element with Excalidraw's defaults. `points` and `text` start
    /// empty; set them for the kinds that use them.
    #[must_use]
    pub fn new(kind: ShapeKind, id: impl Into<String>, x: f64, y: f64) -> Shape {
        Shape {
            kind,
            id: id.into(),
            x,
            y,
            width: 0.0,
            height: 0.0,
            angle: 0.0,
            stroke_color: DEFAULT_STROKE.to_owned(),
            background_color: DEFAULT_BACKGROUND.to_owned(),
            stroke_width: DEFAULT_STROKE_WIDTH,
            opacity: DEFAULT_OPACITY,
            points: Vec::new(),
            text: (kind == ShapeKind::Text).then(|| TextBody {
                text: String::new(),
                font_size: DEFAULT_FONT_SIZE,
            }),
            is_deleted: false,
            version: 1,
            version_nonce: 0,
            seed: 1,
            extra: Map::new(),
        }
    }

    /// Parse a modelled element, or `None` when `value` must stay raw: an
    /// unknown `type`, a missing `id`/`x`/`y`, or a modelled field of an
    /// unexpected JSON type.
    fn from_value(value: &Value) -> Option<Shape> {
        let obj = value.as_object()?;
        let kind = ShapeKind::parse(obj.get("type")?.as_str()?)?;
        let id = obj.get("id")?.as_str()?.to_owned();
        let x = exact_f64(obj.get("x")?)?;
        let y = exact_f64(obj.get("y")?)?;
        let mut shape = Shape::new(kind, id, x, y);
        shape.width = opt_f64(obj, "width")?.unwrap_or(0.0);
        shape.height = opt_f64(obj, "height")?.unwrap_or(0.0);
        shape.angle = opt_f64(obj, "angle")?.unwrap_or(0.0);
        if let Some(c) = opt_str(obj, "strokeColor")? {
            shape.stroke_color = c;
        }
        if let Some(c) = opt_str(obj, "backgroundColor")? {
            shape.background_color = c;
        }
        shape.stroke_width = opt_f64(obj, "strokeWidth")?.unwrap_or(DEFAULT_STROKE_WIDTH);
        shape.opacity = opt_f64(obj, "opacity")?.unwrap_or(DEFAULT_OPACITY);
        shape.is_deleted = match obj.get("isDeleted") {
            None => false,
            Some(v) => v.as_bool()?,
        };
        shape.version = opt_i64(obj, "version")?.unwrap_or(1);
        shape.version_nonce = opt_i64(obj, "versionNonce")?.unwrap_or(0);
        shape.seed = opt_i64(obj, "seed")?.unwrap_or(1);

        let mut extra = obj.clone();
        for key in MODELLED {
            extra.remove(key);
        }
        if kind.has_points() {
            shape.points = match obj.get("points") {
                None => Vec::new(),
                Some(v) => parse_points(v)?,
            };
        } else if let Some(points) = obj.get("points") {
            // A rectangle with points is unusual but not ours to drop.
            extra.insert("points".into(), points.clone());
        }
        if kind == ShapeKind::Text {
            shape.text = Some(TextBody {
                text: opt_str(obj, "text")?.unwrap_or_default(),
                font_size: opt_f64(obj, "fontSize")?.unwrap_or(DEFAULT_FONT_SIZE),
            });
        } else {
            for key in ["text", "fontSize"] {
                if let Some(v) = obj.get(key) {
                    extra.insert(key.into(), v.clone());
                }
            }
        }
        shape.extra = extra;
        Some(shape)
    }

    fn to_value(&self) -> Value {
        let mut obj = self.extra.clone();
        let mut put = |key: &str, value: Value| {
            obj.insert(key.to_owned(), value);
        };
        put("type", Value::from(self.kind.as_str()));
        put("id", Value::from(self.id.clone()));
        put("x", num(self.x));
        put("y", num(self.y));
        put("width", num(self.width));
        put("height", num(self.height));
        put("angle", num(self.angle));
        put("strokeColor", Value::from(self.stroke_color.clone()));
        put(
            "backgroundColor",
            Value::from(self.background_color.clone()),
        );
        put("strokeWidth", num(self.stroke_width));
        put("opacity", num(self.opacity));
        put("isDeleted", Value::from(self.is_deleted));
        put("version", Value::from(self.version));
        put("versionNonce", Value::from(self.version_nonce));
        put("seed", Value::from(self.seed));
        if self.kind.has_points() {
            let points = self
                .points
                .iter()
                .map(|[px, py]| Value::Array(vec![num(*px), num(*py)]))
                .collect();
            put("points", Value::Array(points));
        }
        if let Some(text) = &self.text {
            put("text", Value::from(text.text.clone()));
            put("fontSize", num(text.font_size));
        }
        Value::Object(obj)
    }
}

impl Element {
    /// Read one scene element: modelled when possible, raw otherwise.
    #[must_use]
    pub fn from_value(value: Value) -> Element {
        match Shape::from_value(&value) {
            Some(shape) => Element::Shape(Box::new(shape)),
            None => Element::Raw(value),
        }
    }

    /// The element as scene JSON.
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Element::Shape(shape) => shape.to_value(),
            Element::Raw(value) => value.clone(),
        }
    }

    /// The modelled shape, or `None` for a raw element.
    #[must_use]
    pub fn shape(&self) -> Option<&Shape> {
        match self {
            Element::Shape(shape) => Some(shape),
            Element::Raw(_) => None,
        }
    }

    /// The element's `id`, when it has a string one.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        match self {
            Element::Shape(shape) => Some(&shape.id),
            Element::Raw(value) => value.get("id").and_then(Value::as_str),
        }
    }

    /// True for a tombstone (`"isDeleted": true`), which is kept in the file
    /// but not drawn.
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        match self {
            Element::Shape(shape) => shape.is_deleted,
            Element::Raw(value) => value.get("isDeleted").and_then(Value::as_bool) == Some(true),
        }
    }
}

impl From<Shape> for Element {
    fn from(shape: Shape) -> Element {
        Element::Shape(Box::new(shape))
    }
}

impl Default for Board {
    fn default() -> Board {
        Board::new()
    }
}

impl Board {
    /// An empty scene that Excalidraw opens as a blank canvas.
    #[must_use]
    pub fn new() -> Board {
        Board {
            version: SCENE_VERSION,
            source: MUNINN_SOURCE.to_owned(),
            elements: Vec::new(),
            app_state: Map::new(),
            files: Map::new(),
            extra: Map::new(),
        }
    }

    /// Parse scene JSON. Anything that is not an Excalidraw scene is an
    /// `InvalidData` error rather than an empty board, so a caller can never
    /// overwrite a file it failed to understand.
    pub fn from_json(json: &str) -> io::Result<Board> {
        let value: Value = serde_json::from_str(json).map_err(invalid)?;
        let Value::Object(mut obj) = value else {
            return Err(invalid("a board must be a JSON object"));
        };
        match obj.remove("type") {
            Some(Value::String(t)) if t == SCENE_TYPE => {}
            _ => {
                return Err(invalid(
                    "not an Excalidraw scene (type is not \"excalidraw\")",
                ));
            }
        }
        let version = match obj.remove("version") {
            None => SCENE_VERSION,
            Some(v) => v
                .as_u64()
                .ok_or_else(|| invalid("scene version must be a whole number"))?,
        };
        let source = match obj.remove("source") {
            None => String::new(),
            Some(Value::String(s)) => s,
            Some(_) => return Err(invalid("scene source must be a string")),
        };
        let elements = match obj.remove("elements") {
            None => Vec::new(),
            Some(Value::Array(items)) => items.into_iter().map(Element::from_value).collect(),
            Some(_) => return Err(invalid("scene elements must be an array")),
        };
        let app_state = take_object(&mut obj, "appState")?;
        let files = take_object(&mut obj, "files")?;
        Ok(Board {
            version,
            source,
            elements,
            app_state,
            files,
            extra: obj,
        })
    }

    /// The scene as a JSON value.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut obj = self.extra.clone();
        obj.insert("type".into(), Value::from(SCENE_TYPE));
        obj.insert("version".into(), Value::from(self.version));
        obj.insert("source".into(), Value::from(self.source.clone()));
        obj.insert(
            "elements".into(),
            Value::Array(self.elements.iter().map(Element::to_value).collect()),
        );
        obj.insert("appState".into(), Value::Object(self.app_state.clone()));
        obj.insert("files".into(), Value::Object(self.files.clone()));
        Value::Object(obj)
    }

    /// The scene as pretty JSON with two-space indents, like Excalidraw's
    /// "Save to disk".
    #[must_use]
    pub fn to_json(&self) -> String {
        // Serialising a `Value` built from finite numbers and strings cannot fail.
        serde_json::to_string_pretty(&self.to_value()).unwrap_or_default()
    }

    /// Read and parse a board file.
    pub fn load(path: impl AsRef<Path>) -> io::Result<Board> {
        Board::from_json(&fs::read_to_string(path)?)
    }

    /// Write the board atomically (hidden temp file, fsync, rename), replacing
    /// any existing file at `path`.
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        atomic_write(path.as_ref(), self.to_json().as_bytes())
    }

    /// Elements that are drawn: everything except tombstones.
    pub fn live_elements(&self) -> impl Iterator<Item = &Element> {
        self.elements.iter().filter(|e| !e.is_deleted())
    }
}

/// A board file found in a vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardFile {
    pub path: PathBuf,
    /// The path relative to the vault root, with `/` separators.
    pub rel: String,
    /// The file name without `.excalidraw`.
    pub title: String,
}

impl BoardFile {
    /// Vault-relative folder, or empty at the vault root.
    #[must_use]
    pub fn folder(&self) -> &str {
        match self.rel.rfind('/') {
            Some(at) => &self.rel[..at],
            None => "",
        }
    }
}

impl Vault {
    /// Every `.excalidraw` board under the vault, read fresh from disk.
    ///
    /// The walk is the note scan's own walk: the same 12-level depth limit,
    /// dot-directories and dot-files (including in-flight atomic-write temp
    /// files and `.muninn-trash`) skipped, symlinks not followed, and the same
    /// case-insensitive path order. Boards are never notes, so they are not
    /// cached on the vault or added to the link index.
    #[must_use]
    pub fn boards(&self) -> Vec<BoardFile> {
        list_boards(&self.root)
    }
}

/// [`Vault::boards`] for a folder that has not been loaded as a vault.
#[must_use]
pub fn list_boards(root: &Path) -> Vec<BoardFile> {
    let mut paths = Vec::new();
    let mut dirs = Vec::new();
    walk(root, &mut paths, &mut dirs, root, 0, BOARD_EXTENSION);
    sort_paths(&mut paths);
    paths
        .into_iter()
        .filter_map(|path| {
            let title = path.file_stem()?.to_str()?.to_owned();
            if title.is_empty() {
                return None;
            }
            let rel = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            Some(BoardFile { path, rel, title })
        })
        .collect()
}

/// A number the way `JSON.stringify` writes it: integral values in the
/// exactly representable range as integers, others as shortest round-trip
/// floats. Non-finite values have no JSON form and become 0.
fn num(v: f64) -> Value {
    const SAFE: f64 = 9_007_199_254_740_991.0; // 2^53 - 1
    if v.is_finite() && v.fract() == 0.0 && v.abs() <= SAFE {
        // In range and integral, so the cast is exact.
        return Value::from(v as i64);
    }
    Number::from_f64(v).map_or_else(|| Value::from(0), Value::Number)
}

/// `Ok(None)` when absent, `None` (keep raw) when present with the wrong type.
fn opt_f64(obj: &Map<String, Value>, key: &str) -> Option<Option<f64>> {
    match obj.get(key) {
        None => Some(None),
        Some(v) => exact_f64(v).map(Some),
    }
}

/// A JSON number as `f64`, or `None` for a non-number or an integer an `f64`
/// cannot hold exactly (it would be written back as a different number).
fn exact_f64(v: &Value) -> Option<f64> {
    const SAFE: u64 = 9_007_199_254_740_991; // 2^53 - 1
    if let Some(i) = v.as_i64() {
        return (i.unsigned_abs() <= SAFE).then_some(i as f64);
    }
    if v.is_u64() {
        return None;
    }
    v.as_f64()
}

fn opt_i64(obj: &Map<String, Value>, key: &str) -> Option<Option<i64>> {
    match obj.get(key) {
        None => Some(None),
        Some(v) => v.as_i64().map(Some),
    }
}

fn opt_str(obj: &Map<String, Value>, key: &str) -> Option<Option<String>> {
    match obj.get(key) {
        None => Some(None),
        Some(v) => v.as_str().map(|s| Some(s.to_owned())),
    }
}

fn parse_points(value: &Value) -> Option<Vec<[f64; 2]>> {
    value
        .as_array()?
        .iter()
        .map(|p| match p.as_array()?.as_slice() {
            [x, y] => Some([x.as_f64()?, y.as_f64()?]),
            _ => None,
        })
        .collect()
}

fn take_object(obj: &mut Map<String, Value>, key: &str) -> io::Result<Map<String, Value>> {
    match obj.remove(key) {
        None | Some(Value::Null) => Ok(Map::new()),
        Some(Value::Object(map)) => Ok(map),
        Some(_) => Err(invalid(format!("scene {key} must be an object"))),
    }
}

fn invalid(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests;

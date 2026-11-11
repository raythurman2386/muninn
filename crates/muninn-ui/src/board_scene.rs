//! A loaded board as a static drawing (Board slice 3).
//!
//! This turns a [`muninn_core::Board`] into what the canvas paints: one Rune
//! [`Shape`] holding every stroke and fill in z-order, the text elements as
//! positioned labels, and the content's bounding box for fits. It is
//! read-only and best-effort; editing arrives in later slices:
//!
//! - rectangles (rounded when `roundness` is set), diamonds and ellipses,
//!   stroked and, unless the background is `transparent`, filled; hachure and
//!   cross-hatch fills are drawn as a light solid fill
//! - lines and arrows as straight polylines with simple arrowheads (curved
//!   lines are not smoothed), a closed line filled like Excalidraw does
//! - freehand strokes as round-capped polylines of their points (no pressure)
//! - text as left-aligned lines at the element's font size and colour
//! - rotation (`angle`) for every kind except text
//! - an element Muninn does not model (image, frame, embed) as a faint
//!   outline of its box, so nothing is silently missing
//! - dashed and dotted strokes, fonts and roughness are drawn solid and clean
//!
//! Colours follow Excalidraw's light theme on the board's own
//! `viewBackgroundColor`, so a board reads the way it does on excalidraw.com.

use muninn_core::{Board, Element, Shape as Item, ShapeKind};
use rune::color::hex;
use rune::{Color, LineCap, Shape};
use serde_json::Value;

/// Space around the content inside the world element, in scene units, so
/// strokes on the bounding box are not cut at the element's edge.
const PAD: f32 = 32.0;
/// Excalidraw's default ink and paper.
const INK: &str = "#1e1e1e";
const PAPER: &str = "#ffffff";

/// What the board canvas paints for one board.
#[derive(Clone, Debug)]
pub struct BoardScene {
    /// Scene point at the world element's top-left corner.
    pub origin: [f32; 2],
    /// World element size, in scene units.
    pub size: [f32; 2],
    /// Content bounds (min x, min y, max x, max y) in scene units, or `None`
    /// when nothing is drawn.
    pub bounds: Option<[f32; 4]>,
    /// The board's `viewBackgroundColor`.
    pub paper: Color,
    /// Strokes and fills in world-element coordinates, in z-order.
    pub shape: Shape,
    pub texts: Vec<SceneText>,
    /// Live elements drawn as themselves.
    pub drawn: usize,
    /// Live elements Muninn does not model, drawn as an outline.
    pub outlined: usize,
}

/// The smallest text size drawn, in scene units.
pub const MIN_FONT_SIZE: f32 = 1.0;
/// The largest text size drawn, in scene units. Excalidraw's own sizes stop
/// at a few hundred; a file asking for more is drawn at this size instead of
/// handing the rasterizer a glyph larger than it can allocate.
pub const MAX_FONT_SIZE: f32 = 2048.0;
/// The largest on-screen text size, in points, at the current zoom. A board
/// zoomed far into large text draws it at this size: a glyph mask scales with
/// the square of its size, and one tens of thousands of pixels wide panics the
/// glyph rasterizer.
pub const MAX_SCREEN_FONT: f32 = 1024.0;

/// One text element, in world-element coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneText {
    pub at: [f32; 2],
    pub lines: Vec<String>,
    pub font_size: f32,
    pub line_height: f32,
    pub color: Color,
}

/// A paintable piece before it is placed in the world element.
enum Piece {
    Fill(Vec<[f32; 2]>, Color),
    Stroke {
        points: Vec<[f32; 2]>,
        closed: bool,
        width: f32,
        cap: LineCap,
        color: Color,
    },
    Text {
        at: [f32; 2],
        text: SceneText,
    },
}

impl BoardScene {
    #[must_use]
    pub fn build(board: &Board) -> BoardScene {
        let mut pieces = Vec::new();
        let mut bounds: Option<[f32; 4]> = None;
        let (mut drawn, mut outlined) = (0, 0);
        for element in board.live_elements() {
            let (box_, mut parts) = match element {
                Element::Shape(item) => {
                    drawn += 1;
                    (item_bounds(item), item_pieces(item))
                }
                Element::Raw(value) => {
                    let Some(b) = raw_bounds(value) else { continue };
                    outlined += 1;
                    (Some(b), raw_pieces(b))
                }
            };
            if let Some(b) = box_ {
                bounds = Some(match bounds {
                    None => b,
                    Some(a) => [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[2].max(b[2]),
                        a[3].max(b[3]),
                    ],
                });
            }
            pieces.append(&mut parts);
        }
        let b = bounds.unwrap_or([0.0, 0.0, 0.0, 0.0]);
        let origin = [b[0] - PAD, b[1] - PAD];
        let size = [b[2] - b[0] + PAD * 2.0, b[3] - b[1] + PAD * 2.0];
        let local = |p: [f32; 2]| [p[0] - origin[0], p[1] - origin[1]];
        let mut shape = Shape::new();
        let mut texts = Vec::new();
        for piece in pieces {
            match piece {
                Piece::Fill(points, color) => {
                    shape = shape.fill(points.into_iter().map(local).collect::<Vec<_>>(), color);
                }
                Piece::Stroke {
                    points,
                    closed,
                    width,
                    cap,
                    color,
                } => {
                    let points: Vec<[f32; 2]> = points.into_iter().map(local).collect();
                    shape = if points.len() == 1 {
                        // A single-point stroke (a freehand dot) still shows.
                        let p = points[0];
                        shape.line(p, [p[0] + 0.01, p[1]], width, LineCap::Round, color)
                    } else {
                        shape.stroke_with(points, closed, width, cap, color)
                    };
                }
                Piece::Text { at, mut text } => {
                    text.at = local(at);
                    texts.push(text);
                }
            }
        }
        let paper = board
            .app_state
            .get("viewBackgroundColor")
            .and_then(Value::as_str)
            .and_then(color)
            .unwrap_or_else(|| color(PAPER).unwrap_or(Color::WHITE));
        BoardScene {
            origin,
            size,
            bounds,
            paper,
            shape,
            texts,
            drawn,
            outlined,
        }
    }
}

/// An Excalidraw colour: `#rgb`, `#rrggbb` or `#rrggbbaa`; `None` for
/// `transparent` or anything unreadable.
fn color(value: &str) -> Option<Color> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("transparent") {
        return None;
    }
    hex(value)
}

fn f(v: f64) -> f32 {
    v as f32
}

/// The element's own box (before rotation): scene x, y, width, height.
fn item_box(item: &Item) -> [f32; 4] {
    if item.kind.has_points() && !item.points.is_empty() {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &item.points {
            x0 = x0.min(f(p[0]));
            y0 = y0.min(f(p[1]));
            x1 = x1.max(f(p[0]));
            y1 = y1.max(f(p[1]));
        }
        [f(item.x) + x0, f(item.y) + y0, x1 - x0, y1 - y0]
    } else {
        [
            f(item.x),
            f(item.y),
            f(item.width).max(0.0),
            f(item.height).max(0.0),
        ]
    }
}

fn rotate(points: &mut [[f32; 2]], centre: [f32; 2], angle: f32) {
    if angle == 0.0 || !angle.is_finite() {
        return;
    }
    let (sin, cos) = angle.sin_cos();
    for p in points {
        let (dx, dy) = (p[0] - centre[0], p[1] - centre[1]);
        *p = [
            centre[0] + dx * cos - dy * sin,
            centre[1] + dx * sin + dy * cos,
        ];
    }
}

fn item_bounds(item: &Item) -> Option<[f32; 4]> {
    let b = item_box(item);
    if !b.iter().all(|v| v.is_finite()) {
        return None;
    }
    let mut corners = [
        [b[0], b[1]],
        [b[0] + b[2], b[1]],
        [b[0] + b[2], b[1] + b[3]],
        [b[0], b[1] + b[3]],
    ];
    if item.kind != ShapeKind::Text {
        rotate(
            &mut corners,
            [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0],
            f(item.angle),
        );
    }
    let xs = corners.map(|p| p[0]);
    let ys = corners.map(|p| p[1]);
    Some([
        xs.iter().copied().fold(f32::MAX, f32::min),
        ys.iter().copied().fold(f32::MAX, f32::min),
        xs.iter().copied().fold(f32::MIN, f32::max),
        ys.iter().copied().fold(f32::MIN, f32::max),
    ])
}

/// Ink and fill for an element, with its opacity applied.
fn paints(item: &Item) -> (Color, Option<Color>) {
    let opacity = (f(item.opacity) / 100.0).clamp(0.0, 1.0);
    let ink = color(&item.stroke_color)
        .or_else(|| {
            (!item.stroke_color.eq_ignore_ascii_case("transparent"))
                .then(|| color(INK))
                .flatten()
        })
        .map_or(Color::TRANSPARENT, |c| c.alpha(c.a * opacity));
    let solid = item
        .extra
        .get("fillStyle")
        .and_then(Value::as_str)
        .is_none_or(|s| s == "solid");
    let fill = color(&item.background_color)
        .map(|c| c.alpha(c.a * opacity * if solid { 1.0 } else { 0.35 }));
    (ink, fill)
}

fn width(item: &Item) -> f32 {
    let w = f(item.stroke_width);
    if w.is_finite() && w > 0.0 { w } else { 1.0 }
}

fn item_pieces(item: &Item) -> Vec<Piece> {
    let b = item_box(item);
    if !b.iter().all(|v| v.is_finite()) {
        return Vec::new();
    }
    let centre = [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0];
    let angle = f(item.angle);
    let (ink, fill) = paints(item);
    let stroke = |points: Vec<[f32; 2]>, closed: bool| Piece::Stroke {
        points,
        closed,
        width: width(item),
        cap: LineCap::Round,
        color: ink,
    };
    let mut out = Vec::new();
    let outline = |mut points: Vec<[f32; 2]>, out: &mut Vec<Piece>| {
        rotate(&mut points, centre, angle);
        if let Some(fill) = fill {
            out.push(Piece::Fill(points.clone(), fill));
        }
        out.push(stroke(points, true));
    };
    match item.kind {
        ShapeKind::Rectangle => {
            let rounded = item.extra.get("roundness").is_some_and(|r| !r.is_null());
            outline(
                rect_points(
                    b,
                    if rounded {
                        (b[2].min(b[3]) * 0.25).min(32.0)
                    } else {
                        0.0
                    },
                ),
                &mut out,
            );
        }
        ShapeKind::Diamond => {
            let (x, y, w, h) = (b[0], b[1], b[2], b[3]);
            outline(
                vec![
                    [x + w / 2.0, y],
                    [x + w, y + h / 2.0],
                    [x + w / 2.0, y + h],
                    [x, y + h / 2.0],
                ],
                &mut out,
            );
        }
        ShapeKind::Ellipse => {
            let (rx, ry) = (b[2] / 2.0, b[3] / 2.0);
            let points = (0..72)
                .map(|i| {
                    let t = i as f32 / 72.0 * std::f32::consts::TAU;
                    [centre[0] + rx * t.cos(), centre[1] + ry * t.sin()]
                })
                .collect();
            outline(points, &mut out);
        }
        ShapeKind::Line | ShapeKind::Arrow | ShapeKind::Freedraw => {
            let mut points: Vec<[f32; 2]> = item
                .points
                .iter()
                .map(|p| [f(item.x) + f(p[0]), f(item.y) + f(p[1])])
                .filter(|p| p[0].is_finite() && p[1].is_finite())
                .collect();
            rotate(&mut points, centre, angle);
            if points.is_empty() {
                return out;
            }
            let closed = points.len() > 2 && points.first() == points.last();
            if closed && let Some(fill) = fill {
                out.push(Piece::Fill(points[..points.len() - 1].to_vec(), fill));
            }
            let mut line = stroke(points.clone(), false);
            if item.kind == ShapeKind::Freedraw
                && let Piece::Stroke { width, .. } = &mut line
            {
                // Excalidraw's pressure outline is wider than its stroke width.
                *width *= 1.6;
            }
            out.push(line);
            if item.kind == ShapeKind::Arrow {
                let head = |key: &str| item.extra.get(key).and_then(Value::as_str).is_some();
                let n = points.len();
                if n >= 2 && head("endArrowhead") {
                    out.push(stroke(
                        arrowhead(points[n - 2], points[n - 1], width(item)),
                        false,
                    ));
                }
                if n >= 2 && head("startArrowhead") {
                    out.push(stroke(arrowhead(points[1], points[0], width(item)), false));
                }
            }
        }
        ShapeKind::Text => {
            let Some(body) = &item.text else { return out };
            let font_size = f(body.font_size);
            let font_size = if font_size.is_finite() && font_size > 0.0 {
                font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
            } else {
                20.0
            };
            let line_height = item
                .extra
                .get("lineHeight")
                .and_then(Value::as_f64)
                .map_or(1.25, f);
            out.push(Piece::Text {
                at: [b[0], b[1]],
                text: SceneText {
                    at: [0.0; 2],
                    lines: body.text.split('\n').map(str::to_owned).collect(),
                    font_size,
                    line_height,
                    color: ink,
                },
            });
        }
    }
    out
}

/// A rectangle's outline, with corner arcs of radius `r`.
fn rect_points(b: [f32; 4], r: f32) -> Vec<[f32; 2]> {
    let (x0, y0, x1, y1) = (b[0], b[1], b[0] + b[2], b[1] + b[3]);
    if r <= 0.5 {
        return vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    }
    let mut points = Vec::new();
    let corners = [
        ([x1 - r, y0 + r], -90.0_f32),
        ([x1 - r, y1 - r], 0.0),
        ([x0 + r, y1 - r], 90.0),
        ([x0 + r, y0 + r], 180.0),
    ];
    for (c, start) in corners {
        for step in 0..=6 {
            let t = (start + step as f32 * 15.0).to_radians();
            points.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
        }
    }
    points
}

/// Two barbs at `tip`, pointing back along `from → tip`.
fn arrowhead(from: [f32; 2], tip: [f32; 2], width: f32) -> Vec<[f32; 2]> {
    let (dx, dy) = (tip[0] - from[0], tip[1] - from[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-3 {
        return vec![tip];
    }
    let size = (len * 0.4).min(16.0 + width * 2.0);
    let back = dy.atan2(dx) + std::f32::consts::PI;
    let barb = |turn: f32| {
        let a = back + turn;
        [tip[0] + size * a.cos(), tip[1] + size * a.sin()]
    };
    vec![barb(0.45), tip, barb(-0.45)]
}

/// The box of an element Muninn does not model, when it has one.
fn raw_bounds(value: &Value) -> Option<[f32; 4]> {
    let n = |k: &str| value.get(k).and_then(Value::as_f64).map(f);
    let (x, y, w, h) = (n("x")?, n("y")?, n("width")?, n("height")?);
    let b = [x, y, x + w.max(0.0), y + h.max(0.0)];
    b.iter().all(|v| v.is_finite()).then_some(b)
}

fn raw_pieces(b: [f32; 4]) -> Vec<Piece> {
    let grey = Color {
        r: 0.5,
        g: 0.5,
        b: 0.5,
        a: 0.45,
    };
    vec![Piece::Stroke {
        points: rect_points([b[0], b[1], b[2] - b[0], b[3] - b[1]], 0.0),
        closed: true,
        width: 1.0,
        cap: LineCap::Butt,
        color: grey,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE: &str = include_str!("../../muninn-core/tests/fixtures/board/scene.excalidraw");

    fn board(elements: &str) -> Board {
        Board::from_json(&format!(
            r##"{{"type":"excalidraw","version":2,"source":"test","elements":{elements},"appState":{{"viewBackgroundColor":"#fff9db"}},"files":{{}}}}"##
        ))
        .unwrap()
    }

    #[test]
    fn the_fixture_scene_draws_every_live_element() {
        let scene = BoardScene::build(&Board::from_json(SCENE).unwrap());
        // Every live element is either drawn or outlined.
        let live = Board::from_json(SCENE).unwrap().live_elements().count();
        assert_eq!(scene.drawn + scene.outlined, live);
        assert_eq!(scene.texts.len(), 1);
        assert!(!scene.shape.is_empty());
        assert!(scene.bounds.is_some());
    }

    #[test]
    fn bounds_cover_boxes_and_points_and_the_world_pads_them() {
        let b = board(
            r#"[{"type":"rectangle","id":"r","x":10,"y":20,"width":100,"height":50},
                {"type":"line","id":"l","x":300,"y":100,"points":[[0,0],[50,-120]]}]"#,
        );
        let scene = BoardScene::build(&b);
        assert_eq!(scene.bounds, Some([10.0, -20.0, 350.0, 100.0]));
        assert_eq!(scene.origin, [10.0 - PAD, -20.0 - PAD]);
        assert_eq!(scene.size, [340.0 + 2.0 * PAD, 120.0 + 2.0 * PAD]);
        assert_eq!(scene.drawn, 2);
    }

    #[test]
    fn rotation_grows_the_bounds_and_tombstones_are_skipped() {
        let b = board(
            r#"[{"type":"rectangle","id":"r","x":0,"y":0,"width":100,"height":100,"angle":0.7853981633974483},
                {"type":"ellipse","id":"gone","x":5000,"y":5000,"width":10,"height":10,"isDeleted":true}]"#,
        );
        let scene = BoardScene::build(&b);
        let [x0, y0, x1, y1] = scene.bounds.unwrap();
        let half = 50.0 * std::f32::consts::SQRT_2;
        assert!((x0 - (50.0 - half)).abs() < 1e-3 && (x1 - (50.0 + half)).abs() < 1e-3);
        assert!((y0 - (50.0 - half)).abs() < 1e-3 && (y1 - (50.0 + half)).abs() < 1e-3);
        assert_eq!(scene.drawn, 1);
    }

    #[test]
    fn colours_follow_the_file() {
        assert_eq!(color("transparent"), None);
        assert_eq!(color("#e03131"), hex("#e03131"));
        assert_eq!(color("not a colour"), None);
        let b = board(
            r##"[{"type":"text","id":"t","x":0,"y":0,"width":80,"height":25,"text":"a\nb","fontSize":20,"strokeColor":"#1971c2","opacity":50}]"##,
        );
        let scene = BoardScene::build(&b);
        assert_eq!(scene.paper, hex("#fff9db").unwrap());
        let text = &scene.texts[0];
        assert_eq!(text.lines, ["a", "b"]);
        assert_eq!(text.font_size, 20.0);
        let ink = hex("#1971c2").unwrap();
        assert_eq!(text.color, ink.alpha(0.5));
        assert_eq!(text.at, [PAD, PAD]);
    }

    #[test]
    fn unmodelled_elements_with_a_box_are_outlined_and_others_ignored() {
        let b = board(
            r#"[{"type":"iframe","id":"i","x":0,"y":0,"width":40,"height":30},
                {"type":"mystery","id":"m"}]"#,
        );
        let scene = BoardScene::build(&b);
        assert_eq!((scene.drawn, scene.outlined), (0, 1));
        assert_eq!(scene.bounds, Some([0.0, 0.0, 40.0, 30.0]));
    }

    #[test]
    fn an_empty_board_has_no_bounds() {
        let scene = BoardScene::build(&Board::new());
        assert_eq!(scene.bounds, None);
        assert!(scene.shape.is_empty());
        assert_eq!(scene.paper, hex(PAPER).unwrap());
    }
}

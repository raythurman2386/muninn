//! Editing existing elements (Board slice 5): hit tests, marquee selection,
//! moves, deletes and a bounded undo history.
//!
//! Every edit is written the way Excalidraw's own `mutateElement` writes one:
//! the element's `version` goes up by one, `versionNonce` gets a new random
//! integer and `updated` the edit's time in milliseconds ([`Shape::bump`]).
//! Undo and redo are edits too, so they bump again rather than restoring the
//! old `version`: a version only ever grows, which is what Excalidraw's
//! reconciliation (and a later sync) relies on to tell the newer copy.
//!
//! Delete writes a tombstone, `isDeleted: true`, and keeps the element in
//! the file. That is Excalidraw's own in-scene delete, s1's model already
//! keeps and round-trips tombstones, and it keeps undo and a later merge
//! simple: an id is never reused and a stale copy cannot bring an element
//! back with a lower version. Excalidraw's restore skips deleted elements
//! when it draws, so excalidraw.com shows the board the same way.
//!
//! What can be picked: live (not deleted), unlocked, modelled elements
//! ([`Shape`]). Elements Muninn keeps raw (frames, images, unknown kinds)
//! and locked ones are never selected, moved or deleted, so their bytes stay
//! as Excalidraw wrote them. Picking an element in a group picks its
//! outermost group (the last of its `groupIds`), as Excalidraw does.

use std::collections::VecDeque;

use serde_json::{Value, json};

use super::{Board, Element, IdSource, Shape, ShapeKind};

/// How many edits undo keeps per open board.
pub const HISTORY_LIMIT: usize = 100;

/// Ellipses are hit-tested as polygons of this many sides.
const ELLIPSE_SIDES: usize = 64;

/// Drag offsets are rounded to 0.01 scene units, like the geometry of a
/// drawn element; the offset is rounded, not the result, so an element at
/// 450.1234567890123 moved by 5 lands on 455.1234567890123 and keeps its
/// precision.
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

impl Shape {
    /// Excalidraw's `bumpVersion`: `version + 1`, a new `versionNonce` and
    /// `updated` set to `now` (milliseconds since the epoch).
    pub fn bump(&mut self, ids: &mut IdSource, now: i64) {
        self.version = self.version.saturating_add(1);
        self.version_nonce = ids.integer();
        self.extra.insert("updated".to_owned(), json!(now));
    }

    /// `locked: true`: Excalidraw does not select it with a click or a
    /// marquee.
    #[must_use]
    pub fn is_locked(&self) -> bool {
        self.extra.get("locked").and_then(Value::as_bool) == Some(true)
    }

    /// The outermost group the element is in (the last of `groupIds`).
    #[must_use]
    pub fn outer_group(&self) -> Option<&str> {
        self.extra
            .get("groupIds")
            .and_then(Value::as_array)
            .and_then(|g| g.last())
            .and_then(Value::as_str)
    }

    /// Whether `in_group` is one of the element's `groupIds`.
    fn in_group(&self, group: &str) -> bool {
        self.extra
            .get("groupIds")
            .and_then(Value::as_array)
            .is_some_and(|g| g.iter().any(|v| v.as_str() == Some(group)))
    }

    /// The element's own box before rotation: x, y, width, height. Lines,
    /// arrows and freehand use their points' extent.
    #[must_use]
    pub fn own_box(&self) -> [f64; 4] {
        if self.kind.has_points() && !self.points.is_empty() {
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for p in &self.points {
                x0 = x0.min(p[0]);
                y0 = y0.min(p[1]);
                x1 = x1.max(p[0]);
                y1 = y1.max(p[1]);
            }
            [self.x + x0, self.y + y0, x1 - x0, y1 - y0]
        } else {
            [self.x, self.y, self.width.max(0.0), self.height.max(0.0)]
        }
    }

    fn centre(&self) -> [f64; 2] {
        let b = self.own_box();
        [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0]
    }

    fn angle(&self) -> f64 {
        if self.kind == ShapeKind::Text || !self.angle.is_finite() {
            0.0
        } else {
            self.angle
        }
    }

    /// Axis-aligned bounds of the drawn element (after rotation): min x,
    /// min y, max x, max y in scene units; `None` for non-finite geometry.
    #[must_use]
    pub fn bounds(&self) -> Option<[f64; 4]> {
        let b = self.own_box();
        if !b.iter().all(|v| v.is_finite()) {
            return None;
        }
        let corners = [
            [b[0], b[1]],
            [b[0] + b[2], b[1]],
            [b[0] + b[2], b[1] + b[3]],
            [b[0], b[1] + b[3]],
        ];
        let (c, a) = (self.centre(), self.angle());
        let corners = corners.map(|p| rotate(p, c, a));
        Some([
            corners.iter().map(|p| p[0]).fold(f64::MAX, f64::min),
            corners.iter().map(|p| p[1]).fold(f64::MAX, f64::min),
            corners.iter().map(|p| p[0]).fold(f64::MIN, f64::max),
            corners.iter().map(|p| p[1]).fold(f64::MIN, f64::max),
        ])
    }

    /// Whether scene point `p` hits the element, within `tol` scene units.
    /// Closed shapes with a fill hit anywhere inside; transparent ones only
    /// near their outline, as in Excalidraw. Lines and freehand hit near
    /// their stroke, text anywhere in its box.
    #[must_use]
    pub fn hit(&self, p: [f64; 2], tol: f64) -> bool {
        let b = self.own_box();
        if !b.iter().all(|v| v.is_finite()) || !p[0].is_finite() || !p[1].is_finite() {
            return false;
        }
        // Into the element's own (unrotated) frame.
        let p = rotate(p, self.centre(), -self.angle());
        let reach = tol + self.stroke_width.max(0.0) / 2.0;
        let (x0, y0, w, h) = (b[0], b[1], b[2], b[3]);
        let filled = !self.background_color.eq_ignore_ascii_case("transparent");
        let outline: Vec<[f64; 2]> = match self.kind {
            ShapeKind::Text => {
                return p[0] >= x0 - tol
                    && p[0] <= x0 + w + tol
                    && p[1] >= y0 - tol
                    && p[1] <= y0 + h + tol;
            }
            ShapeKind::Line | ShapeKind::Arrow | ShapeKind::Freedraw => {
                let pts: Vec<[f64; 2]> = self
                    .points
                    .iter()
                    .map(|q| [self.x + q[0], self.y + q[1]])
                    .collect();
                if pts.len() == 1 {
                    return dist(p, pts[0]) <= reach;
                }
                return pts.windows(2).any(|s| seg_dist(p, s[0], s[1]) <= reach);
            }
            ShapeKind::Rectangle => vec![[x0, y0], [x0 + w, y0], [x0 + w, y0 + h], [x0, y0 + h]],
            ShapeKind::Diamond => vec![
                [x0 + w / 2.0, y0],
                [x0 + w, y0 + h / 2.0],
                [x0 + w / 2.0, y0 + h],
                [x0, y0 + h / 2.0],
            ],
            ShapeKind::Ellipse => (0..ELLIPSE_SIDES)
                .map(|i| {
                    let t = i as f64 / ELLIPSE_SIDES as f64 * std::f64::consts::TAU;
                    [
                        x0 + w / 2.0 + w / 2.0 * t.cos(),
                        y0 + h / 2.0 + h / 2.0 * t.sin(),
                    ]
                })
                .collect(),
        };
        let n = outline.len();
        let near = (0..n).any(|i| seg_dist(p, outline[i], outline[(i + 1) % n]) <= reach);
        near || (filled && inside(p, &outline))
    }
}

fn rotate(p: [f64; 2], c: [f64; 2], a: f64) -> [f64; 2] {
    if a == 0.0 {
        return p;
    }
    let (s, k) = a.sin_cos();
    let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
    [c[0] + dx * k - dy * s, c[1] + dx * s + dy * k]
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn seg_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    if len2 <= f64::EPSILON {
        return dist(p, a);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0);
    dist(p, [a[0] + t * dx, a[1] + t * dy])
}

/// Even-odd point-in-polygon.
fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut on = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            on = !on;
        }
    }
    on
}

/// One element's state before and after an edit. Both are whole elements;
/// undo puts `before` back (with a fresh version), redo `after`.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub before: Shape,
    pub after: Shape,
}

impl Change {
    /// A new element: undoing it leaves a tombstone of it.
    #[must_use]
    pub fn created(shape: &Shape) -> Change {
        let mut before = shape.clone();
        before.is_deleted = true;
        Change {
            before,
            after: shape.clone(),
        }
    }
}

impl Board {
    fn shape_index(&self, id: &str) -> Option<usize> {
        self.elements
            .iter()
            .position(|e| matches!(e, Element::Shape(s) if s.id == id))
    }

    /// The modelled element with this id.
    #[must_use]
    pub fn shape(&self, id: &str) -> Option<&Shape> {
        self.shape_index(id).and_then(|i| self.elements[i].shape())
    }

    fn pickable(&self) -> impl DoubleEndedIterator<Item = &Shape> {
        self.elements.iter().filter_map(|e| match e {
            Element::Shape(s) if !s.is_deleted && !s.is_locked() => Some(&**s),
            _ => None,
        })
    }

    /// The topmost pickable element at scene point `p`, within `tol`.
    #[must_use]
    pub fn hit_test(&self, p: [f64; 2], tol: f64) -> Option<&Shape> {
        self.pickable().rev().find(|s| s.hit(p, tol))
    }

    /// Pickable elements whose bounds lie wholly inside `rect` (min x, min
    /// y, max x, max y), as Excalidraw's marquee picks, in z-order.
    #[must_use]
    pub fn in_rect(&self, rect: [f64; 4]) -> Vec<String> {
        let [x0, y0, x1, y1] = rect;
        self.pickable()
            .filter(|s| {
                s.bounds()
                    .is_some_and(|b| b[0] >= x0 && b[1] >= y0 && b[2] <= x1 && b[3] <= y1)
            })
            .map(|s| s.id.clone())
            .collect()
    }

    /// Every pickable element's id, in z-order (`⌘A`).
    #[must_use]
    pub fn pickable_ids(&self) -> Vec<String> {
        self.pickable().map(|s| s.id.clone()).collect()
    }

    /// `ids` plus every pickable element sharing an outermost group with
    /// one of them, without repeats, in z-order. Ids that are not pickable
    /// (deleted, locked, raw, gone) are dropped.
    #[must_use]
    pub fn with_groups(&self, ids: &[String]) -> Vec<String> {
        let groups: Vec<String> = ids
            .iter()
            .filter_map(|id| self.shape(id))
            .filter_map(|s| s.outer_group().map(str::to_owned))
            .collect();
        self.pickable()
            .filter(|s| ids.contains(&s.id) || groups.iter().any(|g| s.in_group(g)))
            .map(|s| s.id.clone())
            .collect()
    }

    /// Union of the bounds of the elements with these ids.
    #[must_use]
    pub fn bounds_of(&self, ids: &[String]) -> Option<[f64; 4]> {
        ids.iter()
            .filter_map(|id| self.shape(id))
            .filter_map(Shape::bounds)
            .reduce(|a, b| {
                [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ]
            })
    }

    /// Move pickable elements by (`dx`, `dy`) scene units without bumping
    /// their versions (a drag in progress); see [`Board::commit`].
    pub fn shift(&mut self, ids: &[String], dx: f64, dy: f64) {
        for e in &mut self.elements {
            if let Element::Shape(s) = e
                && ids.contains(&s.id)
                && !s.is_deleted
                && !s.is_locked()
            {
                s.x += round2(dx);
                s.y += round2(dy);
            }
        }
    }

    /// Put the elements of `start` at their `start` position plus (`dx`,
    /// `dy`), without bumping versions: a drag in progress, where each
    /// pointer move places the selection from where the drag began.
    pub fn place(&mut self, start: &[Shape], dx: f64, dy: f64) {
        for old in start {
            if let Some(i) = self.shape_index(&old.id)
                && let Element::Shape(s) = &mut self.elements[i]
                && !s.is_deleted
                && !s.is_locked()
            {
                s.x = if dx == 0.0 { old.x } else { old.x + round2(dx) };
                s.y = if dy == 0.0 { old.y } else { old.y + round2(dy) };
            }
        }
    }

    /// Finish an edit: every element in `before` that now differs is
    /// bumped, and the edit's changes are returned (empty when nothing
    /// changed, e.g. a drag that came back to where it began).
    pub fn commit(&mut self, before: &[Shape], ids: &mut IdSource, now: i64) -> Vec<Change> {
        let mut out = Vec::new();
        for old in before {
            let Some(i) = self.shape_index(&old.id) else {
                continue;
            };
            let Element::Shape(s) = &mut self.elements[i] else {
                continue;
            };
            if **s == *old {
                continue;
            }
            s.bump(ids, now);
            out.push(Change {
                before: old.clone(),
                after: (**s).clone(),
            });
        }
        out
    }

    /// The current state of these elements, to pass to [`Board::commit`].
    #[must_use]
    pub fn snapshot(&self, ids: &[String]) -> Vec<Shape> {
        ids.iter()
            .filter_map(|id| self.shape(id))
            .cloned()
            .collect()
    }

    /// Move and bump in one edit (arrow keys).
    pub fn move_by(
        &mut self,
        ids: &[String],
        dx: f64,
        dy: f64,
        src: &mut IdSource,
        now: i64,
    ) -> Vec<Change> {
        let before = self.snapshot(ids);
        self.shift(ids, dx, dy);
        self.commit(&before, src, now)
    }

    /// Delete pickable elements: each becomes an `isDeleted: true` tombstone
    /// with a bumped version.
    pub fn delete(&mut self, ids: &[String], src: &mut IdSource, now: i64) -> Vec<Change> {
        let before = self.snapshot(ids);
        for e in &mut self.elements {
            if let Element::Shape(s) = e
                && ids.contains(&s.id)
                && !s.is_locked()
            {
                s.is_deleted = true;
            }
        }
        self.commit(&before, src, now)
    }

    /// Put each element back to `states` (by id), as an edit: the version
    /// continues from the element's current one and is bumped. Returns the
    /// ids that changed.
    fn restore(&mut self, states: &[&Shape], src: &mut IdSource, now: i64) -> Vec<String> {
        let mut out = Vec::new();
        for want in states {
            let Some(i) = self.shape_index(&want.id) else {
                continue;
            };
            let Element::Shape(s) = &mut self.elements[i] else {
                continue;
            };
            let version = s.version;
            let mut next = (*want).clone();
            next.version = version;
            next.bump(src, now);
            **s = next;
            out.push(want.id.clone());
        }
        out
    }
}

/// Undo and redo for one open board: whole edits, newest last, at most
/// [`HISTORY_LIMIT`] of them. A new edit clears redo.
#[derive(Clone, Debug, Default)]
pub struct History {
    undo: VecDeque<Vec<Change>>,
    redo: Vec<Vec<Change>>,
}

impl History {
    /// Remember one edit (ignored when it changed nothing).
    pub fn record(&mut self, edit: Vec<Change>) {
        if edit.is_empty() {
            return;
        }
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(edit);
        self.redo.clear();
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Edits that undo can still take back.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Take back the newest edit. Returns the ids it touched (still live
    /// ones first select again), or `None` with nothing to undo.
    pub fn undo(&mut self, board: &mut Board, src: &mut IdSource, now: i64) -> Option<Vec<String>> {
        let edit = self.undo.pop_back()?;
        let states: Vec<&Shape> = edit.iter().rev().map(|c| &c.before).collect();
        let ids = board.restore(&states, src, now);
        self.redo.push(edit);
        Some(ids)
    }

    /// Apply the newest undone edit again.
    pub fn redo(&mut self, board: &mut Board, src: &mut IdSource, now: i64) -> Option<Vec<String>> {
        let edit = self.redo.pop()?;
        let states: Vec<&Shape> = edit.iter().map(|c| &c.after).collect();
        let ids = board.restore(&states, src, now);
        self.undo.push_back(edit);
        Some(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Board {
        let mut b = Board::new();
        let mut ids = IdSource::new(7);
        b.add_drawn(
            ShapeKind::Rectangle,
            &[[0.0, 0.0], [100.0, 50.0]],
            &mut ids,
            1,
        );
        b.add_drawn(
            ShapeKind::Ellipse,
            &[[200.0, 0.0], [300.0, 100.0]],
            &mut ids,
            1,
        );
        b.add_drawn(
            ShapeKind::Line,
            &[[0.0, 200.0], [100.0, 300.0]],
            &mut ids,
            1,
        );
        b
    }

    fn id(b: &Board, i: usize) -> String {
        b.elements[i].id().unwrap().to_owned()
    }

    #[test]
    fn hits_outlines_of_transparent_shapes_and_lines_and_fills_inside() {
        let mut b = board();
        assert_eq!(
            b.hit_test([50.0, 0.0], 4.0).map(|s| s.id.clone()),
            Some(id(&b, 0))
        );
        assert!(
            b.hit_test([50.0, 25.0], 4.0).is_none(),
            "transparent inside"
        );
        assert_eq!(
            b.hit_test([300.0, 50.0], 4.0).map(|s| s.id.clone()),
            Some(id(&b, 1))
        );
        assert!(
            b.hit_test([205.0, 5.0], 4.0).is_none(),
            "ellipse box corner"
        );
        assert_eq!(
            b.hit_test([50.0, 250.0], 4.0).map(|s| s.id.clone()),
            Some(id(&b, 2))
        );
        assert!(b.hit_test([60.0, 240.0], 4.0).is_none());
        if let Element::Shape(s) = &mut b.elements[0] {
            s.background_color = "#ffc9c9".into();
        }
        assert_eq!(
            b.hit_test([50.0, 25.0], 4.0).map(|s| s.id.clone()),
            Some(id(&b, 0))
        );
    }

    #[test]
    fn rotation_moves_the_hit_area() {
        let mut b = board();
        if let Element::Shape(s) = &mut b.elements[0] {
            s.angle = std::f64::consts::FRAC_PI_2;
        }
        // 100×50 about (50, 25), turned a quarter: now 50 wide, 100 tall.
        assert!(b.hit_test([25.0, -25.0], 3.0).is_some());
        assert!(b.hit_test([50.0, 0.0], 3.0).is_none());
        let bb = b.shape(&id(&b, 0)).unwrap().bounds().unwrap();
        assert!(
            (bb[0] - 25.0).abs() < 1e-9 && (bb[3] - 75.0).abs() < 1e-9,
            "{bb:?}"
        );
    }

    #[test]
    fn marquee_picks_only_whole_elements_and_skips_locked_deleted_raw() {
        let mut b = board();
        assert_eq!(b.in_rect([-5.0, -5.0, 105.0, 55.0]), vec![id(&b, 0)]);
        assert_eq!(b.in_rect([-5.0, -5.0, 99.0, 55.0]), Vec::<String>::new());
        assert_eq!(b.pickable_ids().len(), 3);
        if let Element::Shape(s) = &mut b.elements[1] {
            s.extra.insert("locked".into(), json!(true));
        }
        if let Element::Shape(s) = &mut b.elements[2] {
            s.is_deleted = true;
        }
        b.elements
            .push(Element::Raw(json!({"type":"frame","id":"f","x":0,"y":0})));
        assert_eq!(b.in_rect([-1e9, -1e9, 1e9, 1e9]), vec![id(&b, 0)]);
        assert_eq!(b.pickable_ids(), vec![id(&b, 0)]);
    }

    #[test]
    fn groups_select_together() {
        let mut b = board();
        for i in [0, 2] {
            if let Element::Shape(s) = &mut b.elements[i] {
                s.extra.insert("groupIds".into(), json!(["inner", "outer"]));
            }
        }
        assert_eq!(b.with_groups(&[id(&b, 0)]), vec![id(&b, 0), id(&b, 2)]);
        assert_eq!(b.with_groups(&[id(&b, 1)]), vec![id(&b, 1)]);
    }

    #[test]
    fn moves_and_deletes_bump_versions_and_undo_redo_bump_again() {
        let mut b = board();
        let mut src = IdSource::new(1);
        let mut h = History::default();
        let a = id(&b, 0);
        let v0 = b.shape(&a).unwrap().clone();
        h.record(b.move_by(std::slice::from_ref(&a), 10.0, -5.0, &mut src, 50));
        let s = b.shape(&a).unwrap();
        assert_eq!((s.x, s.y, s.version), (10.0, -5.0, v0.version + 1));
        assert_ne!(s.version_nonce, v0.version_nonce);
        assert_eq!(s.extra["updated"], json!(50));
        h.record(b.delete(std::slice::from_ref(&a), &mut src, 60));
        assert!(b.shape(&a).unwrap().is_deleted);
        assert_eq!(b.shape(&a).unwrap().version, v0.version + 2);
        assert!(
            b.hit_test([60.0, -5.0], 4.0).is_none(),
            "a tombstone is not hit"
        );

        assert_eq!(h.undo(&mut b, &mut src, 70), Some(vec![a.clone()]));
        let s = b.shape(&a).unwrap();
        assert!(!s.is_deleted);
        assert_eq!((s.x, s.version), (10.0, v0.version + 3));
        h.undo(&mut b, &mut src, 80).unwrap();
        let s = b.shape(&a).unwrap();
        assert_eq!((s.x, s.y, s.version), (0.0, 0.0, v0.version + 4));
        assert_eq!(s.extra["updated"], json!(80));
        assert!(h.undo(&mut b, &mut src, 90).is_none());
        h.redo(&mut b, &mut src, 100).unwrap();
        h.redo(&mut b, &mut src, 110).unwrap();
        let s = b.shape(&a).unwrap();
        assert!(s.is_deleted && s.x == 10.0 && s.version == v0.version + 6);
        assert!(h.redo(&mut b, &mut src, 120).is_none());
        // A new edit clears redo.
        h.undo(&mut b, &mut src, 130).unwrap();
        assert!(h.can_redo());
        h.record(b.move_by(std::slice::from_ref(&a), 1.0, 0.0, &mut src, 140));
        assert!(!h.can_redo());
    }

    #[test]
    fn undoing_a_creation_leaves_a_tombstone_and_redo_brings_it_back() {
        let mut b = Board::new();
        let mut src = IdSource::new(3);
        let mut h = History::default();
        let made = b
            .add_drawn(
                ShapeKind::Rectangle,
                &[[0.0, 0.0], [10.0, 10.0]],
                &mut src,
                1,
            )
            .unwrap()
            .clone();
        h.record(vec![Change::created(&made)]);
        h.undo(&mut b, &mut src, 2).unwrap();
        let s = b.shape(&made.id).unwrap();
        assert!(s.is_deleted);
        assert_eq!(s.version, made.version + 1);
        h.redo(&mut b, &mut src, 3).unwrap();
        assert!(!b.shape(&made.id).unwrap().is_deleted);
    }

    #[test]
    fn a_drag_back_to_the_start_changes_nothing() {
        let mut b = board();
        let mut src = IdSource::new(1);
        let ids = vec![id(&b, 0)];
        let before = b.snapshot(&ids);
        b.shift(&ids, 30.0, 0.0);
        b.shift(&ids, -30.0, 0.0);
        assert!(b.commit(&before, &mut src, 5).is_empty());
        assert_eq!(b.snapshot(&ids), before);
    }

    #[test]
    fn history_is_bounded() {
        let mut b = board();
        let mut src = IdSource::new(1);
        let mut h = History::default();
        let ids = vec![id(&b, 0)];
        for _ in 0..HISTORY_LIMIT + 20 {
            h.record(b.move_by(&ids, 1.0, 0.0, &mut src, 1));
        }
        assert_eq!(h.undo_len(), HISTORY_LIMIT);
        while h.undo(&mut b, &mut src, 2).is_some() {}
        assert_eq!(
            b.shape(&ids[0]).unwrap().x,
            20.0,
            "the oldest 20 moves are kept"
        );
    }

    #[test]
    fn locked_elements_are_neither_moved_nor_deleted() {
        let mut b = board();
        let mut src = IdSource::new(1);
        if let Element::Shape(s) = &mut b.elements[0] {
            s.extra.insert("locked".into(), json!(true));
        }
        let ids = vec![id(&b, 0)];
        let before = b.shape(&ids[0]).unwrap().clone();
        assert!(b.move_by(&ids, 5.0, 5.0, &mut src, 1).is_empty());
        assert!(b.delete(&ids, &mut src, 1).is_empty());
        assert_eq!(b.shape(&ids[0]).unwrap(), &before);
    }

    #[test]
    fn moving_keeps_the_precision_of_the_original_position() {
        let mut board = Board::from_json(
            r#"{"type":"excalidraw","elements":[{"type":"text","id":"t","x":0.5,"y":450.1234567890123,"width":10,"height":10,"text":"a"}]}"#,
        )
        .unwrap();
        let ids = vec!["t".to_owned()];
        board.shift(&ids, 5.0, 0.0);
        board.shift(&ids, 0.0, 5.0);
        let start = board.snapshot(&ids);
        board.place(&start, 1.004, 0.0);
        let s = board.shape("t").unwrap();
        assert_eq!(s.x, 0.5 + 5.0 + 1.0);
        assert_eq!(s.y, 450.1234567890123 + 5.0);
    }
}

//! New elements drawn in Muninn (Board slice 4).
//!
//! A drawn element is written the way Excalidraw writes one it just created,
//! so excalidraw.com opens it as its own: the modelled fields on [`Shape`]
//! plus, in [`Shape::extra`], every other field Excalidraw's element factory
//! sets (`fillStyle`, `strokeStyle`, `roughness`, `groupIds`, `frameId`,
//! `roundness`, `boundElements`, `created`, `updated`, `link`, `locked`, the
//! arrow and line binding fields, freedraw `pressures`, …), with the values of
//! its default style. The field set follows the vitest snapshots that
//! `scene.excalidraw` was taken from.
//!
//! Ids are 21 characters from nanoid's URL alphabet, as Excalidraw's are;
//! `seed` and `versionNonce` are random integers below 2³¹, as Excalidraw's
//! `randomInteger` gives. [`IdSource`] is a small seeded generator so the
//! crate needs no random-number dependency and tests can be deterministic.
//!
//! Ordering: Excalidraw keeps z-order both as array order and as a
//! fractional `index` string per element. A new element is appended, and
//! gets the `index` after the last indexed element ([`index_after`], the
//! `fractional-indexing` package's `generateKeyBetween(last, null)`). A board
//! whose elements carry no `index` at all gets none either; Excalidraw fills
//! them on load.

use serde_json::{Map, Value, json};

use super::{Board, Element, Shape, ShapeKind};

/// nanoid's URL-safe alphabet (64 symbols), in nanoid's order.
const ID_ALPHABET: &[u8; 64] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";
const ID_LEN: usize = 21;
/// The digits of a fractional index key (base 62, ASCII order).
const INDEX_DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Random ids, seeds and nonces for new elements: SplitMix64 over a seed.
#[derive(Clone, Debug)]
pub struct IdSource {
    state: u64,
}

impl IdSource {
    /// A generator seeded with `seed` (the UI seeds it from the clock).
    #[must_use]
    pub fn new(seed: u64) -> IdSource {
        IdSource { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A 21-character element id.
    pub fn id(&mut self) -> String {
        let mut out = String::with_capacity(ID_LEN);
        let mut bits = 0u64;
        let mut left = 0;
        for _ in 0..ID_LEN {
            if left < 6 {
                bits = self.next();
                left = 64;
            }
            out.push(char::from(ID_ALPHABET[(bits & 63) as usize]));
            bits >>= 6;
            left -= 6;
        }
        out
    }

    /// A non-negative integer below 2³¹, like Excalidraw's `randomInteger`.
    pub fn integer(&mut self) -> i64 {
        (self.next() >> 33) as i64
    }
}

/// The key after `prev` in fractional-index order, or `None` when `prev` is
/// not a valid key or is the very last integer key (`z` followed by 26 `z`s).
#[must_use]
pub fn index_after(prev: &str) -> Option<String> {
    let bytes = prev.as_bytes();
    let head = *bytes.first()?;
    let len = match head {
        b'a'..=b'z' => usize::from(head - b'a') + 2,
        b'A'..=b'Z' => usize::from(b'Z' - head) + 2,
        _ => return None,
    };
    if bytes.len() < len || !bytes[1..].iter().all(|b| INDEX_DIGITS.contains(b)) {
        return None;
    }
    // generateKeyBetween(a, null) keeps only a's integer part and increments it.
    let mut digits: Vec<u8> = bytes[1..len].to_vec();
    let mut carry = true;
    for d in digits.iter_mut().rev() {
        let at = INDEX_DIGITS.iter().position(|x| x == d)? + 1;
        if at == INDEX_DIGITS.len() {
            *d = INDEX_DIGITS[0];
        } else {
            *d = INDEX_DIGITS[at];
            carry = false;
            break;
        }
    }
    if carry {
        match head {
            b'Z' => return Some("a0".to_owned()),
            b'z' => return None,
            _ => {}
        }
        let next = head + 1;
        if next > b'a' {
            digits.push(INDEX_DIGITS[0]);
        } else {
            digits.pop();
        }
        let mut out = String::from(char::from(next));
        out.extend(digits.into_iter().map(char::from));
        return Some(out);
    }
    let mut out = String::from(char::from(head));
    out.extend(digits.into_iter().map(char::from));
    Some(out)
}

/// Round to 1/100 of a scene unit, so a drawn element's numbers stay short.
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

impl Shape {
    /// A new element of `kind` at (`x`, `y`) with every field Excalidraw's
    /// element factory writes, in its default style. `updated` is the time in
    /// milliseconds since the Unix epoch (`created` and `updated`).
    #[must_use]
    pub fn drawn(kind: ShapeKind, ids: &mut IdSource, x: f64, y: f64, updated: i64) -> Shape {
        let mut shape = Shape::new(kind, ids.id(), round2(x), round2(y));
        shape.seed = ids.integer();
        shape.version_nonce = ids.integer();
        let mut extra = Map::new();
        let mut put = |key: &str, value: Value| {
            extra.insert(key.to_owned(), value);
        };
        put("fillStyle", json!("solid"));
        put("strokeStyle", json!("solid"));
        put("roughness", json!(1));
        put("groupIds", json!([]));
        put("frameId", Value::Null);
        put("boundElements", Value::Null);
        put("created", json!(updated));
        put("updated", json!(updated));
        put("link", Value::Null);
        put("locked", json!(false));
        // Excalidraw's default "round" edges: adaptive radius for rectangles,
        // proportional for the rest; freehand strokes have none.
        put(
            "roundness",
            match kind {
                ShapeKind::Rectangle => json!({ "type": 3 }),
                ShapeKind::Freedraw | ShapeKind::Text => Value::Null,
                _ => json!({ "type": 2 }),
            },
        );
        match kind {
            ShapeKind::Line | ShapeKind::Arrow => {
                put("startBinding", Value::Null);
                put("endBinding", Value::Null);
                put("startArrowhead", Value::Null);
                put(
                    "endArrowhead",
                    if kind == ShapeKind::Arrow {
                        json!("arrow")
                    } else {
                        Value::Null
                    },
                );
                if kind == ShapeKind::Arrow {
                    put("elbowed", json!(false));
                } else {
                    put("polygon", json!(false));
                }
            }
            ShapeKind::Freedraw => {
                put("pressures", json!([]));
                put("simulatePressure", json!(true));
            }
            _ => {}
        }
        shape.extra = extra;
        shape
    }
}

impl Board {
    /// The `index` for an element appended now: after the last indexed
    /// element, `a0` on an empty board, `None` when no element is indexed.
    #[must_use]
    pub fn next_index(&self) -> Option<String> {
        let last = self.elements.iter().rev().find_map(|e| match e {
            Element::Shape(s) => s.extra.get("index").and_then(Value::as_str),
            Element::Raw(v) => v.get("index").and_then(Value::as_str),
        });
        match last {
            Some(key) => index_after(key),
            None if self.elements.is_empty() => Some("a0".to_owned()),
            None => None,
        }
    }

    /// Append a drawn element. `points` are the pointer's scene positions
    /// from press to release: a box kind (rectangle, ellipse, diamond) spans
    /// the first and last, a line or arrow runs from the first to the last,
    /// and a freehand stroke keeps them all. Returns `None`, adding nothing,
    /// for a stroke with no extent (Excalidraw drops those too) or a kind
    /// that is not drawn with a pointer (text).
    pub fn add_drawn(
        &mut self,
        kind: ShapeKind,
        points: &[[f64; 2]],
        ids: &mut IdSource,
        updated: i64,
    ) -> Option<&Shape> {
        let (first, last) = (*points.first()?, *points.last()?);
        if points.iter().flatten().any(|v| !v.is_finite()) {
            return None;
        }
        let mut shape = match kind {
            ShapeKind::Rectangle | ShapeKind::Ellipse | ShapeKind::Diamond => {
                let (x0, y0) = (first[0].min(last[0]), first[1].min(last[1]));
                let (x1, y1) = (first[0].max(last[0]), first[1].max(last[1]));
                let mut s = Shape::drawn(kind, ids, x0, y0, updated);
                s.width = round2(x1 - s.x);
                s.height = round2(y1 - s.y);
                if s.width <= 0.0 || s.height <= 0.0 {
                    return None;
                }
                s
            }
            ShapeKind::Line | ShapeKind::Arrow | ShapeKind::Freedraw => {
                let mut s = Shape::drawn(kind, ids, first[0], first[1], updated);
                let path: &[[f64; 2]] = if kind == ShapeKind::Freedraw {
                    points
                } else {
                    &[first, last]
                };
                let mut rel: Vec<[f64; 2]> = path
                    .iter()
                    .map(|p| [round2(p[0] - s.x), round2(p[1] - s.y)])
                    .collect();
                rel.dedup();
                rel[0] = [0.0, 0.0];
                let xs = rel.iter().map(|p| p[0]);
                let ys = rel.iter().map(|p| p[1]);
                let (lo_x, hi_x) = xs.fold((0.0f64, 0.0f64), |(a, b), v| (a.min(v), b.max(v)));
                let (lo_y, hi_y) = ys.fold((0.0f64, 0.0f64), |(a, b), v| (a.min(v), b.max(v)));
                s.width = round2(hi_x - lo_x);
                s.height = round2(hi_y - lo_y);
                if rel.len() < 2 || (s.width == 0.0 && s.height == 0.0) {
                    return None;
                }
                s.points = rel;
                s
            }
            ShapeKind::Text => return None,
        };
        // Never reuse an id already on the board (tombstones included).
        while self.elements.iter().any(|e| e.id() == Some(&shape.id)) {
            shape.id = ids.id();
        }
        if let Some(index) = self.next_index() {
            shape.extra.insert("index".into(), Value::from(index));
        }
        self.elements.push(Element::from(shape));
        self.elements.last().and_then(Element::shape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE: &str = include_str!("../../tests/fixtures/board/scene.excalidraw");

    #[test]
    fn index_keys_count_up_like_fractional_indexing() {
        let cases = [
            ("a0", "a1"),
            ("a9", "aA"),
            ("aZ", "aa"),
            ("az", "b00"),
            ("a5V", "a6"),
            ("b0z", "b10"),
            ("bzz", "c000"),
            ("Zz", "a0"),
            ("Yzz", "Z0"),
        ];
        for (prev, next) in cases {
            assert_eq!(index_after(prev).as_deref(), Some(next), "after {prev}");
        }
        assert_eq!(index_after(""), None);
        assert_eq!(index_after("a"), None, "too short for its head");
        assert_eq!(index_after("a-"), None, "not a base-62 digit");
        let last = format!("z{}", "z".repeat(26));
        assert_eq!(index_after(&last), None);
    }

    #[test]
    fn ids_look_like_excalidraw_ids_and_seeds_fit_31_bits() {
        let mut ids = IdSource::new(7);
        let a = ids.id();
        let b = ids.id();
        assert_eq!(a.len(), 21);
        assert_ne!(a, b);
        assert!(a.bytes().all(|c| ID_ALPHABET.contains(&c)));
        for _ in 0..1000 {
            let n = ids.integer();
            assert!((0..1 << 31).contains(&n));
        }
        // Seeded: the same seed gives the same sequence.
        assert_eq!(IdSource::new(7).id(), a);
    }

    #[test]
    fn a_drawn_rectangle_has_excalidraws_fields_and_normalised_box() {
        let mut board = Board::new();
        let mut ids = IdSource::new(1);
        let s = board
            .add_drawn(
                ShapeKind::Rectangle,
                &[[120.0, 80.0], [130.0, 90.0], [20.004, 30.0]],
                &mut ids,
                1_700_000_000_000,
            )
            .expect("drawn")
            .clone();
        assert_eq!((s.x, s.y, s.width, s.height), (20.0, 30.0, 100.0, 50.0));
        let v = Element::from(s).to_value();
        for (key, want) in [
            ("type", json!("rectangle")),
            ("angle", json!(0)),
            ("strokeColor", json!("#1e1e1e")),
            ("backgroundColor", json!("transparent")),
            ("fillStyle", json!("solid")),
            ("strokeWidth", json!(2)),
            ("strokeStyle", json!("solid")),
            ("roughness", json!(1)),
            ("opacity", json!(100)),
            ("groupIds", json!([])),
            ("frameId", Value::Null),
            ("roundness", json!({ "type": 3 })),
            ("boundElements", Value::Null),
            ("isDeleted", json!(false)),
            ("link", Value::Null),
            ("locked", json!(false)),
            ("version", json!(1)),
            ("created", json!(1_700_000_000_000_i64)),
            ("updated", json!(1_700_000_000_000_i64)),
            ("index", json!("a0")),
        ] {
            assert_eq!(v.get(key), Some(&want), "{key}");
        }
        assert!(v["seed"].is_i64() && v["versionNonce"].is_i64());
        assert!(v.get("points").is_none());
    }

    #[test]
    fn lines_arrows_and_strokes_keep_points_relative_to_their_start() {
        let mut board = Board::new();
        let mut ids = IdSource::new(2);
        let path = [[10.0, 10.0], [30.0, -5.0], [50.0, 40.0]];
        let line = board
            .add_drawn(ShapeKind::Line, &path, &mut ids, 1)
            .expect("line")
            .clone();
        assert_eq!(line.points, vec![[0.0, 0.0], [40.0, 30.0]]);
        assert_eq!(
            (line.x, line.y, line.width, line.height),
            (10.0, 10.0, 40.0, 30.0)
        );
        assert_eq!(line.extra["endArrowhead"], Value::Null);
        assert_eq!(line.extra["polygon"], json!(false));
        let arrow = board
            .add_drawn(ShapeKind::Arrow, &path, &mut ids, 1)
            .expect("arrow")
            .clone();
        assert_eq!(arrow.extra["endArrowhead"], json!("arrow"));
        assert_eq!(arrow.extra["startArrowhead"], Value::Null);
        assert_eq!(arrow.extra["elbowed"], json!(false));
        let pen = board
            .add_drawn(ShapeKind::Freedraw, &path, &mut ids, 1)
            .expect("freedraw")
            .clone();
        assert_eq!(pen.points, vec![[0.0, 0.0], [20.0, -15.0], [40.0, 30.0]]);
        assert_eq!((pen.width, pen.height), (40.0, 45.0));
        assert_eq!(pen.extra["pressures"], json!([]));
        assert_eq!(pen.extra["simulatePressure"], json!(true));
        assert_eq!(pen.extra["roundness"], Value::Null);
        let indexes: Vec<_> = board
            .elements
            .iter()
            .map(|e| e.shape().unwrap().extra["index"].clone())
            .collect();
        assert_eq!(indexes, vec![json!("a0"), json!("a1"), json!("a2")]);
    }

    #[test]
    fn strokes_without_extent_add_nothing() {
        let mut board = Board::new();
        let mut ids = IdSource::new(3);
        let p = [5.0, 5.0];
        for kind in [ShapeKind::Rectangle, ShapeKind::Line, ShapeKind::Freedraw] {
            assert!(board.add_drawn(kind, &[p, p, p], &mut ids, 1).is_none());
        }
        assert!(
            board
                .add_drawn(ShapeKind::Ellipse, &[[0.0, 0.0], [10.0, 0.0]], &mut ids, 1)
                .is_none(),
            "a box with no height"
        );
        assert!(
            board
                .add_drawn(ShapeKind::Text, &[[0.0, 0.0], [9.0, 9.0]], &mut ids, 1)
                .is_none()
        );
        assert!(
            board
                .add_drawn(ShapeKind::Arrow, &[], &mut ids, 1)
                .is_none()
        );
        assert!(
            board
                .add_drawn(
                    ShapeKind::Arrow,
                    &[[0.0, 0.0], [f64::NAN, 1.0]],
                    &mut ids,
                    1
                )
                .is_none()
        );
        assert!(board.elements.is_empty());
    }

    #[test]
    fn drawing_on_an_excalidraw_scene_appends_and_keeps_every_old_element() {
        let mut board = Board::from_json(SCENE).expect("scene");
        let before: Vec<Value> = board.elements.iter().map(Element::to_value).collect();
        let mut ids = IdSource::new(4);
        let id = board
            .add_drawn(ShapeKind::Diamond, &[[0.0, 0.0], [40.0, 30.0]], &mut ids, 5)
            .expect("diamond")
            .id
            .clone();
        assert_eq!(board.elements.len(), before.len() + 1);
        let after: Vec<Value> = board.elements.iter().map(Element::to_value).collect();
        assert_eq!(&after[..before.len()], &before[..]);
        // The scene's last index is a8.
        assert_eq!(after.last().unwrap()["index"], json!("a9"));
        // Saved and loaded back, the new element is the same modelled shape.
        let reloaded = Board::from_json(&board.to_json()).expect("reload");
        assert_eq!(reloaded, board);
        assert_eq!(reloaded.elements.last().unwrap().id(), Some(id.as_str()));
    }

    #[test]
    fn a_new_id_never_collides_with_one_on_the_board() {
        let mut board = Board::new();
        let taken = IdSource::new(9).id();
        board.elements.push(Element::from(Shape::new(
            ShapeKind::Rectangle,
            taken.clone(),
            0.0,
            0.0,
        )));
        let mut ids = IdSource::new(9);
        let s = board
            .add_drawn(ShapeKind::Rectangle, &[[0.0, 0.0], [1.0, 1.0]], &mut ids, 1)
            .expect("drawn");
        assert_ne!(s.id, taken);
    }

    #[test]
    fn an_unindexed_board_gets_no_index() {
        let mut board = Board::new();
        board.elements.push(Element::from(Shape::new(
            ShapeKind::Ellipse,
            "old",
            0.0,
            0.0,
        )));
        let mut ids = IdSource::new(5);
        let s = board
            .add_drawn(ShapeKind::Rectangle, &[[0.0, 0.0], [3.0, 4.0]], &mut ids, 1)
            .expect("drawn");
        assert!(s.extra.get("index").is_none());
    }
}

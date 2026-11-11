//! Text elements placed and edited in Muninn (Board slice 6).
//!
//! A new text element carries the fields Excalidraw's `newTextElement`
//! writes: `text` and `originalText` (the same string, since Muninn does not
//! wrap), `fontSize` 20, `fontFamily` 5 (Excalifont, Excalidraw's default),
//! `textAlign` `left`, `verticalAlign` `top`, `lineHeight` 1.25,
//! `containerId` null (text bound to a shape is not part of this slice) and
//! `autoResize` true, plus the common fields every drawn element gets
//! ([`Shape::drawn`]). `width` and `height` are the laid-out size the caller
//! measured: the widest line, and `lines × fontSize × lineHeight`.
//!
//! Text is never empty: placing empty text adds nothing, and emptying an
//! existing element deletes it (a tombstone, like any delete), as Excalidraw
//! drops a text element whose editor closes empty. Both edits go through
//! [`Board::commit`], so they bump `version`, `versionNonce` and `updated`
//! and undo like any other edit.

use serde_json::{Value, json};

use super::edit::Change;
use super::{Board, Element, IdSource, Shape, ShapeKind, TextBody};

/// Excalidraw's default text size for a new element.
pub const TEXT_FONT_SIZE: f64 = 20.0;
/// Excalidraw's `lineHeight` for its default font family.
pub const TEXT_LINE_HEIGHT: f64 = 1.25;
/// Excalifont, Excalidraw's default `fontFamily`.
pub const TEXT_FONT_FAMILY: i64 = 5;

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Text as Excalidraw stores it: `\r\n` and `\r` become `\n`.
fn normalise(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn usable(size: [f64; 2]) -> bool {
    size.iter().all(|v| v.is_finite() && *v >= 0.0)
}

impl Board {
    /// Place a new text element with its top-left corner at `at`. `size` is
    /// its measured width and height. Returns the creation, or `None`
    /// (adding nothing) for text that is empty or only whitespace.
    pub fn add_text(
        &mut self,
        at: [f64; 2],
        text: &str,
        size: [f64; 2],
        ids: &mut IdSource,
        now: i64,
    ) -> Option<Change> {
        let text = normalise(text);
        if text.trim().is_empty() || !at.iter().all(|v| v.is_finite()) || !usable(size) {
            return None;
        }
        let mut shape = Shape::drawn(ShapeKind::Text, ids, at[0], at[1], now);
        shape.width = round2(size[0]);
        shape.height = round2(size[1]);
        shape.text = Some(TextBody {
            text: text.clone(),
            font_size: TEXT_FONT_SIZE,
        });
        let mut put = |key: &str, value: Value| {
            shape.extra.insert(key.to_owned(), value);
        };
        put("fontFamily", json!(TEXT_FONT_FAMILY));
        put("textAlign", json!("left"));
        put("verticalAlign", json!("top"));
        put("containerId", Value::Null);
        put("originalText", json!(text));
        put("autoResize", json!(true));
        put("lineHeight", json!(TEXT_LINE_HEIGHT));
        while self.elements.iter().any(|e| e.id() == Some(&shape.id)) {
            shape.id = ids.id();
        }
        if let Some(index) = self.next_index() {
            shape.extra.insert("index".into(), Value::from(index));
        }
        let change = Change::created(&shape);
        self.elements.push(Element::from(shape));
        Some(change)
    }

    /// Replace the text of text element `id` (and its `originalText` and
    /// measured `size`). Empty text deletes the element. Returns the edit's
    /// changes: empty when the text did not change, or when `id` is not an
    /// editable text element (missing, deleted, locked, another kind).
    pub fn set_text(
        &mut self,
        id: &str,
        text: &str,
        size: [f64; 2],
        ids: &mut IdSource,
        now: i64,
    ) -> Vec<Change> {
        let text = normalise(text);
        let Some(current) = self.shape(id) else {
            return Vec::new();
        };
        if current.kind != ShapeKind::Text
            || current.is_deleted
            || current.is_locked()
            || !usable(size)
            || current.text.as_ref().is_some_and(|b| b.text == text)
        {
            return Vec::new();
        }
        let before = vec![current.clone()];
        for e in &mut self.elements {
            let Element::Shape(s) = e else { continue };
            if s.id != id {
                continue;
            }
            if text.trim().is_empty() {
                s.is_deleted = true;
            } else {
                if let Some(body) = &mut s.text {
                    body.text = text.clone();
                }
                s.extra.insert("originalText".to_owned(), json!(text));
                s.width = round2(size[0]);
                s.height = round2(size[1]);
            }
        }
        self.commit(&before, ids, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::History;

    const DRAW: &str = include_str!("../../tests/fixtures/board/draw.excalidraw");

    fn text_of(b: &Board, id: &str) -> String {
        b.shape(id).unwrap().text.as_ref().unwrap().text.clone()
    }

    #[test]
    fn placed_text_has_excalidraws_text_fields_and_round_trips() {
        let mut b = Board::from_json(DRAW).unwrap();
        let mut ids = IdSource::new(3);
        let c = b
            .add_text([40.0, 60.004], "héllo\r\nwörld", [91.4, 50.0], &mut ids, 7)
            .expect("added");
        let s = b.shape(&c.after.id).unwrap().clone();
        assert_eq!(s.kind, ShapeKind::Text);
        assert_eq!((s.x, s.y, s.width, s.height), (40.0, 60.0, 91.4, 50.0));
        assert_eq!(s.text.as_ref().unwrap().text, "héllo\nwörld");
        assert_eq!(s.text.as_ref().unwrap().font_size, 20.0);
        for (k, v) in [
            ("originalText", json!("héllo\nwörld")),
            ("fontFamily", json!(5)),
            ("textAlign", json!("left")),
            ("verticalAlign", json!("top")),
            ("containerId", Value::Null),
            ("autoResize", json!(true)),
            ("lineHeight", json!(1.25)),
            ("updated", json!(7)),
            ("roundness", Value::Null),
        ] {
            assert_eq!(s.extra.get(k), Some(&v), "{k}");
        }
        assert!(s.extra.contains_key("index"));
        let again = Board::from_json(&b.to_json()).unwrap();
        assert_eq!(again.shape(&s.id), Some(&s));
    }

    #[test]
    fn empty_text_adds_nothing_and_emptying_text_deletes_it() {
        let mut b = Board::from_json(DRAW).unwrap();
        let mut ids = IdSource::new(3);
        let n = b.elements.len();
        assert!(
            b.add_text([0.0, 0.0], "  \n ", [1.0, 25.0], &mut ids, 1)
                .is_none()
        );
        assert_eq!(b.elements.len(), n);
        let c = b.set_text("keep-text", "", [0.0, 25.0], &mut ids, 2);
        assert_eq!(c.len(), 1);
        assert!(b.shape("keep-text").unwrap().is_deleted);
        assert_eq!(
            text_of(&b, "keep-text"),
            "keep me",
            "the tombstone keeps its text"
        );
    }

    #[test]
    fn editing_text_bumps_once_and_undo_redo_bump_again() {
        let mut b = Board::from_json(DRAW).unwrap();
        let mut ids = IdSource::new(3);
        let v = b.shape("keep-text").unwrap().version;
        assert!(
            b.set_text("keep-text", "keep me", [1.0, 1.0], &mut ids, 2)
                .is_empty(),
            "unchanged"
        );
        let mut h = History::default();
        h.record(b.set_text("keep-text", "kept ✓", [60.0, 25.0], &mut ids, 3));
        let s = b.shape("keep-text").unwrap();
        assert_eq!((s.version, s.width), (v + 1, 60.0));
        assert_eq!(s.extra.get("originalText"), Some(&json!("kept ✓")));
        h.undo(&mut b, &mut ids, 4).unwrap();
        assert_eq!(text_of(&b, "keep-text"), "keep me");
        assert_eq!(b.shape("keep-text").unwrap().version, v + 2);
        h.redo(&mut b, &mut ids, 5).unwrap();
        assert_eq!(text_of(&b, "keep-text"), "kept ✓");
        assert_eq!(b.shape("keep-text").unwrap().version, v + 3);
    }

    #[test]
    fn undoing_placed_text_leaves_a_tombstone() {
        let mut b = Board::from_json(DRAW).unwrap();
        let mut ids = IdSource::new(3);
        let mut h = History::default();
        let c = b
            .add_text([0.0, 0.0], "x", [10.0, 25.0], &mut ids, 1)
            .unwrap();
        let id = c.after.id.clone();
        h.record(vec![c]);
        h.undo(&mut b, &mut ids, 2).unwrap();
        assert!(b.shape(&id).unwrap().is_deleted);
        h.redo(&mut b, &mut ids, 3).unwrap();
        assert!(!b.shape(&id).unwrap().is_deleted);
    }

    #[test]
    fn locked_or_other_elements_are_not_edited() {
        let mut b = Board::from_json(DRAW).unwrap();
        let mut ids = IdSource::new(3);
        assert!(
            b.set_text("keep-rect", "x", [1.0, 1.0], &mut ids, 1)
                .is_empty()
        );
        assert!(b.set_text("nope", "x", [1.0, 1.0], &mut ids, 1).is_empty());
    }
}

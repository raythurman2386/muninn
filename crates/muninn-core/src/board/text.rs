//! Text elements placed and edited in Muninn (Board slice 6).
//!
//! A new free-standing text element carries the fields Excalidraw's
//! `newTextElement` writes: `text` and `originalText` (the same string, since
//! Muninn does not wrap), `fontSize` 20, `fontFamily` 5 (Excalifont,
//! Excalidraw's default), `textAlign` `left`, `verticalAlign` `top`,
//! `lineHeight` 1.25, `containerId` null and `autoResize` true, plus the
//! common fields every drawn element gets ([`Shape::drawn`]). `width` and
//! `height` are the laid-out size the caller measured: the widest line, and
//! `lines × fontSize × lineHeight`.
//!
//! A label bound to a shape ([`Board::add_label_with`]) is the same element
//! with `containerId` set to the shape's id, `textAlign` `center` and
//! `verticalAlign` `middle` (what Excalidraw writes for bound text), placed
//! so its centre matches the shape's centre; the shape gains the text's id
//! in its `boundElements` as one edit with the label. That is Excalidraw's
//! own pair of keys, so the file round-trips: no custom field is added, and
//! any live binding already in the shape's `boundElements` stays (an entry
//! whose element was deleted is pruned, so a re-added label cannot be hidden
//! behind a tombstone).
//!
//! Text is never empty: placing empty text adds nothing, and emptying an
//! existing element deletes it (a tombstone, like any delete), as Excalidraw
//! drops a text element whose editor closes empty. Both edits go through
//! [`Board::commit`], so they bump `version`, `versionNonce` and `updated`
//! and undo like any other edit.

use std::collections::HashSet;

use serde_json::{Value, json};

use super::edit::Change;
use super::{Board, DEFAULT_BACKGROUND, Element, IdSource, Ink, Shape, ShapeKind, TextBody};

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

/// Where a new text element goes: at a scene point on its own, or centred in
/// the shape it is the bound label of.
enum Place<'a> {
    /// Free text, its top-left corner at this scene point.
    At([f64; 2]),
    /// A label centred in this shape (`containerId`).
    In(&'a str),
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
        self.add_text_with(at, text, size, Ink::DEFAULT, ids, now)
    }

    /// [`Board::add_text`] in the palette's ink: `ink.stroke` is the text's
    /// `strokeColor`, the colour Excalidraw draws text in. Text has no fill,
    /// so `ink.background` is ignored and `backgroundColor` stays
    /// `transparent`.
    pub fn add_text_with(
        &mut self,
        at: [f64; 2],
        text: &str,
        size: [f64; 2],
        ink: Ink<'_>,
        ids: &mut IdSource,
        now: i64,
    ) -> Option<Change> {
        self.add_text_in(Place::At(at), text, size, ink, ids, now)
    }

    /// [`Board::add_text_with`] at `place`: a scene point, or centred in the
    /// shape the text is the bound label of (`containerId`).
    fn add_text_in(
        &mut self,
        place: Place<'_>,
        text: &str,
        size: [f64; 2],
        ink: Ink<'_>,
        ids: &mut IdSource,
        now: i64,
    ) -> Option<Change> {
        let text = normalise(text);
        if text.trim().is_empty() || !usable(size) {
            return None;
        }
        let (at, container) = match place {
            Place::At(at) => (at, None),
            Place::In(container) => {
                let c = self.shape(container)?;
                let b = c.own_box();
                let centre = [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0];
                (
                    [
                        round2(centre[0] - size[0] / 2.0),
                        round2(centre[1] - size[1] / 2.0),
                    ],
                    Some(container),
                )
            }
        };
        if !at.iter().all(|v| v.is_finite()) {
            return None;
        }
        let mut shape = Shape::drawn(ShapeKind::Text, ids, at[0], at[1], now);
        shape.stroke_color = ink.stroke.to_owned();
        shape.background_color = DEFAULT_BACKGROUND.to_owned();
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
        // Excalidraw centres text inside a container and left-aligns free
        // text; the position written here is the same either way.
        put(
            "textAlign",
            json!(if container.is_some() {
                "center"
            } else {
                "left"
            }),
        );
        put(
            "verticalAlign",
            json!(if container.is_some() { "middle" } else { "top" }),
        );
        put("containerId", container.map_or(Value::Null, |id| json!(id)));
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

    /// Add a text label bound to the shape `container`: a text element with
    /// `containerId` set to it, centred on the shape, and the text's id
    /// added to the shape's `boundElements` (Excalidraw's own pair of keys;
    /// no custom field). Both changes are one edit, so one undo takes back
    /// the label and the binding together, and a later move, delete or
    /// snapshot of the shape carries the label ([`Board::with_labels`]).
    ///
    /// Adds nothing, returning no changes, for an empty label, and for a
    /// container that is missing, deleted, locked, itself text, or already
    /// has a live label (a double click edits that label instead). A label
    /// `size` is its measured width and height, as for [`Board::add_text`].
    pub fn add_label_with(
        &mut self,
        container: &str,
        text: &str,
        size: [f64; 2],
        ink: Ink<'_>,
        ids: &mut IdSource,
        now: i64,
    ) -> Vec<Change> {
        if text.trim().is_empty() || !usable(size) {
            return Vec::new();
        }
        let Some(before) = self
            .shape(container)
            .filter(|c| !c.is_deleted && !c.is_locked() && c.kind != ShapeKind::Text)
            .cloned()
        else {
            return Vec::new();
        };
        if self.bound_label(container).is_some() {
            return Vec::new();
        }
        let Some(created) = self.add_text_in(Place::In(container), text, size, ink, ids, now)
        else {
            return Vec::new();
        };
        let mut out = vec![created.clone()];
        self.bind_text(container, &created.after.id);
        out.extend(self.commit(&[before], ids, now));
        out
    }

    /// Add `id` to `container`'s `boundElements` as a `text` entry, as
    /// Excalidraw's `bindTextToContainer` does; anything already listed is
    /// kept.
    fn bind_text(&mut self, container: &str, id: &str) {
        // A label emptied earlier leaves its tombstoned text id in
        // `boundElements`; Excalidraw would resolve that id first and think the
        // shape is already labelled. Prune entries that no longer name a live
        // element before appending, keeping any other binding untouched.
        let live: HashSet<String> = self
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::Shape(s) if !s.is_deleted => Some(s.id.clone()),
                _ => None,
            })
            .collect();
        for e in &mut self.elements {
            let Element::Shape(s) = e else { continue };
            if s.id != container {
                continue;
            }
            let entry = json!({ "type": "text", "id": id });
            match s.extra.get_mut("boundElements") {
                Some(Value::Array(list)) => {
                    list.retain(|bound| {
                        bound
                            .get("id")
                            .and_then(Value::as_str)
                            .is_none_or(|bound| live.contains(bound))
                    });
                    list.push(entry);
                }
                _ => {
                    s.extra
                        .insert("boundElements".to_owned(), Value::Array(vec![entry]));
                }
            }
        }
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
        self.set_text_in(id, text, size, None, ids, now)
    }

    /// [`Board::set_text`] for a label bound to a shape (#40 review M2):
    /// the label is also re-centred on `centre`, its container's centre, as
    /// Excalidraw lays out bound text. `containerId` and the container's
    /// `boundElements` are kept as they are.
    pub fn set_label_text(
        &mut self,
        id: &str,
        text: &str,
        size: [f64; 2],
        centre: [f64; 2],
        ids: &mut IdSource,
        now: i64,
    ) -> Vec<Change> {
        self.set_text_in(id, text, size, Some(centre), ids, now)
    }

    fn set_text_in(
        &mut self,
        id: &str,
        text: &str,
        size: [f64; 2],
        centre: Option<[f64; 2]>,
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
                if let Some(c) = centre {
                    s.x = round2(c[0] - size[0] / 2.0);
                    s.y = round2(c[1] - size[1] / 2.0);
                }
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
    fn placed_text_takes_the_palettes_stroke_colour() {
        let mut b = Board::new();
        let mut ids = IdSource::new(3);
        let c = b
            .add_text_with(
                [0.0, 0.0],
                "blue",
                [30.0, 25.0],
                Ink {
                    stroke: "#1971c2",
                    background: "transparent",
                },
                &mut ids,
                4,
            )
            .expect("added");
        let s = b.shape(&c.after.id).unwrap();
        assert_eq!(s.stroke_color, "#1971c2");
        assert_eq!(s.background_color, "transparent");
        let again = Board::from_json(&b.to_json()).unwrap();
        assert_eq!(
            again.shape(&c.after.id).map(|s| s.stroke_color.as_str()),
            Some("#1971c2")
        );
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

    const LABEL: &str = include_str!("../../tests/fixtures/board/label.excalidraw");

    #[test]
    fn a_new_label_is_bound_centred_and_travels_with_its_shape() {
        let mut b = Board::from_json(LABEL).unwrap();
        let mut ids = IdSource::new(3);
        let mut h = History::default();
        let made = b.add_label_with(
            "other",
            "New\r\nlabel",
            [40.0, 20.0],
            Ink {
                stroke: "#1971c2",
                background: DEFAULT_BACKGROUND,
            },
            &mut ids,
            9,
        );
        assert_eq!(made.len(), 2, "the label and its container: one edit");
        let label = b.shape(&made[0].after.id).unwrap().clone();
        assert_eq!(label.kind, ShapeKind::Text);
        assert_eq!(label.text.as_ref().unwrap().text, "New\nlabel");
        assert_eq!(label.text.as_ref().unwrap().font_size, 20.0);
        assert_eq!(label.stroke_color, "#1971c2");
        assert_eq!(label.extra.get("containerId"), Some(&json!("other")));
        for (k, v) in [
            ("textAlign", json!("center")),
            ("verticalAlign", json!("middle")),
            ("fontFamily", json!(5)),
            ("lineHeight", json!(1.25)),
            ("updated", json!(9)),
        ] {
            assert_eq!(label.extra.get(k), Some(&v), "{k}");
        }
        // `other` is 60 × 40 at (300, 0): its centre is (330, 20).
        assert_eq!((label.x, label.y), (310.0, 10.0));
        let container = b.shape("other").unwrap().clone();
        assert_eq!(
            container.extra.get("boundElements"),
            Some(&json!([{ "type": "text", "id": made[0].after.id }]))
        );
        assert_eq!(container.version, 2, "the shape is bumped with the label");
        // The pair round-trips through the file unchanged.
        let saved = Board::from_json(&b.to_json()).unwrap();
        assert_eq!(saved.shape(&label.id), Some(&label));
        assert_eq!(saved.shape("other"), Some(&container));
        // A move and a delete carry the label, as for any bound label.
        h.record(made);
        let moved = b.move_by(&["other".to_owned()], 10.0, 5.0, &mut ids, 10);
        assert_eq!(moved.len(), 2);
        assert_eq!(b.shape(&label.id).map(|s| (s.x, s.y)), Some((320.0, 15.0)));
        h.record(b.delete(&["other".to_owned()], &mut ids, 11));
        assert!(b.shape(&label.id).unwrap().is_deleted);
        h.undo(&mut b, &mut ids, 12).unwrap();
        assert!(!b.shape(&label.id).unwrap().is_deleted);
        h.undo(&mut b, &mut ids, 13).unwrap();
        assert_eq!(
            b.shape(&label.id).map(|s| (s.x, s.y)),
            Some((310.0, 10.0)),
            "one undo puts the label back where it was made"
        );
    }

    #[test]
    fn a_label_is_not_created_for_nothing_or_for_a_shape_that_cannot_hold_one() {
        let mut b = Board::from_json(LABEL).unwrap();
        let mut ids = IdSource::new(3);
        let label = |b: &mut Board, ids: &mut IdSource, container: &str, text: &str| {
            b.add_label_with(container, text, [40.0, 20.0], Ink::DEFAULT, ids, 1)
        };
        assert!(label(&mut b, &mut ids, "other", "  \n ").is_empty());
        assert!(
            label(&mut b, &mut ids, "box", "second").is_empty(),
            "labelled"
        );
        assert!(label(&mut b, &mut ids, "label", "text").is_empty(), "text");
        assert!(label(&mut b, &mut ids, "nope", "gone").is_empty());
        if let Element::Shape(s) = &mut b.elements[2] {
            s.extra.insert("locked".into(), json!(true));
        }
        assert!(label(&mut b, &mut ids, "other", "locked").is_empty());
        assert_eq!(b.shape("other").unwrap().extra["locked"], json!(true));
        assert_eq!(
            b.live_elements().filter(|e| e.shape().is_some()).count(),
            3,
            "nothing was added"
        );
    }

    #[test]
    fn relabelling_after_an_emptied_label_replaces_the_stale_binding() {
        let mut b = Board::from_json(LABEL).unwrap();
        let mut ids = IdSource::new(3);
        let old = b.bound_label("box").expect("fixture label").id.clone();
        assert!(!b.set_text(&old, "", [0.0, 0.0], &mut ids, 5).is_empty());
        assert!(b.bound_label("box").is_none(), "the label is a tombstone");

        let made = b.add_label_with(
            "box",
            "Fresh",
            [30.0, 20.0],
            Ink {
                stroke: "#1971c2",
                background: DEFAULT_BACKGROUND,
            },
            &mut ids,
            9,
        );
        assert_eq!(made.len(), 2);
        let container = b.shape("box").unwrap();
        let text_ids: Vec<&str> = container.extra["boundElements"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|e| e.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(
            text_ids,
            vec![made[0].after.id.as_str()],
            "the tombstoned label's id must not linger in boundElements"
        );
        assert_eq!(b.bound_label("box").unwrap().id, made[0].after.id);
    }
}

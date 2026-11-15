//! Board slice 8 (#73): an element linked to a vault note.
//!
//! The link lives in Excalidraw's own `link` string on the element, not in a
//! private extra key, so excalidraw.com round-trips it and Muninn never has
//! to teach the format a new field. A note link is
//!
//! ```text
//! muninn://note/<percent-encoded vault-relative path>
//! ```
//!
//! where the path is the note's [`Note::stem`](crate::Note::stem): its path
//! below the vault root without the `.md` suffix, with `/` separators
//! (`projects/Plan`). The path is percent-encoded (RFC 3986 unreserved bytes
//! and `/` kept) so a folder, space or `#` in a name needs no escaping rules
//! of its own. Any other `link` value — an external `https://` URL, say — is
//! left exactly as the file wrote it and is never opened by Board.
//!
//! Setting or clearing a link is one edit for every selected element: each
//! touched element's `version` is bumped once, `versionNonce` and `updated`
//! change as for any other edit, and the whole set is one undo step. Clearing
//! writes Excalidraw's own `link: null` (the value its element factory uses)
//! and keeps every other field, including the element's other `extra` keys,
//! verbatim. Setting the same link again changes nothing and records no edit,
//! so an untouched file stays byte-identical.

use serde_json::Value;

use super::edit::Change;
use super::{Board, Element, IdSource, Shape};

/// The scheme and authority a board element's `link` uses for a vault note.
pub const NOTE_LINK_PREFIX: &str = "muninn://note/";

/// The `link` value for the vault-relative note path `path`.
///
/// Bytes outside RFC 3986's unreserved set (`A-Z`, `a-z`, `0-9`, `-`, `.`,
/// `_`, `~`) and `/` are percent-encoded as uppercase `%XX`, so
/// `projects/My Note` becomes `muninn://note/projects/My%20Note` and stays
/// UTF-8 through the round trip.
#[must_use]
pub fn note_link(path: &str) -> String {
    const HEX: [u8; 16] = *b"0123456789ABCDEF";
    let mut out = String::with_capacity(NOTE_LINK_PREFIX.len() + path.len());
    out.push_str(NOTE_LINK_PREFIX);
    for byte in path.bytes() {
        let plain =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/');
        if plain {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    out
}

/// The note path a `muninn://note/…` link names, or `None` for any other
/// `link` value (an external URL, an empty value, malformed escapes or bytes
/// that are not UTF-8).
#[must_use]
pub fn note_link_target(link: &str) -> Option<String> {
    let encoded = link.strip_prefix(NOTE_LINK_PREFIX)?;
    if encoded.is_empty() {
        return None;
    }
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let high = hex_digit(*bytes.get(i + 1)?)?;
            let low = hex_digit(*bytes.get(i + 2)?)?;
            out.push((high << 4) | low);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok().filter(|path| !path.is_empty())
}

/// One hexadecimal digit of a percent escape, in either case.
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

impl Shape {
    /// The vault-relative path of the note this element links to, when its
    /// `link` is a `muninn://note/…` value. An external URL is `None`.
    #[must_use]
    pub fn note_link(&self) -> Option<String> {
        self.extra
            .get("link")
            .and_then(Value::as_str)
            .and_then(note_link_target)
    }
}

impl Element {
    /// [`Shape::note_link`] for a modelled element, or the same read from a
    /// raw element's JSON (an element Muninn keeps verbatim can carry a link
    /// too, even though it is never edited).
    #[must_use]
    pub fn note_link(&self) -> Option<String> {
        match self {
            Element::Shape(shape) => shape.note_link(),
            Element::Raw(value) => value
                .get("link")
                .and_then(Value::as_str)
                .and_then(note_link_target),
        }
    }
}

impl Board {
    /// Set (`Some(path)`) or clear (`None`, or an empty path) the note link
    /// of the pickable elements in `ids`, as one edit.
    ///
    /// `path` is the note's vault-relative path without `.md`; it is encoded
    /// by [`note_link`]. Elements that are deleted, locked or not modelled are
    /// left alone. Clearing writes Excalidraw's `link: null` only where the
    /// current value is a note link: an element with no `link` key keeps its
    /// exact shape, and an external URL is left verbatim (there is no note
    /// link to clear). Versions are bumped by [`Board::commit`]; the returned
    /// changes are one undo step when recorded together
    /// ([`History::record`](super::History::record)).
    pub fn set_note_link(
        &mut self,
        ids: &[String],
        path: Option<&str>,
        src: &mut IdSource,
        now: i64,
    ) -> Vec<Change> {
        let before = self.snapshot(ids);
        let link = path.filter(|path| !path.is_empty()).map(note_link);
        for element in &mut self.elements {
            let Element::Shape(shape) = element else {
                continue;
            };
            if !ids.contains(&shape.id) || shape.is_deleted || shape.is_locked() {
                continue;
            }
            match &link {
                Some(value) => {
                    shape
                        .extra
                        .insert("link".to_owned(), Value::from(value.clone()));
                }
                None if shape
                    .extra
                    .get("link")
                    .and_then(Value::as_str)
                    .is_some_and(|link| note_link_target(link).is_some()) =>
                {
                    shape.extra.insert("link".to_owned(), Value::Null);
                }
                None => {}
            }
        }
        self.commit(&before, src, now)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::board::{History, ShapeKind};

    const SCENE: &str = include_str!("../../tests/fixtures/board/scene.excalidraw");

    fn board_with_two_boxes() -> (Board, IdSource, String, String) {
        let mut board = Board::new();
        let mut ids = IdSource::new(7);
        let a = board
            .add_drawn(
                ShapeKind::Rectangle,
                &[[0.0, 0.0], [100.0, 50.0]],
                &mut ids,
                1,
            )
            .expect("drawn")
            .id
            .clone();
        let b = board
            .add_drawn(
                ShapeKind::Ellipse,
                &[[200.0, 0.0], [300.0, 100.0]],
                &mut ids,
                1,
            )
            .expect("drawn")
            .id
            .clone();
        (board, ids, a, b)
    }

    #[test]
    fn a_note_link_percent_encodes_a_vault_relative_path() {
        assert_eq!(note_link("Plan"), "muninn://note/Plan");
        assert_eq!(
            note_link("projects/My Note #1"),
            "muninn://note/projects/My%20Note%20%231"
        );
        assert_eq!(note_link("Notizen/Über"), "muninn://note/Notizen/%C3%9Cber");
        // `/` stays a separator, so a path link reads as one.
        assert_eq!(
            note_link_target("muninn://note/projects/My%20Note%20%231").as_deref(),
            Some("projects/My Note #1")
        );
        assert_eq!(
            note_link_target("muninn://note/Notizen/%C3%9Cber").as_deref(),
            Some("Notizen/Über")
        );
    }

    #[test]
    fn only_well_formed_note_links_resolve() {
        assert_eq!(note_link_target("https://example.com/keep"), None);
        assert_eq!(note_link_target("muninn://note/"), None);
        assert_eq!(note_link_target("muninn://note/%ZZ"), None);
        assert_eq!(note_link_target("muninn://note/%FF"), None, "not UTF-8");
        assert_eq!(
            note_link_target("MUNINN://note/Plan"),
            None,
            "scheme is exact"
        );
        let mut shape = Shape::new(ShapeKind::Rectangle, "r", 0.0, 0.0);
        shape
            .extra
            .insert("link".into(), json!("https://example.com/keep"));
        assert_eq!(shape.note_link(), None);
        shape
            .extra
            .insert("link".into(), json!("muninn://note/projects/Plan"));
        assert_eq!(shape.note_link().as_deref(), Some("projects/Plan"));
    }

    #[test]
    fn setting_a_link_is_one_edit_and_a_clear_undoes_it() {
        let (mut board, mut src, a, b) = board_with_two_boxes();
        let mut history = History::default();
        let before_a = board.shape(&a).expect("a").clone();
        let before_b = board.shape(&b).expect("b").clone();
        history.record(board.set_note_link(
            std::slice::from_ref(&a),
            Some("projects/My Note"),
            &mut src,
            50,
        ));
        let after_a = board.shape(&a).expect("a");
        assert_eq!(
            after_a.extra.get("link"),
            Some(&json!("muninn://note/projects/My%20Note"))
        );
        assert_eq!(after_a.version, before_a.version + 1, "one bump");
        assert_eq!(after_a.extra["updated"], json!(50));
        assert_eq!(history.undo_len(), 1, "one history step");
        assert_eq!(board.shape(&b), Some(&before_b), "the rest is untouched");
        // Every field except the edit's own bookkeeping is as it was.
        let mut expected = before_a.clone();
        expected
            .extra
            .insert("link".into(), json!("muninn://note/projects/My%20Note"));
        expected.version = after_a.version;
        expected.version_nonce = after_a.version_nonce;
        expected.extra.insert("updated".into(), json!(50));
        assert_eq!(after_a, &expected);

        // Round trip: the file keeps the link, and reading it back resolves.
        let saved = board.to_json();
        let reloaded = Board::from_json(&saved).expect("reload");
        assert_eq!(reloaded, board);
        assert_eq!(
            reloaded.shape(&a).and_then(Shape::note_link).as_deref(),
            Some("projects/My Note")
        );

        let linked_version = after_a.version;
        history.record(board.set_note_link(std::slice::from_ref(&a), None, &mut src, 60));
        let cleared = board.shape(&a).expect("a");
        assert_eq!(cleared.extra.get("link"), Some(&Value::Null));
        assert_eq!(cleared.version, linked_version + 1);
        assert_eq!(history.undo_len(), 2);
        history.undo(&mut board, &mut src, 70).expect("undo");
        assert_eq!(
            board.shape(&a).and_then(Shape::note_link).as_deref(),
            Some("projects/My Note"),
            "undo puts the link back"
        );
        history.undo(&mut board, &mut src, 80).expect("undo");
        let cleared = board.shape(&a).expect("a");
        assert_eq!(
            cleared.extra.get("link"),
            Some(&Value::Null),
            "and then clears it"
        );
        assert_eq!(cleared.note_link(), None);
        // Undo is an edit of its own, so the version keeps growing; the
        // element's other fields are the ones it started with.
        assert_eq!(
            (cleared.x, cleared.y, cleared.width),
            (before_a.x, before_a.y, before_a.width)
        );
        assert_eq!(cleared.stroke_color, before_a.stroke_color);
    }

    #[test]
    fn setting_the_same_link_changes_nothing_and_keeps_the_bytes() {
        let (mut board, mut src, a, _) = board_with_two_boxes();
        let mut history = History::default();
        history.record(board.set_note_link(std::slice::from_ref(&a), Some("Plan"), &mut src, 10));
        let once = board.to_json();
        let version = board.shape(&a).expect("a").version;
        assert!(
            board
                .set_note_link(std::slice::from_ref(&a), Some("Plan"), &mut src, 20)
                .is_empty(),
            "the same link is not an edit"
        );
        assert_eq!(board.to_json(), once, "the file is byte-identical");
        assert_eq!(board.shape(&a).expect("a").version, version);
        assert_eq!(history.undo_len(), 1);
    }

    #[test]
    fn an_empty_path_clears_and_an_unlinked_element_gains_no_key() {
        let (mut board, mut src, a, _) = board_with_two_boxes();
        assert!(
            board
                .set_note_link(std::slice::from_ref(&a), Some("Plan"), &mut src, 1)
                .len()
                == 1
        );
        assert!(
            board
                .set_note_link(std::slice::from_ref(&a), Some(""), &mut src, 2)
                .len()
                == 1
        );
        assert_eq!(
            board.shape(&a).expect("a").extra.get("link"),
            Some(&Value::Null)
        );

        let (mut fresh, mut src, fresh_id, _) = board_with_two_boxes();
        for element in &mut fresh.elements {
            if let Element::Shape(shape) = element {
                shape.extra.remove("link");
            }
        }
        let once = fresh.to_json();
        assert!(
            fresh
                .set_note_link(std::slice::from_ref(&fresh_id), None, &mut src, 3)
                .is_empty()
        );
        assert_eq!(fresh.to_json(), once, "no link key is invented");
    }

    #[test]
    fn clearing_leaves_an_external_url_verbatim() {
        let (mut board, mut src, a, _) = board_with_two_boxes();
        if let Some(Element::Shape(shape)) = board
            .elements
            .iter_mut()
            .find(|e| e.id() == Some(a.as_str()))
        {
            shape
                .extra
                .insert("link".into(), json!("https://example.com/keep"));
        }
        let before = board.shape(&a).expect("a").clone();
        assert!(
            board
                .set_note_link(std::slice::from_ref(&a), None, &mut src, 5)
                .is_empty(),
            "there is no note link to clear"
        );
        assert_eq!(board.shape(&a), Some(&before), "the URL is untouched");
        assert_eq!(
            board.shape(&a).expect("a").extra["link"],
            json!("https://example.com/keep")
        );
    }

    #[test]
    fn a_linked_excalidraw_scene_round_trips_and_unchanged_files_stay_identical() {
        let mut board = Board::from_json(SCENE).expect("scene");
        let ellipse = board
            .elements
            .iter()
            .find_map(|e| e.shape().filter(|s| s.id == "ellipse-1"))
            .expect("ellipse");
        assert_eq!(ellipse.extra["link"], Value::Null);
        let mut src = IdSource::new(2);
        let edit = board.set_note_link(
            &["rectangle-0".to_owned()],
            Some("projects/Plan"),
            &mut src,
            99,
        );
        assert_eq!(edit.len(), 1);
        let json = board.to_json();
        let reloaded = Board::from_json(&json).expect("reload");
        assert_eq!(reloaded, board);
        assert_eq!(
            reloaded
                .shape("rectangle-0")
                .and_then(Shape::note_link)
                .as_deref(),
            Some("projects/Plan")
        );
        assert_eq!(
            reloaded.shape("ellipse-1").unwrap().extra["link"],
            Value::Null,
            "another element's link is preserved"
        );
        // A board read and written without an edit is stable, and a second
        // save is byte-identical.
        let tmp = crate::testing::TempVault::new(&[]);
        let path = tmp.path().join("Plan.excalidraw");
        std::fs::write(&path, SCENE).expect("write");
        let loaded = Board::load(&path).expect("load");
        loaded.save(&path).expect("save");
        let first = std::fs::read_to_string(&path).expect("read");
        Board::load(&path)
            .expect("reload")
            .save(&path)
            .expect("save");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), first);
    }
}

//! Board format and storage tests.
//!
//! Fixtures (`crates/muninn-core/tests/fixtures/board/`):
//!
//! - `scene.excalidraw`: elements produced by Excalidraw itself, taken from
//!   the vitest snapshots in the excalidraw/excalidraw repository (MIT,
//!   Copyright (c) 2020 Excalidraw) at commit 701b901a0f1b:
//!   rectangle/ellipse/diamond from `tests/__snapshots__/dragCreate.test.tsx.snap`,
//!   arrow/line from `multiPointCreate.test.tsx.snap`, and freedraw, text,
//!   image and frame from `history.test.tsx.snap`. They were converted from
//!   the snapshot format to JSON, given unique ids and offsets, and the
//!   `seed`/`versionNonce` that the history snapshots strip were filled in.
//!   The scene envelope (`appState`, `files` with a 1×1 PNG for the image) is
//!   hand-written to match Excalidraw's `serializeAsJSON` output.
//! - `legacy-smiley.excalidraw`: the scene embedded, verbatim, in
//!   `packages/excalidraw/tests/fixtures/smiley_embedded_v2.svg` from the same
//!   repository and commit (decoded from its compressed payload). It is an
//!   older real export with fields current Excalidraw no longer writes
//!   (`strokeSharpness`, `boundElementIds`, `baseline`) and no `files` key.
//! - `lasso-precision.excalidraw`: the 16 elements (lines and ellipses from a
//!   real drawing) of the "Selects group if any group from group is selected"
//!   case in `packages/excalidraw/tests/lasso.test.tsx`, same repository and
//!   commit, converted from the object literal to JSON with
//!   `JSON.stringify(scene, null, 2)` in Node and wrapped in the scene
//!   envelope. It carries 77 coordinates printed with 17 significant digits,
//!   the ones a lossy float parser drifts by one ULP.

use std::fs;

use serde_json::{Value, json};

use super::*;
use crate::testing::TempVault;

const SCENE: &str = include_str!("../../tests/fixtures/board/scene.excalidraw");
const LEGACY: &str = include_str!("../../tests/fixtures/board/legacy-smiley.excalidraw");
const PRECISION: &str = include_str!("../../tests/fixtures/board/lasso-precision.excalidraw");

fn parse(json: &str) -> Value {
    serde_json::from_str(json).expect("fixture json")
}

fn shape<'a>(board: &'a Board, id: &str) -> &'a Shape {
    board
        .elements
        .iter()
        .find_map(|e| e.shape().filter(|s| s.id == id))
        .unwrap_or_else(|| panic!("no shape {id}"))
}

#[test]
fn an_excalidraw_scene_round_trips_without_losing_anything() {
    let board = Board::from_json(SCENE).expect("scene");
    assert_eq!(board.version, 2);
    assert_eq!(board.source, "https://excalidraw.com");

    let kinds: Vec<&str> = board
        .elements
        .iter()
        .map(|e| match e {
            Element::Shape(s) => s.kind.as_str(),
            Element::Raw(v) => v["type"].as_str().unwrap_or("?"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "rectangle",
            "ellipse",
            "diamond",
            "arrow",
            "line",
            "freedraw",
            "text",
            "image",
            "frame"
        ]
    );
    // Every modelled kind is typed; image and frame are kept raw.
    let raw: Vec<_> = board
        .elements
        .iter()
        .filter(|e| matches!(e, Element::Raw(_)))
        .filter_map(Element::id)
        .collect();
    assert_eq!(raw, ["image-7", "frame-8"]);

    let arrow = shape(&board, "arrow-3");
    assert_eq!(arrow.points, [[0.0, 0.0], [20.0, 30.0], [70.0, 110.0]]);
    assert_eq!(arrow.stroke_color, "#1e1e1e");
    assert_eq!(arrow.extra["endArrowhead"], "arrow");
    let text = shape(&board, "text-6");
    assert_eq!(
        text.text,
        Some(TextBody {
            text: "que pasa".into(),
            font_size: 20.0
        })
    );
    assert_eq!(text.extra["lineHeight"], 1.25);
    let free = shape(&board, "freedraw-5");
    assert_eq!(free.points.len(), 4);
    assert!(free.extra.contains_key("pressures"));
    assert!(board.files.contains_key("id2"), "image bytes are kept");

    // The written JSON is the same value that was read, field for field.
    assert_eq!(board.to_value(), parse(SCENE));
    // And writing is stable: a second pass changes nothing.
    let again = Board::from_json(&board.to_json()).expect("reparse");
    assert_eq!(again, board);
    assert_eq!(again.to_json(), board.to_json());
}

#[test]
fn an_older_excalidraw_export_keeps_its_legacy_fields() {
    let board = Board::from_json(LEGACY).expect("legacy scene");
    let smiley = shape(&board, "SC8cylFGpzB0IwzyDYe_V");
    assert_eq!(smiley.kind, ShapeKind::Text);
    assert_eq!(smiley.text.as_ref().map(|t| t.text.as_str()), Some("😀"));
    assert_eq!(smiley.x, 886.0);
    assert_eq!(smiley.y, 357.5);
    for legacy in ["strokeSharpness", "boundElementIds", "baseline"] {
        assert!(smiley.extra.contains_key(legacy), "{legacy} kept");
    }
    // The only change is the `files` map Excalidraw's restore also adds.
    let mut expected = parse(LEGACY);
    expected["files"] = json!({});
    assert_eq!(board.to_value(), expected);
}

#[test]
fn edits_keep_unknown_fields_elements_and_top_level_keys() {
    let source = json!({
        "type": "excalidraw",
        "version": 2,
        "source": "https://excalidraw.com",
        "futureTopLevel": {"keep": [1, 2, 3]},
        "elements": [
            {"type": "rectangle", "id": "r", "x": 1, "y": 2, "width": 3, "height": 4,
             "customData": {"muninn": {"nested": true}}, "roughness": 0, "link": "https://x"},
            {"type": "magicframe", "id": "m", "x": 0, "y": 0, "unmodelled": [1.5, "two"]},
        ],
        "appState": {"viewBackgroundColor": "#123456", "gridSize": null},
        "files": {}
    });
    let mut board = Board::from_json(&source.to_string()).expect("scene");
    let Element::Shape(rect) = &mut board.elements[0] else {
        panic!("rectangle is modelled");
    };
    rect.x = 10.5;
    rect.version += 1;
    let mut arrow = Shape::new(ShapeKind::Arrow, "a", 0.0, 0.0);
    arrow.points = vec![[0.0, 0.0], [12.0, -3.25]];
    board.elements.push(arrow.into());

    let out = board.to_value();
    let mut expected = source.clone();
    expected["elements"][0]["x"] = json!(10.5);
    // Absent modelled fields are written with Excalidraw's defaults.
    for (key, value) in [
        ("angle", json!(0)),
        ("strokeColor", json!("#1e1e1e")),
        ("backgroundColor", json!("transparent")),
        ("strokeWidth", json!(2)),
        ("opacity", json!(100)),
        ("isDeleted", json!(false)),
        ("version", json!(2)),
        ("versionNonce", json!(0)),
        ("seed", json!(1)),
    ] {
        expected["elements"][0][key] = value;
    }
    assert_eq!(out["elements"][0], expected["elements"][0]);
    assert_eq!(
        out["elements"][1], source["elements"][1],
        "unknown kind verbatim"
    );
    assert_eq!(out["futureTopLevel"], source["futureTopLevel"]);
    assert_eq!(out["appState"], source["appState"]);
    assert_eq!(out["elements"][2]["type"], "arrow");
    assert_eq!(out["elements"][2]["points"], json!([[0, 0], [12, -3.25]]));
}

#[test]
fn elements_muninn_cannot_represent_stay_raw() {
    let source = json!({
        "type": "excalidraw",
        "version": 2,
        "elements": [
            {"type": "rectangle", "id": "x-is-text", "x": "10", "y": 0},
            {"type": "ellipse", "x": 0, "y": 0},
            {"type": "arrow", "id": "bad-point", "x": 0, "y": 0, "points": [[0, 0], [1]]},
            {"type": "text", "id": "bad-size", "x": 0, "y": 0, "text": "hi", "fontSize": "big"},
            {"type": "line", "id": "huge", "x": 9007199254740993_u64, "y": 0},
            {"type": "diamond", "id": "float-seed", "x": 0, "y": 0, "seed": 1.5},
            "not even an object",
        ]
    });
    let board = Board::from_json(&source.to_string()).expect("scene");
    assert!(
        board.elements.iter().all(|e| matches!(e, Element::Raw(_))),
        "{:?}",
        board.elements
    );
    assert_eq!(board.to_value()["elements"], source["elements"]);
}

#[test]
fn deleted_elements_are_kept_but_not_live() {
    let board = Board::from_json(
        &json!({"type": "excalidraw", "version": 2, "elements": [
            {"type": "rectangle", "id": "gone", "x": 0, "y": 0, "isDeleted": true},
            {"type": "image", "id": "gone-raw", "x": 0, "y": 0, "isDeleted": true},
            {"type": "ellipse", "id": "here", "x": 0, "y": 0, "isDeleted": false},
        ]})
        .to_string(),
    )
    .expect("scene");
    let live: Vec<_> = board.live_elements().filter_map(Element::id).collect();
    assert_eq!(live, ["here"]);
    assert_eq!(board.elements.len(), 3);
}

#[test]
fn only_excalidraw_scenes_load() {
    for bad in [
        "",
        "not json",
        "[]",
        r#"{"type": "excalidrawlib", "version": 2}"#,
        r#"{"version": 2, "elements": []}"#,
        r#"{"type": "excalidraw", "version": "2"}"#,
        r#"{"type": "excalidraw", "elements": {}}"#,
        r#"{"type": "excalidraw", "appState": []}"#,
        r#"{"type": "excalidraw", "source": 5}"#,
    ] {
        let err = Board::from_json(bad).expect_err(bad);
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{bad}");
    }
}

#[test]
fn a_new_board_is_an_empty_v2_scene() {
    let out = parse(&Board::new().to_json());
    assert_eq!(
        out,
        json!({
            "type": "excalidraw",
            "version": 2,
            "source": "muninn",
            "elements": [],
            "appState": {},
            "files": {}
        })
    );
    let mut text = Shape::new(ShapeKind::Text, "t", 1.0, 2.0);
    text.text = Some(TextBody {
        text: "hello".into(),
        font_size: 28.0,
    });
    let out = Element::from(text).to_value();
    assert_eq!(out["text"], "hello");
    assert_eq!(out["fontSize"], 28);
    assert!(out.get("points").is_none(), "text has no points");
}

#[test]
fn numbers_are_written_like_json_stringify() {
    assert_eq!(num(20.0).to_string(), "20");
    assert_eq!(num(-167.5).to_string(), "-167.5");
    assert_eq!(num(861.111_111_111_111_1).to_string(), "861.1111111111111");
    assert_eq!(num(f64::NAN).to_string(), "0");
    assert_eq!(num(1e300).to_string(), "1e+300");
}

/// Every decimal number in `json` with at least `digits` significant digits,
/// as written.
fn long_numbers(json: &str, digits: usize) -> Vec<&str> {
    let bytes = json.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let starts = bytes[i] == b'-' || bytes[i].is_ascii_digit();
        let after_word = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'.');
        if !starts || after_word {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || b".eE+-".contains(&bytes[i])) {
            i += 1;
        }
        let token = &json[start..i];
        let significant = token
            .split(['e', 'E'])
            .next()
            .unwrap_or_default()
            .trim_start_matches('-')
            .replace('.', "");
        if token.contains('.') && significant.trim_start_matches('0').len() >= digits {
            found.push(token);
        }
    }
    found
}

#[test]
fn seventeen_digit_coordinates_survive_load_and_save_exactly() {
    // The value that drifted before `float_roundtrip`: it read back as
    // 102.9599151611328, one ULP off.
    let drifted: f64 = serde_json::from_str("102.95991516113281").unwrap();
    assert_eq!(drifted.to_bits(), 102.959_915_161_132_81_f64.to_bits());

    let long = long_numbers(PRECISION, 17);
    assert_eq!(long.len(), 77, "fixture coordinates with 17 digits");

    let tmp = TempVault::new(&[]);
    let path = tmp.path().join("Lasso.excalidraw");
    fs::write(&path, PRECISION).unwrap();
    let board = Board::load(&path).unwrap();
    board.save(&path).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    // Muninn may reorder keys on its first save, so compare as sorted lists.
    let (mut before, mut after) = (long.clone(), long_numbers(&saved, 17));
    before.sort_unstable();
    after.sort_unstable();
    assert_eq!(after, before, "17-digit numbers rewritten by load -> save");
    assert_eq!(
        parse(&saved),
        parse(PRECISION),
        "the scene itself is unchanged"
    );

    Board::load(&path).unwrap().save(&path).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        saved,
        "a second save is byte-identical"
    );
}

#[test]
fn saving_is_atomic_and_leaves_no_temp_files() {
    let tmp = TempVault::new(&[("Note.md", "# Note")]);
    let path = tmp.path().join("Plan.excalidraw");
    let mut board = Board::from_json(SCENE).expect("scene");
    board.save(&path).expect("first save");
    assert_eq!(Board::load(&path).expect("load"), board);

    board.elements.truncate(1);
    board.save(&path).expect("overwrite");
    assert_eq!(Board::load(&path).expect("reload").elements.len(), 1);

    let mut names: Vec<_> = fs::read_dir(tmp.path())
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["Note.md", "Plan.excalidraw"], "no temp files left");

    // A save that cannot complete reports the error and writes nothing.
    let missing = tmp.path().join("no-such-folder").join("X.excalidraw");
    assert!(board.save(&missing).is_err());
    assert!(!missing.exists());
    assert_eq!(fs::read_dir(tmp.path()).expect("read dir").count(), 2);
}

#[test]
fn a_board_that_fails_to_parse_is_never_overwritten_by_load() {
    let tmp = TempVault::new(&[("Broken.excalidraw", "{ truncated")]);
    let path = tmp.path().join("Broken.excalidraw");
    assert_eq!(
        Board::load(&path).expect_err("broken").kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read_to_string(&path).expect("bytes"), "{ truncated");
}

#[test]
fn boards_are_listed_separately_from_notes() {
    let tmp = TempVault::new(&[
        ("Home.md", "[[Plan]]"),
        ("Plan.excalidraw", SCENE),
        ("projects/alpha/Sketch.EXCALIDRAW", SCENE),
        ("projects/Notes.md", "# Notes"),
        // Obsidian's Markdown-wrapped drawings are notes, not boards.
        ("Obsidian.excalidraw.md", "# Obsidian drawing"),
        // Hidden folders and files follow the note scan's rules.
        (".obsidian/Hidden.excalidraw", SCENE),
        (".muninn-trash/1/Trashed.excalidraw", SCENE),
        (".Plan.excalidraw.42.0.tmp", "{"),
        (".dotted.excalidraw", SCENE),
        ("scene.excalidrawlib", "{}"),
        ("projects/a.json", "{}"),
    ]);
    let vault = tmp.load();
    let boards = vault.boards();
    let rels: Vec<_> = boards.iter().map(|b| b.rel.as_str()).collect();
    assert_eq!(
        rels,
        ["Plan.excalidraw", "projects/alpha/Sketch.EXCALIDRAW"]
    );
    assert_eq!(boards[0].title, "Plan");
    assert_eq!(boards[0].folder(), "");
    assert_eq!(boards[1].title, "Sketch");
    assert_eq!(boards[1].folder(), "projects/alpha");
    assert_eq!(boards, list_boards(tmp.path()));

    // Boards never become notes, links or graph nodes.
    let notes: Vec<_> = (0..vault.notes.len())
        .map(|id| vault.note(id).rel.clone())
        .collect();
    assert_eq!(
        notes,
        ["Home.md", "Obsidian.excalidraw.md", "projects/Notes.md"]
    );
    assert_eq!(
        vault.resolve("Plan"),
        None,
        "[[Plan]] does not resolve to a board"
    );

    // The listing reads the disk, so a board saved after the scan shows up.
    Board::new()
        .save(tmp.path().join("projects/New.excalidraw"))
        .expect("save");
    assert_eq!(vault.boards().len(), 3);
}

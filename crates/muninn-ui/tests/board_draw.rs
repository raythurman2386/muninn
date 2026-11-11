//! Board slice 4: drawing tools and saving, headless.
//!
//! Strokes go in as pointer input on the canvas; the tests read the
//! in-memory board, the camera (scene ↔ canvas) and the file on disk.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_core::{Board, Element, ShapeKind};
use muninn_ui::{BoardStatus, BoardTool, Mode, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};
use serde_json::Value;

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");
const SMOKE: &str = include_str!("../../muninn-core/tests/fixtures/board/smoke.excalidraw");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-draw-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn vault() -> TempVault {
    TempVault::new(&[
        ("Alpha.md", "# Alpha\n"),
        ("Draw.excalidraw", DRAW),
        ("Smoke.excalidraw", SMOKE),
    ])
}

fn launch(root: &Path) -> Runtime<State> {
    let (config, state_home) = (unique_dir("config"), unique_dir("state"));
    let state = State::with_settings(
        root.to_path_buf(),
        state_home,
        config.clone(),
        Settings::load_from(&config),
    );
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn plain(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
}

fn text(t: &str) -> KeyEvent {
    plain(Key::Text(t.into()))
}

fn command(c: char) -> KeyEvent {
    KeyEvent {
        key: Key::Command(c),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    }
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 2);
}

fn open(rt: &mut Runtime<State>, title: &str) {
    if rt.state.mode != Mode::Board {
        rt.key(command('4'));
        frames(rt, 2);
    }
    let i = rt
        .state
        .boards
        .iter()
        .position(|b| b.file.title == title)
        .unwrap_or_else(|| panic!("no board {title}"));
    click(rt, &format!("board-row-{i}"));
    frames(rt, 60);
}

fn canvas_origin(rt: &Runtime<State>) -> [f32; 2] {
    let r = rt.presented("board-canvas").expect("board canvas").0;
    [r[0], r[1]]
}

/// Press at canvas point `path[0]`, move through the rest, release at the last.
fn stroke(rt: &mut Runtime<State>, path: &[[f32; 2]]) {
    let o = canvas_origin(rt);
    let at = |p: [f32; 2]| (o[0] + p[0], o[1] + p[1]);
    let (x, y) = at(path[0]);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    for w in path.windows(2) {
        for step in 1..=6 {
            let f = step as f32 / 6.0;
            let p = [
                w[0][0] + (w[1][0] - w[0][0]) * f,
                w[0][1] + (w[1][1] - w[0][1]) * f,
            ];
            let (x, y) = at(p);
            rt.pointer_move(x, y);
            rt.frame(1.0 / 60.0);
        }
    }
    let (x, y) = at(*path.last().unwrap());
    rt.pointer_up(x, y);
    frames(rt, 2);
}

fn scene(rt: &Runtime<State>, p: [f32; 2]) -> [f64; 2] {
    let s = rt.state.board_view().expect("open").scene(p);
    [f64::from(s[0]), f64::from(s[1])]
}

fn model(rt: &Runtime<State>) -> &Board {
    rt.state
        .board_open
        .as_ref()
        .and_then(|o| o.model.as_ref())
        .expect("model")
}

fn last(rt: &Runtime<State>) -> muninn_core::Shape {
    model(rt)
        .elements
        .last()
        .and_then(Element::shape)
        .cloned()
        .expect("a shape")
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 0.6
}

fn status(rt: &Runtime<State>) -> BoardStatus {
    rt.state.board_open.as_ref().expect("open").status.clone()
}

#[test]
fn tool_keys_and_the_toolbar_pick_tools_and_esc_drops_them() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt, "Draw");
    for (key, want) in [
        ("r", Some(BoardTool::Rectangle)),
        ("o", Some(BoardTool::Ellipse)),
        ("d", Some(BoardTool::Diamond)),
        ("l", Some(BoardTool::Line)),
        ("a", Some(BoardTool::Arrow)),
        ("p", Some(BoardTool::Freedraw)),
        ("v", None),
        ("x", Some(BoardTool::Freedraw)),
        ("1", None),
        ("2", Some(BoardTool::Rectangle)),
        ("7", Some(BoardTool::Freedraw)),
    ] {
        rt.key(text(key));
        frames(&mut rt, 1);
        assert_eq!(rt.state.board_tool, want, "key {key}");
    }
    click(&mut rt, "board-tool-ellipse");
    assert_eq!(rt.state.board_tool, Some(BoardTool::Ellipse));
    click(&mut rt, "board-tool-select");
    assert_eq!(rt.state.board_tool, None);
    click(&mut rt, "board-tool-arrow");
    rt.key(plain(Key::Escape));
    frames(&mut rt, 1);
    assert_eq!(rt.state.board_tool, None, "Esc drops the tool");
    assert_eq!(rt.state.mode, Mode::Board, "and stays on Board");
    rt.key(plain(Key::Escape));
    frames(&mut rt, 1);
    assert_ne!(rt.state.mode, Mode::Board, "a second Esc leaves Board");
}

#[test]
fn each_tool_draws_its_element_where_the_pointer_went() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt, "Draw");
    let disk = fs::read_to_string(tmp.path().join("Draw.excalidraw")).unwrap();
    let before = model(&rt).elements.len();

    let boxes = [
        ("r", ShapeKind::Rectangle, [110.0, 140.0], [230.0, 240.0]),
        ("o", ShapeKind::Ellipse, [380.0, 240.0], [260.0, 140.0]),
        ("d", ShapeKind::Diamond, [410.0, 140.0], [530.0, 240.0]),
    ];
    for (key, kind, a, b) in boxes {
        rt.key(text(key));
        stroke(&mut rt, &[a, [a[0] + 30.0, a[1] + 20.0], b]);
        let s = last(&rt);
        assert_eq!(s.kind, kind);
        let (p, q) = (scene(&rt, a), scene(&rt, b));
        assert!(
            close(s.x, p[0].min(q[0])) && close(s.y, p[1].min(q[1])),
            "{kind:?} at {},{}",
            s.x,
            s.y
        );
        assert!(close(s.width, (q[0] - p[0]).abs()) && close(s.height, (q[1] - p[1]).abs()));
    }
    for (key, kind, a, b) in [
        ("l", ShapeKind::Line, [110.0, 300.0], [230.0, 400.0]),
        ("a", ShapeKind::Arrow, [260.0, 350.0], [380.0, 350.0]),
    ] {
        rt.key(text(key));
        stroke(&mut rt, &[a, [a[0] + 10.0, a[1] + 40.0], b]);
        let s = last(&rt);
        assert_eq!(s.kind, kind);
        let (p, q) = (scene(&rt, a), scene(&rt, b));
        assert!(close(s.x, p[0]) && close(s.y, p[1]));
        assert_eq!(s.points.len(), 2, "a line runs from press to release");
        assert!(close(s.points[1][0], q[0] - p[0]) && close(s.points[1][1], q[1] - p[1]));
    }
    rt.key(text("p"));
    let zigzag = [
        [410.0, 300.0],
        [440.0, 400.0],
        [470.0, 300.0],
        [500.0, 400.0],
        [530.0, 300.0],
    ];
    stroke(&mut rt, &zigzag);
    let pen = last(&rt);
    assert_eq!(pen.kind, ShapeKind::Freedraw);
    assert!(pen.points.len() >= 20, "the stroke keeps its path");
    let start = scene(&rt, zigzag[0]);
    for corner in zigzag {
        let c = scene(&rt, corner);
        assert!(
            pen.points
                .iter()
                .any(|p| close(start[0] + p[0], c[0]) && close(start[1] + p[1], c[1])),
            "passes through {corner:?}"
        );
    }
    let span = scene(&rt, [530.0, 400.0]);
    assert!(close(pen.width, span[0] - start[0]) && close(pen.height, span[1] - start[1]));

    assert_eq!(model(&rt).elements.len(), before + 6);
    assert_eq!(status(&rt), BoardStatus::Unsaved);
    assert!(rt.find("board-save-state").is_some());
    assert_eq!(
        fs::read_to_string(tmp.path().join("Draw.excalidraw")).unwrap(),
        disk,
        "drawing alone writes nothing"
    );
    // Ids are unique and indexes follow the fixture's a0…a4.
    let indexes: Vec<String> = model(&rt).elements[before..]
        .iter()
        .map(|e| {
            e.shape().unwrap().extra["index"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(indexes, ["a5", "a6", "a7", "a8", "a9", "aA"]);
}

#[test]
fn a_click_draws_nothing_and_no_tool_still_pans() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt, "Draw");
    let n = model(&rt).elements.len();
    rt.key(text("r"));
    stroke(&mut rt, &[[200.0, 200.0], [201.0, 200.5]]);
    assert_eq!(model(&rt).elements.len(), n, "a click is not a rectangle");
    rt.key(text("v"));
    let view = rt.state.board_view().unwrap();
    stroke(&mut rt, &[[200.0, 200.0], [260.0, 230.0]]);
    assert_eq!(model(&rt).elements.len(), n);
    assert_ne!(rt.state.board_view().unwrap(), view, "the drag panned");
    assert_eq!(status(&rt), BoardStatus::Clean);
}

#[test]
fn command_s_saves_atomically_and_keeps_every_old_element() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let original = Board::from_json(DRAW).unwrap();
    let mut rt = launch(tmp.path());
    open(&mut rt, "Draw");
    rt.key(text("d"));
    stroke(&mut rt, &[[300.0, 300.0], [380.0, 360.0]]);
    rt.key(command('s'));
    frames(&mut rt, 2);
    assert_eq!(status(&rt), BoardStatus::Saved);

    let saved_text = fs::read_to_string(&path).unwrap();
    let saved = Board::from_json(&saved_text).unwrap();
    assert_eq!(saved.elements.len(), original.elements.len() + 1);
    assert_eq!(
        &saved.elements[..original.elements.len()],
        &original.elements[..]
    );
    assert_eq!(
        (saved.app_state.clone(), saved.extra.clone()),
        (original.app_state.clone(), original.extra.clone())
    );
    // The new element is Excalidraw-shaped JSON.
    let v: Value = serde_json::from_str(&saved_text).unwrap();
    let new = v["elements"].as_array().unwrap().last().unwrap();
    for key in [
        "id",
        "type",
        "x",
        "y",
        "width",
        "height",
        "angle",
        "strokeColor",
        "backgroundColor",
        "fillStyle",
        "strokeWidth",
        "strokeStyle",
        "roughness",
        "opacity",
        "groupIds",
        "frameId",
        "index",
        "roundness",
        "seed",
        "version",
        "versionNonce",
        "isDeleted",
        "boundElements",
        "updated",
        "link",
        "locked",
    ] {
        assert!(new.get(key).is_some(), "new element has {key}");
    }
    assert_eq!(new["type"], "diamond");
    // Bytes on disk are exactly what s1's save writes for this board.
    assert_eq!(saved_text, saved.to_json());
    // No temp file is left behind, and the list shows the new count.
    let names: Vec<String> = fs::read_dir(tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().all(|n| !n.starts_with('.')), "{names:?}");
    let row = rt
        .state
        .boards
        .iter()
        .find(|b| b.file.title == "Draw")
        .unwrap();
    assert_eq!(row.detail(), "5 elements · 1 kept as-is");
    // Smoke was never touched.
    assert_eq!(
        fs::read_to_string(tmp.path().join("Smoke.excalidraw")).unwrap(),
        SMOKE
    );
}

#[test]
fn unsaved_drawing_survives_a_mode_switch_and_is_saved_before_another_board_opens() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let mut rt = launch(tmp.path());
    open(&mut rt, "Draw");
    rt.key(text("r"));
    stroke(&mut rt, &[[300.0, 300.0], [380.0, 360.0]]);
    let n = model(&rt).elements.len();
    rt.key(command('1'));
    frames(&mut rt, 2);
    rt.key(command('4'));
    frames(&mut rt, 2);
    assert_eq!(
        model(&rt).elements.len(),
        n,
        "coming back does not re-read over it"
    );
    assert_eq!(status(&rt), BoardStatus::Unsaved);
    assert_eq!(fs::read_to_string(&path).unwrap(), DRAW);
    open(&mut rt, "Smoke");
    let saved = Board::load(&path).unwrap();
    assert_eq!(saved.elements.len(), n, "saved on the way to Smoke");
    assert!(
        rt.state
            .board_open
            .as_ref()
            .unwrap()
            .path
            .ends_with("Smoke.excalidraw"),
        "and Smoke is open"
    );
}

#[test]
fn closing_the_app_saves_unsaved_drawing() {
    let tmp = vault();
    let mut rt = launch(tmp.path());
    open(&mut rt, "Draw");
    rt.key(text("l"));
    stroke(&mut rt, &[[300.0, 300.0], [380.0, 360.0]]);
    rt.state.flush_before_quit();
    let saved = Board::load(tmp.path().join("Draw.excalidraw")).unwrap();
    assert_eq!(
        saved.elements.last().and_then(Element::shape).unwrap().kind,
        ShapeKind::Line
    );
}

#[test]
fn a_file_changed_on_disk_is_never_overwritten() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let mut rt = launch(tmp.path());
    open(&mut rt, "Draw");
    rt.key(text("r"));
    stroke(&mut rt, &[[300.0, 300.0], [380.0, 360.0]]);
    let theirs = DRAW.replace("kept as-is", "edited elsewhere");
    fs::write(&path, &theirs).unwrap();
    rt.key(command('s'));
    frames(&mut rt, 2);
    assert!(matches!(status(&rt), BoardStatus::Failed(ref e) if e.contains("changed on disk")));
    assert_eq!(fs::read_to_string(&path).unwrap(), theirs);
    // Neither another board nor another vault can drop the unsaved drawing.
    let i = rt
        .state
        .boards
        .iter()
        .position(|b| b.file.title == "Smoke")
        .unwrap();
    click(&mut rt, &format!("board-row-{i}"));
    assert!(
        rt.state
            .board_open
            .as_ref()
            .unwrap()
            .path
            .ends_with("Draw.excalidraw")
    );
    let other = TempVault::new(&[("Beta.md", "# Beta\n")]);
    assert!(!rt.state.switch_vault(other.path().to_path_buf()));
    assert_eq!(rt.state.vault.root, tmp.path());
    assert_eq!(fs::read_to_string(&path).unwrap(), theirs);
    // A removed file is not recreated either.
    fs::remove_file(&path).unwrap();
    rt.key(command('s'));
    frames(&mut rt, 2);
    assert!(matches!(status(&rt), BoardStatus::Failed(ref e) if e.contains("removed")));
    assert!(!path.exists());
}

#[test]
fn switching_vaults_saves_into_the_old_vault_only() {
    let tmp = vault();
    let other = TempVault::new(&[("Beta.md", "# Beta\n"), ("Draw.excalidraw", DRAW)]);
    let mut rt = launch(tmp.path());
    open(&mut rt, "Draw");
    rt.key(text("o"));
    stroke(&mut rt, &[[300.0, 300.0], [380.0, 360.0]]);
    assert!(rt.state.switch_vault(other.path().to_path_buf()));
    rt.invalidate();
    frames(&mut rt, 2);
    let saved = Board::load(tmp.path().join("Draw.excalidraw")).unwrap();
    assert_eq!(
        saved.elements.last().and_then(Element::shape).unwrap().kind,
        ShapeKind::Ellipse
    );
    assert_eq!(
        fs::read_to_string(other.path().join("Draw.excalidraw")).unwrap(),
        DRAW
    );
    assert!(rt.state.board_open.is_none());
}

#[test]
fn a_board_from_another_vault_is_refused_by_save() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let mut rt = launch(tmp.path());
    open(&mut rt, "Draw");
    rt.key(text("r"));
    stroke(&mut rt, &[[300.0, 300.0], [380.0, 360.0]]);
    // As if the open board had been opened from a different vault.
    rt.state.board_open.as_mut().unwrap().vault_root = PathBuf::from("/elsewhere");
    assert!(!rt.state.save_board());
    assert!(matches!(status(&rt), BoardStatus::Failed(ref e) if e.contains("another vault")));
    assert_eq!(fs::read_to_string(&path).unwrap(), DRAW);
}

#[test]
fn a_board_that_cannot_be_saved_refuses_a_vault_rescan() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let mut rt = launch(tmp.path());
    open(&mut rt, "Draw");
    rt.key(text("r"));
    stroke(&mut rt, &[[300.0, 300.0], [380.0, 360.0]]);
    let drawn = model(&rt).elements.len();
    // Someone else edits the board, so the flush before the rescan fails.
    let theirs = DRAW.replace("kept as-is", "edited elsewhere");
    fs::write(&path, &theirs).unwrap();
    fs::write(tmp.path().join("Gamma.md"), "# Gamma\n").unwrap();
    assert!(!rt.state.retry_startup(), "the rescan must be refused");
    frames(&mut rt, 2);
    assert!(
        rt.state
            .error
            .as_deref()
            .is_some_and(|e| e.contains("board"))
    );
    // The drawing, the open board and the old vault are all still there.
    let open = rt.state.board_open.as_ref().expect("board stays open");
    assert!(open.path.ends_with("Draw.excalidraw"));
    assert_eq!(model(&rt).elements.len(), drawn);
    assert!(rt.state.board_dirty());
    assert!(
        !rt.state
            .vault
            .notes
            .iter()
            .any(|n| n.rel.ends_with("Gamma.md"))
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), theirs);
    // Once the file is back as it was loaded, the rescan saves the drawing
    // into its own board first and then goes ahead.
    fs::write(&path, DRAW).unwrap();
    assert!(rt.state.retry_startup());
    frames(&mut rt, 2);
    assert!(
        rt.state
            .vault
            .notes
            .iter()
            .any(|n| n.rel.ends_with("Gamma.md"))
    );
    let saved = Board::load(&path).unwrap();
    assert_eq!(saved.elements.len(), drawn);
}

#[test]
fn a_board_that_cannot_be_saved_refuses_a_large_chunked_rescan() {
    let mut files: Vec<(String, String)> = (0..130)
        .map(|i| (format!("n{i:03}.md"), format!("# N{i}\n")))
        .collect();
    files.push(("Draw.excalidraw".into(), DRAW.into()));
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let tmp = TempVault::new(&refs);
    let path = tmp.path().join("Draw.excalidraw");
    let mut rt = launch(tmp.path());
    frames(&mut rt, 20);
    open(&mut rt, "Draw");
    rt.key(text("r"));
    stroke(&mut rt, &[[300.0, 300.0], [380.0, 360.0]]);
    let drawn = model(&rt).elements.len();
    fs::remove_file(&path).unwrap();
    assert!(!rt.state.retry_startup());
    assert!(!rt.state.loading(), "no chunked scan may start");
    frames(&mut rt, 20);
    assert_eq!(model(&rt).elements.len(), drawn);
    assert!(rt.state.board_dirty());
    assert!(!path.exists(), "a removed board is not recreated");
}

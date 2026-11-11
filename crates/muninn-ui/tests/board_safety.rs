//! #38 review: a board with unsaved drawing is never dropped or lost.
//!
//! B1: the file is removed, renamed or changed outside Muninn while the
//! drawing is unsaved. B2: such a board cannot be saved normally, so it gets
//! a recovery copy (as notes do), an explicit Keep mine / Save copy / Load
//! theirs choice, and quitting keeps the copy (or is refused). Also: board
//! and graph keys never fire while a text field has focus.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::Board;
use muninn_core::testing::TempVault;
use muninn_ui::{BoardStatus, Mode, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-board-safety-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn vault() -> TempVault {
    TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", DRAW)])
}

struct Session {
    rt: Runtime<State>,
    config: PathBuf,
    state_home: PathBuf,
}

fn launch_with(root: &Path, config: &Path, state_home: &Path) -> Runtime<State> {
    let state = State::with_settings(
        root.to_path_buf(),
        state_home.to_path_buf(),
        config.to_path_buf(),
        Settings::load_from(config),
    );
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn launch(root: &Path) -> Session {
    let (config, state_home) = (unique_dir("config"), unique_dir("state"));
    Session {
        rt: launch_with(root, &config, &state_home),
        config,
        state_home,
    }
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

fn mode(rt: &mut Runtime<State>, c: char) {
    rt.key(command(c));
    frames(rt, 3);
}

fn open_draw(rt: &mut Runtime<State>) {
    mode(rt, '4');
    let i = rt
        .state
        .boards
        .iter()
        .position(|b| b.file.title == "Draw")
        .expect("Draw");
    click(rt, &format!("board-row-{i}"));
    frames(rt, 60);
}

/// Draw one rectangle with the R tool, away from the fixture's elements.
fn draw_rect(rt: &mut Runtime<State>) {
    rt.key(plain(Key::Text("r".into())));
    frames(rt, 1);
    let r = rt.presented("board-canvas").expect("canvas").0;
    let (x0, y0) = (r[0] + 200.0, r[1] + 200.0);
    rt.pointer_down(x0, y0);
    rt.frame(1.0 / 60.0);
    for i in 1..=6 {
        let f = i as f32 / 6.0;
        rt.pointer_move(x0 + 80.0 * f, y0 + 50.0 * f);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x0 + 80.0, y0 + 50.0);
    frames(rt, 2);
    rt.key(plain(Key::Text("v".into())));
    frames(rt, 1);
}

fn elements(rt: &Runtime<State>) -> usize {
    rt.state
        .board_open
        .as_ref()
        .and_then(|o| o.model.as_ref())
        .map_or(0, |m| m.elements.len())
}

fn status(rt: &Runtime<State>) -> BoardStatus {
    rt.state
        .board_open
        .as_ref()
        .expect("a board is open")
        .status
        .clone()
}

fn texts(rt: &Runtime<State>, key: &str) -> bool {
    rt.find(key).is_some()
}

fn n_draw() -> usize {
    Board::from_json(DRAW).unwrap().elements.len()
}

#[test]
fn a_dirty_board_whose_file_is_removed_stays_open_and_keep_mine_writes_it_back() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let mut s = launch(tmp.path());
    open_draw(&mut s.rt);
    draw_rect(&mut s.rt);
    assert_eq!(elements(&s.rt), n_draw() + 1);
    fs::remove_file(&path).unwrap();
    // The watcher's reload (every 1.5 s) in Board mode...
    frames(&mut s.rt, 150);
    // ...and leaving Board and coming back both re-list the boards.
    mode(&mut s.rt, '1');
    mode(&mut s.rt, '4');
    assert!(
        s.rt.state.board_open.is_some(),
        "the unsaved drawing was dropped"
    );
    assert_eq!(elements(&s.rt), n_draw() + 1, "and still holds the drawing");
    assert!(
        texts(&s.rt, "board-conflict"),
        "the heading flags the board"
    );
    s.rt.key(command('s'));
    frames(&mut s.rt, 2);
    assert!(
        matches!(status(&s.rt), BoardStatus::Failed(ref e) if e.contains("removed")),
        "{:?}",
        status(&s.rt)
    );
    assert!(
        !path.exists(),
        "a normal save never recreates a removed file"
    );
    click(&mut s.rt, "board-keep-mine");
    assert_eq!(status(&s.rt), BoardStatus::Saved);
    let back = Board::load(&path).expect("written back");
    assert_eq!(back.elements.len(), n_draw() + 1);
    assert!(!texts(&s.rt, "board-conflict"));
    assert!(s.rt.state.boards.iter().any(|b| b.file.path == path));
}

#[test]
fn a_dirty_board_whose_file_is_renamed_can_be_saved_as_a_copy() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let moved = tmp.path().join("Moved.excalidraw");
    let mut s = launch(tmp.path());
    open_draw(&mut s.rt);
    draw_rect(&mut s.rt);
    fs::rename(&path, &moved).unwrap();
    mode(&mut s.rt, '1');
    mode(&mut s.rt, '4');
    assert_eq!(elements(&s.rt), n_draw() + 1, "the drawing is kept");
    click(&mut s.rt, "board-save-copy");
    assert_eq!(status(&s.rt), BoardStatus::Saved);
    let copy = s.rt.state.board_open.as_ref().unwrap().path.clone();
    let name = copy.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("Draw (conflict ") && name.ends_with(").excalidraw"),
        "{name}"
    );
    assert!(copy.starts_with(tmp.path()));
    assert_eq!(Board::load(&copy).unwrap().elements.len(), n_draw() + 1);
    assert_eq!(
        fs::read_to_string(&moved).unwrap(),
        DRAW,
        "the renamed file is untouched"
    );
    assert!(!path.exists());
    assert_eq!(s.rt.state.board_selected.as_ref(), Some(&copy));
    assert!(s.rt.state.boards.iter().any(|b| b.file.path == copy));
}

#[test]
fn a_dirty_board_changed_on_disk_offers_keep_mine_save_copy_and_load_theirs() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let mut s = launch(tmp.path());
    open_draw(&mut s.rt);
    draw_rect(&mut s.rt);
    let theirs = DRAW.replace("keep me", "theirs");
    fs::write(&path, &theirs).unwrap();
    mode(&mut s.rt, '1');
    mode(&mut s.rt, '4');
    assert_eq!(elements(&s.rt), n_draw() + 1);
    assert!(texts(&s.rt, "board-conflict"));
    assert!(texts(&s.rt, "board-keep-mine") && texts(&s.rt, "board-save-copy"));
    let rec = s.rt.state.recovery_path("Draw.excalidraw");
    assert!(
        rec.is_file(),
        "a dirty board has a recovery copy, like a note"
    );
    click(&mut s.rt, "board-load-theirs");
    assert_eq!(status(&s.rt), BoardStatus::Clean);
    assert_eq!(elements(&s.rt), n_draw());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        theirs,
        "theirs stays as it was"
    );
    frames(&mut s.rt, 2);
    assert!(!rec.exists(), "discarding the drawing removes its copy");
}

#[test]
fn quitting_with_an_unsaveable_board_keeps_a_recovery_copy_that_restores() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let mut s = launch(tmp.path());
    open_draw(&mut s.rt);
    draw_rect(&mut s.rt);
    fs::remove_file(&path).unwrap();
    s.rt.state.flush_before_quit();
    let rec = s.rt.state.recovery_path("Draw.excalidraw");
    let copy = Board::from_json(&fs::read_to_string(&rec).expect("a recovery copy"))
        .expect("the copy is a board");
    assert_eq!(copy.elements.len(), n_draw() + 1);
    assert!(!path.exists());
    drop(s.rt);

    let mut rt = launch_with(tmp.path(), &s.config, &s.state_home);
    frames(&mut rt, 2);
    assert!(
        rt.state
            .recovered
            .iter()
            .any(|r| r.rel == "Draw.excalidraw"),
        "{:?}",
        rt.state.recovered
    );
    let i = rt
        .state
        .recovered
        .iter()
        .position(|r| r.rel == "Draw.excalidraw")
        .unwrap();
    click(&mut rt, &format!("recovery-restore-{i}"));
    frames(&mut rt, 10);
    assert_eq!(rt.state.mode, Mode::Board);
    assert_eq!(elements(&rt), n_draw() + 1, "the drawing is back");
    assert_eq!(status(&rt), BoardStatus::Unsaved);
    assert!(
        !tmp.path().join("Draw.excalidraw.md").exists(),
        "not restored as a note"
    );
    assert!(texts(&rt, "board-conflict"), "still flagged as removed");
    click(&mut rt, "board-keep-mine");
    assert_eq!(Board::load(&path).unwrap().elements.len(), n_draw() + 1);
    frames(&mut rt, 2);
    assert!(!rec.exists(), "saved, so the copy is gone");
}

#[test]
fn quitting_is_refused_when_the_board_can_neither_be_saved_nor_copied() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let config = unique_dir("config");
    // A state "directory" that is a file: no recovery copy can be written.
    let state_home = unique_dir("state").join("not-a-dir");
    fs::write(&state_home, "x").unwrap();
    let mut rt = launch_with(tmp.path(), &config, &state_home);
    open_draw(&mut rt);
    draw_rect(&mut rt);
    fs::remove_file(&path).unwrap();
    let quit = rt.state.flush_before_quit();
    let error = rt.state.error.clone().unwrap_or_default();
    assert!(
        error.contains("not saved") && error.contains("recovery"),
        "the refusal says why: {error:?}"
    );
    assert!(!quit && !rt.state.can_quit(), "the quit is refused");
    assert_eq!(elements(&rt), n_draw() + 1, "and the drawing is still here");
}

#[test]
fn a_dirty_board_keeps_a_recovery_copy_until_it_is_saved() {
    let tmp = vault();
    let mut s = launch(tmp.path());
    open_draw(&mut s.rt);
    let rec = s.rt.state.recovery_path("Draw.excalidraw");
    assert!(!rec.exists(), "a clean board has none");
    draw_rect(&mut s.rt);
    frames(&mut s.rt, 2);
    let copy = Board::from_json(&fs::read_to_string(&rec).expect("copy")).unwrap();
    assert_eq!(copy.elements.len(), n_draw() + 1);
    s.rt.key(command('s'));
    frames(&mut s.rt, 2);
    assert_eq!(status(&s.rt), BoardStatus::Saved);
    assert!(!rec.exists());
}

#[test]
fn typing_in_the_sidebar_search_on_board_never_picks_a_tool_or_zooms() {
    let tmp = vault();
    let mut s = launch(tmp.path());
    open_draw(&mut s.rt);
    let view = s.rt.state.board_view();
    click(&mut s.rt, "search-box");
    assert!(s.rt.state.search.focused);
    for c in "road1x+0".chars() {
        s.rt.key(plain(Key::Text(c.to_string())));
        frames(&mut s.rt, 1);
    }
    assert_eq!(s.rt.state.search.text, "road1x+0");
    assert_eq!(s.rt.state.board_tool, None, "no tool was picked");
    assert_eq!(s.rt.state.board_view(), view, "no zoom or fit");
    // Once the search lets go of the keyboard, the letters are tools again.
    s.rt.key(plain(Key::Escape));
    frames(&mut s.rt, 2);
    s.rt.key(plain(Key::Text("r".into())));
    frames(&mut s.rt, 1);
    assert!(s.rt.state.board_tool.is_some());
}

#[test]
fn typing_in_the_sidebar_search_on_graph_never_fires_graph_keys() {
    let tmp = vault();
    let mut s = launch(tmp.path());
    mode(&mut s.rt, '3');
    assert_eq!(s.rt.state.mode, Mode::Graph);
    click(&mut s.rt, "search-box");
    assert!(s.rt.state.search.focused);
    for c in "f0+-i".chars() {
        s.rt.key(plain(Key::Text(c.to_string())));
        frames(&mut s.rt, 1);
    }
    assert_eq!(
        s.rt.state.search.text, "f0+-i",
        "graph keys stay out of the search"
    );
    assert_eq!(s.rt.state.mode, Mode::Graph);
}

#[test]
fn restoring_a_board_over_a_changed_file_never_writes_without_keep_mine() {
    let tmp = vault();
    let path = tmp.path().join("Draw.excalidraw");
    let mut s = launch(tmp.path());
    open_draw(&mut s.rt);
    draw_rect(&mut s.rt);
    let theirs = DRAW.replace("keep me", "theirs");
    fs::write(&path, &theirs).unwrap();
    assert!(
        s.rt.state.flush_before_quit(),
        "the recovery copy makes the quit safe"
    );
    drop(s.rt);

    let mut rt = launch_with(tmp.path(), &s.config, &s.state_home);
    frames(&mut rt, 2);
    let i = rt
        .state
        .recovered
        .iter()
        .position(|r| r.rel == "Draw.excalidraw")
        .expect("listed");
    click(&mut rt, &format!("recovery-restore-{i}"));
    frames(&mut rt, 10);
    assert_eq!(elements(&rt), n_draw() + 1);
    assert!(
        texts(&rt, "board-conflict"),
        "flagged: the file holds theirs"
    );
    rt.key(command('s'));
    frames(&mut rt, 2);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        theirs,
        "a plain save is refused"
    );
    click(&mut rt, "board-keep-mine");
    assert_eq!(Board::load(&path).unwrap().elements.len(), n_draw() + 1);
}

#[test]
fn clicking_the_canvas_after_searching_gives_the_keys_back_to_the_board() {
    let tmp = vault();
    let mut s = launch(tmp.path());
    open_draw(&mut s.rt);
    click(&mut s.rt, "search-box");
    s.rt.key(plain(Key::Text("x".into())));
    frames(&mut s.rt, 1);
    assert!(s.rt.state.is_typing());
    let r = s.rt.presented("board-canvas").expect("canvas").0;
    let p = (r[0] + r[2] - 40.0, r[1] + r[3] - 60.0);
    s.rt.pointer_down(p.0, p.1);
    frames(&mut s.rt, 1);
    s.rt.pointer_up(p.0, p.1);
    frames(&mut s.rt, 2);
    s.rt.key(plain(Key::Text("r".into())));
    frames(&mut s.rt, 1);
    assert!(
        s.rt.state.board_tool.is_some(),
        "a click on the canvas ends the search's hold on the keys"
    );
}

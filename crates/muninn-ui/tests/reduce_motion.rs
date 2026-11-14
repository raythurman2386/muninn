//! Settings › Graph › Reduce motion: a persisted System / On / Off choice
//! that resolves, together with `MUNINN_REDUCED_MOTION` and the OS
//! preference, into the one value the graph obeys.
//!
//! Tests use explicit config/state roots so the user's real
//! `~/.config/muninn/settings.json` is never touched.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::{TempVault, big_vault};
use muninn_ui::{Mode, ReduceMotion, Scope, Settings, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-motion-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn settings_file(config: &Path) -> PathBuf {
    config.join("muninn").join("settings.json")
}

fn write_settings(config: &Path, json: &str) {
    let path = settings_file(config);
    fs::create_dir_all(path.parent().expect("parent")).expect("config dir");
    fs::write(path, json).expect("settings.json");
}

fn temp_vault() -> TempVault {
    TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]] and [[Gamma]].\n"),
        ("Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
        ("Gamma.md", "# Gamma\n\nSee [[Delta]].\n"),
        ("Delta.md", "# Delta\n\nSee [[Epsilon]].\n"),
        ("Epsilon.md", "# Epsilon\n\nSee [[Alpha]].\n"),
        ("Zeta.md", "# Zeta\n\nAn island.\n"),
    ])
}

/// A board with a drawing, for the camera's glide.
const SMOKE: &str = include_str!("../../muninn-core/tests/fixtures/board/smoke.excalidraw");
/// A Kanban board with four lanes and a done lane, for drag and Find.
const KANBAN: &str = include_str!("../../muninn-core/tests/fixtures/kanban/drag-board.md");

fn board_vault() -> TempVault {
    TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Smoke.excalidraw", SMOKE)])
}

fn kanban_vault() -> TempVault {
    TempVault::new(&[("Agenda.md", "# Agenda\n"), ("Board.md", KANBAN)])
}

fn key(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
}

/// The Kanban cards the runtime is still springing this frame, by key.
fn moving_cards(rt: &Runtime<State>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter(|e| e.moving)
        .filter_map(|e| e.key)
        .filter(|key| key.starts_with("kanban-card-"))
        .collect()
}

/// Enter Board and open the fixture board, letting the canvas size and its
/// first (instant) fit land.
fn open_smoke_board(rt: &mut Runtime<State>) {
    let path = rt.state.vault.root.join("Smoke.excalidraw");
    rt.update(|s, _| s.open_board_at(path));
    frames(rt, 60);
}

fn open_kanban_board(rt: &mut Runtime<State>) {
    let id = rt.state.vault.resolve("Board").expect("Board.md");
    rt.update(|s, _| s.open_kanban(id));
    frames(rt, 30);
}

/// Open Board Find and type `query`, as `⌘F` and typing do.
fn open_kanban_find(rt: &mut Runtime<State>, query: &str) {
    rt.update(|s, _| {
        s.open_kanban_find();
        let find = s.kanban_find.as_mut().expect("Board Find");
        find.field.replace_text(query);
        find.field.set_caret(query.len());
        find.field.focused = true;
    });
    frames(rt, 30);
}

fn board_source(rt: &Runtime<State>) -> String {
    rt.state
        .kanban
        .as_ref()
        .and_then(|k| k.board.as_ref())
        .expect("a board")
        .source()
        .to_owned()
}

/// Press a card at `from`'s centre, travel to `to`'s, and release.
fn drag_card(rt: &mut Runtime<State>, from: &str, to: &str) {
    let (fx, fy) = rt.find(from).unwrap_or_else(|| panic!("missing {from}"));
    let (tx, ty) = rt.find(to).unwrap_or_else(|| panic!("missing {to}"));
    rt.pointer_move(fx, fy);
    rt.pointer_down(fx, fy);
    frames(rt, 1);
    for step in 1..=8 {
        let f = step as f32 / 8.0;
        rt.pointer_move(fx + (tx - fx) * f, fy + (ty - fy) * f);
        frames(rt, 1);
    }
    frames(rt, 10);
    rt.pointer_move(tx, ty);
    rt.pointer_up(tx, ty);
    // The release is an input event: it applies on the next frame.
    frames(rt, 1);
}

/// A windowless app with its own config root, `MUNINN_REDUCED_MOTION`
/// cleared, and the OS preference set to `os_reduced`.
fn runtime(tmp: &TempVault, config: &Path, mode: ReduceMotion, os_reduced: bool) -> Runtime<State> {
    let settings = Settings {
        reduce_motion: mode,
        ..Settings::default()
    };
    let mut state = State::with_settings(
        tmp.path().to_path_buf(),
        tmp.path().with_extension("state"),
        config.to_path_buf(),
        settings,
    );
    state.reduced_motion_env = false;
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.set_system_reduced_motion(os_reduced);
    rt.frame(0.0);
    rt
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} must be mounted"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
}

fn positions(rt: &Runtime<State>) -> Vec<[f32; 2]> {
    rt.state.graph.nodes.iter().map(|node| node.pos).collect()
}

/// Enter the global graph with a hot layout and run a few frames. Returns
/// whether any node moved.
fn graph_layout_moves(rt: &mut Runtime<State>) -> bool {
    rt.update(|s, _| {
        s.scope = Scope::Global;
        s.set_mode(Mode::Graph);
        s.graph.reheat();
    });
    let before = positions(rt);
    frames(rt, 10);
    positions(rt) != before
}

fn screen_gap(rt: &Runtime<State>, world: [f32; 2]) -> f32 {
    let point = rt.state.cam.screen(world);
    let centre = [rt.state.canvas[0] / 2.0, rt.state.canvas[1] / 2.0];
    ((point[0] - centre[0]).powi(2) + (point[1] - centre[1]).powi(2)).sqrt()
}

/// Settle the graph, stop auto-fit, then click the node furthest from the
/// centre. Returns whether a camera glide started.
fn click_starts_a_glide(rt: &mut Runtime<State>) -> bool {
    rt.update(|s, _| {
        let alpha = s.vault.resolve("Alpha").expect("Alpha");
        s.load_note(alpha, false);
        s.scope = Scope::Global;
        s.set_mode(Mode::Graph);
    });
    frames(rt, 600);
    rt.state.auto_fit = false;
    rt.state.needs_fit = false;
    frames(rt, 40);
    let mut best = (f32::NEG_INFINITY, 0usize);
    for &id in &rt.state.visible_notes() {
        if let Some(node) = rt.state.graph.node(id) {
            let gap = screen_gap(rt, node.pos);
            if gap > best.0 {
                best = (gap, id);
            }
        }
    }
    assert!(best.0 > 8.0, "a node sits off centre");
    click(rt, &format!("graph-node-{}", best.1));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.selected, Some(best.1));
    rt.state.camera_gliding()
}

#[test]
fn every_choice_round_trips_through_settings_json() {
    for mode in ReduceMotion::ALL {
        let config = unique_dir("round-trip");
        let settings = Settings {
            reduce_motion: mode,
            ..Settings::default()
        };
        settings.save_to(&config).expect("save");
        let raw = fs::read_to_string(settings_file(&config)).expect("written");
        assert!(
            raw.contains(&format!("\"reduce_motion\": \"{}\"", mode.as_key())),
            "{mode:?} is stored by name: {raw}"
        );
        assert_eq!(Settings::load_from(&config).reduce_motion, mode);
        let _ = fs::remove_dir_all(&config);
    }
}

#[test]
fn legacy_and_unknown_values_load_as_system() {
    let config = unique_dir("legacy");
    // A v0.5.2 file: every field it knew about, no reduce_motion key.
    write_settings(
        &config,
        r#"{"theme":"dark","collapsed":[],"mode":"graph","scope":"global","autosave_delay":0.8,"recent_vaults":[],"note_template":""}"#,
    );
    let loaded = Settings::load_from(&config);
    assert_eq!(loaded.reduce_motion, ReduceMotion::System);
    assert!(
        (loaded.autosave_delay - 0.8).abs() < 1e-9,
        "the rest still loads"
    );

    for junk in [r#""sometimes""#, "true", "1", "null"] {
        write_settings(&config, &format!(r#"{{"reduce_motion":{junk}}}"#));
        assert_eq!(
            Settings::load_from(&config).reduce_motion,
            ReduceMotion::System,
            "{junk} falls back to System"
        );
    }
    assert_eq!(Settings::default().reduce_motion, ReduceMotion::System);
    let _ = fs::remove_dir_all(&config);
}

#[test]
fn system_follows_the_os_and_on_off_override_it() {
    let tmp = temp_vault();
    let cases = [
        (ReduceMotion::System, false, false),
        (ReduceMotion::System, true, true),
        (ReduceMotion::On, false, true),
        (ReduceMotion::On, true, true),
        (ReduceMotion::Off, false, false),
        (ReduceMotion::Off, true, false),
    ];
    for (mode, os, expected) in cases {
        let config = unique_dir("resolve");
        let mut rt = runtime(&tmp, &config, mode, os);
        frames(&mut rt, 2);
        assert_eq!(
            rt.state.reduced_motion(),
            expected,
            "{mode:?} with OS reduce-motion {os}"
        );
        let _ = fs::remove_dir_all(&config);
    }
}

#[test]
fn the_environment_flag_forces_reduced_motion_even_over_off() {
    let tmp = temp_vault();
    let config = unique_dir("env");
    for mode in ReduceMotion::ALL {
        let mut rt = runtime(&tmp, &config, mode, false);
        rt.update(|s, _| s.reduced_motion_env = true);
        frames(&mut rt, 2);
        assert!(
            rt.state.reduced_motion(),
            "MUNINN_REDUCED_MOTION wins over {mode:?}"
        );
        assert!(
            !graph_layout_moves(&mut rt),
            "and the layout stays still under {mode:?}"
        );
    }
    for (mode, os) in [(ReduceMotion::System, true), (ReduceMotion::On, false)] {
        let rt = runtime(&tmp, &config, mode, os);
        assert!(
            rt.state.reduced_motion(),
            "unset env never turns motion back on"
        );
    }
    let _ = fs::remove_dir_all(&config);
}

#[test]
fn reduced_motion_freezes_the_layout_and_off_lets_it_settle() {
    let tmp = temp_vault();
    let config = unique_dir("layout");

    let mut on = runtime(&tmp, &config, ReduceMotion::On, false);
    assert!(!graph_layout_moves(&mut on), "On: no node moves");
    assert_eq!(on.state.graph.heat, 0.0, "On: nothing left to settle");

    let mut system = runtime(&tmp, &config, ReduceMotion::System, true);
    assert!(
        !graph_layout_moves(&mut system),
        "System + OS reduce: no node moves"
    );

    let mut off = runtime(&tmp, &config, ReduceMotion::Off, true);
    assert!(
        graph_layout_moves(&mut off),
        "Off beats the OS: the layout steps"
    );

    let mut normal = runtime(&tmp, &config, ReduceMotion::System, false);
    assert!(
        graph_layout_moves(&mut normal),
        "System + OS normal: the layout steps"
    );
    let _ = fs::remove_dir_all(&config);
}

#[test]
fn reduced_motion_snaps_selection_instead_of_gliding() {
    let tmp = temp_vault();
    let config = unique_dir("glide");
    let mut on = runtime(&tmp, &config, ReduceMotion::On, false);
    assert!(
        !click_starts_a_glide(&mut on),
        "On: the camera snaps to the selection"
    );
    let mut off = runtime(&tmp, &config, ReduceMotion::Off, false);
    assert!(click_starts_a_glide(&mut off), "Off: the camera glides");
    let _ = fs::remove_dir_all(&config);
}

#[test]
fn turning_it_on_mid_glide_lands_the_camera() {
    let tmp = temp_vault();
    let config = unique_dir("mid-glide");
    let mut rt = runtime(&tmp, &config, ReduceMotion::Off, false);
    assert!(click_starts_a_glide(&mut rt));
    rt.update(|s, _| s.set_reduce_motion(ReduceMotion::On));
    frames(&mut rt, 2);
    assert!(
        !rt.state.camera_gliding(),
        "the glide in flight lands at once"
    );
    let at_rest = (rt.state.cam.pan(), rt.state.cam.zoom());
    frames(&mut rt, 20);
    assert_eq!((rt.state.cam.pan(), rt.state.cam.zoom()), at_rest);
    let _ = fs::remove_dir_all(&config);
}

#[test]
fn the_graph_tab_picker_persists_and_applies_immediately() {
    let tmp = temp_vault();
    let config = unique_dir("picker");
    let mut rt = runtime(&tmp, &config, ReduceMotion::System, false);
    rt.update(|s, _| s.open_settings());
    frames(&mut rt, 4);
    click(&mut rt, "settings-tabs/segment3");
    frames(&mut rt, 4);
    assert!(
        rt.find("settings-reduce-motion").is_some(),
        "the picker is on the Graph tab"
    );
    let status = |rt: &Runtime<State>| {
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("settings-reduced-motion-status"))
            .and_then(|e| e.text)
            .expect("status line")
    };
    assert!(status(&rt).contains("off"), "{}", status(&rt));

    // Segments are System, On, Off.
    click(&mut rt, "settings-reduce-motion/segment1");
    frames(&mut rt, 2);
    assert_eq!(rt.state.reduce_motion(), ReduceMotion::On);
    assert!(rt.state.reduced_motion());
    assert!(status(&rt).contains("on"), "{}", status(&rt));
    assert_eq!(
        Settings::load_from(&config).reduce_motion,
        ReduceMotion::On,
        "the choice is written immediately"
    );

    click(&mut rt, "settings-reduce-motion/segment2");
    frames(&mut rt, 2);
    assert_eq!(
        Settings::load_from(&config).reduce_motion,
        ReduceMotion::Off
    );
    assert!(!rt.state.reduced_motion());

    // A fresh start restores the saved choice.
    let restored = State::with_settings(
        tmp.path().to_path_buf(),
        tmp.path().with_extension("state"),
        config.clone(),
        Settings::load_from(&config),
    );
    assert_eq!(restored.reduce_motion(), ReduceMotion::Off);
    let _ = fs::remove_dir_all(&config);
}

/// Whether any element is mid-transition this frame (Rune's `moving` flag).
/// A Kanban choice is a segmented control with a sliding indicator, so it
/// exercises the same control motion the Settings tab bar used to, without
/// changing the theme or the motion setting under test.
fn indicator_moving(rt: &Runtime<State>) -> bool {
    rt.elements().iter().any(|e| {
        e.path.contains("kanban-setting-new-line-trigger-choice")
            && e.path.ends_with("indicator")
            && e.moving
    })
}

#[test]
fn reduced_motion_makes_ui_transitions_land_at_once() {
    let tmp = temp_vault();
    let config = unique_dir("transitions");
    let mut seen = Vec::new();
    for (mode, os) in [
        (ReduceMotion::Off, false),
        (ReduceMotion::On, false),
        (ReduceMotion::System, true),
    ] {
        let mut rt = runtime(&tmp, &config, mode, os);
        rt.update(|s, _| s.open_settings());
        frames(&mut rt, 30);
        // A choice control's selection indicator slides between segments; the
        // Settings tab bar itself is a wrapping page bar now.
        click(&mut rt, "settings-tabs/segment5");
        frames(&mut rt, 30);
        click(&mut rt, "kanban-setting-new-line-trigger-choice/segment1");
        rt.frame(1.0 / 60.0);
        seen.push((mode, os, indicator_moving(&rt)));
    }
    assert_eq!(
        seen,
        [
            (ReduceMotion::Off, false, true),
            (ReduceMotion::On, false, false),
            (ReduceMotion::System, true, false),
        ],
        "the selection indicator slides only when motion is not reduced"
    );
    let _ = fs::remove_dir_all(&config);
}

/// More notes than the dense-layout limit (200), where a layout with zero
/// heat and zero velocity is treated as settled and stops stepping.
const DENSE_NOTES: usize = 240;

/// Finish the (chunked) startup scan, enter the global graph and run a few
/// frames under whatever motion setting is active. No manual reheat.
fn dense_graph(rt: &mut Runtime<State>) {
    for _ in 0..400 {
        if !rt.state.loading() {
            break;
        }
        rt.frame(1.0 / 60.0);
    }
    assert!(!rt.state.loading(), "the dense vault finished scanning");
    assert!(rt.state.vault.notes.len() > 200, "the fixture is dense");
    rt.update(|s, _| {
        s.scope = Scope::Global;
        s.set_mode(Mode::Graph);
    });
    frames(rt, 30);
}

#[test]
fn turning_reduce_motion_off_resumes_a_dense_layout() {
    let tmp = big_vault(DENSE_NOTES);
    let config = unique_dir("dense-toggle");
    let mut rt = runtime(&tmp, &config, ReduceMotion::On, false);
    dense_graph(&mut rt);
    let frozen = positions(&rt);
    frames(&mut rt, 30);
    assert_eq!(
        positions(&rt),
        frozen,
        "On: the dense layout stays on its seeds"
    );

    rt.update(|s, _| s.set_reduce_motion(ReduceMotion::Off));
    frames(&mut rt, 30);
    assert_ne!(
        positions(&rt),
        frozen,
        "Off: the dense layout starts settling without a manual reheat"
    );
    let _ = fs::remove_dir_all(&config);
}

#[test]
fn an_os_flip_under_system_resumes_a_dense_layout() {
    let tmp = big_vault(DENSE_NOTES);
    let config = unique_dir("dense-os");
    let mut rt = runtime(&tmp, &config, ReduceMotion::System, true);
    dense_graph(&mut rt);
    let frozen = positions(&rt);
    frames(&mut rt, 30);
    assert_eq!(
        positions(&rt),
        frozen,
        "OS reduce on: the dense layout is frozen"
    );

    rt.set_system_reduced_motion(false);
    frames(&mut rt, 30);
    assert_ne!(
        positions(&rt),
        frozen,
        "OS reduce off: the dense layout starts settling without a manual reheat"
    );
    let _ = fs::remove_dir_all(&config);
}

/// #72: the Board camera makes the graph camera's choice. A zoom, pan or fit
/// under reduced motion is a single set, not a glide; with motion allowed the
/// same press starts a spring that lands on the same target.
#[test]
fn board_camera_snaps_under_reduced_motion_and_glides_otherwise() {
    let tmp = board_vault();
    let config = unique_dir("board-camera");

    let mut on = runtime(&tmp, &config, ReduceMotion::On, false);
    open_smoke_board(&mut on);
    let fitted = on.state.board_view().expect("an open board");
    key(&mut on, Key::Text("+".into()));
    // Keys are input events: the frame applies the zoom.
    frames(&mut on, 1);
    let zoomed = on.state.board_view().expect("an open board");
    assert!(
        (zoomed.zoom - fitted.zoom * 1.25).abs() < 1e-4,
        "On: + zooms at once: {zoomed:?} from {fitted:?}"
    );
    assert!(!on.state.board_camera_gliding(), "On: no glide is started");

    let before = on.state.board_view().expect("an open board");
    key(&mut on, Key::Right);
    frames(&mut on, 1);
    let after = on.state.board_view().expect("an open board");
    assert!(
        (after.eye[0] - (before.eye[0] + 60.0 / before.zoom)).abs() < 1e-4,
        "On: Right pans at once: {after:?} from {before:?}"
    );
    assert!(!on.state.board_camera_gliding(), "On: no glide is started");

    key(&mut on, Key::Text("0".into()));
    frames(&mut on, 1);
    assert_eq!(
        on.state.board_view().expect("an open board"),
        fitted,
        "On: 0 refits at once"
    );
    assert!(!on.state.board_camera_gliding(), "On: no glide is started");
    frames(&mut on, 30);
    assert_eq!(
        on.state.board_view().expect("an open board"),
        fitted,
        "On: the camera stays put"
    );

    let mut off = runtime(&tmp, &config, ReduceMotion::Off, false);
    open_smoke_board(&mut off);
    let fitted = off.state.board_view().expect("an open board");
    key(&mut off, Key::Text("+".into()));
    // The press's own frame starts the spring at the pose it was pressed on.
    frames(&mut off, 1);
    assert!(
        off.state.board_camera_gliding(),
        "Off: + starts a camera glide"
    );
    assert_eq!(
        off.state.board_view().expect("an open board"),
        fitted,
        "Off: the camera has not jumped on the press's frame"
    );
    frames(&mut off, 1);
    let first = off.state.board_view().expect("an open board");
    assert!(
        first.zoom > fitted.zoom && first.zoom < fitted.zoom * 1.25,
        "Off: the next frame is on the way: {first:?}"
    );
    frames(&mut off, 40);
    assert!(!off.state.board_camera_gliding(), "Off: the glide landed");
    assert!(
        (off.state.board_view().expect("an open board").zoom - fitted.zoom * 1.25).abs() < 1e-3,
        "Off: + lands at the same zoom On snapped to"
    );

    let before = off.state.board_view().expect("an open board");
    key(&mut off, Key::Right);
    frames(&mut off, 1);
    assert!(
        off.state.board_camera_gliding(),
        "Off: Arrow pans with a glide"
    );
    frames(&mut off, 40);
    let after = off.state.board_view().expect("an open board");
    assert!(
        (after.eye[0] - (before.eye[0] + 60.0 / before.zoom)).abs() < 1e-3,
        "Off: the pan lands where On snapped: {after:?} from {before:?}"
    );

    key(&mut off, Key::Text("0".into()));
    frames(&mut off, 1);
    assert!(
        off.state.board_camera_gliding(),
        "Off: 0 glides back to the fit"
    );
    frames(&mut off, 40);
    assert_eq!(
        off.state.board_view().expect("an open board"),
        fitted,
        "Off: the glide lands on the exact fit"
    );
    assert!(!off.state.board_camera_gliding());

    let _ = fs::remove_dir_all(&config);
}

/// The camera glide is bounded by the same setting mid-flight as the graph's:
/// flipping Reduce motion on lands the camera at its target at once, so the
/// window stops waking for it.
#[test]
fn turning_reduced_motion_on_mid_board_glide_lands_the_camera() {
    let tmp = board_vault();
    let config = unique_dir("board-mid-glide");
    let mut rt = runtime(&tmp, &config, ReduceMotion::Off, false);
    open_smoke_board(&mut rt);
    let fitted = rt.state.board_view().expect("an open board");
    key(&mut rt, Key::Text("+".into()));
    frames(&mut rt, 1);
    assert!(rt.state.board_camera_gliding());
    frames(&mut rt, 3);
    rt.update(|s, _| s.set_reduce_motion(ReduceMotion::On));
    frames(&mut rt, 2);
    assert!(
        !rt.state.board_camera_gliding(),
        "the glide in flight lands at once"
    );
    assert!(
        (rt.state.board_view().expect("an open board").zoom - fitted.zoom * 1.25).abs() < 1e-4,
        "it lands on the target the press asked for"
    );
    let at_rest = rt.state.board_view().expect("an open board");
    frames(&mut rt, 20);
    assert_eq!(
        rt.state.board_view().expect("an open board"),
        at_rest,
        "and does not keep moving"
    );
    let _ = fs::remove_dir_all(&config);
}

/// #72: a Kanban card drop is one edit and lands in the frame, and Board
/// Find's outline moves to the next match the same way. Neither path has a
/// glide of its own, so reduced motion has nothing extra to suppress — this
/// pins that the resolved setting never turns one into an animation.
#[test]
fn kanban_drag_and_find_outline_land_in_the_frame_under_either_motion_mode() {
    for (mode, os) in [(ReduceMotion::On, false), (ReduceMotion::Off, false)] {
        let tmp = kanban_vault();
        let config = unique_dir("kanban-motion");
        let mut rt = runtime(&tmp, &config, mode, os);
        open_kanban_board(&mut rt);

        let before = board_source(&rt);
        drag_card(&mut rt, "kanban-card-0-0", "kanban-card-0-2");
        assert_ne!(
            board_source(&rt),
            before,
            "{mode:?}: the drop moved the card"
        );
        assert!(
            moving_cards(&rt).is_empty(),
            "{mode:?}: the drop lands in the frame, no card glides: {:?}",
            moving_cards(&rt)
        );

        open_kanban_find(&mut rt, "card");
        let current = rt.state.kanban_selected.expect("a match is selected");
        assert!(
            rt.state.kanban_find_mark(current.0, current.1).1,
            "{mode:?}: the current match is outlined"
        );
        rt.update(|s, _| s.kanban_find_next());
        frames(&mut rt, 1);
        assert_ne!(
            rt.state.kanban_selected,
            Some(current),
            "{mode:?}: Next steps the match"
        );
        assert!(
            moving_cards(&rt).is_empty(),
            "{mode:?}: the outline lands in the frame, no card glides: {:?}",
            moving_cards(&rt)
        );
        let stepped = rt.state.kanban_selected.expect("a match");
        assert!(
            rt.state.kanban_find_mark(stepped.0, stepped.1).1,
            "{mode:?}: the stepped-to match is outlined"
        );
        let _ = fs::remove_dir_all(&config);
    }
}

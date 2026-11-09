//! v0.5 animated graph focus: the camera glides on a node click and snaps under
//! `MUNINN_REDUCED_MOTION`.

use muninn_core::testing::TempVault;
use muninn_graph::Camera as GraphCamera;
use muninn_ui::{Mode, Scope, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn temp_vault() -> TempVault {
    TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]] and [[Gamma]].\n"),
        ("Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
        ("Gamma.md", "# Gamma\n\nA quiet note.\n"),
        ("Delta.md", "# Delta\n\nSee [[Epsilon]].\n"),
        ("Epsilon.md", "# Epsilon\n\nSee [[Delta]].\n"),
        ("Zeta.md", "# Zeta\n\nAn island.\n"),
    ])
}

fn runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state")),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn graph_runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = runtime(tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.load_note(alpha, false);
    rt.state.scope = Scope::Global;
    rt.state.set_mode(Mode::Graph);
    // Let the deterministic layout settle and the initial fit land before the
    // test takes over the camera.
    for _ in 0..600 {
        rt.frame(1.0 / 60.0);
    }
    rt.state.auto_fit = false;
    rt.state.needs_fit = false;
    for _ in 0..40 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} must be mounted"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
}

fn screen_gap(rt: &Runtime<State>, world: [f32; 2]) -> f32 {
    let point = rt.state.cam.screen(world);
    let centre = [rt.state.canvas[0] / 2.0, rt.state.canvas[1] / 2.0];
    ((point[0] - centre[0]).powi(2) + (point[1] - centre[1]).powi(2)).sqrt()
}

/// The visible node whose card sits furthest from the canvas centre, so a
/// click on it has somewhere to glide to.
fn off_centre_node(rt: &Runtime<State>) -> usize {
    let mut best = (f32::NEG_INFINITY, 0usize);
    for &id in &rt.state.visible_notes() {
        if let Some(node) = rt.state.graph.node(id) {
            let gap = screen_gap(rt, node.pos);
            if gap > best.0 {
                best = (gap, id);
            }
        }
    }
    best.1
}

#[test]
fn clicking_a_node_glides_the_camera_over_frames() {
    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    let target = off_centre_node(&rt);
    let pos = rt.state.graph.node(target).expect("node").pos;
    let before = rt.state.cam.pan();
    let start_gap = screen_gap(&rt, pos);
    assert!(start_gap > 8.0, "the test node must start off-centre");

    click(&mut rt, &format!("graph-node-{target}"));
    // The click's own commit starts the glide; the next frame is the first
    // one that moves the camera.
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.selected,
        Some(target),
        "the click selects the note"
    );
    assert_eq!(
        rt.state.cam.pan(),
        before,
        "the selection frame must not teleport the camera"
    );

    rt.frame(1.0 / 60.0);
    assert_ne!(rt.state.cam.pan(), before, "the camera must start moving");
    let first_frame = screen_gap(&rt, pos);
    assert!(
        first_frame > 1.0 && first_frame < start_gap,
        "the camera must ease in, not snap: {start_gap} -> {first_frame} pt from centre"
    );

    let mut previous = first_frame;
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
        let gap = screen_gap(&rt, pos);
        assert!(
            gap <= previous + 0.05,
            "the glide must close the gap monotonically: {previous} -> {gap}"
        );
        previous = gap;
    }
    assert!(
        previous < 0.75,
        "the camera must land on the note by the end of the glide: {previous} pt off"
    );
    assert!(!rt.state.camera_gliding(), "the glide must finish");
}

#[test]
fn enter_on_the_focused_open_note_glides_instead_of_jumping() {
    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    let target = off_centre_node(&rt);
    let pos = rt.state.graph.node(target).expect("node").pos;
    // Select the note without centring it, as the opening note would be, then
    // put the focus ring on it.
    rt.state.load_note(target, false);
    rt.state.graph_focus = Some(target);
    let before = rt.state.cam.pan();
    let start_gap = screen_gap(&rt, pos);
    assert!(start_gap > 8.0, "the node must start off-centre");

    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.mode,
        Mode::Graph,
        "Enter on the already-open focused note keeps the graph"
    );
    assert_eq!(rt.state.cam.pan(), before, "no teleport on the key frame");

    rt.frame(1.0 / 60.0);
    let first = screen_gap(&rt, pos);
    assert_ne!(rt.state.cam.pan(), before);
    assert!(
        first > 1.0 && first < start_gap,
        "Enter must glide, not snap: {start_gap} -> {first} pt off"
    );

    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    assert!(screen_gap(&rt, pos) < 0.75);
}

#[test]
fn reduced_motion_snaps_the_camera_and_does_not_animate() {
    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    rt.state.reduced_motion_env = true;
    let target = off_centre_node(&rt);
    let pos = rt.state.graph.node(target).expect("node").pos;
    assert!(screen_gap(&rt, pos) > 8.0);

    click(&mut rt, &format!("graph-node-{target}"));
    rt.frame(1.0 / 60.0);

    assert_eq!(rt.state.selected, Some(target));
    assert!(
        !rt.state.camera_gliding(),
        "reduced motion must not start a glide"
    );
    let snapped = screen_gap(&rt, pos);
    assert!(
        snapped < 0.75,
        "reduced motion centres the note in the click frame: {snapped} pt off"
    );

    let at_rest = (rt.state.cam.pan(), rt.state.cam.zoom());
    for _ in 0..20 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(
        (rt.state.cam.pan(), rt.state.cam.zoom()),
        at_rest,
        "a reduced-motion camera must stay put after the snap"
    );
}

#[test]
fn switching_scope_glides_to_the_new_fit() {
    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    rt.frame(1.0 / 60.0);
    rt.state.auto_fit = false;
    rt.state.needs_fit = false;

    let local = rt.state.local_notes();
    let bounds = rt.state.graph.bounds(&local).expect("local bounds");
    let want = GraphCamera::fitted_to(bounds, rt.state.canvas).expect("a usable viewport");
    let from = (rt.state.cam.pan(), rt.state.cam.zoom());
    assert!(
        (from.0[0] - want.pan[0]).abs() > 1.0 || (from.1 - want.zoom).abs() > 0.01,
        "Global and Local fit differently in this vault"
    );

    click(&mut rt, "scope-local");
    // First the press commit starts the glide...
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.scope, Scope::Local);
    assert_eq!(
        (rt.state.cam.pan(), rt.state.cam.zoom()),
        from,
        "the scope commit must not hard-swap the camera"
    );
    // ...then the following frames ease toward the Local fit.
    rt.frame(1.0 / 60.0);
    let first = (rt.state.cam.pan(), rt.state.cam.zoom());
    assert_ne!(first, from, "the scope switch must start moving the camera");
    assert!(
        (first.0[0] - want.pan[0]).abs() > 1.0 || (first.1 - want.zoom).abs() > 0.01,
        "the scope switch must not land the camera in one frame"
    );

    for _ in 0..600 {
        rt.frame(1.0 / 60.0);
    }
    // Let the last retarget settle after the layout stops moving.
    for _ in 0..60 {
        if !rt.state.camera_gliding() {
            break;
        }
        rt.frame(1.0 / 60.0);
    }
    let local = rt.state.local_notes();
    let bounds = rt.state.graph.bounds(&local).expect("local bounds");
    let want = GraphCamera::fitted_to(bounds, rt.state.canvas).expect("a usable viewport");
    assert!(
        (rt.state.cam.pan()[0] - want.pan[0]).abs() < 1.0
            && (rt.state.cam.pan()[1] - want.pan[1]).abs() < 1.0
            && (rt.state.cam.zoom() - want.zoom).abs() < 0.005,
        "the camera must ease into the Local fit: {:?} vs {:?}",
        (rt.state.cam.pan(), rt.state.cam.zoom()),
        (want.pan, want.zoom)
    );
    assert!(!rt.state.camera_gliding());
}

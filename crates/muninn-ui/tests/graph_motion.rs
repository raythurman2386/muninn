//! v0.5 animated graph focus: the camera glides on a node click and snaps under
//! `MUNINN_REDUCED_MOTION`. Also the delete burn, on a graph node and (#135) on
//! the note's sidebar row.

use muninn_core::testing::TempVault;
use muninn_graph::Camera as GraphCamera;
use muninn_ui::fx_marks::{FX_BURN, FX_EDGE_HOT, FX_NODE_SEL};
use muninn_ui::{BurnSpace, GraphEffects, Mode, Scope, State, on_key, poll, view};
use rune::render::KIND_EFFECT;
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

/// The visible note with the most links, so an electric test has edges to arc.
fn linked_node(rt: &Runtime<State>) -> usize {
    rt.state
        .visible_notes()
        .into_iter()
        .max_by_key(|&id| {
            let note = &rt.state.vault.notes[id];
            note.links.len() + note.backlinks.len()
        })
        .expect("a visible note")
}

/// The tags of the `El::effect` slots the last frame built, reassembled from
/// the slot record the same way the app compositor does.
fn effect_marks(rt: &Runtime<State>) -> Vec<u32> {
    rt.instances
        .iter()
        .filter(|i| i.params[1] == KIND_EFFECT)
        .map(|i| (i.border[0] as u32) | ((i.border[1] as u32) << 16))
        .collect()
}

#[test]
fn classic_emits_no_graph_effect_slots() {
    let tmp = temp_vault();
    let rt = graph_runtime(&tmp);
    assert_eq!(rt.state.graph_effects(), GraphEffects::Classic);
    assert!(
        effect_marks(&rt).is_empty(),
        "Classic is today's graph and keeps the compositor idle"
    );
}

#[test]
fn electric_tags_edges_and_the_selection_as_one_family() {
    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    rt.update(|s, _| s.set_graph_effects(GraphEffects::Electric));
    // A hub, so the selection has both a border and edges to arc.
    let target = linked_node(&rt);
    click(&mut rt, &format!("graph-node-{target}"));
    rt.frame(1.0 / 60.0);

    let marks = effect_marks(&rt);
    assert!(
        marks.iter().any(|m| m & 0xffff_0000 == FX_NODE_SEL),
        "the selection gets an electric border: {marks:?}"
    );
    assert!(
        marks.iter().any(|m| m & 0xffff_0000 == FX_EDGE_HOT),
        "the selection's edges get electric arcs: {marks:?}"
    );
    assert!(
        marks
            .iter()
            .any(|m| m & 0xffff_0000 == muninn_ui::fx_marks::FX_NODE_NEAR),
        "direct neighbours get the charged border: {marks:?}"
    );

    rt.update(|s, _| s.set_graph_effects(GraphEffects::Off));
    rt.frame(1.0 / 60.0);
    assert!(
        effect_marks(&rt).is_empty(),
        "Off drops every graph slot on the next frame"
    );
}

#[test]
fn reduced_motion_keeps_the_electric_slots_but_freezes_them() {
    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    rt.state.reduced_motion_env = true;
    rt.update(|s, _| s.set_graph_effects(GraphEffects::Electric));
    let target = off_centre_node(&rt);
    click(&mut rt, &format!("graph-node-{target}"));
    rt.frame(1.0 / 60.0);

    assert!(rt.state.reduced_motion());
    assert!(
        effect_marks(&rt)
            .iter()
            .any(|m| m & 0xffff_0000 == FX_NODE_SEL),
        "the preset still draws; only its motion is frozen"
    );
}

#[test]
fn deleting_a_note_burns_its_node_out_for_a_moment() {
    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    let target = off_centre_node(&rt);
    let pos = rt.state.graph.node(target).expect("node").pos;

    rt.update(|s, _| {
        s.delete_note(target);
        s.confirm_delete();
    });
    rt.frame(1.0 / 60.0);

    let burn = rt.state.burn.clone().expect("the deletion leaves a burn");
    assert_eq!(burn.pos, pos, "it burns where the node was");
    assert!(
        effect_marks(&rt).iter().any(|m| m & 0xffff_0000 == FX_BURN),
        "a fire slot covers the deleted node"
    );

    // The burn is brief: a second of frames retires it and the slot goes too.
    for _ in 0..70 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.state.burn.is_none(), "the burn expires");
    assert!(
        !effect_marks(&rt).iter().any(|m| m & 0xffff_0000 == FX_BURN),
        "and takes its slot with it"
    );
}

#[test]
fn reduced_motion_and_off_skip_the_delete_burn() {
    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    rt.state.reduced_motion_env = true;
    let target = off_centre_node(&rt);
    rt.update(|s, _| {
        s.delete_note(target);
        s.confirm_delete();
    });
    rt.frame(1.0 / 60.0);
    assert!(rt.state.burn.is_none(), "reduced motion skips the burn");

    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    rt.update(|s, _| s.set_graph_effects(GraphEffects::Off));
    let target = off_centre_node(&rt);
    rt.update(|s, _| {
        s.delete_note(target);
        s.confirm_delete();
    });
    rt.frame(1.0 / 60.0);
    assert!(rt.state.burn.is_none(), "Off / Minimal skips the burn");
}

/// #135: an editor-mode runtime whose sidebar has drawn its rows once, so
/// their live rects are known before a scripted delete.
fn sidebar_runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = runtime(tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.load_note(alpha, false);
    // One frame lays the sidebar rows out and writes their drawn rects.
    rt.frame(1.0 / 60.0);
    assert!(
        rt.find("note-row-0").is_some() || rt.find("note-row-1").is_some(),
        "the sidebar must be showing note rows"
    );
    rt
}

/// The row whose centre is inside `rect`, as the list draws it now: the row
/// that moved up into the burned one's place.
fn row_inside(rt: &Runtime<State>, rect: [f32; 4]) -> Option<usize> {
    (0..rt.state.vault.notes.len()).find(|&id| {
        rt.presented(&format!("note-row-{id}"))
            .is_some_and(|(r, _)| {
                let cx = r[0] + r[2] / 2.0;
                let cy = r[1] + r[3] / 2.0;
                cx >= rect[0] && cx <= rect[0] + rect[2] && cy >= rect[1] && cy <= rect[1] + rect[3]
            })
    })
}

#[test]
fn deleting_a_note_from_the_sidebar_burns_its_row() {
    let tmp = temp_vault();
    let mut rt = sidebar_runtime(&tmp);
    assert_ne!(rt.state.mode, Mode::Graph, "the sidebar case is Editor");
    let target = rt.state.vault.resolve("Beta").expect("Beta");
    let row = rt
        .presented(&format!("note-row-{target}"))
        .expect("the row is drawn")
        .0;

    rt.update(|s, _| {
        s.delete_note(target);
        s.confirm_delete();
    });
    rt.frame(1.0 / 60.0);

    let burn = rt
        .state
        .burn
        .clone()
        .expect("a sidebar delete leaves a burn");
    assert_eq!(burn.space, BurnSpace::Row, "it is a window-space row burn");
    assert!(
        (burn.pos[0] - (row[0] + row[2] / 2.0)).abs() < 0.5
            && (burn.pos[1] - (row[1] + row[3] / 2.0)).abs() < 0.5,
        "it burns where the row was: {:?} vs {row:?}",
        burn.pos
    );
    // The ghost is the row's own last drawn rect, so the fade covers the row.
    let (ghost, _) = rt.presented("burn-ghost").expect("the ghost is drawn");
    assert!(
        (ghost[0] - row[0]).abs() < 0.5 && (ghost[1] - row[1]).abs() < 0.5,
        "the ghost lands on the row: {ghost:?} vs {row:?}"
    );
    assert!(
        effect_marks(&rt).iter().any(|m| m & 0xffff_0000 == FX_BURN),
        "a fire slot covers the row"
    );

    // The ghost takes no input: a press on it still reaches the row the list
    // moved up under it.
    let under = row_inside(&rt, ghost).expect("a row moved into the burned spot");
    click(&mut rt, &format!("note-row-{under}"));
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.selected,
        Some(under),
        "the burn must not swallow the click"
    );

    // As in the graph, it is gone well under a second later.
    for _ in 0..70 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.state.burn.is_none(), "the burn expires");
    assert!(rt.presented("burn-ghost").is_none(), "and the ghost goes");
    assert!(
        !effect_marks(&rt).iter().any(|m| m & 0xffff_0000 == FX_BURN),
        "and takes its slot with it"
    );
}

#[test]
fn off_and_reduced_motion_skip_the_sidebar_burn() {
    let tmp = temp_vault();
    let mut rt = sidebar_runtime(&tmp);
    rt.state.reduced_motion_env = true;
    let target = rt.state.vault.resolve("Beta").expect("Beta");
    rt.update(|s, _| {
        s.delete_note(target);
        s.confirm_delete();
    });
    rt.frame(1.0 / 60.0);
    assert!(rt.state.burn.is_none(), "reduced motion skips the row burn");
    assert!(rt.presented("burn-ghost").is_none(), "and draws no ghost");

    let tmp = temp_vault();
    let mut rt = sidebar_runtime(&tmp);
    rt.update(|s, _| s.set_graph_effects(GraphEffects::Off));
    let target = rt.state.vault.resolve("Beta").expect("Beta");
    rt.update(|s, _| {
        s.delete_note(target);
        s.confirm_delete();
    });
    rt.frame(1.0 / 60.0);
    assert!(rt.state.burn.is_none(), "Off / Minimal skips the row burn");
    assert!(
        !effect_marks(&rt).iter().any(|m| m & 0xffff_0000 == FX_BURN),
        "and emits no fire slot"
    );
}

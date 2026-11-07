//! Headless interaction tests for the Muninn shell.
//!
//! These drive the real Rune runtime against a temporary vault: no window and no snapshots.

use std::fs;

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Scope, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime, Tx};

fn temp_vault() -> TempVault {
    TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]] and [[Gamma]].\n"),
        ("Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
        ("Gamma.md", "# Gamma\n\nA quiet note.\n"),
    ])
}

fn runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::new(tmp.path().to_path_buf()),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
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

#[test]
fn note_rows_open_notes_in_the_editor() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let beta = rt.state.vault.resolve("Beta").expect("Beta");
    let (x, y) = rt.find(&format!("note-row-{beta}")).expect("Beta row");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.selected, Some(beta));
    assert!(rt.state.editor.text.contains("Back to"));
    assert_eq!(rt.state.mode, Mode::Editor);
}

#[test]
fn editing_autosaves_after_an_idle() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    let path = rt.state.vault.notes[alpha].path.clone();
    rt.state.focus_editor();
    rt.state.editor.insert("A new sentence.\n");
    rt.frame(0.1);
    rt.frame(0.8);
    let saved = fs::read_to_string(path).expect("saved note");
    assert!(saved.contains("A new sentence."), "{saved}");
    assert!(!rt.state.dirty());
}

#[test]
fn command_n_creates_and_opens_a_note() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    rt.key(command('n'));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.vault.notes.len(), 4);
    assert!(
        rt.state
            .selected_path
            .as_ref()
            .is_some_and(|p| p.ends_with("Untitled.md"))
    );
    assert_eq!(rt.state.mode, Mode::Editor);
    assert!(rt.state.editor.text.starts_with("# Untitled"));
}

#[test]
fn graph_mode_mounts_nodes_and_local_mode_trims_them() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.load_note(alpha, false);
    rt.state.set_mode(Mode::Graph);
    rt.state.scope = Scope::Global;
    rt.state.needs_fit = false;
    rt.frame(1.0 / 60.0);
    assert!(rt.find("graph-node-0").is_some());
    assert_eq!(rt.state.visible_notes().len(), 3);

    rt.state.scope = Scope::Local;
    let local = rt.state.visible_notes();
    // Alpha links to Beta and Gamma, so the local view keeps all three.
    assert_eq!(local.len(), 3);
}

#[test]
fn clicking_a_missing_link_creates_the_note() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let mut tx = Tx::default();
    rt.state.open_link("missing:Orchard", &mut tx);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.vault.notes.iter().any(|n| n.title == "Orchard"));
    assert_eq!(rt.state.mode, Mode::Editor);
}

#[test]
fn preview_rewrites_only_real_links() {
    let tmp = temp_vault();
    let rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    let preview = rt
        .state
        .vault
        .preview(&rt.state.vault.notes[alpha].source, alpha);
    assert!(preview.contains("[Beta](<note:Beta>)"), "{preview}");
    assert!(preview.contains("[Gamma](<note:Gamma>)"), "{preview}");
    assert!(!preview.contains("[["));
}

#[test]
fn command_k_focuses_the_search_field() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    rt.state.set_mode(Mode::Preview);
    rt.frame(1.0 / 60.0);
    rt.key(command('k'));
    rt.frame(1.0 / 60.0);
    assert!(rt.state.search.focused, "search should request focus");
    rt.key(KeyEvent {
        key: Key::Text("g".into()),
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.search.text, "g");
}

#[test]
fn graph_shortcut_round_trips_to_the_last_note_mode() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    rt.state.set_mode(Mode::Preview);
    rt.key(command('g'));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.mode, Mode::Graph);
    rt.key(command('g'));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.mode, Mode::Preview);
}

#[test]
fn poll_reports_no_change_while_dirty_before_autosave() {
    let tmp = temp_vault();
    let mut s = State::new(tmp.path().to_path_buf());
    s.focus_editor();
    s.editor.insert("A new sentence.\n");
    assert!(s.dirty());
    let mut tx = Tx::default();
    for t in [0.0, 0.1, 0.3, 0.5] {
        assert!(
            !poll(&mut s, t, &mut tx),
            "poll at {t} must be quiet while dirty"
        );
        assert!(
            s.dirty(),
            "note must stay dirty until the idle window passes"
        );
    }
    assert!(
        poll(&mut s, 0.6, &mut tx),
        "the autosave frame must report a change"
    );
    assert!(!s.dirty(), "save must clear the dirty flag");
    assert!(
        !poll(&mut s, 0.7, &mut tx),
        "a clean poll with no graph motion must be quiet"
    );
}

#[test]
fn graph_poll_settles_and_stays_quiet() {
    let tmp = temp_vault();
    let mut s = State::new(tmp.path().to_path_buf());
    s.set_mode(Mode::Graph);
    assert_eq!(s.scope, Scope::Global);
    let mut tx = Tx::default();
    let mut t = 0.0;
    let mut moved_frames = 0;
    for _ in 0..600 {
        t += 1.0 / 60.0;
        if poll(&mut s, t, &mut tx) {
            moved_frames += 1;
        }
    }
    assert!(moved_frames > 0, "settling must report visible motion");
    // After settling, heat alone must not wake the window: 30 quiet frames
    // in a row is the Rune warning threshold.
    let mut quiet = 0;
    for _ in 0..30 {
        t += 1.0 / 60.0;
        if !poll(&mut s, t, &mut tx) {
            quiet += 1;
        }
    }
    assert_eq!(quiet, 30, "settled graph must stay quiet for 30 frames");
    // Heat without motion must also stay quiet (the old auto-fit bug). Sync
    // the camera to the settled layout first: manual polling above keeps
    // stepping past the point where the real runtime would have slept, so
    // the camera would otherwise lag the invisible creep.
    s.refit();
    s.graph.heat = 1.0;
    s.auto_fit = true;
    s.needs_fit = false;
    assert!(
        !poll(&mut s, t + 1.0, &mut tx),
        "heat without motion or camera change must not report true"
    );
}

fn folder_vault() -> TempVault {
    TempVault::new(&[
        ("Welcome.md", "# Welcome\n\nRoot note.\n"),
        ("projects/alpha/note.md", "# Note\n\nAlpha note.\n"),
        (
            "projects/alpha/overview.md",
            "# Overview\n\nAlpha overview.\n",
        ),
        ("projects/beta/note.md", "# Note\n\nBeta note.\n"),
    ])
}

#[test]
fn sidebar_groups_notes_by_folder_and_collapses() {
    let tmp = folder_vault();
    let mut rt = runtime(&tmp);
    assert!(rt.find("brand-logo").is_some());
    assert!(rt.find("folder-header-root").is_some());
    assert!(rt.find("folder-header-projects-alpha").is_some());
    assert!(rt.find("folder-header-projects-beta").is_some());
    let welcome = rt.state.vault.resolve("Welcome").expect("Welcome");
    let alpha_note = rt
        .state
        .vault
        .resolve("projects/alpha/note")
        .expect("alpha note");
    assert!(rt.find(&format!("note-row-{welcome}")).is_some());
    assert!(rt.find(&format!("note-row-{alpha_note}")).is_some());
    // Collapse the alpha group: its rows must disappear.
    let (x, y) = rt
        .find("folder-header-projects-alpha")
        .expect("alpha header");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.is_collapsed("projects/alpha"));
    assert!(rt.find(&format!("note-row-{alpha_note}")).is_none());
    assert!(rt.find(&format!("note-row-{welcome}")).is_some());
    // Search expands collapsed groups again (drive it like a user types, so
    // the runtime rebuilds the tree).
    rt.key(command('k'));
    rt.frame(1.0 / 60.0);
    for c in "note".chars() {
        rt.key(KeyEvent {
            key: Key::Text(c.to_string()),
            mods: Mods::default(),
        });
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(rt.state.search.text, "note");
    assert!(rt.find(&format!("note-row-{alpha_note}")).is_some());
}

#[test]
fn folder_new_button_creates_in_that_folder() {
    let tmp = folder_vault();
    let mut rt = runtime(&tmp);
    let (x, y) = rt
        .find("folder-new-projects-alpha")
        .expect("alpha new button");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    let path = rt.state.selected_path.clone().expect("selected path");
    assert!(
        path.ends_with("projects/alpha/Untitled.md"),
        "{}",
        path.display()
    );
    assert_eq!(rt.state.current_folder(), "projects/alpha");
    assert_eq!(rt.state.mode, Mode::Editor);
    assert!(
        !rt.state.is_collapsed("projects/alpha"),
        "the + button must not toggle the group it sits in"
    );
}

#[test]
fn new_note_targets_the_selected_notes_folder() {
    let tmp = folder_vault();
    let mut rt = runtime(&tmp);
    let beta_note = rt
        .state
        .vault
        .resolve("projects/beta/note")
        .expect("beta note");
    rt.state.load_note(beta_note, false);
    rt.state.new_note();
    rt.frame(1.0 / 60.0);
    let path = rt.state.selected_path.clone().expect("selected path");
    assert!(
        path.ends_with("projects/beta/Untitled.md"),
        "{}",
        path.display()
    );
}

#[test]
fn duplicate_titles_open_by_path_and_create_by_path() {
    let tmp = folder_vault();
    let mut rt = runtime(&tmp);
    let alpha_note = rt
        .state
        .vault
        .resolve("projects/alpha/note")
        .expect("alpha note");
    let beta_note = rt
        .state
        .vault
        .resolve("projects/beta/note")
        .expect("beta note");
    assert_ne!(alpha_note, beta_note);
    assert!(rt.state.vault.is_duplicate(alpha_note));
    assert_eq!(
        rt.state.vault.display_label(alpha_note),
        "projects/alpha/note"
    );
    // A path-qualified rendered link must open the matching duplicate.
    let mut tx = Tx::default();
    rt.state.open_link("note:projects/beta/note", &mut tx);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.selected, Some(beta_note));
    // A path-qualified missing link creates inside that folder.
    rt.state.open_link("missing:projects/alpha/Fresh", &mut tx);
    rt.frame(1.0 / 60.0);
    let path = rt.state.selected_path.clone().expect("selected path");
    assert!(
        path.ends_with("projects/alpha/Fresh.md"),
        "{}",
        path.display()
    );
    // Graph still works: both duplicates have distinct nodes.
    rt.state.set_mode(Mode::Graph);
    rt.frame(1.0 / 60.0);
    assert!(rt.find(&format!("graph-node-{alpha_note}")).is_some());
    assert!(rt.find(&format!("graph-node-{beta_note}")).is_some());
}

#[test]
fn graph_wheel_zooms_and_shift_wheel_pans() {
    use rune::{Mods, WheelEvent};
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    rt.state.set_mode(Mode::Graph);
    rt.state.needs_fit = false;
    rt.frame(1.0 / 60.0);
    // Settle the layout, then freeze the camera to observe the wheel alone.
    rt.state.auto_fit = false;
    rt.state.cam.set([0.0, 0.0], 1.0);
    assert!(rt.find("graph-canvas").is_some());

    let canvas_at = |rt: &Runtime<State>| rt.find("graph-canvas").unwrap();

    // Plain wheel up zooms in at the pointer.
    let (cx, cy) = canvas_at(&rt);
    rt.wheel_event(WheelEvent::new(0.0, -40.0).at(cx, cy));
    rt.frame(1.0 / 60.0);
    assert!(rt.state.cam.zoom() > 1.0, "zoom={}", rt.state.cam.zoom());
    let zoomed = rt.state.cam.zoom();
    let pan_at_zoom = rt.state.cam.pan();

    // Plain wheel down zooms back out; pan only moves to keep the pointer point.
    let (cx, cy) = canvas_at(&rt);
    rt.wheel_event(WheelEvent::new(0.0, 40.0).at(cx, cy));
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state.cam.zoom() < zoomed,
        "zoom={} vs {zoomed}",
        rt.state.cam.zoom()
    );

    // Ctrl+wheel and Alt+wheel also zoom.
    rt.state.cam.set([0.0, 0.0], 1.0);
    let (cx, cy) = canvas_at(&rt);
    rt.wheel_event(WheelEvent::new(0.0, -40.0).at(cx, cy).mods(Mods {
        ctrl: true,
        cmd: true,
        ..Mods::default()
    }));
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state.cam.zoom() > 1.0,
        "ctrl zoom={}",
        rt.state.cam.zoom()
    );
    rt.state.cam.set([0.0, 0.0], 1.0);
    let (cx, cy) = canvas_at(&rt);
    rt.wheel_event(WheelEvent::new(0.0, -40.0).at(cx, cy).mods(Mods {
        alt: true,
        ..Mods::default()
    }));
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state.cam.zoom() > 1.0,
        "alt zoom={}",
        rt.state.cam.zoom()
    );

    // Shift+wheel pans instead of zooming.
    rt.state.cam.set(pan_at_zoom, 1.5);
    let before = rt.state.cam.pan();
    let (cx, cy) = canvas_at(&rt);
    rt.wheel_event(WheelEvent::new(20.0, 30.0).at(cx, cy).mods(Mods {
        shift: true,
        ..Mods::default()
    }));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.cam.zoom(), 1.5, "shift must not zoom");
    assert_ne!(rt.state.cam.pan(), before, "shift must pan");
}

#[test]
fn graph_low_zoom_stays_interactive_and_culls_offscreen() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    rt.state.set_mode(Mode::Graph);
    rt.frame(1.0 / 60.0);
    let node0 = format!("graph-node-{}", rt.state.vault.resolve("Alpha").unwrap());
    assert!(rt.find(&node0).is_some(), "nodes mount in Graph mode");
    // At low Global zoom the LOD dots must keep the same keys and presses.
    rt.state.cam.set([0.0, 0.0], 0.3);
    rt.frame(1.0 / 60.0);
    assert!(rt.find(&node0).is_some(), "low-zoom dots keep node keys");
    let (x, y) = rt.find(&node0).expect("node0");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.selected, rt.state.vault.resolve("Alpha"));
    // Panning far away culls non-selected nodes from the tree.
    rt.state.cam.set([8000.0, 8000.0], 1.0);
    rt.frame(1.0 / 60.0);
    let beta = format!("graph-node-{}", rt.state.vault.resolve("Beta").unwrap());
    // Beta is not selected after the click-hit Alpha selection above, so it
    // must be culled; the selected node survives culling.
    assert!(
        rt.find(&beta).is_none(),
        "off-screen non-selected nodes cull"
    );
}

//! Settings › Graph › Reduce motion: a persisted System / On / Off choice
//! that resolves, together with `MUNINN_REDUCED_MOTION` and the OS
//! preference, into the one value the graph obeys.
//!
//! Tests use explicit config/state roots so the user's real
//! `~/.config/muninn/settings.json` is never touched.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, ReduceMotion, Scope, Settings, State, on_key, poll, view};
use rune::{Env, Runtime};

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
fn indicator_moving(rt: &Runtime<State>) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.path.contains("settings-tabs") && e.path.ends_with("indicator") && e.moving)
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
        // The tab bar's selection indicator slides between segments.
        click(&mut rt, "settings-tabs/segment4");
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

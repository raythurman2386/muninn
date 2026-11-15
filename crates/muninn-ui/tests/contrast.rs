//! Issue #72: the WCAG 2.1 AA contrast audit, headless.
//!
//! The audited pairs are the palette's secondary inks on the page and the
//! raised layers through `layer_03` (including the hover overlay over the
//! selected `layer_03`), the Kanban lane header's status inks ("Over limit" and
//! the done lane's "Complete"), Excalidraw's default ink on a dark
//! `viewBackgroundColor`, and the Board palette's swatches (see the unit test
//! in `board_palette`). The colour maths has unit tests in
//! `muninn_ui::contrast`; this file checks what the app actually paints and
//! what it writes back to a `.excalidraw` file.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::Element;
use muninn_core::testing::TempVault;
use muninn_ui::contrast::over;
use muninn_ui::{
    Mode, Settings, State, TEXT, ThemeMode, UI, contrast_ratio, meets, on_key, poll, resolve_kit,
    view,
};
use rune::{ElementInfo, Env, Runtime};
use rune_kit::Kit;

const BUSY: &str = include_str!("../../muninn-core/tests/fixtures/kanban/over-limit.md");

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// An Excalidraw board with no elements and only the paper named, so the
/// empty-board placeholder paints on it.
fn empty_board(paper: &str) -> String {
    format!(
        r##"{{"type":"excalidraw","version":2,"source":"test","elements":[],
            "appState":{{"viewBackgroundColor":"{paper}","gridSize":null}},"files":{{}}}}"##
    )
}

/// A board whose paper is `viewBackgroundColor` on the dark theme, holding one
/// shape already inked with Excalidraw's default `#1e1e1e`.
const DARK_BOARD: &str = r##"{
  "type": "excalidraw",
  "version": 2,
  "source": "test",
  "elements": [
    {
      "id": "keep",
      "type": "rectangle",
      "x": 0,
      "y": 0,
      "width": 60,
      "height": 40,
      "angle": 0,
      "strokeColor": "#1e1e1e",
      "backgroundColor": "transparent",
      "fillStyle": "solid",
      "strokeWidth": 2,
      "strokeStyle": "solid",
      "roughness": 1,
      "opacity": 100,
      "groupIds": [],
      "frameId": null,
      "index": "a0",
      "roundness": null,
      "seed": 1,
      "version": 1,
      "versionNonce": 1,
      "isDeleted": false,
      "boundElements": null,
      "updated": 1,
      "link": null,
      "locked": false
    }
  ],
  "appState": {"viewBackgroundColor": "#121212", "gridSize": null},
  "files": {}
}"##;

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-contrast-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn state_at(root: &Path, config: &Path, state_home: &Path, theme: ThemeMode) -> State {
    State::with_settings(
        root.to_path_buf(),
        state_home.to_path_buf(),
        config.to_path_buf(),
        Settings {
            theme,
            kanban_autosave: false,
            ..Settings::load_from(config)
        },
    )
}

fn runtime(root: &Path, theme: ThemeMode) -> Runtime<State> {
    let (config, state_home) = (unique_dir("config"), unique_dir("state"));
    let state = state_at(root, &config, &state_home, theme);
    let mut rt = Runtime::new(
        state,
        view,
        (1400.0, 900.0),
        Env::new(theme == ThemeMode::Dark, 2.0),
    );
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

fn element(rt: &Runtime<State>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("missing element {key}"))
}

/// The colour a labelled text paints with — for text the view writes without
/// a key of its own, such as the done lane's "Complete".
fn labelled(rt: &Runtime<State>, label: &str) -> rune::Color {
    rt.elements()
        .into_iter()
        .find(|e| e.text.as_deref() == Some(label) && !e.exiting)
        .and_then(|e| e.text_color)
        .unwrap_or_else(|| panic!("missing text {label:?}"))
}

/// A keyed element's fill: the surface its header text sits on.
fn surface_of(rt: &Runtime<State>, key: &str) -> rune::Color {
    element(rt, key).fill
}

#[test]
fn every_theme_keeps_secondary_inks_above_their_floors() {
    let vault = TempVault::new(&[("Agenda.md", "# Agenda\n")]);
    let (config, state_home) = (unique_dir("kit-config"), unique_dir("kit-state"));
    for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::Follow] {
        for system_dark in [false, true] {
            let state = state_at(vault.path(), &config, &state_home, theme);
            let kit = resolve_kit(&state, system_dark);
            for surface in [kit.c.bg_base, kit.c.layer_01, kit.c.layer_02] {
                for (token, ink, floor) in [
                    ("text_muted", kit.c.text_muted, TEXT),
                    ("text_faint", kit.c.text_faint, TEXT),
                    // Icons are components: WCAG's 3:1, not 4.5:1.
                    ("icon_muted", kit.c.icon_muted, 3.0),
                ] {
                    assert!(
                        meets(ink, surface, floor),
                        "{theme:?} (system dark {system_dark}) {token} {} on {} is {:.2}",
                        ink.hex(),
                        surface.hex(),
                        contrast_ratio(ink, surface),
                    );
                }
            }
        }
    }
}

/// Omarchy palette fixtures: a dark and a light theme, enough for a kit.
const OMARCHY_DARK: &str = "mode = \"dark\"\nbackground = \"#141414\"\nforeground = \"#e1e1e1\"\naccent = \"#34d399\"\nred = \"#f7768e\"\n";
const OMARCHY_LIGHT: &str = "mode = \"light\"\nbackground = \"#f5f4ed\"\nforeground = \"#3f4a45\"\naccent = \"#064e3b\"\nred = \"#c92a2a\"\n";

/// Every appearance `resolve_kit` can return: Light, Dark, Follow in either
/// system appearance, and Omarchy's light and dark palettes.
fn resolved_kits(root: &Path, config: &Path, state_home: &Path) -> Vec<(String, Kit)> {
    let mut kits = Vec::new();
    for (name, theme) in [
        ("Light", ThemeMode::Light),
        ("Dark", ThemeMode::Dark),
        ("Follow", ThemeMode::Follow),
    ] {
        for system_dark in [false, true] {
            let state = state_at(root, config, state_home, theme);
            kits.push((
                format!("{name} (system dark {system_dark})"),
                resolve_kit(&state, system_dark),
            ));
        }
    }
    for (name, palette) in [
        ("Omarchy dark", OMARCHY_DARK),
        ("Omarchy light", OMARCHY_LIGHT),
    ] {
        let dir = unique_dir("omarchy-palette");
        let path = dir.join("colors.toml");
        fs::write(&path, palette).expect("palette");
        let mut state = state_at(root, config, state_home, ThemeMode::Omarchy);
        state.set_omarchy_path(path);
        kits.push((name.to_owned(), resolve_kit(&state, true)));
    }
    kits
}

/// #72 follow-up: `layer_03` is a text surface too — the selected fill behind
/// mode chips, Board tool buttons and Board rows — and its hover/pressed
/// overlays lighten or darken it. Every ink painted there clears its floor,
/// and the one secondary ink on a hovered selected row (`text_muted`) clears
/// 4.5:1 on the composite.
///
/// `layer_04` carries no text or icon in Muninn (rune-kit uses it as a switch
/// track), so only the base ink a future component would reach for is pinned.
#[test]
fn layer_03_and_its_overlays_clear_their_floors() {
    let vault = TempVault::new(&[("Agenda.md", "# Agenda\n")]);
    let (config, state_home) = (unique_dir("layer-config"), unique_dir("layer-state"));
    for (name, kit) in resolved_kits(vault.path(), &config, &state_home) {
        let c = &kit.c;
        // The selected fill is a stable surface for the secondary inks too.
        for surface in [c.bg_base, c.layer_01, c.layer_02, c.layer_03] {
            for (token, ink, floor) in [
                ("text_muted", c.text_muted, TEXT),
                ("text_faint", c.text_faint, TEXT),
                ("icon_muted", c.icon_muted, UI),
                ("icon_faint", c.icon_faint, UI),
                ("warning", c.warning.fg, TEXT),
                ("success", c.success.fg, TEXT),
                ("danger", c.danger.fg, TEXT),
                ("info", c.info.fg, TEXT),
            ] {
                assert!(
                    meets(ink, surface, floor),
                    "{name} {token} {} on {} is {:.2}, floor {floor}",
                    ink.hex(),
                    surface.hex(),
                    contrast_ratio(ink, surface),
                );
            }
        }

        let hovered = over(c.hover, c.layer_03);
        let pressed = over(c.pressed, c.layer_03);
        for (state, surface) in [
            ("layer_03", c.layer_03),
            ("hover", hovered),
            ("pressed", pressed),
        ] {
            // The primary ink on a selected control, and its accent icon.
            assert!(
                meets(c.text_base, surface, TEXT),
                "{name} text_base {} on {state} {} is {:.2}",
                c.text_base.hex(),
                surface.hex(),
                contrast_ratio(c.text_base, surface),
            );
            assert!(
                meets(c.text_accent, surface, UI),
                "{name} accent icon {} on {state} {} is {:.2}",
                c.text_accent.hex(),
                surface.hex(),
                contrast_ratio(c.text_accent, surface),
            );
        }

        // A selected Board row keeps its hover tint; its detail is `muted`.
        assert!(
            meets(c.text_muted, hovered, TEXT),
            "{name} selected-row detail {} on hovered {} is {:.2}",
            c.text_muted.hex(),
            hovered.hex(),
            contrast_ratio(c.text_muted, hovered),
        );

        // No Muninn text sits on `layer_04`; if one ever did, the base ink is
        // the token to reach for, and it clears its floor in both appearances.
        assert!(
            meets(c.text_base, c.layer_04, TEXT),
            "{name} text_base {} on layer_04 {} is {:.2}",
            c.text_base.hex(),
            c.layer_04.hex(),
            contrast_ratio(c.text_base, c.layer_04),
        );
        assert!(
            meets(c.icon_base, c.layer_04, UI),
            "{name} icon_base {} on layer_04 {} is {:.2}",
            c.icon_base.hex(),
            c.layer_04.hex(),
            contrast_ratio(c.icon_base, c.layer_04),
        );
    }
}

/// The selected Board row is filled with `layer_03`; its detail must use the
/// stronger secondary ink and clear 4.5:1 on that fill, not `text_faint`.
#[test]
fn a_selected_board_row_keeps_its_detail_legible() {
    let vault = TempVault::new(&[
        ("Agenda.md", "# Agenda\n"),
        ("Sketch.excalidraw", &empty_board("#ffffff")),
    ]);
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let mut rt = runtime(vault.path(), theme);
        open_board(&mut rt, "Sketch");
        let i = rt
            .state
            .boards
            .iter()
            .position(|b| b.file.title == "Sketch")
            .expect("Sketch");
        let row = element(&rt, &format!("board-row-{i}"));
        let detail = element(&rt, &format!("board-row-{i}-detail"));
        // The mode tabs paint the kit's `text_muted`; the selected row's fill
        // must be `layer_03` with that same ink, not `text_faint`.
        let muted = labelled(&rt, "Write");
        assert_eq!(
            row.fill,
            element(&rt, "mode-board").fill,
            "{theme:?}: the row uses the selected fill"
        );
        let ink = detail.text_color.expect("the detail paints");
        assert_eq!(
            ink, muted,
            "{theme:?}: the selected row's detail uses the muted ink"
        );
        assert!(
            meets(ink, row.fill, TEXT),
            "{theme:?}: detail {} on selected row {} is {:.2}",
            ink.hex(),
            row.fill.hex(),
            contrast_ratio(ink, row.fill),
        );
    }
}

/// The Kanban lane header's "Over limit" and the done lane's "Complete" are
/// normal-size text on `layer_01`: both need 4.5:1, in both themes.
#[test]
fn kanban_lane_status_labels_clear_the_text_floor() {
    let vault = TempVault::new(&[("Agenda.md", "# Agenda\n"), ("Busy.md", BUSY)]);
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        let mut rt = runtime(vault.path(), theme);
        let id = rt.state.vault.resolve("Busy").expect("Busy");
        rt.update(|s, _| s.open_kanban(id));
        frames(&mut rt, 2);
        assert_eq!(rt.state.mode, Mode::Kanban);

        for (label, ink, lane_key) in [
            (
                "over limit",
                element(&rt, "kanban-lane-1-over")
                    .text_color
                    .expect("Over limit paints"),
                "kanban-lane-1",
            ),
            ("done lane", labelled(&rt, "Complete"), "kanban-lane-2"),
        ] {
            let lane = surface_of(&rt, lane_key);
            assert!(
                meets(ink, lane, TEXT),
                "{theme:?} {label}: {} on lane {} is {:.2}",
                ink.hex(),
                lane.hex(),
                contrast_ratio(ink, lane),
            );
        }
    }
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 2);
}

fn canvas_origin(rt: &Runtime<State>) -> [f32; 2] {
    let r = rt.presented("board-canvas").expect("board canvas").0;
    [r[0], r[1]]
}

/// Press at canvas point `path[0]`, drag through the rest, release.
fn stroke(rt: &mut Runtime<State>, path: &[[f32; 2]]) {
    let o = canvas_origin(rt);
    let at = |p: [f32; 2]| (o[0] + p[0], o[1] + p[1]);
    let (x, y) = at(path[0]);
    rt.pointer_down(x, y);
    for w in path.windows(2) {
        let (x, y) = at(w[1]);
        rt.pointer_move(x, y);
        rt.frame(1.0 / 60.0);
    }
    let (x, y) = at(*path.last().unwrap());
    rt.pointer_up(x, y);
    frames(rt, 2);
}

fn open_board(rt: &mut Runtime<State>, title: &str) {
    rt.key(rune::KeyEvent {
        key: rune::Key::Command('4'),
        mods: rune::Mods {
            cmd: true,
            ..rune::Mods::default()
        },
    });
    frames(rt, 2);
    let i = rt
        .state
        .boards
        .iter()
        .position(|b| b.file.title == title)
        .unwrap_or_else(|| panic!("no board {title}"));
    click(rt, &format!("board-row-{i}"));
    frames(rt, 60);
}

fn last_shape(rt: &Runtime<State>) -> muninn_core::Shape {
    rt.state
        .board_open
        .as_ref()
        .and_then(|o| o.model.as_ref())
        .and_then(|m| m.elements.last())
        .and_then(Element::shape)
        .cloned()
        .expect("a shape")
}

/// `#1e1e1e` on `#121212` is 1.12:1: invisible. The file's shapes keep the
/// ink they were saved with, but a *new* element is written in Excalidraw's
/// dark-theme ink, which round-trips as an ordinary colour.
#[test]
fn a_new_shape_on_a_dark_board_stays_visible_without_rewriting_old_ink() {
    let vault = TempVault::new(&[("Agenda.md", "# Agenda\n"), ("Dark.excalidraw", DARK_BOARD)]);
    let mut rt = runtime(vault.path(), ThemeMode::Dark);
    open_board(&mut rt, "Dark");
    assert_eq!(rt.state.mode, Mode::Board);

    let existing = rt
        .state
        .board_open
        .as_ref()
        .and_then(|o| o.model.as_ref())
        .and_then(|m| m.shape("keep"))
        .cloned()
        .expect("the committed shape");
    assert_eq!(existing.stroke_color, "#1e1e1e", "file ink is drawn as-is");
    assert_eq!(
        rt.state.board_stroke, "#1e1e1e",
        "the palette pick itself is not changed"
    );

    rt.key(rune::KeyEvent {
        key: rune::Key::Text("r".into()),
        mods: rune::Mods::default(),
    });
    stroke(&mut rt, &[[120.0, 120.0], [260.0, 220.0]]);
    let made = last_shape(&rt);
    assert_eq!(made.kind, muninn_core::ShapeKind::Rectangle);
    assert_eq!(made.stroke_color, "#e6e6e6", "Excalidraw's dark-theme ink");
    let paper = rune::color::hex("#121212").expect("paper");
    assert!(
        meets(
            rune::color::hex(&made.stroke_color).expect("ink"),
            paper,
            3.0
        ),
        "a new stroke is visible on the paper"
    );

    // Saving keeps both inks: the old shape's bytes are untouched, and the
    // new one round-trips as the explicit colour it was written with.
    rt.key(rune::KeyEvent {
        key: rune::Key::Command('s'),
        mods: rune::Mods {
            cmd: true,
            ..rune::Mods::default()
        },
    });
    frames(&mut rt, 2);
    let saved = fs::read_to_string(vault.path().join("Dark.excalidraw")).expect("saved board");
    assert!(saved.contains("\"id\": \"keep\""), "the old shape is kept");
    assert!(
        saved.matches("\"strokeColor\": \"#1e1e1e\"").count() == 1,
        "only the committed shape keeps the default ink"
    );
    assert!(saved.contains("\"strokeColor\": \"#e6e6e6\""));
}

/// The empty-board placeholder is canvas ink too: `#808080` was 3.95:1 on
/// Excalidraw's white paper and invisible on a dark one.
#[test]
fn the_empty_board_placeholder_clears_the_text_floor_on_either_paper() {
    let vault = TempVault::new(&[
        ("Agenda.md", "# Agenda\n"),
        ("Dark.excalidraw", &empty_board("#121212")),
        ("White.excalidraw", &empty_board("#ffffff")),
    ]);
    let mut rt = runtime(vault.path(), ThemeMode::Dark);
    for (title, paper) in [("Dark", "#121212"), ("White", "#ffffff")] {
        open_board(&mut rt, title);
        let ink = element(&rt, "board-empty")
            .text_color
            .expect("the placeholder paints");
        let paper = rune::color::hex(paper).expect("paper");
        assert!(
            meets(ink, paper, TEXT),
            "{title}: placeholder {} on {} is {:.2}",
            ink.hex(),
            paper.hex(),
            contrast_ratio(ink, paper),
        );
    }
}

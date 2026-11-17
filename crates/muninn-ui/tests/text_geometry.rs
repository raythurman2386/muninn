//! #192/#193: translated copy must fit at the app's narrow and default sizes.
//!
//! The pseudo language is longer than English (it brackets every catalog
//! string), so it is the growth proxy a language pack will resemble; Spanish
//! is the first real pack and proves the shipped copy fits. This renders every
//! copy-heavy surface at 840×640 and 1180×780 in **both** languages and reads
//! the runtime's laid-out geometry: a text element's `visible` rect is its
//! rect intersected with every ancestor clip, so `visible` smaller than the
//! part a scroll viewport allows means a box, card or column clipped its text.
//! A non-scrolled text element must also sit inside the window.
//!
//! Legitimate clipping, and only this:
//!
//! - an exiting element (`ElementInfo::exiting`): its transition still moves it;
//! - text inside a scroll viewport (`*-scroll`): the viewport's rectangle is
//!   the allowance, so content scrolled off-screen or wider than a horizontal
//!   viewport (Kanban lanes) passes, while a card *inside* the viewport that
//!   clips its own text still fails;
//! - intentional ellipsis/truncation: Rune lays truncated text out to the
//!   box's width, so `rect` already fits and there is no clip to exempt. Rune
//!   exposes no `truncated` flag on `ElementInfo`; if one lands, this harness
//!   should name it rather than infer it.
//!
//! `clipped_text` is unit-tested against a synthetic view below: the check
//! fails on a deliberately too-narrow clipped text and passes the same text
//! unclipped, so a green screen run is not a pass-only assertion.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_i18n::Lang;
use muninn_ui::{LanguageChoice, Settings, SettingsTab, State, ThemeMode, on_key, poll, view};
use rune::{Color, Cx, El, ElementInfo, Env, Runtime, col, text};

const WINDOW_SIZES: [(f32, f32); 2] = [(840.0, 640.0), (1180.0, 780.0)];
const TOLERANCE: f32 = 1.0;

const BOARD: &str = "\
---
kanban-plugin: board
---

## To do

- [ ] Ship the locale pack
- [ ] A much longer card title that wraps inside its card when the language grows
- [x] Finished card

## Done

- [x] Something already done
";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-geometry-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

/// A scene rendered under one language. Each one is a full frame: the window
/// chrome is always there, and the overlay replaces the main column.
#[derive(Clone, Copy, Debug)]
enum Scene {
    Editor,
    Help,
    Settings(SettingsTab),
    Palette,
    Kanban,
}

impl Scene {
    fn name(self) -> String {
        match self {
            Scene::Editor => "editor and sidebar".to_owned(),
            Scene::Help => "Help".to_owned(),
            Scene::Settings(tab) => format!("Settings › {tab:?}"),
            Scene::Palette => "command palette".to_owned(),
            Scene::Kanban => "Kanban".to_owned(),
        }
    }
}

fn runtime(scene: Scene, size: (f32, f32), lang: Lang) -> (TempVault, Runtime<State>) {
    let vault = TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nA note with some body text.\n"),
        ("Board.md", BOARD),
    ]);
    let config = unique_dir("config");
    let state_home = unique_dir("state");
    let settings = Settings {
        theme: ThemeMode::Dark,
        // Pseudo is the longest catalog and is longer than any wave-1
        // translation. Spanish and French are real packs: they must fit too,
        // so the harness runs all three. A German worst-string fixture for
        // #195 belongs here, beside this setting, once its pack exists.
        ui_language: LanguageChoice::Language(lang),
        ..Settings::default()
    };
    let state = State::with_settings(vault.path().to_path_buf(), state_home, config, settings);
    let mut rt = Runtime::new(state, view, size, Env::new(true, 1.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    match scene {
        Scene::Editor => {}
        Scene::Help => {
            rt.update(|s, _| s.open_help());
        }
        Scene::Settings(tab) => {
            rt.update(|s, _| {
                s.open_settings();
                s.set_settings_tab(tab);
            });
        }
        Scene::Palette => {
            rt.update(|s, _| s.open_palette());
        }
        Scene::Kanban => {
            let id = rt.state.vault.resolve("Board").expect("Board note");
            rt.update(move |s, _| s.open_kanban(id));
        }
    }
    for _ in 0..12 {
        rt.frame(1.0 / 60.0);
    }
    (vault, rt)
}

#[derive(Debug)]
struct Clipped {
    text: String,
    key: Option<String>,
    role: &'static str,
    rect: [f32; 4],
    visible: [f32; 4],
    allowed: [f32; 4],
    why: &'static str,
    ancestors: String,
}

fn ancestry(elements: &[ElementInfo], index: usize) -> String {
    let mut out = String::new();
    let mut parent = elements[index].parent;
    while let Some(i) = parent {
        out.push_str(&format!(
            " {}:{:?}",
            elements[i].key.as_deref().unwrap_or(elements[i].role),
            elements[i].rect
        ));
        parent = elements[i].parent;
    }
    out
}

fn intersect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let left = a[0].max(b[0]);
    let top = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let bottom = (a[1] + a[3]).min(b[1] + b[3]);
    [left, top, (right - left).max(0.0), (bottom - top).max(0.0)]
}

fn is_scroll(element: &ElementInfo) -> bool {
    element
        .key
        .as_deref()
        .is_some_and(|key| key.ends_with("-scroll"))
}

/// The part of `rect` that ancestor scroll viewports allow.
fn scroll_allowance(elements: &[ElementInfo], index: usize, rect: [f32; 4]) -> Option<[f32; 4]> {
    let mut allowed = None;
    let mut parent = elements[index].parent;
    while let Some(i) = parent {
        if is_scroll(&elements[i]) {
            let next = elements[i].rect;
            allowed = Some(allowed.map_or(next, |current: [f32; 4]| intersect(current, next)));
        }
        parent = elements[i].parent;
    }
    allowed.map(|clip| intersect(rect, clip))
}

fn same(a: [f32; 4], b: [f32; 4]) -> bool {
    (a[0] - b[0]).abs() <= TOLERANCE
        && (a[1] - b[1]).abs() <= TOLERANCE
        && (a[2] - b[2]).abs() <= TOLERANCE
        && (a[3] - b[3]).abs() <= TOLERANCE
}

/// Visible text whose drawn box is clipped beyond what a scroll viewport
/// allows, or lies outside the window.
fn clipped_text(elements: &[ElementInfo], window: (f32, f32)) -> Vec<Clipped> {
    let mut out = Vec::new();
    for (index, element) in elements.iter().enumerate() {
        if element.exiting || element.virtual_extent {
            continue;
        }
        let Some(text) = element.text.as_deref() else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        let scroll = scroll_allowance(elements, index, element.rect);
        let (allowed, why) = match scroll {
            Some(allowed) => (
                allowed,
                "clipped beyond the scroll viewport that contains it",
            ),
            None => (element.rect, "clipped by a non-scroll ancestor"),
        };
        if !same(element.visible, allowed) {
            out.push(Clipped {
                text: text.to_owned(),
                key: element.key.clone(),
                role: element.role,
                rect: element.rect,
                visible: element.visible,
                allowed,
                why,
                ancestors: ancestry(elements, index),
            });
            continue;
        }
        if scroll.is_none() {
            let [x, y, w, h] = element.rect;
            let inside = x >= -TOLERANCE
                && y >= -TOLERANCE
                && x + w <= window.0 + TOLERANCE
                && y + h <= window.1 + TOLERANCE;
            if !inside {
                out.push(Clipped {
                    text: text.to_owned(),
                    key: element.key.clone(),
                    role: element.role,
                    rect: element.rect,
                    visible: element.visible,
                    allowed,
                    why: "outside the window",
                    ancestors: ancestry(elements, index),
                });
            }
        }
    }
    out
}

fn report(scene: Scene, size: (f32, f32), found: &[Clipped]) -> String {
    let mut message = format!(
        "{} at {}×{} clips {} visible string(s):\n",
        scene.name(),
        size.0,
        size.1,
        found.len()
    );
    for finding in found.iter().take(20) {
        message.push_str(&format!(
            "  {:?} role={} key={:?}\n    rect={:?} visible={:?} allowed={:?} ({})\n    ancestors: {}\n",
            finding.text,
            finding.role,
            finding.key,
            finding.rect,
            finding.visible,
            finding.allowed,
            finding.why,
            finding.ancestors
        ));
    }
    message
}

#[test]
fn every_copy_heavy_screen_fits_its_clips_at_both_sizes() {
    let scenes = [
        Scene::Editor,
        Scene::Help,
        Scene::Palette,
        Scene::Kanban,
        Scene::Settings(SettingsTab::Vaults),
        Scene::Settings(SettingsTab::Editor),
        Scene::Settings(SettingsTab::Appearance),
        Scene::Settings(SettingsTab::Graph),
        Scene::Settings(SettingsTab::Sync),
        Scene::Settings(SettingsTab::Kanban),
    ];
    // Pseudo is the growth proxy; Spanish and French are the shipped packs.
    // All must fit at both sizes, and the pack runs are real copy, not an
    // allowlist.
    for lang in [Lang::Pseudo, Lang::Es, Lang::Fr] {
        for size in WINDOW_SIZES {
            for scene in scenes {
                let (_vault, rt) = runtime(scene, size, lang);
                let found = clipped_text(&rt.elements(), size);
                assert!(
                    found.is_empty(),
                    "[{}] {}",
                    lang.tag(),
                    report(scene, size, &found)
                );
            }
        }
    }
}

/// The check bites: the same long text is reported when a clipping parent is
/// too narrow, and passes when it is not clipped.
#[test]
fn a_deliberately_clipped_text_is_reported() {
    fn clipped_view(_: &(), _: &mut Cx<()>) -> El {
        col().w_full().h_full().child(
            col()
                .key("clip-box")
                .w(150.0)
                .h(28.0)
                .fill(Color::BLACK)
                .clip_soft(0.0)
                .child(text("This sentence is deliberately much too long for its box").wrap()),
        )
    }
    fn open_view(_: &(), _: &mut Cx<()>) -> El {
        col().w_full().h_full().child(
            col()
                .key("clip-box")
                .w(150.0)
                .h(28.0)
                .fill(Color::BLACK)
                .child(text("This sentence is deliberately much too long for its box").wrap()),
        )
    }

    let mut clipped = Runtime::new((), clipped_view, (300.0, 200.0), Env::new(true, 1.0));
    clipped.frame(0.0);
    let found = clipped_text(&clipped.elements(), (300.0, 200.0));
    assert!(
        found
            .iter()
            .any(|finding| finding.text.contains("deliberately")),
        "a too-narrow clip must be reported, got {found:?}"
    );

    let mut open = Runtime::new((), open_view, (300.0, 200.0), Env::new(true, 1.0));
    open.frame(0.0);
    let found = clipped_text(&open.elements(), (300.0, 200.0));
    assert!(
        !found
            .iter()
            .any(|finding| finding.text.contains("deliberately")),
        "unclipped text is not a finding, got {found:?}"
    );
}

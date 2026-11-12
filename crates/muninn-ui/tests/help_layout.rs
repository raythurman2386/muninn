//! DES-01: the shortcuts page fills the main column and stays inside it.
//!
//! Column choice is covered next to `help_layout`. These drive the real runtime
//! so a regression that ignores that choice, drops a shortcut, or lets a log
//! line paint past the page fails here.

use std::fs;

use muninn_core::testing::TempVault;
use muninn_ui::{State, ThemeMode, on_key, poll, view};
use rune::{Env, Runtime};

fn help_at(width: f32, height: f32, dark: bool) -> (TempVault, Runtime<State>) {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut state =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    // Pin Dark/Light so the kit is the OpenCode one under test. The default
    // Omarchy mode would read the host's real palette (on Omarchy machines)
    // and the ink checks below would depend on the user's theme.
    state.theme = if dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    let mut rt = Runtime::new(state, view, (width, height), Env::new(dark, 1.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt.update(|s, _| s.open_help());
    for _ in 0..6 {
        rt.frame(1.0 / 60.0);
    }
    (tmp, rt)
}

fn rect(rt: &Runtime<State>, key: &str) -> [f32; 4] {
    rt.presented(key)
        .unwrap_or_else(|| panic!("missing {key}"))
        .0
}

fn text_of(rt: &Runtime<State>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .and_then(|element| element.text)
        .unwrap_or_else(|| panic!("missing text {key}"))
}

const ACTIONS: &[&str] = &[
    "Focus search",
    "Move the highlighted result",
    "Open the highlighted result",
    "Back in note history",
    "Forward in note history",
    "Write / Read / Graph / Board / Kanban",
    "Close the outermost layer",
    "Open the command palette",
    "Find in the open note",
    "Insert a wikilink",
    "New note",
    "Save now (autosave is on)",
    "Toggle Write / Read",
    "Wikilink completion",
    "Indent four spaces",
    "Rename the selected note (on Board: the board)",
    "Delete the selected note",
    "Toggle Graph",
    "Toggle Global / Local",
    "Move the focus ring",
    "Open the focused note",
    "Zoom in / out",
    "Fit the visible nodes",
    "Toggle Isolate",
    "Leave the graph",
    "Open this Help",
    "Open Help when not typing",
    "Move between panes",
    "Move through a list",
    "Open the focused row",
    "Note and folder actions",
    "Settings (vaults, theme, autosave)",
];

#[test]
fn help_keeps_every_shortcut_and_the_footer_actions() {
    let (_tmp, rt) = help_at(1180.0, 780.0, true);
    let texts: Vec<String> = rt
        .elements()
        .into_iter()
        .filter_map(|element| element.text)
        .collect();
    for action in ACTIONS {
        assert!(
            texts.iter().any(|text| text == action),
            "missing shortcut {action}"
        );
    }
    assert_eq!(text_of(&rt, "help-key-0-0-0-label"), "⌘");
    assert_eq!(text_of(&rt, "help-key-0-0-1-label"), "K");
    assert_eq!(text_of(&rt, "help-key-0-0-2-label"), "Ctrl");
    assert_eq!(text_of(&rt, "help-key-0-0-3-label"), "K");
    assert_eq!(text_of(&rt, "help-key-2-4-0-label"), "+");
    assert_eq!(text_of(&rt, "help-key-2-4-1-label"), "-");
    assert_eq!(text_of(&rt, "help-note-0-1"), "(search)");
    assert!(
        texts
            .iter()
            .any(|text| text.contains("Autosave writes") && text.contains("0.55")),
        "autosave timing"
    );
    assert!(texts.iter().any(|text| text.starts_with("Version ")));
    assert!(rt.find("help-close").is_some());
    assert!(rt.find("help-copy-diagnostics").is_some());
    assert!(rt.find("help-report-issue").is_some());
    assert!(rt.find("help-empty-trash").is_some());
    assert!(rt.find("help-about").is_some());
    assert!(rt.find("help-diagnostics").is_some());
    assert!(rt.find("help-trash").is_some());

    let (x, y) = rt.find("help-close").expect("close");
    let mut rt = rt;
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.help, "Close dismisses the page");
    assert!(rt.find("help-panel").is_none());
}

#[test]
fn sections_share_rows_on_a_wide_window_and_stack_when_narrow() {
    let (_tmp, wide) = help_at(1600.0, 900.0, true);
    let nav = rect(&wide, "help-section-navigation");
    let edit = rect(&wide, "help-section-edit");
    let graph = rect(&wide, "help-section-graph");
    let sidebar = rect(&wide, "help-section-sidebar");
    assert!(
        (nav[1] - edit[1]).abs() < 2.0 && edit[0] > nav[0] + 40.0,
        "edit should sit beside navigation: nav {nav:?} edit {edit:?}"
    );
    assert!(
        (nav[1] - graph[1]).abs() < 2.0 && graph[0] > edit[0] + 40.0,
        "graph should sit on the same row: {graph:?}"
    );
    assert!(
        (nav[1] - sidebar[1]).abs() < 2.0 && sidebar[0] > graph[0] + 40.0,
        "sidebar should be the fourth column: {sidebar:?}"
    );
    assert!(
        nav[2] > 200.0 && (nav[2] - edit[2]).abs() < 2.0,
        "columns share the width: {nav:?} {edit:?}"
    );

    let (_tmp, mid) = help_at(1180.0, 780.0, true);
    let nav = rect(&mid, "help-section-navigation");
    let edit = rect(&mid, "help-section-edit");
    let graph = rect(&mid, "help-section-graph");
    assert!(
        (nav[1] - edit[1]).abs() < 2.0 && edit[0] > nav[0] + 40.0,
        "default window is two columns: nav {nav:?} edit {edit:?}"
    );
    assert!(
        graph[1] > nav[1] + nav[3] - 4.0 && (graph[0] - nav[0]).abs() < 2.0,
        "graph wraps under navigation: {graph:?} nav {nav:?}"
    );

    let (_tmp, narrow) = help_at(840.0, 640.0, true);
    let nav = rect(&narrow, "help-section-navigation");
    let edit = rect(&narrow, "help-section-edit");
    assert!(
        (nav[0] - edit[0]).abs() < 2.0 && edit[1] > nav[1] + 24.0,
        "narrow window is one column: nav {nav:?} edit {edit:?}"
    );
    let page = rect(&narrow, "help-panel");
    assert!(
        nav[2] > page[2] * 0.7,
        "the single column uses the page, not a fixed 720pt strip: section {nav:?} page {page:?}"
    );
}

#[test]
fn help_content_stays_inside_the_page_in_both_themes() {
    for (width, dark) in [
        (840.0, true),
        (1180.0, true),
        (1600.0, true),
        (1180.0, false),
    ] {
        let (_tmp, rt) = help_at(width, 720.0, dark);
        let page_right = rt.viewport.0 + 1.0;
        for element in rt.elements() {
            let key = element.key.as_deref().unwrap_or("");
            let ours = key.starts_with("help-") && key != "help-open";
            if !ours {
                continue;
            }
            let right = element.rect[0] + element.rect[2];
            assert!(
                element.rect[0] >= -1.0 && right <= page_right,
                "{key} overflows a {width}pt window: {:?}",
                element.rect
            );
        }
        let cap = rt
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some("help-key-0-0-1"))
            .expect("keycap");
        let label = rt
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some("help-key-0-0-1-label"))
            .expect("keycap label");
        let section = rt
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some("help-section-navigation"))
            .expect("section");
        assert_ne!(
            cap.fill,
            section.fill,
            "a keycap reads as its own surface on {}",
            if dark { "dark" } else { "light" }
        );
        let ink = label.text_color.expect("keycap ink");
        if dark {
            assert!(ink.r > 0.85, "dark key label stays light: {ink:?}");
        } else {
            assert!(ink.r < 0.25, "light key label stays dark: {ink:?}");
        }
    }
}

#[test]
fn a_long_log_line_wraps_inside_the_diagnostics_block() {
    let (_tmp, mut rt) = help_at(1180.0, 780.0, true);
    let path = rt.state.session_log_path().to_path_buf();
    let marker = format!("WRAPMARKER{}", "x".repeat(420));
    fs::write(&path, format!("{marker}\n")).expect("log");
    rt.update(|s, _| s.open_help());
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    let elements = rt.elements();
    let line = elements
        .iter()
        .find(|element| {
            element
                .text
                .as_deref()
                .is_some_and(|text| text.contains("WRAPMARKER"))
        })
        .expect("log line");
    let block = elements
        .iter()
        .find(|element| element.key.as_deref() == Some("help-diagnostics"))
        .expect("diagnostics");
    assert!(
        line.rect[2] <= block.rect[2] - 16.0,
        "log line {}pt is wider than the {}pt diagnostics card",
        line.rect[2],
        block.rect[2]
    );
    assert!(
        line.rect[3] > 24.0,
        "a 420-character token should wrap onto more than one line, height {}",
        line.rect[3]
    );
    let right = line.rect[0] + line.rect[2];
    assert!(
        right <= rt.viewport.0 + 1.0,
        "wrapped line leaves the window"
    );
}

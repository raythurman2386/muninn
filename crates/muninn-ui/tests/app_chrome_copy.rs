//! The app shell draws its chrome from the catalog.
//!
//! This is an extraction guard, not a translation test: the sidebar, the mode
//! switcher, the welcome banner, the no-note page and the Settings page still
//! render exactly the English they did before the move, looked up by [`Key`].

use std::path::PathBuf;

use muninn_core::testing::TempVault;
use muninn_i18n::{
    Key, link_impact, note_links, results, sidebar_summary, t, theme_label, unsaved_saves,
};
use muninn_ui::{Mode, State, poll, view};
use rune::{Env, Runtime};

fn runtime(root: PathBuf, state_home: PathBuf) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::with_state_home(root, state_home),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.frame(0.0);
    rt
}

fn drawn(rt: &Runtime<State>, expected: &str) -> bool {
    rt.elements()
        .iter()
        .filter(|element| !element.exiting)
        .any(|element| {
            element.text.as_deref() == Some(expected) || element.label.as_deref() == Some(expected)
        })
}

fn labelled(rt: &Runtime<State>, expected: &str) -> bool {
    rt.elements()
        .iter()
        .filter(|element| !element.exiting)
        .any(|element| element.label.as_deref() == Some(expected))
}

#[test]
fn sidebar_and_mode_switch_draw_catalog_copy() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let rt = runtime(tmp.path().to_path_buf(), tmp.path().with_extension("state"));

    assert_eq!(t(Key::CommonNewNote), "New note");
    assert!(
        drawn(&rt, t(Key::CommonNewNote)),
        "the sidebar draws New note"
    );
    assert!(
        drawn(&rt, t(Key::ModeWrite)),
        "the mode switcher draws Write"
    );
    assert_eq!(t(Key::ModeWrite), "Write");
    assert!(
        drawn(&rt, t(Key::ModeKanban)),
        "the mode switcher draws Kanban"
    );
    assert!(
        labelled(&rt, t(Key::SidebarSearch)),
        "the sidebar search is named from the catalog"
    );
}

#[test]
fn no_note_page_draws_catalog_empty_state() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut rt = runtime(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    assert_eq!(rt.state.mode, Mode::Editor);
    rt.update(|s, _| s.selected = None);
    rt.frame(1.0 / 60.0);

    assert_eq!(t(Key::TopbarNoNoteSelected), "No note selected");
    assert!(drawn(&rt, t(Key::TopbarNoNoteSelected)));
    assert!(drawn(&rt, t(Key::PreviewStartWriting)));
    assert_eq!(t(Key::PreviewStartWriting), "Make a note to start writing.");
}

#[test]
fn first_run_banner_draws_catalog_copy() {
    let tmp = TempVault::new(&[]);
    let root = tmp.path().join("fresh-vault");
    let rt = runtime(root.clone(), tmp.path().join("state"));
    assert!(rt.state.welcome, "a created vault arms the banner");

    assert!(drawn(&rt, t(Key::WelcomeGraphPill)));
    assert_eq!(t(Key::WelcomeGraphPill), "[[a note]] → graph");
    assert!(
        labelled(&rt, t(Key::WelcomeGraphLabel)),
        "the graph hint is named from the catalog"
    );
    assert!(
        drawn(
            &rt,
            &muninn_i18n::welcome_vault_created(&root.to_string_lossy())
        ),
        "the banner names the new vault path"
    );
}

#[test]
fn settings_page_draws_catalog_headings() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut rt = runtime(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    rt.update(|s, _| s.open_settings());
    for _ in 0..6 {
        rt.frame(1.0 / 60.0);
    }

    assert_eq!(t(Key::SettingsTitle), "Settings");
    assert!(drawn(&rt, t(Key::SettingsTitle)));
    assert!(drawn(&rt, t(Key::SettingsChangesSave)));
    assert_eq!(t(Key::SettingsChangesSave), "Changes save automatically.");
    for tab in [
        Key::SettingsTabVaults,
        Key::SettingsTabEditor,
        Key::SettingsTabAppearance,
        Key::SettingsTabGraph,
        Key::SettingsTabSync,
        Key::SettingsTabKanban,
    ] {
        assert!(drawn(&rt, t(tab)), "missing Settings tab {}", tab.id());
    }
}

#[test]
fn new_helpers_keep_their_english() {
    assert_eq!(
        theme_label("Dark"),
        "Theme: Dark — click to cycle (System, Dark, Light)"
    );
    assert_eq!(note_links(1, 2), "1 link · 2 backlinks");
    assert_eq!(sidebar_summary(12, 3), "12 notes · 3 links");
    assert_eq!(link_impact(2, t(Key::LinkImpactHere)), "2 notes link here");
    assert_eq!(results(1), "1 result");
    assert_eq!(unsaved_saves("⌘S"), "Unsaved · ⌘S saves");
    assert_eq!(t(Key::FocusEnter), "Focus mode");
    assert_eq!(t(Key::PreviewFind), "Find in note");
}

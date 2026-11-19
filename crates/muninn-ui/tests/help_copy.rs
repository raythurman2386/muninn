//! The Help page reads its copy from the catalog.
//!
//! This is an extraction guard, not a translation test: every group title and
//! shortcut description still renders exactly the English it did before the
//! move, and the accessible group labels match the visible titles.

use muninn_core::testing::TempVault;
use muninn_i18n::Key;
use muninn_ui::{State, ThemeMode, on_key, poll, view};
use rune::{Env, Runtime};

fn help() -> (TempVault, Runtime<State>) {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut state =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    state.theme = ThemeMode::Dark;
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 1.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt.update(|s, _| s.open_help());
    for _ in 0..6 {
        rt.frame(1.0 / 60.0);
    }
    (tmp, rt)
}

const GROUPS: &[(&str, Key)] = &[
    ("help-section-navigation", Key::HelpGroupNavigation),
    ("help-section-edit", Key::HelpGroupEdit),
    ("help-section-kanban", Key::HelpGroupKanban),
    ("help-section-graph", Key::HelpGroupGraph),
    ("help-section-board", Key::HelpGroupBoard),
    ("help-section-sidebar", Key::HelpGroupSidebar),
];

#[test]
fn the_six_group_titles_render_as_before() {
    let (_tmp, rt) = help();
    let elements = rt.elements();
    for (key, title) in GROUPS {
        let expected = muninn_i18n::t(*title);
        let section = elements
            .iter()
            .find(|element| element.key.as_deref() == Some(*key))
            .unwrap_or_else(|| panic!("missing section {key}"));
        assert_eq!(
            section.label.as_deref(),
            Some(expected),
            "{key} does not use its catalog title as the group label"
        );
        assert!(
            elements
                .iter()
                .filter_map(|element| element.text.as_deref())
                .any(|text| text == expected),
            "missing group title {expected:?}"
        );
    }
}

#[test]
fn the_locked_notes_card_renders_its_catalog_copy() {
    let (_tmp, rt) = help();
    let elements = rt.elements();
    assert!(
        elements
            .iter()
            .any(|element| element.key.as_deref() == Some("help-locked-notes")),
        "the locked-notes card is drawn"
    );
    let texts: Vec<&str> = elements
        .iter()
        .filter_map(|element| element.text.as_deref())
        .collect();
    for key in [
        Key::HelpGroupLock,
        Key::HelpLockLock,
        Key::HelpLockUnlock,
        Key::HelpLockLost,
        Key::HelpLockRecovery,
        Key::HelpLockRename,
    ] {
        let expected = muninn_i18n::t(key);
        assert!(
            texts.contains(&expected),
            "missing locked-notes copy {expected:?}"
        );
    }
}

#[test]
fn a_shortcut_description_renders_from_the_catalog() {
    let (_tmp, rt) = help();
    let action = muninn_i18n::t(Key::HelpNavFocusMode);
    assert_eq!(action, "Focus mode: hide the chrome and centre the caret");
    assert!(
        rt.elements()
            .iter()
            .filter_map(|element| element.text.as_deref())
            .any(|text| text == action),
        "missing shortcut description {action:?}"
    );
}

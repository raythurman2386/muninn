//! #72: every text field carries an accessible name.
//!
//! A real AT-SPI pass (the same stack Orca consumes) found the sidebar search,
//! the graph node search, the note editor, the vault-path field and the five
//! Sync fields exposed as focusable entries with no name — a screen reader
//! announced only "text field". These tests pin the names so that cannot
//! regress; the palette, Help, Board toolbar/canvas and Kanban pages already
//! passed the pass.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Settings, SettingsTab, State, poll, view};
use rune::{ElementInfo, Env, Runtime};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-a11y-names-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn element(rt: &Runtime<State>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("missing element {key}"))
}

fn label(rt: &Runtime<State>, key: &str) -> String {
    element(rt, key)
        .label
        .unwrap_or_else(|| panic!("element {key} has no accessible name"))
}

#[test]
fn text_fields_the_at_pass_found_unnamed_are_named() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let (config, state_home) = (unique_dir("config"), unique_dir("state"));
    let mut settings = Settings::load_from(&config);
    settings.last_vault = Some(vault.path().to_path_buf());
    let state = State::with_settings(
        vault.path().to_path_buf(),
        state_home.clone(),
        config.clone(),
        settings,
    );
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.frame(0.0);

    // Sidebar search: the wrapper button and the field both name the action.
    assert_eq!(label(&rt, "search-box"), "Search notes");
    assert_eq!(label(&rt, "search"), "Search notes");

    // The note editor, with a note open.
    let id = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.update(|s, _| s.load_note(id, false));
    rt.frame(0.0);
    assert_eq!(label(&rt, "editor"), "Note editor");

    // Graph node search.
    rt.update(|s, _| s.set_mode(Mode::Graph));
    rt.frame(0.0);
    assert_eq!(label(&rt, "graph-search"), "Find graph node");

    // Find-in-note.
    rt.update(|s, _| s.open_find());
    rt.frame(0.0);
    assert_eq!(label(&rt, "find"), "Find in note");

    // Settings: the vault folder path.
    rt.update(|s, _| {
        s.close_find();
        s.open_settings();
        s.set_settings_tab(SettingsTab::Vaults);
    });
    rt.frame(0.0);
    assert_eq!(label(&rt, "vault-path"), "Vault folder path");

    // Settings: every Sync field reuses its visible label as the AT name.
    rt.update(|s, _| {
        s.set_settings_tab(SettingsTab::Sync);
        s.set_sync_provider(0);
    });
    rt.frame(0.0);
    for (key, name) in [
        ("sync-field-github-owner", "GitHub owner"),
        ("sync-field-repository", "Repository"),
        ("sync-field-branch", "Branch"),
        ("sync-field-prefix", "Prefix"),
    ] {
        assert_eq!(label(&rt, key), name, "{key}");
    }

    let _ = fs::remove_dir_all(&state_home);
    let _ = fs::remove_dir_all(&config);
}

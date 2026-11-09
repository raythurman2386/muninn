//! M3 DES-01/LIF-01: settings persistence, session restore, vault switching,
//! the recent-vault MRU, first-run welcome and the Help overlay.
//!
//! Settings tests use explicit config/state roots so the user's real
//! `~/.config/muninn/settings.json` is never touched.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, Scope, Settings, State, ThemeMode, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-lif-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn vault_fixture() -> TempVault {
    // A four-note fixture (and an explicit `to_vec`) keeps this helper distinct
    // from the other suites' three-note `temp_vault` builders.
    let notes = [
        ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
        ("Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
        ("folder/Gamma.md", "# Gamma\n\nNested.\n"),
        ("Later.md", "# Later\n\nA later note.\n"),
    ];
    let notes = notes.to_vec();
    TempVault::new(&notes)
}

fn state_at(root: PathBuf, config: &Path, state_home: &Path) -> State {
    State::with_settings(
        root,
        state_home.to_path_buf(),
        config.to_path_buf(),
        Settings::load_from(config),
    )
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

fn escape() -> KeyEvent {
    KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    }
}

#[test]
fn settings_round_trip_restores_theme_collapsed_note_and_mode() {
    let vault = vault_fixture();
    let config = unique_dir("restore-config");
    let state_home = unique_dir("restore-state");

    let settings = Settings {
        theme: ThemeMode::Dark,
        collapsed: vec!["folder".to_owned()],
        last_vault: Some(vault.path().to_path_buf()),
        selected_rel: Some("folder/Gamma.md".to_owned()),
        mode: Mode::Preview,
        scope: Scope::Local,
        ..Settings::default()
    };
    let state = State::with_settings(
        vault.path().to_path_buf(),
        state_home.clone(),
        config.clone(),
        settings,
    );

    assert_eq!(state.theme, ThemeMode::Dark, "theme restored");
    assert!(state.is_collapsed("folder"), "collapsed folder restored");
    let selected = state.selected.expect("a note is open");
    assert_eq!(state.vault.notes[selected].rel, "folder/Gamma.md");
    assert_eq!(state.mode, Mode::Preview);
    assert_eq!(state.scope, Scope::Local);

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
}

#[test]
fn toggles_and_note_opens_are_persisted_immediately() {
    let vault = vault_fixture();
    let config = unique_dir("persist-config");
    let state_home = unique_dir("persist-state");
    let mut state = state_at(vault.path().to_path_buf(), &config, &state_home);

    state.set_theme(ThemeMode::Dark);
    state.toggle_folder("folder");
    let beta = state.vault.resolve("Beta").expect("Beta");
    state.load_note(beta, true);
    state.set_mode(Mode::Graph);
    state.set_scope(Scope::Local);

    // Read the file back without a second State so the assertion covers the
    // write path, not just in-memory fields.
    let saved = Settings::load_from(&config);
    assert_eq!(saved.theme, ThemeMode::Dark);
    assert!(saved.collapsed.iter().any(|folder| folder == "folder"));
    assert_eq!(saved.selected_rel.as_deref(), Some("Beta.md"));
    assert_eq!(saved.mode, Mode::Graph);
    assert_eq!(saved.scope, Scope::Local);
    assert_eq!(
        saved.last_vault.as_deref(),
        Some(vault.path()),
        "opening a note keeps the vault in settings"
    );

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
}

#[test]
fn corrupt_settings_fall_back_to_defaults_without_failing_startup() {
    let vault = vault_fixture();
    let config = unique_dir("corrupt-config");
    let state_home = unique_dir("corrupt-state");
    let path = config.join("muninn").join("settings.json");
    fs::create_dir_all(path.parent().expect("parent")).expect("settings dir");
    fs::write(&path, "{ broken json").expect("garbage");

    assert_eq!(Settings::load_from(&config), Settings::default());
    let state = state_at(vault.path().to_path_buf(), &config, &state_home);
    assert_eq!(
        state.theme,
        ThemeMode::Omarchy,
        "omarchy is the default theme source"
    );
    assert!(!state.is_collapsed("folder"));
    assert!(state.selected.is_some(), "the vault still opens");
    assert!(state.startup_error.is_none());

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
}

#[test]
fn vaults_switch_by_path_and_recent_vaults_stay_mru() {
    let first = vault_fixture();
    let second = unique_dir("second-vault");
    fs::write(second.join("Second.md"), "second vault").expect("note");
    let config = unique_dir("vault-config");
    let state_home = unique_dir("vault-state");
    let mut state = state_at(first.path().to_path_buf(), &config, &state_home);

    assert!(state.open_vault(&second.to_string_lossy()), "open by path");
    assert_eq!(state.vault.root, second);
    assert_eq!(state.recent_vaults().first(), Some(&second));

    assert!(state.open_vault(&first.path().to_string_lossy()));
    assert_eq!(state.recent_vaults()[0], first.path().to_path_buf());
    assert_eq!(state.recent_vaults()[1], second);

    // Reopening an older vault promotes it without duplicating the entry.
    assert!(state.open_recent_vault(1));
    assert_eq!(state.vault.root, second);
    assert_eq!(state.recent_vaults()[0], second);
    assert_eq!(
        state
            .recent_vaults()
            .iter()
            .filter(|path| path.as_path() == second.as_path())
            .count(),
        1
    );

    // The Settings path field's Open action uses the same switch path.
    state.begin_vault_prompt();
    state
        .vault_field
        .replace_text(first.path().to_string_lossy().as_ref());
    state.submit_vault_path();
    assert_eq!(state.vault.root, first.path());
    assert!(!state.vault_field.focused);

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
}

#[test]
fn a_missing_vault_is_created_and_shows_the_first_run_welcome() {
    let config = unique_dir("welcome-config");
    let state_home = unique_dir("welcome-state");
    let root = state_home.join("fresh-vault");
    assert!(!root.exists());

    let state = State::with_settings(
        root.clone(),
        state_home.clone(),
        config.clone(),
        Settings::default(),
    );
    assert!(root.is_dir(), "Vault::load creates a missing root");
    assert!(state.vault.notes.is_empty());
    assert!(state.welcome, "first-run banner is armed");
    assert!(state.startup_error.is_none());

    // Creating a vault from the menu also accepts a folder that does not exist.
    let mut state = state;
    let created = state_home.join("menu-vault");
    assert!(state.create_vault(&created.to_string_lossy()));
    assert!(created.is_dir());
    assert_eq!(state.vault.root, created);

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
}

#[test]
fn settings_vaults_section_opens_and_creates_vaults() {
    let vault = vault_fixture();
    let config = unique_dir("chip-config");
    let state_home = unique_dir("chip-state");
    let settings = Settings {
        last_vault: Some(vault.path().to_path_buf()),
        ..Settings::default()
    };
    let mut rt = Runtime::new(
        State::with_settings(
            vault.path().to_path_buf(),
            state_home.clone(),
            config.clone(),
            settings,
        ),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);

    // The sidebar no longer carries a vault chip: Settings is the vault surface.
    assert!(
        rt.find("vault-chip").is_none(),
        "the sidebar vault chip row is gone"
    );

    rt.key(command(','));
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.state.settings_modal);
    assert!(
        rt.find("settings-dialog").is_some(),
        "the dialog is mounted"
    );
    assert!(
        rt.find("vault-prompt").is_some(),
        "the typed-path card lives in the Vaults section"
    );
    assert!(rt.find("vault-path").is_some());
    for key in ["vault-open", "vault-create", "vault-cancel"] {
        assert!(rt.find(key).is_some(), "{key} lives in Settings");
    }
    assert!(
        rt.find("settings-recents").is_some(),
        "Settings is the single place to manage recent vaults"
    );

    // Open vault by typed path: submit closes Settings and switches vaults.
    let opened = unique_dir("settings-open-vault");
    rt.update(|s, _| {
        s.vault_field
            .replace_text(opened.to_string_lossy().as_ref())
    });
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    let (x, y) = rt.find("vault-open").expect("Open vault");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    assert!(
        !rt.state.settings_modal,
        "a successful open closes Settings"
    );
    assert_eq!(rt.state.vault.root, opened);

    // New vault creates a missing folder and closes Settings the same way.
    rt.key(command(','));
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
    let created = state_home.join("settings-new-vault");
    rt.update(|s, _| {
        s.vault_field
            .replace_text(created.to_string_lossy().as_ref())
    });
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    let (x, y) = rt.find("vault-create").expect("New vault");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    assert!(!rt.state.settings_modal);
    assert!(created.is_dir(), "New vault creates the missing folder");
    assert_eq!(rt.state.vault.root, created);

    // Esc from the focused path field closes Settings instead of only blurring.
    rt.key(command(','));
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
    let (x, y) = rt.find("vault-path").expect("vault path field");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.state.vault_field.focused, "the path field owns typing");
    rt.key(escape());
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    assert!(!rt.state.settings_modal, "Escape closes Settings");
    assert!(
        !rt.state.vault_field.focused,
        "the hidden field drops focus"
    );

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
    let _ = fs::remove_dir_all(&opened);
}

#[test]
fn help_overlay_opens_with_command_question_and_escape_closes() {
    let vault = vault_fixture();
    let state_home = unique_dir("help-state");
    let mut rt = Runtime::new(
        State::with_state_home(vault.path().to_path_buf(), state_home.clone()),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);

    rt.key(command('?'));
    rt.frame(1.0 / 60.0);
    assert!(rt.state.help, "Cmd+? opens Help");
    assert!(rt.find("help-panel").is_some(), "the overlay is mounted");
    assert!(rt.find("help-report-issue").is_some());

    rt.key(escape());
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.help, "Escape closes Help");
    assert!(rt.find("help-panel").is_none());

    let _ = fs::remove_dir_all(&state_home);
}

#[test]
fn settings_modal_opens_by_shortcut_changes_persist_and_returns_focus() {
    let vault = vault_fixture();
    let config = unique_dir("modal-config");
    let state_home = unique_dir("modal-state");
    let settings = Settings {
        theme: ThemeMode::Dark,
        last_vault: Some(vault.path().to_path_buf()),
        ..Settings::default()
    };
    let mut rt = Runtime::new(
        State::with_settings(
            vault.path().to_path_buf(),
            state_home.clone(),
            config.clone(),
            settings,
        ),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);

    // Typing focus is the invoker: the modal must take it and give it back.
    rt.state.focus_editor();
    rt.frame(1.0 / 60.0);
    assert!(rt.state.editor.focused);

    rt.key(command(','));
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.state.settings_modal, "Ctrl/Cmd+, opens Settings");
    assert!(
        rt.find("settings-dialog").is_some(),
        "the dialog is mounted"
    );
    assert!(
        !rt.state.editor.focused,
        "the overlay owns the keyboard while open"
    );

    // Other app shortcuts must not reach the page behind the scrim.
    let notes = rt.state.vault.notes.len();
    rt.key(command('n'));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.vault.notes.len(), notes);
    assert!(rt.state.new_note_prompt.is_none());

    // Theme lives on the Appearance tab (the third tab); it is a segmented
    // control whose fourth segment is Light.
    let (x, y) = rt.find("settings-tabs/segment2").expect("Appearance tab");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    let (x, y) = rt.find("settings-theme/segment3").expect("Light segment");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.theme, ThemeMode::Light);

    // Autosave steps by 0.05 s and is clamped by the setter. It lives on the
    // Editor tab (the second tab).
    let (x, y) = rt.find("settings-tabs/segment1").expect("Editor tab");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    let (x, y) = rt.find("settings-autosave-plus").expect("autosave +");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);

    let saved = Settings::load_from(&config);
    assert_eq!(saved.theme, ThemeMode::Light, "theme persisted immediately");
    assert!(
        (saved.autosave_delay - 0.60).abs() < 1e-6,
        "autosave delay persisted: {}",
        saved.autosave_delay
    );

    rt.key(escape());
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    assert!(!rt.state.settings_modal, "Escape closes Settings");
    assert!(rt.find("settings-dialog").is_none());
    assert!(
        rt.state.editor.focused,
        "Escape returns typing focus to the editor"
    );

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
}

#[test]
fn vault_prompt_text_stays_inside_the_dialog() {
    // The hint (and any vault error) now lives in the Settings Vaults card, so
    // an unwrapped line would paint past the card border and the dialog.
    for (width, height) in [(1180.0, 780.0), (840.0, 640.0)] {
        let vault = vault_fixture();
        let config = unique_dir("prompt-config");
        let state_home = unique_dir("prompt-state");
        let settings = Settings {
            last_vault: Some(vault.path().to_path_buf()),
            ..Settings::default()
        };
        let mut rt = Runtime::new(
            State::with_settings(
                vault.path().to_path_buf(),
                state_home.clone(),
                config.clone(),
                settings,
            ),
            view,
            (width, height),
            Env::new(true, 2.0),
        );
        rt.poll = Some(poll);
        rt.on_key = Some(on_key);
        rt.frame(0.0);
        rt.update(|s, _| s.open_settings());
        for _ in 0..4 {
            rt.frame(1.0 / 60.0);
        }
        assert!(rt.state.settings_modal);

        let card = rt.presented("vault-prompt").expect("prompt card").0;
        let card_right = card[0] + card[2];
        let elements = rt.elements();
        let hint = elements
            .iter()
            .find(|element| {
                element
                    .text
                    .as_deref()
                    .is_some_and(|text| text.starts_with("Paste a folder path"))
            })
            .expect("hint line");
        let hint_right = hint.rect[0] + hint.rect[2];
        assert!(
            hint_right <= card_right + 1.0,
            "hint overflows the card at {width}pt window: card {card:?} hint {:?}",
            hint.rect
        );
        for key in ["vault-open", "vault-create", "vault-cancel"] {
            let rect = rt.presented(key).expect(key).0;
            assert!(
                rect[0] + rect[2] <= card_right + 1.0,
                "{key} overflows the card at {width}pt window: card {card:?} rect {rect:?}"
            );
        }

        // Vault errors (path plus OS reason) can be longer than the card (540 pt
        // since the tabbed layout), so the
        // error line must wrap too instead of painting past the border.
        rt.update(|s, _| {
            s.error = Some(
                "Could not create the vault: /very/long/path/without/spaces/to/a/deeply/nested/notes denied (os error 13)"
                    .to_owned(),
            );
        });
        for _ in 0..4 {
            rt.frame(1.0 / 60.0);
        }
        let elements = rt.elements();
        let error = elements
            .iter()
            .find(|element| element.key.as_deref() == Some("vault-prompt-error"))
            .expect("error line");
        assert!(
            error.rect[0] + error.rect[2] <= card_right + 1.0,
            "error overflows the card at {width}pt window: card {card:?} error {:?}",
            error.rect
        );
        assert!(
            error.rect[3] > 24.0,
            "error is one unwrapped line at {width}pt window: error {:?}",
            error.rect
        );

        let _ = fs::remove_dir_all(&config);
        let _ = fs::remove_dir_all(&state_home);
    }
}

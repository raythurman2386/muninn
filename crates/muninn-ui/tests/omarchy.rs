//! Omarchy theming: palette following, live switch via `poll`, and the theme
//! button's cycle. Fixture palettes live in temp dirs; the user's real
//! `~/.local/state/omarchy/current/theme/colors.toml` is never read.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Settings, State, ThemeMode, on_key, poll, view};
use rune::{Env, Runtime};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-omarchy-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn vault_fixture() -> TempVault {
    let notes = [
        ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
        ("Beta.md", "# Beta\n"),
    ]
    .to_vec();
    TempVault::new(&notes)
}

fn runtime(state: State) -> Runtime<State> {
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt
}

const DARK_PALETTE: &str = "mode = \"dark\"\naccent = \"#34d399\"\nbackground = \"#141414\"\nforeground = \"#e1e1e1\"\nmuted = \"#6c6c6c\"\nred = \"#f7768e\"\n";
const LIGHT_PALETTE: &str = "mode = \"light\"\naccent = \"#064e3b\"\nbackground = \"#f5f4ed\"\nforeground = \"#3f4a45\"\nlight_foreground = \"#5c6658\"\nmuted = \"#7a8478\"\nred = \"#c92a2a\"\n";

#[test]
fn theme_switch_in_the_palette_file_restarts_the_view_without_a_restart() {
    let vault = vault_fixture();
    let config = unique_dir("switch-config");
    let state_home = unique_dir("switch-state");
    let palette_dir = unique_dir("switch-palette");
    let palette = palette_dir.join("colors.toml");
    fs::write(&palette, DARK_PALETTE).expect("palette");

    let settings = Settings {
        theme: ThemeMode::Omarchy,
        ..Settings::default()
    };
    let mut state = State::with_settings(
        vault.path().to_path_buf(),
        state_home.clone(),
        config.clone(),
        settings,
    );
    state.set_omarchy_path(palette.clone());

    let mut rt = runtime(state);
    rt.frame(0.0);
    let first_rev = rt.state.omarchy_rev;
    assert_ne!(first_rev, 0, "dark palette picked up");
    assert!(
        rt.find("theme").is_none(),
        "following Omarchy hides the switcher"
    );

    fs::write(&palette, LIGHT_PALETTE).expect("switch palette");
    rt.frame(1.0 / 60.0);
    assert_ne!(
        rt.state.omarchy_rev, first_rev,
        "live palette change visible"
    );

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
    let _ = fs::remove_dir_all(&palette_dir);
}

#[test]
fn missing_palette_file_falls_back_without_a_revision() {
    let vault = vault_fixture();
    let config = unique_dir("missing-config");
    let state_home = unique_dir("missing-state");
    let palette_dir = unique_dir("missing-palette");

    let settings = Settings {
        theme: ThemeMode::Omarchy,
        ..Settings::default()
    };
    let mut state = State::with_settings(
        vault.path().to_path_buf(),
        state_home.clone(),
        config.clone(),
        settings,
    );
    // No palette file anywhere under this path: the OpenCode fallback renders.
    state.set_omarchy_path(palette_dir.join("colors.toml"));

    let mut rt = runtime(state);
    rt.frame(0.0);
    assert_eq!(rt.state.omarchy_rev, 0, "no palette, no revision");
    assert!(rt.find("theme").is_some(), "fallback kit still mounts");

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
    let _ = fs::remove_dir_all(&palette_dir);
}

#[test]
fn disabled_theme_never_watches_the_palette_file() {
    let vault = vault_fixture();
    let config = unique_dir("disabled-config");
    let state_home = unique_dir("disabled-state");
    let palette_dir = unique_dir("disabled-palette");
    let palette = palette_dir.join("colors.toml");
    fs::write(&palette, DARK_PALETTE).expect("palette");

    let settings = Settings {
        theme: ThemeMode::Omarchy,
        ..Settings::default()
    };
    let mut state = State::with_settings(
        vault.path().to_path_buf(),
        state_home.clone(),
        config.clone(),
        settings,
    );
    state.set_omarchy_path(palette.clone());
    state.disable_omarchy_theme();

    let mut rt = runtime(state);
    rt.frame(0.0);
    assert_eq!(rt.state.omarchy_rev, 0, "disabled lookup reads nothing");

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
    let _ = fs::remove_dir_all(&palette_dir);
}

#[test]
fn theme_button_cycles_system_dark_light_and_persists_it() {
    let vault = vault_fixture();
    let config = unique_dir("cycle-config");
    let state_home = unique_dir("cycle-state");
    let palette_dir = unique_dir("cycle-palette");

    // Start diverged from Omarchy (a missing palette file, a pinned source):
    // the switcher is visible and Omarchy is never a cycle stop.
    let settings = Settings {
        theme: ThemeMode::Follow,
        ..Settings::default()
    };
    let mut state = State::with_settings(
        vault.path().to_path_buf(),
        state_home.clone(),
        config.clone(),
        settings,
    );
    state.set_omarchy_path(palette_dir.join("colors.toml"));
    let mut rt = runtime(state);
    rt.frame(0.0);
    assert_eq!(rt.state.theme, ThemeMode::Follow);

    let (x, y) = rt.find("theme").expect("theme button");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.theme, ThemeMode::Dark, "System -> Dark");
    assert_eq!(
        Settings::load_from(&config).theme,
        ThemeMode::Dark,
        "persisted"
    );

    let (x, y) = rt.find("theme").expect("theme button");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.theme, ThemeMode::Light, "Dark -> Light");

    let (x, y) = rt.find("theme").expect("theme button");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.theme, ThemeMode::Follow, "Light -> System");

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
    let _ = fs::remove_dir_all(&palette_dir);
}

#[test]
fn pinned_theme_keeps_the_switcher_visible_with_a_palette_present() {
    let vault = vault_fixture();
    let config = unique_dir("pinned-config");
    let state_home = unique_dir("pinned-state");
    let palette_dir = unique_dir("pinned-palette");
    let palette = palette_dir.join("colors.toml");
    fs::write(&palette, DARK_PALETTE).expect("palette");

    let settings = Settings {
        theme: ThemeMode::Dark,
        ..Settings::default()
    };
    let mut state = State::with_settings(
        vault.path().to_path_buf(),
        state_home.clone(),
        config.clone(),
        settings,
    );
    state.set_omarchy_path(palette.clone());

    let mut rt = runtime(state);
    rt.frame(0.0);
    assert!(
        rt.find("theme").is_some(),
        "diverged from Omarchy, so the switcher stays"
    );

    let _ = fs::remove_dir_all(&config);
    let _ = fs::remove_dir_all(&state_home);
    let _ = fs::remove_dir_all(&palette_dir);
}

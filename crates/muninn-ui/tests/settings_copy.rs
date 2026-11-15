//! Settings extraction guard (#156): the Settings page and the Kanban
//! settings surface draw their copy from the `muninn-i18n` catalog.
//!
//! The catalog pins the English (`en_us.rs`); these tests pin that the drawn
//! page really reads it, so a string that quietly goes back to a literal is a
//! failure here. The exact English is asserted on both sides: the key resolves
//! to it, and the element carries it.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_i18n::{Key, t};
use muninn_ui::{Settings, State, on_key, poll, view};
use rune::{Env, Key as RuneKey, KeyEvent, Mods, Runtime};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-settings-copy-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn command(c: char) -> KeyEvent {
    KeyEvent {
        key: RuneKey::Command(c),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    }
}

/// The texts the frame draws, in draw order.
fn drawn(rt: &Runtime<State>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter_map(|element| element.text)
        .collect()
}

fn has_text(rt: &Runtime<State>, text: &str) -> bool {
    drawn(rt).iter().any(|drawn| drawn == text)
}

#[test]
fn the_settings_page_draws_the_catalog_vaults_copy() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let config = unique_dir("vaults-config");
    let state_home = unique_dir("vaults-state");
    let settings = Settings {
        last_vault: Some(vault.path().to_path_buf()),
        ..Settings::default()
    };
    let mut rt = Runtime::new(
        State::with_settings(
            vault.path().to_path_buf(),
            state_home.clone(),
            config,
            settings,
        ),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);

    rt.key(command(','));
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.state.settings_open, "Cmd+, opens Settings");

    // The card's heading and caption come from the catalog, not a literal.
    assert_eq!(t(Key::SettingsVaultsOpenTitle), "Open a vault");
    assert_eq!(
        t(Key::SettingsVaultsOpenCaption),
        "Paste a folder path, or create a new vault in a new folder."
    );
    assert!(
        has_text(&rt, "Open a vault"),
        "the Vaults card heading is drawn"
    );
    assert!(
        has_text(
            &rt,
            "Paste a folder path, or create a new vault in a new folder."
        ),
        "the Vaults card caption is drawn"
    );
    // A second card on the same tab, so the guard covers more than one key.
    assert_eq!(t(Key::SettingsVaultsRecentTitle), "Recent vaults");
    assert!(
        has_text(&rt, "Recent vaults"),
        "the recent-vaults card heading is drawn"
    );

    let _ = fs::remove_dir_all(&state_home);
}

#[test]
fn the_kanban_settings_tab_draws_the_catalog_schema_copy() {
    let vault = TempVault::new(&[
        (
            "Board.md",
            "---\nkanban-plugin: board\n---\n\n## To do\n\n- [ ] one\n",
        ),
        ("Alpha.md", "# Alpha\n"),
    ]);
    let config = unique_dir("kanban-config");
    let state_home = unique_dir("kanban-state");
    let settings = Settings {
        last_vault: Some(vault.path().to_path_buf()),
        ..Settings::default()
    };
    let mut rt = Runtime::new(
        State::with_settings(
            vault.path().to_path_buf(),
            state_home.clone(),
            config,
            settings,
        ),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);

    rt.key(command(','));
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
    // The Kanban tab is the sixth and last.
    let (x, y) = rt.find("settings-tabs/segment5").expect("Kanban tab");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }

    // The section heading and caption come from `KanbanSection`.
    assert_eq!(t(Key::KanbanSectionDatesTitle), "Dates and times");
    assert_eq!(
        t(Key::KanbanSectionDatesCaption),
        "How dates and times are typed, written and shown."
    );
    assert!(
        has_text(&rt, "Dates and times"),
        "the Dates section heading is drawn"
    );
    assert!(
        has_text(&rt, "How dates and times are typed, written and shown."),
        "the Dates section caption is drawn"
    );
    // One key's label and help come from `KanbanKey`.
    assert_eq!(t(Key::KanbanSettingDateTriggerLabel), "Date trigger");
    assert_eq!(
        t(Key::KanbanSettingDateTriggerHelp),
        "Typed before a date, as in @{2026-11-12}."
    );
    assert!(
        has_text(&rt, "Date trigger"),
        "the date-trigger label is drawn"
    );
    assert!(
        has_text(&rt, "Typed before a date, as in @{2026-11-12}."),
        "the date-trigger help is drawn"
    );
    // The tab's own caption is a UI row in the same catalog.
    assert!(
        has_text(
            &rt,
            "A board's own settings footer wins; otherwise these apply to every board."
        ),
        "the Kanban tab caption is drawn"
    );

    let _ = fs::remove_dir_all(&state_home);
}

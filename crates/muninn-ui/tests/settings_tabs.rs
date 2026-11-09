//! v0.6: the Settings modal is split into tabs (Vaults, Editor, Appearance,
//! Graph, Sync). Each tab mounts only its own section; the selected tab is
//! ephemeral and resets when Settings reopens.
//!
//! Tests use explicit config/state roots so the user's real
//! `~/.config/muninn/settings.json` is never touched.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{Settings, SettingsTab, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-tabs-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

struct Fixture {
    rt: Runtime<State>,
    config: PathBuf,
    state_home: PathBuf,
    _vault: TempVault,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.config);
        let _ = fs::remove_dir_all(&self.state_home);
    }
}

fn fixture(tag: &str) -> Fixture {
    let vault = TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
        ("Beta.md", "# Beta\n"),
    ]);
    let config = unique_dir(&format!("{tag}-config"));
    let state_home = unique_dir(&format!("{tag}-state"));
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
    Fixture {
        rt,
        config,
        state_home,
        _vault: vault,
    }
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn open_settings(rt: &mut Runtime<State>) {
    rt.key(KeyEvent {
        key: Key::Command(','),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    frames(rt, 8);
    assert!(rt.state.settings_modal, "Ctrl/Cmd+, opens Settings");
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is on screen"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 4);
}

/// Every tab's body key, in tab-bar order.
const BODIES: [&str; 5] = [
    "settings-tab-vaults",
    "settings-tab-editor",
    "settings-tab-appearance",
    "settings-tab-graph",
    "settings-tab-sync",
];

#[test]
fn tab_bar_lists_every_tab_in_order() {
    let labels: Vec<&str> = SettingsTab::ALL.iter().map(|tab| tab.label()).collect();
    assert_eq!(labels, ["Vaults", "Editor", "Appearance", "Graph", "Sync"]);
    for (index, tab) in SettingsTab::ALL.iter().enumerate() {
        assert_eq!(tab.index(), index);
    }
    assert_eq!(SettingsTab::default(), SettingsTab::Vaults);
}

#[test]
fn each_tab_mounts_only_its_own_section() {
    let mut f = fixture("mount");
    open_settings(&mut f.rt);
    assert!(
        f.rt.find("settings-tabs").is_some(),
        "the tab bar is mounted"
    );
    assert!(f.rt.find("settings-close").is_some());

    // Signature controls per tab, so a section that moves tabs fails loudly.
    let signatures: [&[&str]; 5] = [
        &["vault-prompt", "vault-path", "settings-recents"],
        &["settings-autosave-plus", "settings-autosave-value"],
        // The theme picker, or its "Following Omarchy" note on an Omarchy
        // desktop; checked separately below.
        &[],
        &["settings-reduce-motion", "settings-reduced-motion-status"],
        &["settings-sync-placeholder"],
    ];
    for (index, tab) in SettingsTab::ALL.iter().enumerate() {
        click(&mut f.rt, &format!("settings-tabs/segment{index}"));
        assert_eq!(
            f.rt.state.settings_tab, *tab,
            "segment{index} selects {tab:?}"
        );
        for (other, body) in BODIES.iter().enumerate() {
            assert_eq!(
                f.rt.find(body).is_some(),
                other == index,
                "{body} mounted on the {tab:?} tab"
            );
        }
        if *tab == SettingsTab::Appearance {
            assert!(
                f.rt.find("settings-theme").is_some() || f.rt.find("settings-theme-note").is_some(),
                "the theme control lives on the Appearance tab"
            );
        }
        for key in signatures[index] {
            assert!(f.rt.find(key).is_some(), "{key} lives on the {tab:?} tab");
        }
        assert!(
            f.rt.find("settings-close").is_some(),
            "Close stays visible on the {tab:?} tab"
        );
    }
}

#[test]
fn selected_tab_resets_on_reopen_and_is_not_persisted() {
    let mut f = fixture("reset");
    open_settings(&mut f.rt);
    assert_eq!(f.rt.state.settings_tab, SettingsTab::Vaults);
    click(&mut f.rt, "settings-tabs/segment4");
    assert_eq!(f.rt.state.settings_tab, SettingsTab::Sync);

    f.rt.update(|s, _| s.close_settings());
    frames(&mut f.rt, 4);
    open_settings(&mut f.rt);
    assert_eq!(
        f.rt.state.settings_tab,
        SettingsTab::Vaults,
        "Settings reopens on the first tab"
    );

    // The tab is session UI only: settings.json never carries it.
    f.rt.state.save_settings();
    let path = f.config.join("muninn").join("settings.json");
    let raw = fs::read_to_string(&path).expect("settings.json was written");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("settings.json is JSON");
    let keys: Vec<&String> = json.as_object().expect("a JSON object").keys().collect();
    assert!(!keys.is_empty(), "settings.json has fields: {raw}");
    for key in &keys {
        assert!(
            *key != "tab" && *key != "settings_tab",
            "settings.json does not persist the selected tab: {raw}"
        );
    }
}

#[test]
fn leaving_the_vaults_tab_drops_path_field_focus() {
    let mut f = fixture("focus");
    open_settings(&mut f.rt);
    click(&mut f.rt, "vault-path");
    assert!(f.rt.state.vault_field.focused, "the path field owns typing");

    click(&mut f.rt, "settings-tabs/segment1");
    assert_eq!(f.rt.state.settings_tab, SettingsTab::Editor);
    assert!(
        !f.rt.state.vault_field.focused,
        "a hidden path field must not keep typing focus"
    );
}

#[test]
fn escape_closes_settings_from_any_tab() {
    let mut f = fixture("escape");
    for index in 0..SettingsTab::ALL.len() {
        open_settings(&mut f.rt);
        click(&mut f.rt, &format!("settings-tabs/segment{index}"));
        f.rt.key(KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        });
        frames(&mut f.rt, 4);
        assert!(!f.rt.state.settings_modal, "Escape closes from tab {index}");
        assert!(f.rt.find("settings-dialog").is_none());
    }
}

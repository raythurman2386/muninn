//! Tray slice two (Settings › Appearance): Close to tray, Start Muninn at
//! login, the global show-Muninn shortcut and the tray's Sync now. Fake
//! ports stand in for the tray, the login item and the shortcut, so no
//! desktop session is needed. Explicit config/state roots keep the real
//! settings untouched.

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{
    AutostartPort, Settings, SettingsTab, ShortcutPhase, ShortcutPort, State, TRAY_SYNC_NOW,
    TrayFactory, TrayPort, autostart_supported, desktop_entry_path, poll, remove_desktop_entry,
    tray_supported, view, write_desktop_entry,
};
use rune::{Env, Runtime, TrayItem};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-tray2-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[derive(Default)]
struct TrayLog {
    menus: Vec<Vec<TrayItem>>,
    fail: bool,
}

struct FakeTray(Rc<RefCell<TrayLog>>);

impl TrayPort for FakeTray {
    fn set_menu(&self, menu: Vec<TrayItem>) {
        self.0.borrow_mut().menus.push(menu);
    }
}

fn fake_tray(log: &Rc<RefCell<TrayLog>>) -> TrayFactory {
    let log = Rc::clone(log);
    Box::new(move |menu| {
        if log.borrow().fail {
            return Err("no session bus".to_owned());
        }
        log.borrow_mut().menus.push(menu);
        Ok(Box::new(FakeTray(Rc::clone(&log))) as Box<dyn TrayPort>)
    })
}

#[derive(Default)]
struct AutostartLog {
    calls: Vec<bool>,
    fail: Option<String>,
}

struct FakeAutostart(Rc<RefCell<AutostartLog>>);

impl AutostartPort for FakeAutostart {
    fn apply(&self, on: bool) -> Result<(), String> {
        let mut log = self.0.borrow_mut();
        log.calls.push(on);
        log.fail.clone().map_or(Ok(()), Err)
    }
}

#[derive(Default)]
struct ShortcutLog {
    binds: usize,
    unbinds: usize,
}

struct FakeShortcut(Rc<RefCell<ShortcutLog>>);

impl ShortcutPort for FakeShortcut {
    fn bind(&self) {
        self.0.borrow_mut().binds += 1;
    }
    fn unbind(&self) {
        self.0.borrow_mut().unbinds += 1;
    }
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

fn fixture(tag: &str, settings: Settings) -> Fixture {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let config = unique_dir(&format!("{tag}-config"));
    let state_home = unique_dir(&format!("{tag}-state"));
    let settings = Settings {
        last_vault: Some(vault.path().to_path_buf()),
        ..settings
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

fn saved(f: &Fixture, key: &str) -> Option<bool> {
    let raw = fs::read_to_string(f.config.join("muninn").join("settings.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    json.get(key).and_then(serde_json::Value::as_bool)
}

fn on(tray_icon: bool, close_to_tray: bool) -> Settings {
    Settings {
        tray_icon,
        close_to_tray,
        ..Settings::default()
    }
}

// Close to tray

#[test]
fn closing_hides_only_with_a_live_tray_and_the_setting_on() {
    // Off by default: closing quits.
    let f = fixture("close-default", Settings::default());
    assert!(!f.rt.state.close_to_tray());
    assert!(!f.rt.state.closes_to_tray());

    // Setting on but no tray factory (headless): still quits.
    let f = fixture("close-headless", on(true, true));
    assert!(!f.rt.state.closes_to_tray(), "no tray was created");

    // Setting on, tray switch off: quits.
    let mut f = fixture("close-tray-off", on(false, true));
    let log = Rc::new(RefCell::new(TrayLog::default()));
    f.rt.state.install_tray(fake_tray(&log));
    frames(&mut f.rt, 2);
    assert!(!f.rt.state.closes_to_tray());

    // Tray on and created, setting off: quits.
    let mut f = fixture("close-off", on(true, false));
    let log = Rc::new(RefCell::new(TrayLog::default()));
    f.rt.state.install_tray(fake_tray(&log));
    frames(&mut f.rt, 2);
    assert!(!f.rt.state.closes_to_tray());

    // Both on with a live tray: closing hides (where Rune has a tray).
    f.rt.update(|s, _| s.set_close_to_tray(true));
    assert_eq!(saved(&f, "close_to_tray"), Some(true));
    assert_eq!(f.rt.state.closes_to_tray(), tray_supported());

    // Turning the tray off takes close-to-tray with it.
    f.rt.update(|s, _| s.set_tray_icon(false));
    frames(&mut f.rt, 2);
    assert!(f.rt.state.close_to_tray(), "the saved choice stays");
    assert!(!f.rt.state.closes_to_tray(), "but closing quits again");
}

#[test]
fn a_failed_tray_never_closes_to_tray() {
    let mut f = fixture("close-failed", on(true, true));
    let log = Rc::new(RefCell::new(TrayLog {
        fail: true,
        ..TrayLog::default()
    }));
    f.rt.state.install_tray(fake_tray(&log));
    frames(&mut f.rt, 4);
    assert!(!f.rt.state.tray_shown());
    assert!(!f.rt.state.closes_to_tray(), "no icon to come back from");
}

#[test]
fn the_close_switch_is_on_the_appearance_tab_and_needs_the_tray() {
    let mut f = fixture("close-switch", Settings::default());
    let log = Rc::new(RefCell::new(TrayLog::default()));
    f.rt.state.install_tray(fake_tray(&log));
    f.rt.update(|s, _| {
        s.open_settings();
        s.settings_tab = SettingsTab::Appearance;
    });
    frames(&mut f.rt, 8);
    let (x, y) =
        f.rt.find("settings-close-to-tray")
            .expect("switch on screen");
    f.rt.pointer_down(x, y);
    f.rt.pointer_up(x, y);
    frames(&mut f.rt, 4);
    assert!(
        !f.rt.state.close_to_tray(),
        "disabled while the tray is off"
    );

    f.rt.update(|s, _| s.set_tray_icon(true));
    frames(&mut f.rt, 6);
    let (x, y) =
        f.rt.find("settings-close-to-tray")
            .expect("switch on screen");
    f.rt.pointer_down(x, y);
    f.rt.pointer_up(x, y);
    frames(&mut f.rt, 4);
    assert_eq!(f.rt.state.close_to_tray(), tray_supported());
    assert!(f.rt.find("settings-close-to-tray-status").is_some());
}

// Sync now in the tray

fn sync_now_enabled(menu: &[TrayItem]) -> Option<bool> {
    menu.iter().find_map(|item| match item {
        TrayItem::Action { id, enabled, .. } if id == TRAY_SYNC_NOW => Some(*enabled),
        _ => None,
    })
}

#[test]
fn tray_sync_now_is_greyed_without_a_provider_and_does_nothing() {
    if !tray_supported() {
        return;
    }
    let mut f = fixture("sync-now", on(true, false));
    let log = Rc::new(RefCell::new(TrayLog::default()));
    f.rt.state.install_tray(fake_tray(&log));
    frames(&mut f.rt, 2);
    let menu = log.borrow().menus.last().cloned().expect("menu");
    assert_eq!(sync_now_enabled(&menu), Some(false), "not connected");

    let status = f.rt.state.sync_status().to_owned();
    f.rt.update(|s, _| s.tray_sync_now());
    frames(&mut f.rt, 2);
    assert_eq!(f.rt.state.sync_status(), status, "no run, no new message");
    assert!(!f.rt.state.sync_busy());
}

// Start at login

#[test]
fn start_at_login_is_off_and_untouched_by_default() {
    let mut f = fixture("login-default", Settings::default());
    let log = Rc::new(RefCell::new(AutostartLog::default()));
    f.rt.state
        .install_autostart(Box::new(FakeAutostart(Rc::clone(&log))));
    assert!(!f.rt.state.auto_start());
    assert!(
        log.borrow().calls.is_empty(),
        "startup with it off writes nothing"
    );
}

#[test]
fn toggling_start_at_login_updates_the_os_entry_and_persists() {
    let mut f = fixture("login-toggle", Settings::default());
    let log = Rc::new(RefCell::new(AutostartLog::default()));
    f.rt.state
        .install_autostart(Box::new(FakeAutostart(Rc::clone(&log))));
    f.rt.update(|s, _| s.set_auto_start(true));
    assert_eq!(saved(&f, "auto_start"), Some(true));
    f.rt.update(|s, _| s.set_auto_start(false));
    assert_eq!(saved(&f, "auto_start"), Some(false));
    let expected: Vec<bool> = if autostart_supported() {
        vec![true, false]
    } else {
        Vec::new()
    };
    assert_eq!(log.borrow().calls, expected);
}

#[test]
fn startup_with_start_at_login_on_repoints_the_entry() {
    let mut f = fixture(
        "login-startup",
        Settings {
            auto_start: true,
            ..Settings::default()
        },
    );
    let log = Rc::new(RefCell::new(AutostartLog::default()));
    f.rt.state
        .install_autostart(Box::new(FakeAutostart(Rc::clone(&log))));
    let expected: Vec<bool> = if autostart_supported() {
        vec![true]
    } else {
        Vec::new()
    };
    assert_eq!(log.borrow().calls, expected);
}

#[test]
fn a_failed_login_item_shows_its_error_until_the_switch_changes() {
    if !autostart_supported() {
        return;
    }
    let mut f = fixture("login-fail", Settings::default());
    let log = Rc::new(RefCell::new(AutostartLog {
        fail: Some("permission denied".to_owned()),
        ..AutostartLog::default()
    }));
    f.rt.state
        .install_autostart(Box::new(FakeAutostart(Rc::clone(&log))));
    f.rt.update(|s, _| s.set_auto_start(true));
    frames(&mut f.rt, 30);
    assert_eq!(
        log.borrow().calls.len(),
        1,
        "one attempt, not one per frame"
    );
    assert_eq!(f.rt.state.auto_start_error(), Some("permission denied"));

    log.borrow_mut().fail = None;
    f.rt.update(|s, _| s.set_auto_start(false));
    assert_eq!(f.rt.state.auto_start_error(), None);
}

#[test]
fn the_desktop_entry_is_written_and_removed() {
    let config = unique_dir("autostart-files");
    let dir = config.join("autostart");
    write_desktop_entry(&dir, std::path::Path::new("/usr/bin/muninn")).expect("write");
    let text = fs::read_to_string(desktop_entry_path(&dir)).expect("entry");
    assert!(text.contains("\nExec=\"/usr/bin/muninn\"\n"));
    // Writing again replaces it (a moved binary is re-pointed).
    write_desktop_entry(&dir, std::path::Path::new("/opt/muninn/muninn")).expect("rewrite");
    let text = fs::read_to_string(desktop_entry_path(&dir)).expect("entry");
    assert!(text.contains("\nExec=\"/opt/muninn/muninn\"\n"));
    remove_desktop_entry(&dir).expect("remove");
    assert!(!desktop_entry_path(&dir).exists());
    remove_desktop_entry(&dir).expect("a missing entry is fine");
    let _ = fs::remove_dir_all(&config);
}

// The global shortcut

#[test]
fn the_shortcut_binds_once_and_unbinds_when_turned_off() {
    let mut f = fixture("shortcut", Settings::default());
    let log = Rc::new(RefCell::new(ShortcutLog::default()));
    f.rt.state
        .install_shortcut(Box::new(FakeShortcut(Rc::clone(&log))));
    assert_eq!(log.borrow().binds, 0, "off by default");

    f.rt.update(|s, _| s.set_show_shortcut(true));
    assert_eq!(saved(&f, "show_shortcut"), Some(true));
    assert_eq!(*f.rt.state.shortcut_phase(), ShortcutPhase::Pending);
    f.rt.update(|s, _| s.shortcut_result(Ok(())));
    assert_eq!(*f.rt.state.shortcut_phase(), ShortcutPhase::Bound);
    frames(&mut f.rt, 10);
    assert_eq!(log.borrow().binds, 1, "not re-bound every frame");

    f.rt.update(|s, _| s.set_show_shortcut(false));
    assert_eq!(log.borrow().unbinds, 1);
    assert_eq!(*f.rt.state.shortcut_phase(), ShortcutPhase::Off);
}

#[test]
fn a_late_success_after_turning_off_is_released() {
    let mut f = fixture("shortcut-late", Settings::default());
    let log = Rc::new(RefCell::new(ShortcutLog::default()));
    f.rt.state
        .install_shortcut(Box::new(FakeShortcut(Rc::clone(&log))));
    f.rt.update(|s, _| s.set_show_shortcut(true));
    f.rt.update(|s, _| s.set_show_shortcut(false));
    f.rt.update(|s, _| s.set_show_shortcut(true));
    assert_eq!(log.borrow().binds, 1, "one bind in flight at a time");
    f.rt.update(|s, _| s.set_show_shortcut(false));
    f.rt.update(|s, _| s.shortcut_result(Ok(())));
    assert_eq!(log.borrow().unbinds, 1, "released as soon as it lands");
    assert_eq!(*f.rt.state.shortcut_phase(), ShortcutPhase::Off);
}

#[test]
fn a_refused_shortcut_shows_why_and_retries_only_on_toggle() {
    let mut f = fixture(
        "shortcut-fail",
        Settings {
            show_shortcut: true,
            ..Settings::default()
        },
    );
    let log = Rc::new(RefCell::new(ShortcutLog::default()));
    f.rt.state
        .install_shortcut(Box::new(FakeShortcut(Rc::clone(&log))));
    assert_eq!(log.borrow().binds, 1, "startup binds when on");
    f.rt.update(|s, _| s.shortcut_result(Err("shortcut is already registered".to_owned())));
    frames(&mut f.rt, 10);
    assert_eq!(
        *f.rt.state.shortcut_phase(),
        ShortcutPhase::Failed("shortcut is already registered".to_owned())
    );
    assert_eq!(log.borrow().binds, 1, "not retried every frame");

    f.rt.update(|s, _| {
        s.open_settings();
        s.settings_tab = SettingsTab::Appearance;
    });
    frames(&mut f.rt, 6);
    assert!(f.rt.find("settings-show-shortcut-status").is_some());

    f.rt.update(|s, _| s.set_show_shortcut(false));
    f.rt.update(|s, _| s.set_show_shortcut(true));
    assert_eq!(log.borrow().binds, 2, "toggling is the retry");
}

#[test]
fn without_ports_nothing_is_registered_or_written() {
    // Headless runs install no ports: the settings can be on with no effect.
    let mut f = fixture(
        "headless",
        Settings {
            tray_icon: true,
            close_to_tray: true,
            auto_start: true,
            show_shortcut: true,
            ..Settings::default()
        },
    );
    frames(&mut f.rt, 4);
    assert!(!f.rt.state.tray_shown());
    assert!(!f.rt.state.closes_to_tray());
    assert_eq!(*f.rt.state.shortcut_phase(), ShortcutPhase::Off);
    assert_eq!(f.rt.state.auto_start_error(), None);
}

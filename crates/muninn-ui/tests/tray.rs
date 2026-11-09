//! The optional tray icon (Settings › Appearance › Show tray icon), driven
//! through a fake [`TrayFactory`] so no desktop session or tray host is
//! needed. Explicit config/state roots keep the real settings untouched.

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{
    Settings, SettingsTab, State, TRAY_SYNC_STATUS, TrayFactory, TrayPort, on_key, poll, tray_menu,
    tray_supported, view,
};
use rune::{Env, Key, KeyEvent, Mods, Runtime, TrayItem};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-tray-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// What the fake tray saw.
#[derive(Default)]
struct Log {
    created: usize,
    dropped: usize,
    failures: usize,
    /// Menus handed over, at creation and on each refresh.
    menus: Vec<Vec<TrayItem>>,
    /// When set, the factory fails with this message.
    fail: Option<String>,
}

struct FakeTray(Rc<RefCell<Log>>);

impl TrayPort for FakeTray {
    fn set_menu(&self, menu: Vec<TrayItem>) {
        self.0.borrow_mut().menus.push(menu);
    }
}

impl Drop for FakeTray {
    fn drop(&mut self) {
        self.0.borrow_mut().dropped += 1;
    }
}

fn fake_factory(log: &Rc<RefCell<Log>>) -> TrayFactory {
    let log = Rc::clone(log);
    Box::new(move |menu| {
        let mut l = log.borrow_mut();
        if let Some(error) = l.fail.clone() {
            l.failures += 1;
            return Err(error);
        }
        l.created += 1;
        l.menus.push(menu);
        drop(l);
        Ok(Box::new(FakeTray(Rc::clone(&log))) as Box<dyn TrayPort>)
    })
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

fn fixture(tag: &str, tray_icon: bool) -> Fixture {
    let vault = TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
        ("Beta.md", "# Beta\n"),
    ]);
    let config = unique_dir(&format!("{tag}-config"));
    let state_home = unique_dir(&format!("{tag}-state"));
    let settings = Settings {
        last_vault: Some(vault.path().to_path_buf()),
        tray_icon,
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

fn saved_tray_icon(f: &Fixture) -> Option<bool> {
    let raw = fs::read_to_string(f.config.join("muninn").join("settings.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    json.get("tray_icon").and_then(serde_json::Value::as_bool)
}

#[test]
fn off_by_default_and_old_files_load_as_off() {
    assert!(!Settings::default().tray_icon);
    let home = unique_dir("old-file");
    let dir = home.join("muninn");
    fs::create_dir_all(&dir).expect("config dir");
    fs::write(dir.join("settings.json"), r#"{"theme":"dark"}"#).expect("old settings");
    assert!(!Settings::load_from(&home).tray_icon);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn without_a_factory_no_tray_is_created() {
    // Headless runs (`--snap`, tests, scripts) never install a factory, so
    // the setting can be on without an icon.
    let mut f = fixture("headless", true);
    frames(&mut f.rt, 4);
    assert!(f.rt.state.tray_icon());
    assert!(!f.rt.state.tray_shown());
    assert_eq!(f.rt.state.tray_error(), None);
}

#[test]
fn toggling_creates_and_drops_the_tray_and_persists() {
    let mut f = fixture("toggle", false);
    let log = Rc::new(RefCell::new(Log::default()));
    f.rt.state.install_tray(fake_factory(&log));
    frames(&mut f.rt, 4);
    assert_eq!(log.borrow().created, 0, "off: no tray");

    f.rt.update(|s, _| s.set_tray_icon(true));
    frames(&mut f.rt, 4);
    assert_eq!(saved_tray_icon(&f), Some(true), "the choice is saved");
    if !tray_supported() {
        assert!(!f.rt.state.tray_shown(), "no tray where Rune has none");
        assert_eq!(log.borrow().created, 0);
        return;
    }
    assert!(f.rt.state.tray_shown());
    assert_eq!(log.borrow().created, 1, "created once, not every frame");
    assert_eq!(log.borrow().menus.len(), 1, "no refresh without a change");

    f.rt.update(|s, _| s.set_tray_icon(false));
    frames(&mut f.rt, 2);
    assert!(!f.rt.state.tray_shown());
    assert_eq!(log.borrow().dropped, 1, "turning it off drops the icon");
    assert_eq!(saved_tray_icon(&f), Some(false));
}

#[test]
fn installing_the_factory_with_the_setting_on_creates_the_tray() {
    let mut f = fixture("startup", true);
    let log = Rc::new(RefCell::new(Log::default()));
    f.rt.state.install_tray(fake_factory(&log));
    frames(&mut f.rt, 2);
    assert_eq!(f.rt.state.tray_shown(), tray_supported());
    assert_eq!(log.borrow().created, usize::from(tray_supported()));
}

#[test]
fn the_menu_follows_the_sync_status() {
    if !tray_supported() {
        return;
    }
    let mut f = fixture("refresh", true);
    let log = Rc::new(RefCell::new(Log::default()));
    f.rt.state.install_tray(fake_factory(&log));
    frames(&mut f.rt, 2);
    let first = log.borrow().menus.last().cloned().expect("first menu");
    assert_eq!(first, tray_menu(f.rt.state.sync_status(), false));

    f.rt.state.sync_ui.status = "Last successful sync 1a2b3c4d. Nothing changed.".to_owned();
    frames(&mut f.rt, 2);
    let l = log.borrow();
    assert_eq!(l.menus.len(), 2, "one refresh for one status change");
    let row = l.menus[1]
        .iter()
        .find_map(|item| match item {
            TrayItem::Action { id, label, enabled } if id == TRAY_SYNC_STATUS => {
                Some((label.clone(), *enabled))
            }
            _ => None,
        })
        .expect("status row");
    assert_eq!(
        row,
        ("Sync: Last successful sync 1a2b3c4d.".to_owned(), false)
    );
    assert_eq!(l.created, 1, "a refresh does not recreate the icon");
}

#[test]
fn a_failed_tray_shows_its_error_and_is_not_retried_every_frame() {
    if !tray_supported() {
        return;
    }
    let mut f = fixture("fail", false);
    let log = Rc::new(RefCell::new(Log {
        fail: Some("no session bus".to_owned()),
        ..Log::default()
    }));
    f.rt.state.install_tray(fake_factory(&log));
    f.rt.update(|s, _| s.set_tray_icon(true));
    frames(&mut f.rt, 30);
    assert_eq!(log.borrow().failures, 1, "one attempt, not one per frame");
    assert_eq!(f.rt.state.tray_error(), Some("no session bus"));
    assert!(!f.rt.state.tray_shown());

    // The error shows under the switch.
    f.rt.update(|s, _| {
        s.open_settings();
        s.settings_tab = SettingsTab::Appearance;
    });
    frames(&mut f.rt, 6);
    assert!(f.rt.find("settings-tray-icon").is_some());
    assert!(f.rt.find("settings-tray-status").is_some());

    // Toggling off and on again is an explicit retry.
    log.borrow_mut().fail = None;
    f.rt.update(|s, _| s.set_tray_icon(false));
    f.rt.update(|s, _| s.set_tray_icon(true));
    frames(&mut f.rt, 2);
    assert!(f.rt.state.tray_shown());
    assert_eq!(f.rt.state.tray_error(), None);
}

#[test]
fn the_switch_lives_on_the_appearance_tab_and_toggles() {
    let mut f = fixture("switch", false);
    let log = Rc::new(RefCell::new(Log::default()));
    f.rt.state.install_tray(fake_factory(&log));
    f.rt.key(KeyEvent {
        key: Key::Command(','),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    frames(&mut f.rt, 8);
    f.rt.update(|s, _| s.settings_tab = SettingsTab::Appearance);
    frames(&mut f.rt, 6);
    let (x, y) =
        f.rt.find("settings-tray-icon")
            .expect("the switch is on screen");
    f.rt.pointer_down(x, y);
    f.rt.pointer_up(x, y);
    frames(&mut f.rt, 4);
    // On macOS the switch is disabled and the click does nothing.
    assert_eq!(f.rt.state.tray_icon(), tray_supported());
    assert_eq!(f.rt.state.tray_shown(), tray_supported());
}

#[test]
fn new_note_closes_settings_and_opens_the_name_field() {
    let mut f = fixture("new-note", true);
    f.rt.update(|s, _| s.open_settings());
    frames(&mut f.rt, 4);
    assert!(f.rt.state.settings_modal);
    f.rt.update(|s, _| s.tray_new_note());
    frames(&mut f.rt, 4);
    assert!(!f.rt.state.settings_modal, "Settings closed");
    assert!(f.rt.state.new_note_prompt.is_some(), "new-note field open");
}

#[test]
fn quit_flushes_the_dirty_note() {
    let mut f = fixture("quit", false);
    let id = f.rt.state.vault.resolve("Alpha").expect("Alpha");
    f.rt.state.load_note(id, false);
    let path = f.rt.state.vault.notes[id].path.clone();
    f.rt.state.focus_editor();
    f.rt.state
        .editor
        .replace_text("# Alpha\n\nTyped just before Quit.\n");
    assert!(f.rt.state.dirty());
    f.rt.state.flush_before_quit();
    assert!(!f.rt.state.dirty());
    assert_eq!(
        fs::read_to_string(&path).expect("disk"),
        "# Alpha\n\nTyped just before Quit.\n"
    );
    assert!(saved_tray_icon(&f).is_some(), "settings were written too");
}

//! #192: the UI language setting, its persistence, the live switch and the
//! picker/tray surfaces that follow it.
//!
//! These tests change the process-wide language, so each takes one mutex.
//! The rest of the suite runs in other binaries (a separate process) and does
//! not depend on this default.

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use muninn_core::testing::TempVault;
use muninn_i18n::{
    Key, Lang, PSEUDO_CLOSE, PSEUDO_OPEN, clear_lang, process_lang, set_lang, set_process_lang, t,
};
use muninn_ui::{
    LanguageChoice, Settings, SettingsTab, State, TRAY_SHOW, TrayFactory, TrayPort, on_key, poll,
    tray_supported, view,
};
use rune::{Env, Runtime, TrayItem};

static NEXT: AtomicUsize = AtomicUsize::new(0);
static LANGUAGE: Mutex<()> = Mutex::new(());

/// Ties every test that reads or writes the language globals to one at a time,
/// and restores both seams afterwards (including after a panic).
struct LanguageGuard {
    process: Lang,
    thread: Lang,
    _lock: MutexGuard<'static, ()>,
}

impl LanguageGuard {
    fn enter() -> Self {
        let lock = LANGUAGE.lock().unwrap_or_else(|error| error.into_inner());
        Self {
            process: process_lang(),
            thread: Lang::current(),
            _lock: lock,
        }
    }
}

impl Drop for LanguageGuard {
    fn drop(&mut self) {
        set_process_lang(self.process);
        set_lang(self.thread);
    }
}

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-language-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn fixture(tag: &str) -> (TempVault, PathBuf, PathBuf, State) {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let config = unique_dir(&format!("{tag}-config"));
    let state_home = unique_dir(&format!("{tag}-state"));
    let state = State::with_settings(
        vault.path().to_path_buf(),
        state_home.clone(),
        config.clone(),
        Settings::default(),
    );
    (vault, config, state_home, state)
}

fn frame(rt: &mut Runtime<State>) {
    for _ in 0..6 {
        rt.frame(1.0 / 60.0);
    }
}

fn drawn(rt: &Runtime<State>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .flat_map(|element| [element.text, element.label])
        .flatten()
        .collect()
}

fn is_bracketed(text: &str) -> bool {
    text.starts_with(PSEUDO_OPEN) && text.ends_with(PSEUDO_CLOSE)
}

#[test]
fn the_language_setting_round_trips_and_loads_forgivingly() {
    let home = unique_dir("setting");
    let write = |json: &str| {
        let dir = home.join("muninn");
        fs::create_dir_all(&dir).expect("config dir");
        fs::write(dir.join("settings.json"), json).expect("write settings");
    };

    // Default and a pre-feature file (no key).
    assert_eq!(Settings::default().ui_language, LanguageChoice::System);
    write(r#"{"theme":"dark"}"#);
    assert_eq!(
        Settings::load_from(&home).ui_language,
        LanguageChoice::System
    );

    // Explicit choice round-trips through the file.
    write(r#"{"ui_language":"en-US"}"#);
    let loaded = Settings::load_from(&home);
    assert_eq!(loaded.ui_language, LanguageChoice::Language(Lang::EnUs));
    loaded.save_to(&home).expect("save");
    let text = fs::read_to_string(home.join("muninn/settings.json")).expect("read");
    assert!(text.contains("\"ui_language\": \"en-US\""), "{text}");

    // System stays the word "system", never a tag.
    Settings::default().save_to(&home).expect("save system");
    let text = fs::read_to_string(home.join("muninn/settings.json")).expect("read");
    assert!(text.contains("\"ui_language\": \"system\""), "{text}");

    // A tag this build does not ship, and a malformed value, are System.
    for json in [
        r#"{"ui_language":"fr-FR"}"#,
        r#"{"ui_language":"klingon"}"#,
        r#"{"ui_language":3}"#,
        r#"{"ui_language":null}"#,
    ] {
        write(json);
        assert_eq!(
            Settings::load_from(&home).ui_language,
            LanguageChoice::System,
            "{json}"
        );
    }

    // Corrupt JSON is still the startup defaults, now including System.
    write("{not json");
    assert_eq!(Settings::load_from(&home), Settings::default());

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn system_resolution_uses_the_detected_locale_and_falls_back_to_english() {
    use muninn_i18n::LocaleTag;

    // A stubbed detector: no process-environment mutation needed.
    assert_eq!(LanguageChoice::System.resolve(None), Lang::EnUs);
    assert_eq!(
        LanguageChoice::System.resolve(Some(LocaleTag::EnUs)),
        Lang::EnUs
    );
    assert_eq!(
        LanguageChoice::System.resolve(Some(LocaleTag::Fr)),
        Lang::EnUs,
        "a recognised but unshipped locale falls back to English"
    );
    assert_eq!(
        LanguageChoice::Language(Lang::EnUs).resolve(Some(LocaleTag::Fr)),
        Lang::EnUs,
        "an explicit choice ignores detection"
    );
}

#[test]
fn set_ui_language_flips_both_seams_persists_and_redraws() {
    let _guard = LanguageGuard::enter();
    let (_vault, config, _state_home, state) = fixture("switch");
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 1.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    frame(&mut rt);

    // Explicit English: the process default, thread override and file agree.
    rt.update(|s, _| s.set_ui_language(LanguageChoice::Language(Lang::EnUs)));
    assert_eq!(Lang::current(), Lang::EnUs);
    assert_eq!(process_lang(), Lang::EnUs);
    assert_eq!(
        Settings::load_from(&config).ui_language,
        LanguageChoice::Language(Lang::EnUs)
    );

    // Switch to Pseudo: the next frame draws bracketed catalog copy.
    rt.update(|s, _| s.set_ui_language(LanguageChoice::Language(Lang::Pseudo)));
    assert_eq!(Lang::current(), Lang::Pseudo);
    clear_lang();
    assert_eq!(
        process_lang(),
        Lang::Pseudo,
        "the process default follows the switch for the tray and workers"
    );
    assert_eq!(Lang::current(), Lang::Pseudo);
    frame(&mut rt);
    let texts = drawn(&rt);
    assert!(
        texts.iter().any(|text| is_bracketed(text)),
        "a switched frame should draw Pseudo copy: {texts:?}"
    );

    // Restore English; the redraw loses the brackets.
    rt.update(|s, _| s.set_ui_language(LanguageChoice::Language(Lang::EnUs)));
    frame(&mut rt);
    let texts = drawn(&rt);
    assert!(
        !texts.iter().any(|text| is_bracketed(text)),
        "English still draws English: {texts:?}"
    );
}

#[test]
fn the_picker_lists_system_and_every_shipped_endonym() {
    let _guard = LanguageGuard::enter();
    let (_vault, _config, _state_home, state) = fixture("picker");
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 1.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt.update(|s, _| {
        s.open_settings();
        s.set_settings_tab(SettingsTab::Appearance);
    });
    frame(&mut rt);

    assert!(
        rt.find("settings-language").is_some(),
        "the Appearance tab has a language control"
    );
    let texts = drawn(&rt);
    assert!(
        texts
            .iter()
            .any(|text| text == t(Key::SettingsAppearanceLanguageSystem)),
        "System is listed: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text == t(Key::SettingsAppearanceLanguageTitle)),
        "the section title is drawn"
    );
    for &lang in Lang::SHIPPED {
        assert!(
            texts.iter().any(|text| text == lang.endonym()),
            "{} is listed by its endonym: {texts:?}",
            lang.tag()
        );
    }

    // With one shipped language the control is still a usable System/English
    // pair: two segments and the explicit choice selected after a pick.
    let control = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("settings-language"))
        .expect("language control");
    assert!(control.visible[2] > 0.0 && control.visible[3] > 0.0);
}

#[test]
fn the_tray_menu_relabels_on_a_language_switch() {
    if !tray_supported() {
        return;
    }
    let _guard = LanguageGuard::enter();
    let (_vault, _config, _state_home, mut state) = fixture("tray-language");
    let log = Rc::new(RefCell::new(Vec::<Vec<TrayItem>>::new()));
    let menus = Rc::clone(&log);
    let factory: TrayFactory = Box::new(move |menu| {
        menus.borrow_mut().push(menu);
        Ok(Box::new(RecordingTray(Rc::clone(&menus))) as Box<dyn TrayPort>)
    });
    state.install_tray(factory);
    state.set_tray_icon(true);
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 1.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    frame(&mut rt);

    let english = show_label(log.borrow().last().expect("first tray menu"));
    assert_eq!(english, t(Key::TrayShow));
    assert!(!is_bracketed(&english), "English menu: {english:?}");

    rt.update(|s, _| s.set_ui_language(LanguageChoice::Language(Lang::Pseudo)));
    frame(&mut rt);
    let pseudo = show_label(
        log.borrow()
            .last()
            .expect("a rebuilt menu after the switch"),
    );
    assert!(
        is_bracketed(&pseudo),
        "the tray menu must be rebuilt in the new language: {pseudo:?}"
    );

    rt.update(|s, _| s.set_ui_language(LanguageChoice::Language(Lang::EnUs)));
    frame(&mut rt);
    assert_eq!(
        show_label(log.borrow().last().expect("restored English menu")),
        english
    );
}

struct RecordingTray(Rc<RefCell<Vec<Vec<TrayItem>>>>);

impl TrayPort for RecordingTray {
    fn set_menu(&self, menu: Vec<TrayItem>) {
        self.0.borrow_mut().push(menu);
    }
}

fn show_label(menu: &[TrayItem]) -> String {
    menu.iter()
        .find_map(|item| match item {
            TrayItem::Action { id, label, .. } if id == TRAY_SHOW => Some(label.clone()),
            _ => None,
        })
        .expect("the tray has a Show item")
}

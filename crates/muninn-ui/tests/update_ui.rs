//! #234: the notify-only release check's UI half — the banner, the Settings ›
//! Appearance toggle and the state transitions.
//!
//! These tests never perform a network request. They apply a `CheckOutcome`
//! directly, exactly as `poll` would after a worker sends one, and read the
//! persisted settings back from the temporary config directory.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_i18n::{Key, t, update_available};
use muninn_ui::{
    AvailableUpdate, CheckOutcome, InstallMethod, Settings, SettingsTab, State, on_key, poll, view,
};
use rune::{Env, Runtime};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    rt: Runtime<State>,
    config: PathBuf,
    _vault: TempVault,
}

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-update-ui-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn fixture(tag: &str, settings: Settings) -> Fixture {
    fixture_sized(tag, settings, 780.0)
}

fn fixture_sized(tag: &str, settings: Settings, height: f32) -> Fixture {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let config = unique_dir(tag);
    let state_home = unique_dir(tag);
    let mut rt = Runtime::new(
        State::with_settings(
            vault.path().to_path_buf(),
            state_home,
            config.clone(),
            settings,
        ),
        view,
        (1180.0, height),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    Fixture {
        rt,
        config,
        _vault: vault,
    }
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn drawn(rt: &Runtime<State>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter_map(|element| element.text)
        .collect()
}

fn has_text(rt: &Runtime<State>, words: &str) -> bool {
    drawn(rt).iter().any(|drawn| drawn == words)
}

fn has_label(rt: &Runtime<State>, words: &str) -> bool {
    rt.elements()
        .iter()
        .any(|element| element.label.as_deref() == Some(words))
}

fn banner_count(rt: &Runtime<State>) -> usize {
    rt.elements()
        .iter()
        .filter(|element| element.key.as_deref() == Some("update-banner") && !element.exiting)
        .count()
}

fn available(version: &str, method: InstallMethod) -> CheckOutcome {
    CheckOutcome::Available(AvailableUpdate {
        version: version.to_owned(),
        url: "https://github.com/raythurman2386/muninn/releases/tag/v0.10.0".to_owned(),
        method,
        etag: Some("\"release-1\"".to_owned()),
    })
}

fn apply(rt: &mut Runtime<State>, outcome: CheckOutcome) {
    rt.update(|s, _| s.apply_update_outcome(outcome, 1_700_000_000, 60));
    // The outcome asks for a deferred settings write; let the idle tick land it.
    frames(rt, 40);
}

fn saved(f: &Fixture) -> Settings {
    Settings::load_from(&f.config)
}

#[test]
fn a_newer_release_draws_one_banner_with_the_version_hint_and_actions() {
    let mut f = fixture("available", Settings::default());
    apply(&mut f.rt, available("0.10.0", InstallMethod::Tarball));

    assert_eq!(banner_count(&f.rt), 1, "exactly one update banner");
    assert!(has_text(&f.rt, &update_available("0.10.0")));
    assert!(has_text(&f.rt, t(Key::UpdateHintTarball)));
    assert!(has_text(&f.rt, t(Key::UpdateReleaseNotes)));
    assert!(has_text(&f.rt, t(Key::UpdateDismiss)));
    assert!(
        has_label(&f.rt, t(Key::UpdateAvailableTitle)),
        "the banner is named from the catalog"
    );

    assert_eq!(
        saved(&f).last_seen_version.as_deref(),
        Some("0.10.0"),
        "the seen version is persisted"
    );
    assert_eq!(saved(&f).update_etag.as_deref(), Some("\"release-1\""));
    assert_eq!(saved(&f).update_failures, 0);
}

#[test]
fn every_install_method_uses_its_catalog_hint() {
    for (method, key) in [
        (InstallMethod::AppImage, Key::UpdateHintAppImage),
        (InstallMethod::Aur, Key::UpdateHintAur),
        (InstallMethod::Deb, Key::UpdateHintDeb),
        (InstallMethod::Tarball, Key::UpdateHintTarball),
        (InstallMethod::Unknown, Key::UpdateHintUnknown),
    ] {
        let mut f = fixture("hint", Settings::default());
        apply(&mut f.rt, available("0.10.0", method));
        assert_eq!(banner_count(&f.rt), 1, "{method:?}");
        assert!(has_text(&f.rt, t(key)), "{method:?} draws {}", key.id());
    }
}

#[test]
fn current_not_modified_and_failed_checks_stay_silent() {
    for (tag, outcome) in [
        (
            "up-to-date",
            CheckOutcome::UpToDate {
                etag: Some("\"new\"".to_owned()),
            },
        ),
        ("not-modified", CheckOutcome::NotModified),
        ("failed", CheckOutcome::Failed),
    ] {
        let mut f = fixture(tag, Settings::default());
        apply(&mut f.rt, outcome);
        assert_eq!(banner_count(&f.rt), 0, "{tag}: no banner");
        assert!(f.rt.state.error.is_none(), "{tag}: no error bar");
        assert!(f.rt.state.notice.is_none(), "{tag}: no status notice");
        assert!(!f.rt.state.update_checking, "{tag}: the check settled");
        assert!(f.rt.state.update_rx.is_none(), "{tag}: no receiver left");
    }

    // Success persists the new ETag and clears the failure count.
    let mut f = fixture("etag", Settings::default());
    apply(
        &mut f.rt,
        CheckOutcome::UpToDate {
            etag: Some("\"new\"".to_owned()),
        },
    );
    assert_eq!(saved(&f).update_etag.as_deref(), Some("\"new\""));
    assert_eq!(saved(&f).update_failures, 0);

    // A 304 keeps the stored ETag, since the server sent none.
    let mut f = fixture(
        "not-modified-etag",
        Settings {
            update_etag: Some("\"old\"".to_owned()),
            ..Settings::default()
        },
    );
    apply(&mut f.rt, CheckOutcome::NotModified);
    assert_eq!(saved(&f).update_etag.as_deref(), Some("\"old\""));

    // A failure backs off and stays silent.
    let mut f = fixture("failed-backoff", Settings::default());
    apply(&mut f.rt, CheckOutcome::Failed);
    let settings = saved(&f);
    assert_eq!(settings.update_failures, 1);
    assert!(settings.update_next_check.is_some());
    assert!(f.rt.state.error.is_none());
    assert!(f.rt.state.notice.is_none());
}

#[test]
fn a_skipped_version_does_not_draw_and_dismiss_remembers_it() {
    let mut f = fixture(
        "skipped",
        Settings {
            skip_version: Some("0.10.0".to_owned()),
            ..Settings::default()
        },
    );
    apply(&mut f.rt, available("0.10.0", InstallMethod::Deb));
    assert_eq!(banner_count(&f.rt), 0, "a skipped release stays hidden");
    assert_eq!(saved(&f).last_seen_version.as_deref(), Some("0.10.0"));

    let mut f = fixture("dismiss", Settings::default());
    apply(&mut f.rt, available("0.10.0", InstallMethod::Deb));
    assert_eq!(banner_count(&f.rt), 1);
    f.rt.update(|s, _| s.dismiss_update());
    frames(&mut f.rt, 2);
    assert_eq!(banner_count(&f.rt), 0, "Dismiss clears the banner");
    assert_eq!(saved(&f).skip_version.as_deref(), Some("0.10.0"));

    // The opt-out survives a fresh load from disk.
    let reloaded = Settings::load_from(&f.config);
    assert_eq!(reloaded.skip_version.as_deref(), Some("0.10.0"));
}

#[test]
fn a_seen_version_stays_hidden_after_a_reload_until_a_newer_release() {
    let mut first = fixture("seen-first", Settings::default());
    apply(&mut first.rt, available("0.10.0", InstallMethod::Tarball));
    assert_eq!(banner_count(&first.rt), 1);

    // A fresh load carries last_seen_version; the same release is not offered
    // again, but last_seen_version still records it.
    let reloaded = Settings::load_from(&first.config);
    assert_eq!(reloaded.last_seen_version.as_deref(), Some("0.10.0"));
    let mut second = fixture("seen-second", reloaded);
    apply(&mut second.rt, available("0.10.0", InstallMethod::Tarball));
    assert_eq!(banner_count(&second.rt), 0, "a seen release stays hidden");
    assert_eq!(saved(&second).last_seen_version.as_deref(), Some("0.10.0"));

    // A newer version is offered again and advances the seen version.
    apply(&mut second.rt, available("0.11.0", InstallMethod::Tarball));
    assert_eq!(banner_count(&second.rt), 1, "a newer release draws again");
    assert_eq!(saved(&second).last_seen_version.as_deref(), Some("0.11.0"));
}

#[test]
fn a_disabled_setting_starts_no_worker_and_shows_nothing() {
    let mut f = fixture(
        "disabled",
        Settings {
            check_for_updates: false,
            ..Settings::default()
        },
    );
    f.rt.update(|s, _| s.start_update_check());
    assert!(!f.rt.state.update_checking);
    assert!(
        f.rt.state.update_rx.is_none(),
        "no receiver is created when checks are off"
    );
    assert_eq!(banner_count(&f.rt), 0);
}

#[test]
fn a_check_that_is_not_due_starts_no_worker() {
    let mut f = fixture(
        "not-due",
        Settings {
            update_next_check: Some(u64::MAX),
            ..Settings::default()
        },
    );
    f.rt.update(|s, _| s.start_update_check());
    assert!(!f.rt.state.update_checking);
    assert!(f.rt.state.update_rx.is_none());
}

#[test]
fn turning_checks_off_clears_the_banner_and_persists() {
    let mut f = fixture("toggle-off", Settings::default());
    apply(&mut f.rt, available("0.10.0", InstallMethod::AppImage));
    assert_eq!(banner_count(&f.rt), 1);

    f.rt.update(|s, _| s.set_check_for_updates(false));
    frames(&mut f.rt, 2);
    assert!(!f.rt.state.check_for_updates());
    assert_eq!(
        banner_count(&f.rt),
        0,
        "turning checks off hides the banner"
    );
    assert!(!saved(&f).check_for_updates, "the opt-out is persisted");

    // Turning them back on clears the schedule, so the next launch checks
    // immediately; the toggle itself starts no request.
    f.rt.update(|s, _| s.set_check_for_updates(true));
    let settings = saved(&f);
    assert!(settings.check_for_updates);
    assert_eq!(settings.last_update_check, None);
    assert_eq!(settings.update_next_check, None);
    assert_eq!(settings.update_failures, 0);
}

#[test]
fn the_updates_switch_is_on_the_appearance_tab_and_toggles() {
    // Tall enough that the last Appearance card stays on screen for the click.
    let mut f = fixture_sized("settings-switch", Settings::default(), 1600.0);
    f.rt.update(|s, _| {
        s.open_settings();
        s.set_settings_tab(SettingsTab::Appearance);
    });
    frames(&mut f.rt, 8);

    assert!(
        f.rt.find("settings-check-updates").is_some(),
        "the Updates switch is on the Appearance tab"
    );
    assert!(has_label(&f.rt, t(Key::SettingsAppearanceUpdateTitle)));
    assert!(has_text(&f.rt, t(Key::SettingsAppearanceUpdateOn)));

    let (x, y) =
        f.rt.find("settings-check-updates")
            .expect("the switch is on screen");
    f.rt.pointer_down(x, y);
    f.rt.pointer_up(x, y);
    frames(&mut f.rt, 4);

    assert!(
        !f.rt.state.check_for_updates(),
        "the click turns checks off"
    );
    assert!(has_text(&f.rt, t(Key::SettingsAppearanceUpdateOff)));
    assert!(
        !saved(&f).check_for_updates,
        "the click persists the choice"
    );
}

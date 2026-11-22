//! #254: the self-update UI half — the banner action, phase display, restart
//! notice and failure notice.
//!
//! No test downloads or installs anything. The worker is never started: the
//! tests drive [`State::apply_self_update_message`] directly, exactly as
//! `poll` would after a worker sends a message.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_i18n::{Key, t, update_failed, update_installed};
use muninn_ui::self_update::{Applied, Phase, SelfUpdateMessage};
use muninn_ui::{AvailableUpdate, CheckOutcome, InstallMethod, Settings, State, poll, view};
use rune::{Env, Runtime};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    rt: Runtime<State>,
    _vault: TempVault,
}

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-self-update-ui-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

fn fixture(tag: &str) -> Fixture {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let config = unique_dir(tag);
    let state_home = unique_dir(tag);
    let mut rt = Runtime::new(
        State::with_settings(
            vault.path().to_path_buf(),
            state_home,
            config,
            Settings::default(),
        ),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.frame(0.0);
    Fixture { rt, _vault: vault }
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn drawn(rt: &Runtime<State>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter(|element| !element.exiting)
        .filter_map(|element| element.text)
        .collect()
}

fn has_text(rt: &Runtime<State>, words: &str) -> bool {
    drawn(rt).iter().any(|drawn| drawn == words)
}

fn banner_count(rt: &Runtime<State>) -> usize {
    rt.elements()
        .iter()
        .filter(|element| element.key.as_deref() == Some("update-banner") && !element.exiting)
        .count()
}

fn offered(method: InstallMethod) -> CheckOutcome {
    CheckOutcome::Available(AvailableUpdate {
        version: "0.11.0".to_owned(),
        url: "https://github.com/raythurman2386/muninn/releases/tag/v0.11.0".to_owned(),
        method,
        etag: Some("\"release-2\"".to_owned()),
    })
}

fn show_banner(rt: &mut Runtime<State>, method: InstallMethod) {
    rt.update(|s, _| s.apply_update_outcome(offered(method), 1_700_000_000, 60));
    frames(rt, 2);
}

fn apply_message(rt: &mut Runtime<State>, message: SelfUpdateMessage) {
    rt.update(|s, _| s.apply_self_update_message(message));
    frames(rt, 2);
}

fn applied(version: &str, warning: Option<&str>) -> Applied {
    Applied {
        version: version.to_owned(),
        warning: warning.map(str::to_owned),
        previous: None,
        restart_required: true,
    }
}

#[test]
fn a_finished_install_clears_the_banner_and_reports_the_restart() {
    let mut f = fixture("finished");
    show_banner(&mut f.rt, InstallMethod::Tarball);
    assert_eq!(banner_count(&f.rt), 1);

    apply_message(
        &mut f.rt,
        SelfUpdateMessage::Finished(Ok(applied("0.11.0", None))),
    );

    let notice = update_installed("0.11.0");
    assert_eq!(banner_count(&f.rt), 0, "the banner is cleared");
    assert_eq!(f.rt.state.notice.as_deref(), Some(notice.as_str()));
    assert!(has_text(&f.rt, &notice), "the restart notice is drawn");
    assert!(!f.rt.state.self_update_running());
    assert_eq!(f.rt.state.self_update_phase(), None);
    assert!(f.rt.state.self_update_rx.is_none());
    assert!(f.rt.state.error.is_none(), "success is not an error bar");
}

#[test]
fn a_warning_from_the_engine_is_appended_to_the_restart_notice() {
    let mut f = fixture("warning");
    show_banner(&mut f.rt, InstallMethod::AppImage);

    apply_message(
        &mut f.rt,
        SelfUpdateMessage::Finished(Ok(applied("0.11.0", Some("cosign was not found")))),
    );

    let expected = format!("{} cosign was not found", update_installed("0.11.0"));
    assert_eq!(banner_count(&f.rt), 0);
    assert_eq!(f.rt.state.notice.as_deref(), Some(expected.as_str()));
    assert!(has_text(&f.rt, &expected));
}

#[test]
fn a_failed_install_keeps_the_banner_and_reports_the_failure() {
    let mut f = fixture("failed");
    show_banner(&mut f.rt, InstallMethod::Tarball);
    assert_eq!(banner_count(&f.rt), 1);

    apply_message(
        &mut f.rt,
        SelfUpdateMessage::Finished(Err("SHA-256 mismatch for the release archive".to_owned())),
    );

    let notice = update_failed("SHA-256 mismatch for the release archive");
    assert_eq!(banner_count(&f.rt), 1, "the banner stays for a retry");
    assert_eq!(f.rt.state.notice.as_deref(), Some(notice.as_str()));
    assert!(has_text(&f.rt, &notice));
    assert!(!f.rt.state.self_update_running());
    assert_eq!(f.rt.state.self_update_phase(), None);
    assert!(f.rt.state.self_update_rx.is_none());
}

#[test]
fn phases_show_progress_and_hide_the_banner_actions_while_running() {
    let mut f = fixture("phases");
    show_banner(&mut f.rt, InstallMethod::Tarball);
    // The action itself is Unix-only, so check it starts present only there;
    // the phase display below is platform-independent.
    if cfg!(unix) {
        assert!(f.rt.find("update-now").is_some(), "Update starts present");
    }

    apply_message(&mut f.rt, SelfUpdateMessage::Phase(Phase::Downloading));
    assert!(f.rt.state.self_update_running());
    assert_eq!(f.rt.state.self_update_phase(), Some(Phase::Downloading));
    assert!(has_text(&f.rt, t(Key::UpdateDownloading)));
    assert!(
        f.rt.find("update-now").is_none(),
        "Update is hidden while running"
    );
    assert!(
        !has_text(&f.rt, t(Key::UpdateReleaseNotes)),
        "Release notes is hidden while running"
    );
    assert!(
        !has_text(&f.rt, t(Key::UpdateDismiss)),
        "Dismiss is hidden while running"
    );

    apply_message(&mut f.rt, SelfUpdateMessage::Phase(Phase::Verifying));
    assert!(has_text(&f.rt, t(Key::UpdateVerifying)));
    assert!(!has_text(&f.rt, t(Key::UpdateDownloading)));

    apply_message(&mut f.rt, SelfUpdateMessage::Phase(Phase::Extracting));
    assert!(has_text(&f.rt, t(Key::UpdateExtracting)));

    apply_message(&mut f.rt, SelfUpdateMessage::Phase(Phase::Installing));
    assert!(has_text(&f.rt, t(Key::UpdateInstalling)));
}

#[test]
fn the_update_action_is_offered_only_for_tarball_and_appimage() {
    if !cfg!(unix) {
        return;
    }
    for method in [InstallMethod::Tarball, InstallMethod::AppImage] {
        let mut f = fixture("supported");
        show_banner(&mut f.rt, method);
        assert_eq!(banner_count(&f.rt), 1, "{method:?}");
        assert!(
            f.rt.find("update-now").is_some(),
            "{method:?} offers Update"
        );
        assert!(has_text(&f.rt, t(Key::UpdateNow)), "{method:?}");
    }

    for method in [
        InstallMethod::Deb,
        InstallMethod::Aur,
        InstallMethod::Unknown,
    ] {
        let mut f = fixture("unsupported");
        show_banner(&mut f.rt, method);
        assert_eq!(banner_count(&f.rt), 1, "{method:?}");
        assert!(
            f.rt.find("update-now").is_none(),
            "{method:?} keeps the package-manager hint"
        );
        assert!(!has_text(&f.rt, t(Key::UpdateNow)), "{method:?}");
    }
}

#[test]
fn start_self_update_on_an_unsupported_install_creates_no_worker() {
    for method in [
        InstallMethod::Deb,
        InstallMethod::Aur,
        InstallMethod::Unknown,
    ] {
        let mut f = fixture("no-worker");
        show_banner(&mut f.rt, method);
        f.rt.update(|s, _| s.start_self_update());
        assert!(!f.rt.state.self_update_running, "{method:?}");
        assert_eq!(f.rt.state.self_update_phase(), None, "{method:?}");
        assert!(
            f.rt.state.self_update_rx.is_none(),
            "{method:?} creates no receiver"
        );
        assert_eq!(banner_count(&f.rt), 1, "{method:?} keeps its banner");
    }
}

#[test]
fn the_state_constructors_start_no_update_worker() {
    let mut f = fixture("offline");
    assert!(!f.rt.state.self_update_running());
    assert_eq!(f.rt.state.self_update_phase(), None);
    assert!(f.rt.state.self_update_rx.is_none());
    assert!(!f.rt.state.self_update_supported(), "no release is offered");

    // With no offered release the action is a no-op, not a worker.
    f.rt.update(|s, _| s.start_self_update());
    assert!(!f.rt.state.self_update_running);
    assert!(f.rt.state.self_update_rx.is_none());
}

#[test]
fn the_catalog_carries_the_action_phase_and_notice_copy() {
    assert_eq!(t(Key::UpdateNow), "Update");
    assert_eq!(t(Key::UpdateDownloading), "Downloading…");
    assert_eq!(t(Key::UpdateVerifying), "Verifying…");
    assert_eq!(t(Key::UpdateExtracting), "Extracting…");
    assert_eq!(t(Key::UpdateInstalling), "Installing…");
    assert_eq!(
        update_installed("0.11.0"),
        "Muninn 0.11.0 is installed. Restart Muninn to use it."
    );
    assert_eq!(update_failed("bad checksum"), "Update failed: bad checksum");
}

//! Lane A: large-vault startup scans in bounded chunks, with Escape to cancel.

use muninn_core::Vault;
use muninn_core::testing::{TempVault, big_vault};
use muninn_ui::{State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime, Tx};

fn runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state")),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

#[test]
fn chunked_startup_reads_the_same_vault_as_a_full_load() {
    let tmp = big_vault(120);
    let mut state =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    assert!(state.loading(), "120 notes must take the chunked path");
    assert_eq!(state.load_progress(), Some((0, 120)));
    assert!(state.vault.notes.is_empty(), "no partial vault is exposed");

    let mut tx = Tx::default();
    let mut t = 0.0;
    let mut ticks = 0;
    while state.loading() {
        t += 1.0 / 60.0;
        assert!(
            poll(&mut state, t, &mut tx),
            "progress must report a change"
        );
        if state.loading() {
            assert!(
                state.vault.notes.is_empty(),
                "partial index leaked mid-scan"
            );
        }
        ticks += 1;
        assert!(ticks < 100, "chunked scan did not finish");
    }
    assert!(ticks >= 3, "120 files at 50/tick should take a few ticks");

    let full = Vault::load(tmp.path()).expect("full load");
    assert!(!state.loading());
    assert_eq!(state.load_progress(), None);
    assert_eq!(state.vault.notes.len(), full.notes.len());
    assert_eq!(state.vault.dirs, full.dirs);
    for (a, b) in full.notes.iter().zip(&state.vault.notes) {
        assert_eq!(a.rel, b.rel);
        assert_eq!(a.source, b.source);
        assert_eq!(a.links, b.links);
        assert_eq!(a.backlinks, b.backlinks);
    }
    // The first note opens once the index exists, and the editor matches it.
    assert_eq!(state.selected, Some(0));
    assert!(state.editor.text.starts_with("# Note 0000"));
    assert!(!poll(&mut state, t + 1.0, &mut tx), "loaded state is quiet");
}

#[test]
fn escape_cancels_a_chunked_scan_and_retry_recovers() {
    let tmp = big_vault(120);
    let mut rt = runtime(&tmp);
    assert!(rt.state.loading());

    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);

    assert!(!rt.state.loading(), "Escape must cancel the scan");
    assert!(
        rt.state.startup_error.is_some(),
        "cancel leaves the startup error with Retry"
    );
    assert!(
        rt.state.vault.notes.is_empty(),
        "cancel must not install a partial vault"
    );
    assert_eq!(rt.state.selected, None);

    // Retry starts a fresh chunked scan; polling finishes it.
    assert!(rt.state.retry_startup());
    let mut tx = Tx::default();
    let mut t = 0.0;
    let mut ticks = 0;
    while rt.state.loading() {
        t += 1.0 / 60.0;
        poll(&mut rt.state, t, &mut tx);
        ticks += 1;
        assert!(ticks < 100);
    }
    assert!(rt.state.startup_error.is_none());
    assert_eq!(rt.state.vault.notes.len(), 120);
    assert_eq!(rt.state.selected, Some(0));
}

#[test]
fn a_small_vault_still_loads_synchronously() {
    let tmp = TempVault::new(&[("Alpha.md", "[[Beta]]"), ("Beta.md", "[[Alpha]]")]);
    let state =
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"));
    assert!(
        !state.loading(),
        "the fast path must not show a progress bar"
    );
    assert_eq!(state.vault.notes.len(), 2);
    assert_eq!(state.selected, Some(0));
}

//! Status bar, recovery banner, Broken-links, top bar, Help-trash and tray copy
//! extraction guard (#158): these surfaces draw their static text from the
//! `muninn-i18n` catalog.
//!
//! The catalog pins the English (`en_us.rs`); these tests pin that the drawn
//! surfaces really read it, so a string that quietly goes back to a literal is
//! a failure here. The exact English is asserted on both sides: the key
//! resolves to it, and the element carries it (as text or as its accessible
//! name). The parameterised sentences are typed helpers, not keys, so their
//! exact English is asserted directly.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_i18n::{
    Key, deleted, files_skipped, more_edits, recovered_copies, recovered_for, referrers, scan_ms,
    status_counts, sync_status, t, targets, trash_empty, trash_summary,
};
use muninn_ui::{
    Mode, RecoveryEntry, RecoverySlot, Settings, State, TRAY_SYNC_STATUS, on_key, poll, tray_menu,
    view,
};
use rune::{Env, Runtime, TrayItem};

const SPRINT: &str = include_str!("../../muninn-core/tests/fixtures/kanban/lanes-and-limits.md");

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-status-copy-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    muninn_core::testing::remove_on_thread_exit(&dir);
    dir
}

/// A headless app over `vault`.
fn runtime(tag: &str, vault: &TempVault) -> Runtime<State> {
    let config = unique_dir(tag);
    let state_home = unique_dir(tag);
    let state = State::with_settings(
        vault.path().to_path_buf(),
        state_home,
        config.clone(),
        Settings::load_from(&config),
    );
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

/// The texts the frame draws, in draw order.
fn drawn(rt: &Runtime<State>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter_map(|element| element.text)
        .collect()
}

fn has_text(rt: &Runtime<State>, words: &str) -> bool {
    drawn(rt).iter().any(|drawn| drawn == words)
}

/// Whether some element is named `words` for assistive technology.
fn has_label(rt: &Runtime<State>, words: &str) -> bool {
    rt.elements()
        .iter()
        .any(|element| element.label.as_deref() == Some(words))
}

fn status_text(rt: &Runtime<State>) -> String {
    rt.elements()
        .into_iter()
        .filter(|element| element.key.as_deref() == Some("status-text") && !element.exiting)
        .filter_map(|element| element.text)
        .next_back()
        .expect("the status bar draws its state text")
}

#[test]
fn the_status_bar_draws_the_catalog_states() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut rt = runtime("status-states", &vault);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.update(|s, _| s.load_note(alpha, false));
    rt.frame(1.0 / 60.0);

    // A clean buffer with no save yet is the calm autosave line.
    assert_eq!(t(Key::StatusAutosaveOn), "Autosave on");
    assert_eq!(status_text(&rt), t(Key::StatusAutosaveOn));

    // A dirty buffer says Saving… while it waits for the debounce.
    rt.update(|s, _| s.editor.insert("x"));
    rt.frame(1.0 / 60.0);
    assert_eq!(t(Key::StatusSaving), "Saving…");
    assert_eq!(status_text(&rt), t(Key::StatusSaving));
    assert!(rt.find("status-state-warning").is_some());

    // A completed write settles on Saved.
    assert!(rt.state.save_now());
    rt.invalidate();
    frames(&mut rt, 4);
    assert_eq!(t(Key::StatusSaved), "Saved");
    assert_eq!(status_text(&rt), t(Key::StatusSaved));
    assert!(rt.find("status-state-saved").is_some());

    // The right-hand counts come from the helper.
    assert!(has_text(&rt, &status_counts(rt.state.words(), 0, 0)));
}

#[test]
fn the_status_bar_draws_deleted_undo_and_skipped_counts() {
    // A skipped file is counted from the first scan.
    let vault = TempVault::new(&[("Good.md", "# Good\n")]);
    fs::write(vault.path().join("Broken.md"), [0xff, 0xfe, 0x00]).expect("invalid utf8");
    let rt = runtime("status-skipped", &vault);
    assert_eq!(rt.state.vault.skipped_count(), 1);
    let skipped = files_skipped(1);
    assert_eq!(skipped, "1 file skipped");
    assert!(
        has_text(&rt, &skipped),
        "the skipped-file count is drawn from the catalog"
    );

    // Deleting a note offers its undo line and button.
    let vault = TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
        ("Beta.md", "# Beta\n"),
    ]);
    let mut rt = runtime("status-deleted", &vault);
    let beta = rt.state.vault.resolve("Beta").expect("Beta");
    rt.update(|s, _| {
        s.delete_note(beta);
        s.confirm_delete();
    });
    frames(&mut rt, 10);

    let label = rt
        .state
        .trash_undo
        .as_ref()
        .map(|undo| undo.label.clone())
        .expect("the deletion is undoable");
    assert_eq!(label, "Beta");
    assert!(
        has_text(&rt, &deleted(&label)),
        "the Deleted line is a catalog sentence"
    );
    assert_eq!(t(Key::StatusUndo), "Undo");
    assert!(
        has_text(&rt, t(Key::StatusUndo)),
        "the Undo button is drawn from the catalog"
    );
    assert!(rt.find("trash-undo").is_some());
}

#[test]
fn the_help_trash_card_draws_the_catalog_copy() {
    // An empty trash: the fixed sentence, the action and the explainer.
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut rt = runtime("trash-empty", &vault);
    rt.update(|s, _| s.open_help());
    frames(&mut rt, 10);

    assert!(rt.find("help-trash").is_some(), "the trash card is present");
    assert_eq!(trash_empty(), "Trash is empty");
    assert_eq!(t(Key::TrashTitle), "Trash");
    assert_eq!(t(Key::TrashEmptyAction), "Empty trash");
    for words in [
        trash_empty(),
        t(Key::TrashTitle),
        t(Key::TrashEmptyAction),
        t(Key::TrashExplainer),
    ] {
        assert!(has_text(&rt, words), "the empty trash card draws {words:?}");
    }
    assert!(
        has_label(&rt, t(Key::TrashEmptyAction)),
        "the Empty trash button is named from the catalog"
    );

    // One deleted note: the counted readout keeps the catalog's shape.
    let vault = TempVault::new(&[("Alpha.md", "alpha"), ("Beta.md", "beta")]);
    let mut rt = runtime("trash-one", &vault);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.update(|s, _| {
        s.delete_note(alpha);
        s.confirm_delete();
        s.open_help();
    });
    frames(&mut rt, 10);

    let summary = drawn(&rt)
        .into_iter()
        .find(|words| words.starts_with("Trash · 1 note · "))
        .expect("the non-empty trash readout is drawn");
    let size = summary
        .strip_prefix("Trash · 1 note · ")
        .expect("the readout has the catalog prefix");
    assert!(!size.is_empty(), "the readout names the size");
    assert_eq!(summary, trash_summary(1, size));
    assert_eq!(summary, trash_summary(rt.state.trash_size().0, size));
}

#[test]
fn the_recovery_banner_and_broken_links_draw_the_catalog_copy() {
    // Four recovered copies: the count phrase, three rows and the overflow chip.
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut rt = runtime("recovery", &vault);
    let recovered: Vec<RecoveryEntry> = (0..4)
        .map(|index| RecoveryEntry {
            rel: format!("Note{index}.md"),
            ts: index as u64,
            slot: RecoverySlot::Editor,
        })
        .collect();
    rt.update(|s, _| s.recovered = recovered);
    frames(&mut rt, 10);

    assert_eq!(recovered_copies(4, 0), "4 notes");
    assert_eq!(more_edits(1), "+1 more");
    assert_eq!(t(Key::RecoveryRestore), "Restore");
    assert_eq!(t(Key::RecoveryRestoreAll), "Restore all");
    assert_eq!(t(Key::RecoveryDismiss), "Dismiss");
    for words in [
        recovered_for(&recovered_copies(4, 0)),
        more_edits(1),
        t(Key::RecoveryRestore).to_owned(),
        t(Key::RecoveryRestoreAll).to_owned(),
        t(Key::RecoveryDismiss).to_owned(),
    ] {
        assert!(has_text(&rt, &words), "the recovery banner draws {words:?}");
    }

    // A vault with one unresolved link: the panel's title, counts and Close.
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n\nSee [[Missing]].\n")]);
    let mut rt = runtime("broken-links", &vault);
    rt.update(|s, _| s.open_broken_links());
    frames(&mut rt, 10);

    assert_eq!(t(Key::BrokenTitle), "Broken links");
    assert_eq!(targets(1), "1 target");
    assert_eq!(referrers(1), "1 referrer");
    assert_eq!(t(Key::Close), "Close");
    for words in [
        t(Key::BrokenTitle).to_owned(),
        targets(1),
        referrers(1),
        t(Key::Close).to_owned(),
    ] {
        assert!(
            has_text(&rt, &words),
            "the Broken-links panel draws {words:?}"
        );
    }

    // A clean vault says so instead.
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut rt = runtime("broken-empty", &vault);
    rt.update(|s, _| s.open_broken_links());
    frames(&mut rt, 10);
    assert_eq!(t(Key::BrokenAllResolve), "All links resolve");
    assert!(has_text(&rt, t(Key::BrokenAllResolve)));
    assert!(has_text(&rt, &targets(0)));
}

#[test]
fn the_topbar_draws_its_catalog_copy() {
    // No notes: the title says so.
    let vault = TempVault::new(&[]);
    let mut rt = runtime("topbar-empty", &vault);
    rt.update(|s, _| s.set_mode(Mode::Editor));
    frames(&mut rt, 4);
    assert_eq!(t(Key::TopbarNoNoteSelected), "No note selected");
    assert!(
        has_text(&rt, t(Key::TopbarNoNoteSelected)),
        "the top bar draws the no-note title"
    );

    // A kanban note opened as a note offers its board, named from the catalog.
    let vault = TempVault::new(&[
        ("Agenda.md", "# Agenda\n\nSee [[Sprint]].\n"),
        ("Plans/Sprint.md", SPRINT),
    ]);
    let mut rt = runtime("topbar-kanban", &vault);
    let sprint = rt.state.vault.resolve("Sprint").expect("Sprint");
    rt.update(|s, _| s.load_note(sprint, false));
    frames(&mut rt, 4);
    assert_eq!(t(Key::TopbarOpenAsKanban), "Open as kanban");
    assert!(
        has_label(&rt, t(Key::TopbarOpenAsKanban)),
        "the Open as kanban button is named from the catalog"
    );
    assert!(has_text(&rt, t(Key::TopbarOpenAsKanban)));
}

#[test]
fn the_tray_menu_draws_the_catalog_copy() {
    assert_eq!(t(Key::TrayShow), "Show Muninn");
    assert_eq!(t(Key::TrayNewNote), "New note");
    assert_eq!(t(Key::TraySyncNow), "Sync now");
    assert_eq!(t(Key::TrayQuit), "Quit Muninn");
    assert_eq!(t(Key::TrayNotConnected), "Not connected.");
    assert_eq!(t(Key::TrayTooltip), "Muninn");

    let menu = tray_menu("Not connected.", false);
    let labels: Vec<(&str, &str)> = menu
        .iter()
        .filter_map(|item| match item {
            TrayItem::Action { id, label, .. } => Some((id.as_str(), label.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        labels,
        vec![
            ("show", t(Key::TrayShow)),
            ("new-note", t(Key::TrayNewNote)),
            ("sync-now", t(Key::TraySyncNow)),
            (TRAY_SYNC_STATUS, "Sync: Not connected."),
            ("quit", t(Key::TrayQuit)),
        ]
    );
    assert_eq!(
        sync_status(t(Key::TrayNotConnected)),
        "Sync: Not connected."
    );
}

#[test]
fn the_parameterised_sentences_keep_their_english() {
    // Names, counts, sizes and durations are data inside a fixed sentence, so
    // these are typed helpers rather than keys.
    assert_eq!(deleted("Plan.md"), "Deleted Plan.md");
    assert_eq!(scan_ms(12.4), "scan 12 ms");
    assert_eq!(
        recovered_for("1 note"),
        "Recovered unsaved edits for 1 note"
    );
    assert_eq!(recovered_copies(1, 0), "1 note");
    assert_eq!(recovered_copies(0, 2), "2 boards");
    assert_eq!(recovered_copies(1, 2), "1 note and 2 boards");
    assert_eq!(more_edits(2), "+2 more");
    assert_eq!(targets(1), "1 target");
    assert_eq!(referrers(2), "2 referrers");
    assert_eq!(status_counts(12, 2, 0), "12 words · 2 links · 0 backlinks");
    assert_eq!(trash_empty(), "Trash is empty");
    assert_eq!(trash_summary(1, "2.0 KB"), "Trash · 1 note · 2.0 KB");
    assert_eq!(trash_summary(3, "1.4 MB"), "Trash · 3 notes · 1.4 MB");
    assert_eq!(sync_status("Not connected."), "Sync: Not connected.");

    // Every key moved in this slice resolves to the byte-identical English.
    assert_eq!(t(Key::StatusSaving), "Saving…");
    assert_eq!(t(Key::StatusSaved), "Saved");
    assert_eq!(t(Key::StatusAutosaveOn), "Autosave on");
    assert_eq!(t(Key::StatusRetry), "Retry");
    assert_eq!(t(Key::StatusUndo), "Undo");
    assert_eq!(t(Key::RecoveryWriteEdits), "— Write edits");
    assert_eq!(t(Key::RecoveryKanbanEdits), "— Kanban edits");
    assert_eq!(
        t(Key::TrashExplainer),
        "Deleted notes stay here until undo or permanent removal. At most 500 files or 100 MB are kept; the oldest timestamp folder is pruned first."
    );
}

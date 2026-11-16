//! Status-bar errors, notices and conflict bars read their copy from the
//! `muninn-i18n` catalog (#158).
//!
//! The catalog pins the English (`en_us.rs`, `lib.rs`); these tests pin that
//! the drawn surfaces really read it, so a sentence that quietly goes back to
//! a literal is a failure here. Static copy is keyed, so the test asserts on
//! both sides: the key resolves to the English, and the status bar draws it.
//! Parameterised sentences are typed helpers, so their exact English is
//! asserted directly and the drawn line is compared with the helper's output.
//!
//! Both conflict bars are covered: the note conflict that replaces the status
//! line (`view.rs`), and the page header a board or kanban uses when its file
//! changed on disk (`doc_sync.rs`).

use std::path::PathBuf;

use muninn_core::testing::TempVault;
use muninn_i18n::{
    Key, board_quit_refused, could_not_be_read, could_not_open_vault, could_not_save,
    import_notice, links_not_updated, refusing_unsafe_recovery_path, scan_cancelled, t,
};
use muninn_ui::{ConflictReason, ConflictState, DocConflict, Mode, State, view};
use rune::{ElementInfo, Env, Runtime};

const DRAW: &str = include_str!("../../muninn-core/tests/fixtures/board/draw.excalidraw");

/// A headless app plus the temp homes it must outlive.
struct App {
    rt: Runtime<State>,
    _state: TempVault,
}

fn runtime(vault: &TempVault) -> App {
    let state_home = TempVault::new(&[]);
    let state = State::with_state_home(vault.path().to_path_buf(), state_home.path().to_path_buf());
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.frame(0.0);
    App {
        rt,
        _state: state_home,
    }
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

fn element(rt: &Runtime<State>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .rev()
        .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
        .unwrap_or_else(|| panic!("the frame draws {key}"))
}

fn status_text(rt: &Runtime<State>) -> String {
    element(rt, "status-text").text.expect("status text")
}

#[test]
fn the_error_and_conflict_keys_keep_their_english() {
    for (key, words) in [
        (Key::ErrorEnterVaultPath, "Enter a vault folder path"),
        (
            Key::ErrorEnterNewVaultPath,
            "Enter a folder path for the new vault",
        ),
        (
            Key::ErrorSaveNoteBeforeSwitch,
            "Save or resolve the current note before switching vaults",
        ),
        (
            Key::ErrorSaveBoardBeforeSwitch,
            "Save the current board before switching vaults",
        ),
        (
            Key::ErrorSaveKanbanBeforeSwitch,
            "Save the current kanban before switching vaults",
        ),
        (
            Key::ErrorSaveNoteBeforeRestore,
            "Save or discard the current note before restoring another",
        ),
        (
            Key::ErrorSaveBoardBeforeRestore,
            "Save or discard the current board before restoring another",
        ),
        (
            Key::ErrorSaveBoardBeforeRescan,
            "Save the current board before rescanning the vault",
        ),
        (
            Key::ErrorOpenerHintUnix,
            "Install xdg-utils (for xdg-open) or open the file manually.",
        ),
        (
            Key::ErrorOpenerHintMacos,
            "The system `open` command failed; open the file from Finder.",
        ),
        (
            Key::ErrorOpenerHintWindows,
            "The system opener failed; open the file from Explorer.",
        ),
        (Key::ErrorOpenerHintOther, "Open the file manually."),
        (Key::ErrorDocumentBoard, "board"),
        (Key::ErrorDocumentKanban, "kanban"),
        (Key::StatusTrashEmptied, "Trash emptied"),
        (Key::StatusLinkCopied, "Copied a link to the card"),
        (Key::ConflictExternalChange, "External change detected"),
        (Key::ConflictFileDeleted, "file deleted externally"),
        (Key::ConflictChangedOnDisk, "Changed on disk by another app"),
        (Key::ConflictMissingOnDisk, "Removed or renamed on disk"),
        (Key::ConflictKeepMine, "Keep mine"),
        (Key::ConflictLoadTheirs, "Load theirs"),
        (Key::ConflictSaveCopy, "Save copy"),
        (Key::ConflictDiscard, "Discard"),
    ] {
        assert_eq!(t(key), words, "{key:?} keeps its English");
    }
}

#[test]
fn the_parameterised_error_helpers_keep_their_english() {
    assert_eq!(
        could_not_save("disk is full"),
        "Could not save: disk is full"
    );
    assert_eq!(
        could_not_open_vault("permission denied"),
        "Could not open the vault: permission denied"
    );
    assert_eq!(
        links_not_updated(Some("Board")),
        "Links in Board were not updated"
    );
    assert_eq!(
        links_not_updated(None),
        "Links in the kanban were not updated"
    );
    assert_eq!(
        could_not_be_read(t(Key::ErrorDocumentKanban)),
        "the kanban could not be read"
    );
    assert_eq!(
        scan_cancelled(12, 40),
        "Scan cancelled after 12 of 40 notes"
    );
    assert_eq!(
        refusing_unsafe_recovery_path("../x.md"),
        "Refusing unsafe recovery path ../x.md"
    );
    assert_eq!(
        board_quit_refused("disk full"),
        "Board not saved and no recovery copy could be written: disk full. Use Save copy or fix the folder, then quit again."
    );
    assert_eq!(
        import_notice(2, 1, true),
        "Imported 2 files · 1 note · non-Markdown files stay attachments"
    );
    assert_eq!(import_notice(1, 1, false), "Imported 1 file · 1 note");
}

#[test]
fn an_empty_vault_path_draws_the_catalog_prompt() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut app = runtime(&vault);

    app.rt.update(|s, _| {
        assert!(!s.open_vault("   "), "an empty path is refused");
    });
    frames(&mut app.rt, 2);

    assert_eq!(t(Key::ErrorEnterVaultPath), "Enter a vault folder path");
    assert_eq!(
        app.rt.state.error.as_deref(),
        Some(t(Key::ErrorEnterVaultPath))
    );
    assert_eq!(status_text(&app.rt), t(Key::ErrorEnterVaultPath));
    assert!(app.rt.find("status-state-danger").is_some());
}

#[test]
fn a_parameterised_error_is_drawn_from_its_helper() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut app = runtime(&vault);

    let error = could_not_save("disk is full");
    app.rt.update(|s, _| s.error = Some(error.clone()));
    frames(&mut app.rt, 2);

    assert_eq!(status_text(&app.rt), error);
    assert_eq!(
        status_text(&app.rt),
        "Could not save: disk is full",
        "the drawn line is the helper's exact English"
    );
}

#[test]
fn the_note_conflict_bar_draws_the_catalog_copy() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let mut app = runtime(&vault);
    let alpha = app.rt.state.vault.resolve("Alpha").expect("Alpha");

    app.rt.update(|s, _| {
        s.load_note(alpha, false);
        s.conflict = Some(ConflictState {
            note_id: alpha,
            expected: None,
            observed: None,
            reason: ConflictReason::Modified,
        });
    });
    frames(&mut app.rt, 2);

    assert_eq!(
        element(&app.rt, "conflict-text").text.as_deref(),
        Some(t(Key::ConflictExternalChange))
    );
    for (key, words) in [
        ("conflict-keep-mine", Key::ConflictKeepMine),
        ("conflict-load-theirs", Key::ConflictLoadTheirs),
        ("conflict-save-copy", Key::ConflictSaveCopy),
    ] {
        assert!(app.rt.find(key).is_some(), "{key} is drawn");
        assert!(has_text(&app.rt, t(words)), "{key} draws {words:?}");
    }

    // A file deleted under the buffer says so, with the same three choices.
    app.rt.update(|s, _| {
        s.conflict.as_mut().expect("conflict").reason = ConflictReason::Deleted;
    });
    frames(&mut app.rt, 2);
    assert_eq!(
        element(&app.rt, "conflict-text").text.as_deref(),
        Some(t(Key::ConflictFileDeleted))
    );
    assert!(has_text(&app.rt, t(Key::ConflictKeepMine)));
}

#[test]
fn the_page_conflict_bar_draws_the_catalog_copy() {
    let vault = TempVault::new(&[("Alpha.md", "# Alpha\n"), ("Draw.excalidraw", DRAW)]);
    let mut app = runtime(&vault);
    let board: PathBuf = vault.path().join("Draw.excalidraw");

    app.rt.update(|s, _| {
        s.open_board_at(board);
        let open = s.board_open.as_mut().expect("the board opens");
        open.conflict = Some(DocConflict::Changed);
    });
    frames(&mut app.rt, 3);
    assert_eq!(app.rt.state.mode, Mode::Board, "the board page is up");

    // The bar is an alert dialog whose sentence names the catalog's words.
    let bar = element(&app.rt, "board-conflict");
    assert_eq!(bar.role, "alertdialog");
    assert_eq!(bar.label.as_deref(), Some(t(Key::ConflictChangedOnDisk)));
    assert_eq!(
        element(&app.rt, "board-conflict-text").text.as_deref(),
        Some(t(Key::ConflictChangedOnDisk))
    );
    for (key, words) in [
        ("board-keep-mine", Key::ConflictKeepMine),
        ("board-save-copy", Key::ConflictSaveCopy),
        ("board-load-theirs", Key::ConflictLoadTheirs),
    ] {
        assert_eq!(
            element(&app.rt, key).label.as_deref(),
            Some(t(words)),
            "{key} is named from the catalog"
        );
    }

    // A board file that vanished offers Discard instead of Load theirs.
    app.rt.update(|s, _| {
        let open = s.board_open.as_mut().expect("open board");
        open.conflict = Some(DocConflict::Missing);
    });
    frames(&mut app.rt, 3);
    assert_eq!(
        element(&app.rt, "board-conflict-text").text.as_deref(),
        Some(t(Key::ConflictMissingOnDisk))
    );
    assert_eq!(
        element(&app.rt, "board-load-theirs").label.as_deref(),
        Some(t(Key::ConflictDiscard))
    );
}

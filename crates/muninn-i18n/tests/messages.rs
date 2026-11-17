//! The message table's contract: one template per key, arities that match the
//! placeholders, English output identical to the `format!` the helpers
//! replaced, and a pseudo render that wraps the **rendered** sentence.
//!
//! Missing English text is a compile error (the `messages` match is
//! exhaustive), so there is no runtime "message has no text" test; the
//! placeholder shape is checked in `src/messages.rs`.

use std::collections::BTreeSet;

use muninn_i18n::{
    Key, Lang, MsgKey, PSEUDO_CLOSE, PSEUDO_OPEN, autosave_help, board_canvas, board_kept_as_is,
    card_label, connected_as, could_not_restore, delete_lane_text, files_skipped, filter_by_tag,
    import_notice, lane_label, lines, link_impact, links_not_updated, more_edits, new_note_in,
    notes, phrase, phrase_current, recovered_copies, recovered_for, relative_date, relative_day,
    scan_ms, set_lang, status_counts, t, theme_label, trash_summary, tray_error, version,
};
/// One message and the English sentence it must render today.
const CASES: &[(MsgKey, &[&str], &str)] = &[
    (MsgKey::FilesSkippedOne, &["1"], "1 file skipped"),
    (MsgKey::FilesSkippedOther, &["0"], "0 files skipped"),
    (MsgKey::NoteOne, &["1"], "1 note"),
    (MsgKey::NoteOther, &["3"], "3 notes"),
    (MsgKey::TargetOne, &["1"], "1 target"),
    (MsgKey::TargetOther, &["2"], "2 targets"),
    (
        MsgKey::StatusCounts,
        &["12 words", "2 links", "0 backlinks"],
        "12 words · 2 links · 0 backlinks",
    ),
    (
        MsgKey::TrashSummary,
        &["1 note", "2.0 KB"],
        "Trash · 1 note · 2.0 KB",
    ),
    (MsgKey::NoLinks, &[], "no links"),
    (
        MsgKey::NoteLinks,
        &["1 link", "2 backlinks"],
        "1 link · 2 backlinks",
    ),
    (MsgKey::NoteLinksRoot, &["1 link"], "vault root · 1 link"),
    (
        MsgKey::NoteLinksFolder,
        &["Projects", "2 links"],
        "Projects · 2 links",
    ),
    (
        MsgKey::SidebarSummary,
        &["12 notes", "3 links"],
        "12 notes · 3 links",
    ),
    (
        MsgKey::GraphLinks,
        &["1 outgoing link", "2 backlinks"],
        "1 outgoing link · 2 backlinks",
    ),
    (
        MsgKey::RecoveredCopies,
        &["1 note", "2 boards"],
        "1 note and 2 boards",
    ),
    (MsgKey::MoreEdits, &["2"], "+2 more"),
    (MsgKey::Deleted, &["Plan.md"], "Deleted Plan.md"),
    (MsgKey::ScanMs, &["12"], "scan 12 ms"),
    (MsgKey::RestoreTo, &["To do"], "Restore to To do"),
    (MsgKey::FilterByTag, &["x"], "Filter by #x"),
    (MsgKey::LinkImpactOne, &["1", "here"], "1 note links here"),
    (MsgKey::LinkImpactOther, &["2", "here"], "2 notes link here"),
    (
        MsgKey::CouldNotSave,
        &["disk is full"],
        "Could not save: disk is full",
    ),
    (
        MsgKey::CouldNotRestore,
        &["Plan.md", "name taken"],
        "Could not restore Plan.md: name taken",
    ),
    (
        MsgKey::CouldNotOpenFile,
        &["Plan.md", "denied", "Install xdg-utils."],
        "Could not open Plan.md: denied. Install xdg-utils.",
    ),
    (
        MsgKey::CouldNotBeRead,
        &["board"],
        "the board could not be read",
    ),
    (
        MsgKey::RefusingUnsafeRecoveryPath,
        &["../x.md"],
        "Refusing unsafe recovery path ../x.md",
    ),
    (
        MsgKey::BoardQuitRefused,
        &["disk full"],
        "Board not saved and no recovery copy could be written: disk full. Use Save copy or fix the folder, then quit again.",
    ),
    (
        MsgKey::ScanCancelled,
        &["12", "40"],
        "Scan cancelled after 12 of 40 notes",
    ),
    (
        MsgKey::ImportUnsupported,
        &["notes.bin"],
        "Could not import notes.bin: this file type cannot be converted.",
    ),
    (
        MsgKey::ImportNeedsOcrOne,
        &["scan.pdf", "2", "9"],
        "Could not import scan.pdf: page 2 of 9 is a scanned image and needs OCR, which Muninn does not do. Export a text version first.",
    ),
    (
        MsgKey::ImportNeedsOcrOther,
        &["scan.pdf", "2, 3", "9"],
        "Could not import scan.pdf: pages 2, 3 of 9 are scanned images and need OCR, which Muninn does not do. Export a text version first.",
    ),
    (
        MsgKey::ImportTooLarge,
        &["big.docx", "51 MB", "50 MB"],
        "Could not import big.docx: the file is 51 MB, over the 50 MB limit.",
    ),
    (MsgKey::RefusedNotAFile, &["a.txt"], "a.txt: not a file"),
    (
        MsgKey::ConnectedAs,
        &["octo"],
        "Connected as octo. The token is in the OS credential store. Sync Now when ready.",
    ),
    (
        MsgKey::LastSync,
        &["a1b2c3d"],
        "Last successful sync a1b2c3d.",
    ),
    (
        MsgKey::SyncStatus,
        &["Not connected."],
        "Sync: Not connected.",
    ),
    (
        MsgKey::SyncExcluded,
        &["2"],
        "2 path(s) excluded by the sync contract.",
    ),
    (
        MsgKey::SyncConflicts,
        &["1", "Plan.md"],
        "1 conflict(s): Plan.md. Nothing was overwritten.",
    ),
    (
        MsgKey::BoardNameTaken,
        &["Plan"],
        "A board named “Plan” is already in this folder",
    ),
    (
        MsgKey::NoNoteNamed,
        &["Plan"],
        "No note named “Plan” in this vault",
    ),
    (
        MsgKey::LinkedNoteHint,
        &["Plan"],
        "Linked note: Plan · ⌘Enter opens",
    ),
    (
        MsgKey::BoardCanvas,
        &["2 elements", "rectangle"],
        "Board canvas, 2 elements, rectangle tool",
    ),
    (
        MsgKey::BoardCanvasSelected,
        &["1 element", "hand", "1"],
        "Board canvas, 1 element, hand tool, 1 selected",
    ),
    (MsgKey::CardLabelEmpty, &["Open"], "Open, empty card"),
    (MsgKey::CardLabel, &["Done", "Ship it"], "Done: Ship it"),
    (
        MsgKey::ThemeLabel,
        &["Dark"],
        "Theme: Dark — click to cycle (System, Dark, Light)",
    ),
    (
        MsgKey::AutosaveHelp,
        &["1.50"],
        "Autosave writes 1.50 s after you stop typing.",
    ),
    (MsgKey::Version, &["0.8.0"], "Version 0.8.0."),
    (
        MsgKey::ShowWithChord,
        &["Ctrl+Alt+M"],
        "Show Muninn with Ctrl+Alt+M",
    ),
    (
        MsgKey::ShortcutFailed,
        &["Ctrl+Alt+M", "taken"],
        "Ctrl+Alt+M could not be registered: taken",
    ),
    (
        MsgKey::ShortcutOn,
        &["Ctrl+Alt+M"],
        "On. Ctrl+Alt+M shows Muninn from any app.",
    ),
    (MsgKey::RelativeToday, &[], "today"),
    (MsgKey::RelativeTomorrow, &[], "tomorrow"),
    (MsgKey::RelativeYesterday, &[], "yesterday"),
    (MsgKey::RelativeInDays, &["3"], "in 3 days"),
    (MsgKey::RelativeDaysAgo, &["2"], "2 days ago"),
    (MsgKey::DayLabel, &["14"], "Day 14"),
    (
        MsgKey::RelativeDate,
        &["2026-11-14", "today"],
        "2026-11-14 · today",
    ),
    (
        MsgKey::NewNoteIn,
        &["the vault root"],
        "New note in the vault root",
    ),
    (MsgKey::NewFolderIn, &["Projects"], "New folder in Projects"),
    (
        MsgKey::WelcomeVaultCreated,
        &["~/notes"],
        "New vault created at ~/notes — make your first note",
    ),
    (
        MsgKey::ScanProgress,
        &["3", "12"],
        "Scanning 3/12 notes — Esc to cancel",
    ),
    (
        MsgKey::FocusHelp,
        &["⌘⇧F"],
        "Hide the chrome and keep the caret's line centred (⌘⇧F)",
    ),
    (
        MsgKey::FocusExitHelp,
        &["⌘⇧F"],
        "Back to the note's chrome (⌘⇧F)",
    ),
    (
        MsgKey::PaletteKanbanFolder,
        &["Projects"],
        "Kanban · Projects",
    ),
    (
        MsgKey::LinksNotUpdated,
        &["the kanban"],
        "Links in the kanban were not updated",
    ),
    (
        MsgKey::RenamedLinksFailed,
        &["1", "Plan.md: denied"],
        "Renamed, but 1 inbound link file(s) could not be rewritten: Plan.md: denied",
    ),
    (
        MsgKey::MovedLinksFailed,
        &["2", "Plan.md: denied"],
        "Moved, but 2 inbound link file(s) could not be rewritten: Plan.md: denied",
    ),
    (
        MsgKey::NoteNotInVault,
        &["7"],
        "Note 7 is no longer in this vault",
    ),
    (
        MsgKey::CannotOpenWithoutNote,
        &["pic.png"],
        "Cannot open pic.png: no note is selected",
    ),
    (
        MsgKey::AttachmentNotInVault,
        &["pic.png"],
        "Attachment pic.png is not in this vault — drop it in, or fix the embed",
    ),
    (MsgKey::BrokenLinksCount, &["3"], "3 broken"),
    (
        MsgKey::NotAFile,
        &["/vault/assets"],
        "/vault/assets is not a file",
    ),
    (
        MsgKey::PathGone,
        &["/vault/assets/pic.png"],
        "/vault/assets/pic.png no longer exists",
    ),
    (
        MsgKey::TrayError,
        &["no dock"],
        "The tray icon could not be added: no dock",
    ),
    (
        MsgKey::LoginItemError,
        &["permission denied"],
        "The login item could not be changed: permission denied",
    ),
    (
        MsgKey::OutsideVault,
        &["/tmp/notes"],
        "/tmp/notes is outside the vault",
    ),
    (
        MsgKey::NotABoardInVault,
        &["Plan.excalidraw"],
        "Plan.excalidraw is not a board inside the vault",
    ),
    (
        MsgKey::AlreadyExists,
        &["Plan.md"],
        "Plan.md already exists",
    ),
    (
        MsgKey::NotATrashPath,
        &["../Plan.md"],
        "../Plan.md is not a trash path",
    ),
    (
        MsgKey::NoLongerInTrash,
        &["1/Plan.md"],
        "1/Plan.md is no longer in the trash",
    ),
    (
        MsgKey::NotAFolder,
        &["Notes.md"],
        "Notes.md is not a folder",
    ),
    (
        MsgKey::BoardNameInvalid,
        &["cannot be empty"],
        "a board name cannot be empty",
    ),
    (
        MsgKey::BoardSceneKeyObject,
        &["appState"],
        "scene appState must be an object",
    ),
    (
        MsgKey::BoardRenamedEmbedsFailed,
        &["1", "a.md"],
        "Renamed, but 1 note(s) embedding it could not be updated: a.md",
    ),
    (
        MsgKey::AutostartRegFailed,
        &["access denied"],
        "reg.exe could not run: access denied",
    ),
    (
        MsgKey::AutostartRegExit,
        &["exit code: 1"],
        "reg.exe exited with exit code: 1",
    ),
    (
        MsgKey::KanbanQuitRefused,
        &["disk full"],
        "Kanban not saved and no recovery copy could be written: disk full. Use Save copy or fix the folder, then quit again.",
    ),
    (
        MsgKey::KanbanCouldNotSaveCopy,
        &["disk full"],
        "could not save a copy: disk full",
    ),
    (
        MsgKey::CouldNotDo,
        &["add the card", "read-only"],
        "Could not add the card: read-only",
    ),
    (
        MsgKey::CouldNotCreateBoard,
        &["denied"],
        "Could not create a board: denied",
    ),
    (
        MsgKey::CouldNotCreateKanban,
        &["denied"],
        "Could not create a kanban: denied",
    ),
    (
        MsgKey::CouldNotConvertKanban,
        &["has text"],
        "Could not convert to a kanban: has text",
    ),
    (
        MsgKey::BoardKeptAsIs,
        &["1 element", "2"],
        "1 element · 2 kept as-is",
    ),
    (
        MsgKey::BoardCouldNotRead,
        &["bad json"],
        "Could not read: bad json",
    ),
    (MsgKey::BoardNoteLinkMissing, &["Plan"], "Plan (missing)"),
    (MsgKey::UnsavedSaves, &["⌘S"], "Unsaved · ⌘S saves"),
    (
        MsgKey::NotSaved,
        &["the file was removed"],
        "Not saved: the file was removed",
    ),
    (MsgKey::LinkDescription, &["Plan"], "Open Plan"),
    (MsgKey::NoteLabelRoot, &["Plan"], "Plan (root)"),
    (
        MsgKey::GraphNodeParent,
        &["Plan", "Projects"],
        "Plan (Projects)",
    ),
    (MsgKey::GraphStats, &["12", "30"], "12 notes · 30 edges"),
    (MsgKey::KanbanFilterFrom, &["2026-11-01"], "from 2026-11-01"),
    (MsgKey::KanbanFilterTo, &["2026-11-30"], "to 2026-11-30"),
    (
        MsgKey::KanbanFilterRange,
        &["2026-11-01", "2026-11-30"],
        "2026-11-01 – 2026-11-30",
    ),
    (MsgKey::LaneActions, &["To do"], "Lane actions: To do"),
    (MsgKey::LaneControls, &["To do"], "To do controls"),
    (MsgKey::CollapseLane, &["To do"], "Collapse lane: To do"),
    (MsgKey::ExpandLane, &["To do"], "Expand lane: To do"),
    (MsgKey::LaneCards, &["To do"], "To do cards"),
    (MsgKey::AddCardToLane, &["To do"], "Add card to To do"),
    (MsgKey::LaneLabel, &["To do", "3"], "To do, 3 cards"),
    (MsgKey::LaneOverLimit, &[], ", over limit"),
    (MsgKey::LaneDone, &[], ", done lane"),
    (
        MsgKey::LaneRegionLabel,
        &["To do", "3"],
        "To do lane, 3 cards",
    ),
    (
        MsgKey::CollapsedLaneLabel,
        &["To do", "3"],
        "To do, 3 cards, collapsed",
    ),
    (MsgKey::CollapsedCards, &["3"], "Collapsed · 3 cards"),
    (
        MsgKey::DeleteLane,
        &["To do", " and its card"],
        "Delete “To do” and its card? Any note text in it stays in the file.",
    ),
    (
        MsgKey::DeleteLaneText,
        &["To do", " and its card", "1 line", "hidden"],
        "Delete “To do” and its card? 1 line of note text the board doesn't show will be deleted too, starting “hidden”.",
    ),
    (MsgKey::DeleteCardsOne, &[], " and its card"),
    (MsgKey::DeleteCardsOther, &["2"], " and its 2 cards"),
    (MsgKey::CardActions, &["Ship it"], "Card actions: Ship it"),
    (MsgKey::CardActionsEmpty, &[], "Card actions: empty card"),
    (
        MsgKey::CardActionsInLane,
        &["To do"],
        "Card actions: empty card in To do",
    ),
    (MsgKey::ChecklistProgress, &["1", "2"], "1 of 2 done"),
    (
        MsgKey::ChecklistLabel,
        &["1", "2"],
        "Checklist, 1 of 2 done",
    ),
    (
        MsgKey::CardLabelInLane,
        &["To do", "Done", "Ship it"],
        "To do · Done: Ship it",
    ),
    (
        MsgKey::DeviceCode,
        &["ABCD-1234", "https://github.com/login/device"],
        "Enter code ABCD-1234 at https://github.com/login/device. Your browser is opening it; Muninn finishes when GitHub approves.",
    ),
    (
        MsgKey::BrowserFailed,
        &["https://example.test"],
        "Could not open a browser. Open this address yourself to continue: https://example.test",
    ),
    (
        MsgKey::SignInFailed,
        &["access denied"],
        "Sign-in did not finish: access denied No token was stored.",
    ),
    (
        MsgKey::TailnetHosting,
        &["8080", "tailscale serve --bg 8080"],
        "Hosting on 127.0.0.1:8080. On this device run: tailscale serve --bg 8080",
    ),
    (
        MsgKey::TailnetHostingReady,
        &["tailscale serve --bg 8080"],
        "Hosting. Run once on this device: tailscale serve --bg 8080. Use Copy key to give the shared key to your other devices.",
    ),
    (
        MsgKey::SharedKeyCopied,
        &["60"],
        "Shared key copied. The clipboard clears in 60 seconds; paste it on the joining device.",
    ),
    (
        MsgKey::SyncPreviousCredential,
        &["permission denied"],
        "The previous connection's credential could not be removed: permission denied",
    ),
    (
        MsgKey::SyncNothingSaved,
        &["the keyring is locked"],
        "the keyring is locked Nothing was saved.",
    ),
    (
        MsgKey::SyncKeyRotationFailed,
        &["the keyring is locked"],
        "the keyring is locked The old key still works.",
    ),
    (
        MsgKey::SyncNowAfterError,
        &["access denied"],
        "access denied Connect again before Sync Now.",
    ),
    (
        MsgKey::SyncOffline,
        &["connection refused"],
        "Offline: connection refused",
    ),
    (
        MsgKey::SyncRateLimitedAfter,
        &["1500"],
        "Rate limited; retry after 1500 ms",
    ),
    (
        MsgKey::SyncProviderError,
        &["500", "internal error"],
        "Provider error 500: internal error",
    ),
    (
        MsgKey::SyncAuthenticationNeeded,
        &["token expired"],
        "Authentication needed: token expired",
    ),
    (
        MsgKey::SyncPermissionDenied,
        &["no access to the repository"],
        "Permission denied: no access to the repository",
    ),
    (
        MsgKey::SyncOauthScopeRejected,
        &["drive.file required"],
        "OAuth scope rejected: drive.file required",
    ),
    (
        MsgKey::SyncRemoteScopeNotFound,
        &["branch main"],
        "Remote scope not found: branch main",
    ),
    (
        MsgKey::SyncRemoteChanged,
        &["commit moved"],
        "Remote changed: commit moved",
    ),
    (
        MsgKey::SyncOutcomeUnknown,
        &["connection dropped"],
        "Outcome unknown, reconcile before retrying: connection dropped",
    ),
    (
        MsgKey::SyncListingIncomplete,
        &["page 3 of 5"],
        "Remote listing incomplete; refusing to treat that as deletion: page 3 of 5",
    ),
    (
        MsgKey::SyncUnsupported,
        &["Data.xlsx", "Workspace document"],
        "Unsupported Data.xlsx: Workspace document",
    ),
    (
        MsgKey::SyncLimitExceeded,
        &["file larger than 100 MB"],
        "Sync limit exceeded: file larger than 100 MB",
    ),
    (
        MsgKey::SyncCredentialsUnavailable,
        &["the keyring is locked"],
        "Credentials unavailable: the keyring is locked",
    ),
    (
        MsgKey::SyncDirtyBuffer,
        &["Plan.md"],
        "Save or discard unsaved edits in Plan.md before sync can change it",
    ),
    (
        MsgKey::SyncLocalChanged,
        &["Plan.md"],
        "Plan.md changed during sync; run Sync Now again to replan",
    ),
    (
        MsgKey::SyncNotConfigured,
        &["missing repository"],
        "Sync is not configured: missing repository",
    ),
    (
        MsgKey::SyncFileError,
        &["read-only folder"],
        "File error: read-only folder",
    ),
    (
        MsgKey::RecoveredFor,
        &["1 note"],
        "Recovered unsaved edits for 1 note",
    ),
    (MsgKey::ImportedFiles, &["2 files"], "Imported 2 files"),
    (MsgKey::ImportJoin, &["1 note"], " · 1 note"),
    (
        MsgKey::ImportAttachmentsNote,
        &[],
        " · non-Markdown files stay attachments",
    ),
];

/// Build `arity` distinct arguments for a key.
fn arguments(key: MsgKey) -> Vec<String> {
    (0..key.arity())
        .map(|index| format!("arg{index}"))
        .collect()
}

#[test]
fn the_message_table_is_not_empty() {
    assert!(!MsgKey::ALL.is_empty(), "the table must seed some messages");
}

#[test]
fn ids_are_unique_and_dotted() {
    let mut seen = BTreeSet::new();
    for key in MsgKey::ALL {
        let id = key.id();
        assert!(
            seen.insert(id),
            "duplicate message id {id:?} (from {key:?})"
        );
        assert!(id.starts_with("msg."), "message id {id:?} is not in msg.*");
        for segment in id.split('.') {
            assert!(
                !segment.is_empty()
                    && segment
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "message id {id:?} (from {key:?}) has a non-snake-case segment"
            );
        }
    }
}

/// Every template is non-empty, substitutes each declared argument exactly
/// once, and leaves no placeholder behind. The raw placeholder shape is
/// checked in `src/messages.rs`; this is the rendered proof through `phrase`.
#[test]
fn every_message_renders_with_its_arguments() {
    for &key in MsgKey::ALL {
        let args = arguments(key);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let rendered = phrase(Lang::EnUs, key, &refs);
        assert!(!rendered.is_empty(), "{} rendered nothing", key.id());
        for arg in &args {
            assert!(
                rendered.contains(arg),
                "{} dropped its argument {arg:?}: {rendered:?}",
                key.id()
            );
        }
        assert!(
            !rendered.contains('{') && !rendered.contains('}'),
            "{} left a placeholder unsubstituted: {rendered:?}",
            key.id()
        );
        assert!(
            rendered.len() > args.iter().map(String::len).sum::<usize>(),
            "{} is only its arguments: {rendered:?}",
            key.id()
        );
    }
}

#[test]
fn english_is_the_sentence_the_helpers_shipped() {
    for &(key, args, expected) in CASES {
        assert_eq!(
            phrase(Lang::EnUs, key, args),
            expected,
            "{} changed in English",
            key.id()
        );
        assert_eq!(
            phrase(Lang::Pseudo, key, args),
            format!("{PSEUDO_OPEN}{expected}{PSEUDO_CLOSE}"),
            "{} is not wrapped around the rendered sentence",
            key.id()
        );
    }
}

#[test]
fn helpers_pin_the_same_english() {
    assert_eq!(files_skipped(1), "1 file skipped");
    assert_eq!(files_skipped(0), "0 files skipped");
    assert_eq!(notes(1), "1 note");
    assert_eq!(notes(2), "2 notes");
    assert_eq!(status_counts(1, 2, 0), "1 word · 2 links · 0 backlinks");
    assert_eq!(status_counts(12, 2, 0), "12 words · 2 links · 0 backlinks");
    assert_eq!(trash_summary(1, "2.0 KB"), "Trash · 1 note · 2.0 KB");
    assert_eq!(more_edits(2), "+2 more");
    assert_eq!(scan_ms(12.4), "scan 12 ms");
    assert_eq!(link_impact(1, "here"), "1 note links here");
    assert_eq!(link_impact(2, "here"), "2 notes link here");
    assert_eq!(muninn_i18n::restore_to("To do"), "Restore to To do");
    assert_eq!(filter_by_tag("x"), "Filter by #x");
    assert_eq!(
        could_not_restore("Plan.md", "name taken"),
        "Could not restore Plan.md: name taken"
    );
    assert_eq!(
        connected_as("octo"),
        "Connected as octo. The token is in the OS credential store. Sync Now when ready."
    );
    assert_eq!(
        board_canvas("2 elements", "rectangle", 0),
        "Board canvas, 2 elements, rectangle tool"
    );
    assert_eq!(card_label("Open", ""), "Open, empty card");
    assert_eq!(
        theme_label("Dark"),
        "Theme: Dark — click to cycle (System, Dark, Light)"
    );
    assert_eq!(
        autosave_help(1.5),
        "Autosave writes 1.50 s after you stop typing."
    );
    assert_eq!(version("0.8.0"), "Version 0.8.0.");
    assert_eq!(relative_day(0), "today");
    assert_eq!(relative_day(-1), "yesterday");
    assert_eq!(relative_day(3), "in 3 days");
    assert_eq!(relative_day(-2), "2 days ago");
    assert_eq!(
        relative_date("2026-11-14", &relative_day(0)),
        "2026-11-14 · today"
    );
    assert_eq!(new_note_in("Projects"), "New note in Projects");
    assert_eq!(
        links_not_updated(None),
        "Links in the kanban were not updated"
    );
    assert_eq!(
        links_not_updated(Some("Board")),
        "Links in Board were not updated"
    );
    assert_eq!(
        tray_error("no dock"),
        "The tray icon could not be added: no dock"
    );
    assert_eq!(
        import_notice(2, 1, true),
        "Imported 2 files · 1 note · non-Markdown files stay attachments"
    );
    assert_eq!(import_notice(1, 1, false), "Imported 1 file · 1 note");
    assert_eq!(import_notice(0, 0, false), "Imported 0 files");
    assert_eq!(
        recovered_for(&recovered_copies(1, 0)),
        "Recovered unsaved edits for 1 note"
    );
    assert_eq!(
        lane_label("Done", "2", true, true),
        "Done, 2 cards, over limit, done lane"
    );
    assert_eq!(
        delete_lane_text("To do", 1, &lines(1), "hidden"),
        "Delete “To do” and its card? 1 line of note text the board doesn't show will be deleted too, starting “hidden”."
    );
    assert_eq!(board_kept_as_is("1 element", 2), "1 element · 2 kept as-is");
}

/// Puts this thread in [`Lang::Pseudo`] and back to [`Lang::EnUs`] on drop,
/// including when a test panics. The language is thread-local.
struct PseudoLanguage;

impl Drop for PseudoLanguage {
    fn drop(&mut self) {
        set_lang(Lang::EnUs);
    }
}

/// The plural branch renders through the same seam: each branch is its own
/// template, and the pseudo wrapper goes around the **rendered** phrase.
#[test]
fn pseudo_wraps_a_plural_helper() {
    set_lang(Lang::Pseudo);
    let _language = PseudoLanguage;
    assert_eq!(notes(1), "⟦1 note⟧");
    assert_eq!(notes(3), "⟦3 notes⟧");
    assert_eq!(files_skipped(2), "⟦2 files skipped⟧");
    assert_eq!(
        phrase_current(MsgKey::Deleted, &["Plan.md"]),
        "⟦Deleted Plan.md⟧"
    );
}

/// A sentence assembled from fragments is bracketed **once**: the pseudo
/// marker goes around the joined line, not around each count and label.
#[test]
fn composed_sentences_are_bracketed_once() {
    set_lang(Lang::Pseudo);
    let _language = PseudoLanguage;
    assert_eq!(
        import_notice(2, 1, true),
        "⟦Imported 2 files · 1 note · non-Markdown files stay attachments⟧"
    );
    assert_eq!(import_notice(1, 0, false), "⟦Imported 1 file⟧");
    assert_eq!(
        recovered_for(&recovered_copies(1, 2)),
        "⟦Recovered unsaved edits for 1 note and 2 boards⟧"
    );
    assert_eq!(recovered_copies(1, 2), "⟦1 note and 2 boards⟧");
    assert_eq!(
        lane_label("To do", "3", true, true),
        "⟦To do, 3 cards, over limit, done lane⟧"
    );
    assert_eq!(
        relative_date("2026-11-14", &relative_day(0)),
        "⟦2026-11-14 · today⟧"
    );
    assert_eq!(
        delete_lane_text("To do", 1, &lines(1), "hidden"),
        "⟦Delete “To do” and its card? 1 line of note text the board doesn't show will be deleted too, starting “hidden”.⟧"
    );
    assert_eq!(
        board_kept_as_is("1 element", 2),
        "⟦1 element · 2 kept as-is⟧"
    );
    assert_eq!(
        new_note_in("Projects"),
        "⟦New note in Projects⟧",
        "a catalogued place joins the sentence instead of nesting a marker"
    );
}

/// The catalog's no-placeholder rule: a `{n}` template lives in the message
/// table only. An id may not collide, and no `Key` text may contain a
/// placeholder (the two on-disk token examples are data, not copy).
#[test]
fn templates_never_enter_the_catalog() {
    const TOKEN_EXAMPLES: [&str; 2] = [
        "kanban.setting.date_trigger.help",
        "kanban.setting.time_trigger.help",
    ];
    let key_ids: BTreeSet<&str> = Key::ALL.iter().map(|key| key.id()).collect();
    for &msg in MsgKey::ALL {
        assert!(
            !key_ids.contains(msg.id()),
            "{} is both a catalogue id and a message id",
            msg.id()
        );
    }
    for &key in Key::ALL {
        let text = t(key);
        if TOKEN_EXAMPLES.contains(&key.id()) {
            continue;
        }
        assert!(
            !text.contains('{') && !text.contains('}'),
            "{} contains a placeholder; arguments belong in the message table",
            key.id()
        );
    }
}

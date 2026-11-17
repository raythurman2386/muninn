//! The message table's contract: one template per key, arities that match the
//! placeholders, English output identical to the `format!` the helpers
//! replaced, and a pseudo render that wraps the **rendered** sentence.
//!
//! Missing English text is a compile error (the `messages` match is
//! exhaustive), so there is no runtime "message has no text" test; the
//! placeholder shape is checked in `src/messages.rs`.

use std::collections::BTreeSet;

use muninn_i18n::{
    Key, Lang, MsgKey, PSEUDO_CLOSE, PSEUDO_OPEN, autosave_help, board_canvas, card_label,
    connected_as, could_not_restore, files_skipped, filter_by_tag, link_impact, more_edits, notes,
    phrase, phrase_current, scan_ms, set_lang, status_counts, t, theme_label, trash_summary,
    version,
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

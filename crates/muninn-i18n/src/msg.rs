//! The message-template key type.
//!
//! [`MsgKey`] is a **second table** next to [`crate::Key`], and deliberately
//! not merged into it:
//!
//! - a [`crate::Key`] is one static string with no arguments, resolved by
//!   [`crate::t`] to `&'static str`;
//! - a `MsgKey` is a sentence **template** with positional arguments
//!   (`{0}`, `{1}`), resolved by [`crate::phrase`] to an owned `String`.
//!
//! Keeping them apart preserves the catalog's no-placeholder rule
//! (`tests/catalog.rs` forbids `{` in `Key` text), and it is why a template
//! can never be mistaken for a static label. Adding a variant here without
//! template text in [`crate::messages`] fails `cargo check`, because that
//! match is exhaustive.
//!
//! `messages!` is the same local declarative macro shape as `keys!`: one row
//! per sentence, `Variant => "stable.id", arity`, and it emits [`MsgKey`],
//! [`MsgKey::ALL`], [`MsgKey::id`] and [`MsgKey::arity`] together so they
//! cannot drift.

/// Declares [`MsgKey`], its [`MsgKey::ALL`] slice, its stable ids and arities.
///
/// One row per user-visible sentence: `Variant => "dotted.id", arity`. The id
/// is not the English text, so a copy edit never changes it; `arity` is the
/// number of positional placeholders the template consumes.
macro_rules! messages {
    ($($variant:ident => $id:literal, $arity:literal;)*) => {
        /// The identity of one parameterised sentence.
        ///
        /// Call sites do not build a sentence with `format!`; a helper in
        /// [`crate`] names a key and passes its arguments to
        /// [`crate::phrase`], which renders it in the current language. A key
        /// never carries the text itself, so the same call site can render any
        /// language.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum MsgKey {
            $(
                /// See [`MsgKey::id`] for the stable id, [`MsgKey::arity`]
                /// for the argument count and [`crate::phrase`] for the text.
                $variant,
            )*
        }

        impl MsgKey {
            /// Every message key in the table, in declaration order.
            ///
            /// Tests use this to check for duplicate ids, missing text and
            /// arity mismatches without naming each key.
            pub const ALL: &'static [MsgKey] = &[
                $(MsgKey::$variant,)*
            ];

            /// The stable dotted id, for debugging and future catalogs.
            ///
            /// Use this in log lines and test failure messages; use
            /// [`crate::phrase`] for anything a user reads. The id is part of
            /// the catalog's contract: renaming one is a catalog change, not a
            /// copy edit.
            pub const fn id(self) -> &'static str {
                match self {
                    $(MsgKey::$variant => $id,)*
                }
            }

            /// How many positional arguments this sentence's template takes.
            ///
            /// Placeholders are `{0}` through `{arity - 1}`, each exactly
            /// once. [`crate::phrase`] substitutes them in order.
            pub const fn arity(self) -> usize {
                match self {
                    $(MsgKey::$variant => $arity,)*
                }
            }

            /// Position of this key in [`MsgKey::ALL`].
            ///
            /// Private and test-only: the tests walk the table in declaration
            /// order, so they need the position without a scan.
            #[cfg(test)]
            pub(crate) const fn index(self) -> usize {
                let mut index = 0usize;
                $(
                    if let MsgKey::$variant = self {
                        return index;
                    }
                    index += 1;
                )*
                index
            }
        }
    };
}

messages! {
    // ---- Counts and plurals ------------------------------------------------
    // One template per grammatical branch (`one` / `other`): a language may
    // need a different form in each, so the branch is data, not an `if`.
    FilesSkippedOne => "msg.files_skipped.one", 1;
    FilesSkippedOther => "msg.files_skipped.other", 1;
    NoteOne => "msg.note.one", 1;
    NoteOther => "msg.note.other", 1;
    BoardOne => "msg.board.one", 1;
    BoardOther => "msg.board.other", 1;
    TargetOne => "msg.target.one", 1;
    TargetOther => "msg.target.other", 1;
    ReferrerOne => "msg.referrer.one", 1;
    ReferrerOther => "msg.referrer.other", 1;
    WordOne => "msg.word.one", 1;
    WordOther => "msg.word.other", 1;
    LinkOne => "msg.link.one", 1;
    LinkOther => "msg.link.other", 1;
    BacklinkOne => "msg.backlink.one", 1;
    BacklinkOther => "msg.backlink.other", 1;
    GraphOutgoingLinkOne => "msg.graph_outgoing_link.one", 1;
    GraphOutgoingLinkOther => "msg.graph_outgoing_link.other", 1;
    ResultOne => "msg.result.one", 1;
    ResultOther => "msg.result.other", 1;
    ElementOne => "msg.element.one", 1;
    ElementOther => "msg.element.other", 1;
    ArchivedCardOne => "msg.archived_card.one", 1;
    ArchivedCardOther => "msg.archived_card.other", 1;
    LineOne => "msg.line.one", 1;
    LineOther => "msg.line.other", 1;
    FileOne => "msg.file.one", 1;
    FileOther => "msg.file.other", 1;

    // ---- Count phrases joined into one sentence ----------------------------
    StatusCounts => "msg.status.counts", 3;
    TrashSummary => "msg.trash.summary", 2;
    NoteLinks => "msg.note.links", 2;
    NoLinks => "msg.note.no_links", 0;
    NoteLinksRoot => "msg.note.links.root", 1;
    NoteLinksFolder => "msg.note.links.folder", 2;
    SidebarSummary => "msg.sidebar.summary", 2;
    GraphLinks => "msg.graph.links", 2;
    RecoveredCopies => "msg.recovered.copies", 2;

    // ---- Status bar and notices -------------------------------------------
    MoreEdits => "msg.more_edits", 1;
    Deleted => "msg.deleted", 1;
    ScanMs => "msg.scan_ms", 1;
    RestoreTo => "msg.restore_to", 1;
    FilterByTag => "msg.filter_by_tag", 1;
    LinkImpactOne => "msg.link_impact.one", 2;
    LinkImpactOther => "msg.link_impact.other", 2;

    // ---- Errors ------------------------------------------------------------
    CouldNotSave => "msg.error.could_not_save", 1;
    CouldNotOpenVault => "msg.error.could_not_open_vault", 1;
    CouldNotReadVault => "msg.error.could_not_read_vault", 1;
    CouldNotReadNoteState => "msg.error.could_not_read_note_state", 1;
    CouldNotCreateVault => "msg.error.could_not_create_vault", 1;
    CouldNotPersistKanbanRecovery => "msg.error.could_not_persist_kanban_recovery", 1;
    CouldNotPersistRecovery => "msg.error.could_not_persist_recovery", 1;
    CouldNotPersistBoardRecovery => "msg.error.could_not_persist_board_recovery", 1;
    CouldNotReadRecovery => "msg.error.could_not_read_recovery", 1;
    CouldNotRecreateNoteFolder => "msg.error.could_not_recreate_note_folder", 1;
    CouldNotRecreateNote => "msg.error.could_not_recreate_note", 1;
    CouldNotReloadNote => "msg.error.could_not_reload_note", 1;
    CouldNotSaveCopy => "msg.error.could_not_save_copy", 1;
    CouldNotRenameNote => "msg.error.could_not_rename_note", 1;
    CouldNotRenameFolder => "msg.error.could_not_rename_folder", 1;
    CouldNotDeleteNote => "msg.error.could_not_delete_note", 1;
    CouldNotDeleteFolder => "msg.error.could_not_delete_folder", 1;
    CouldNotEmptyTrash => "msg.error.could_not_empty_trash", 1;
    CouldNotMoveNote => "msg.error.could_not_move_note", 1;
    CouldNotCreateFolder => "msg.error.could_not_create_folder", 1;
    CouldNotCreateNote => "msg.error.could_not_create_note", 1;
    CouldNotAddNoteTemplate => "msg.error.could_not_add_note_template", 1;
    CouldNotBeRead => "msg.error.could_not_be_read", 1;
    CouldNotRestore => "msg.error.could_not_restore", 2;
    CouldNotOpenFile => "msg.error.could_not_open_file", 3;
    CouldNotRevealFile => "msg.error.could_not_reveal_file", 3;
    CouldNotImportInto => "msg.error.could_not_import_into", 2;
    RefusingUnsafeRecoveryPath => "msg.error.refusing_unsafe_recovery_path", 1;
    BoardQuitRefused => "msg.error.board_quit_refused", 1;
    ScanCancelled => "msg.error.scan_cancelled", 2;

    // ---- Import ------------------------------------------------------------
    ImportRefused => "msg.import.refused", 1;
    ImportPartlyRefused => "msg.import.partly_refused", 3;
    CannotImport => "msg.import.cannot_import", 1;
    ImportProgress => "msg.import.progress", 1;
    ImportDocumentNotice => "msg.import.document_notice", 1;
    ImportUnsupported => "msg.import.unsupported", 1;
    ImportNeedsOcrOne => "msg.import.needs_ocr.one", 3;
    ImportNeedsOcrOther => "msg.import.needs_ocr.other", 3;
    ImportEncrypted => "msg.import.encrypted", 1;
    ImportMalformed => "msg.import.malformed", 1;
    ImportResourceLimit => "msg.import.resource_limit", 1;
    ImportMissingPart => "msg.import.missing_part", 1;
    ImportIo => "msg.import.io", 1;
    ImportFailed => "msg.import.failed", 1;
    ImportTooLarge => "msg.import.too_large", 3;
    ImportBodyTooLarge => "msg.import.body_too_large", 3;
    RefusedNotAFile => "msg.import.refused_not_a_file", 1;
    RefusedUnsupportedType => "msg.import.refused_unsupported_type", 1;
    RefusedAlreadyExists => "msg.import.refused_already_exists", 1;

    // ---- Sync --------------------------------------------------------------
    ConnectedAs => "msg.sync.connected_as", 1;
    LastSync => "msg.sync.last_sync", 1;
    SyncStatus => "msg.sync.status", 1;
    SyncExcluded => "msg.sync.excluded", 1;
    SyncConflicts => "msg.sync.conflicts", 2;
    SyncConflictCopies => "msg.sync.conflict_copies", 1;

    // ---- Board and links ---------------------------------------------------
    BoardNameTaken => "msg.board.name_taken", 1;
    NoNoteNamed => "msg.board.no_note_named", 1;
    LinkedNoteHint => "msg.board.linked_note_hint", 1;
    BoardCanvas => "msg.board.canvas", 2;
    BoardCanvasSelected => "msg.board.canvas.selected", 3;
    CardLabelEmpty => "msg.card_label.empty", 1;
    CardLabel => "msg.card_label.text", 2;

    // ---- Settings ----------------------------------------------------------
    ThemeLabel => "msg.settings.theme_label", 1;
    AutosaveHelp => "msg.settings.autosave_help", 1;
    Version => "msg.settings.version", 1;
    ShowWithChord => "msg.settings.show_with_chord", 1;
}

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
    ShortcutFailed => "msg.settings.shortcut_failed", 2;
    ShortcutOn => "msg.settings.shortcut_on", 1;

    // ---- Dates and relative days ------------------------------------------
    // One template per branch: a language that inflects a day count supplies
    // its own wording for each.
    RelativeToday => "msg.date.today", 0;
    RelativeTomorrow => "msg.date.tomorrow", 0;
    RelativeYesterday => "msg.date.yesterday", 0;
    RelativeInDays => "msg.date.in_days", 1;
    RelativeDaysAgo => "msg.date.days_ago", 1;
    DayLabel => "msg.date.day_label", 1;
    RelativeDate => "msg.date.relative_date", 2;

    // ---- Actions and welcome ----------------------------------------------
    NewNoteIn => "msg.action.new_note_in", 1;
    NewFolderIn => "msg.action.new_folder_in", 1;
    WelcomeVaultCreated => "msg.welcome.vault_created", 1;

    // ---- Status, Help and palette -----------------------------------------
    ScanProgress => "msg.status.scan_progress", 2;
    FocusHelp => "msg.help.focus", 1;
    FocusExitHelp => "msg.help.focus_exit", 1;
    PaletteKanbanFolder => "msg.palette.kanban_folder", 1;

    // ---- Link notices ------------------------------------------------------
    LinksNotUpdated => "msg.links.not_updated", 1;
    RenamedLinksFailed => "msg.links.renamed_failed", 2;
    MovedLinksFailed => "msg.links.moved_failed", 2;
    NoteNotInVault => "msg.links.note_not_in_vault", 1;
    CannotOpenWithoutNote => "msg.links.cannot_open_without_note", 1;
    AttachmentNotInVault => "msg.links.attachment_not_in_vault", 1;
    BrokenLinksCount => "msg.links.broken", 1;

    // ---- Path and vault refusals ------------------------------------------
    NotAFile => "msg.error.not_a_file", 1;
    PathGone => "msg.error.path_gone", 1;
    TrayError => "msg.error.tray", 1;
    LoginItemError => "msg.error.login_item", 1;
    OutsideVault => "msg.error.outside_vault", 1;
    NotABoardInVault => "msg.error.not_a_board_in_vault", 1;
    AlreadyExists => "msg.error.already_exists", 1;
    NotATrashPath => "msg.error.not_a_trash_path", 1;
    NoLongerInTrash => "msg.error.no_longer_in_trash", 1;
    NotAFolder => "msg.error.not_a_folder", 1;
    BoardNameInvalid => "msg.error.board_name_invalid", 1;
    BoardSceneKeyObject => "msg.error.board_scene_key_object", 1;
    BoardRenamedEmbedsFailed => "msg.error.board_renamed_embeds_failed", 2;
    AutostartRegFailed => "msg.error.autostart_reg_failed", 1;
    AutostartRegExit => "msg.error.autostart_reg_exit", 1;
    KanbanQuitRefused => "msg.error.kanban_quit_refused", 1;
    KanbanCouldNotSaveCopy => "msg.error.kanban_could_not_save_copy", 1;
    CouldNotDo => "msg.error.could_not_do", 2;
    CouldNotCreateBoard => "msg.error.could_not_create_board", 1;
    CouldNotCreateKanban => "msg.error.could_not_create_kanban", 1;
    CouldNotConvertKanban => "msg.error.could_not_convert_kanban", 1;

    // ---- Board and graph chrome -------------------------------------------
    BoardKeptAsIs => "msg.board.kept_as_is", 2;
    BoardCouldNotRead => "msg.board.could_not_read", 1;
    BoardNoteLinkMissing => "msg.board.note_link_missing", 1;
    UnsavedSaves => "msg.board.unsaved_saves", 1;
    NotSaved => "msg.board.not_saved", 1;
    LinkDescription => "msg.board.link_description", 1;
    NoteLabelRoot => "msg.graph.note_label_root", 1;
    GraphNodeParent => "msg.graph.node_parent", 2;
    GraphStats => "msg.graph.stats", 2;

    // ---- Kanban chrome -----------------------------------------------------
    KanbanFilterFrom => "msg.kanban.filter_from", 1;
    KanbanFilterTo => "msg.kanban.filter_to", 1;
    KanbanFilterRange => "msg.kanban.filter_range", 2;
    LaneActions => "msg.kanban.lane_actions", 1;
    LaneControls => "msg.kanban.lane_controls", 1;
    CollapseLane => "msg.kanban.collapse_lane", 1;
    ExpandLane => "msg.kanban.expand_lane", 1;
    LaneCards => "msg.kanban.lane_cards", 1;
    AddCardToLane => "msg.kanban.add_card_to_lane", 1;
    LaneLabel => "msg.kanban.lane_label", 2;
    LaneOverLimit => "msg.kanban.over_limit", 0;
    LaneDone => "msg.kanban.done_lane", 0;
    LaneRegionLabel => "msg.kanban.lane_region_label", 2;
    CollapsedLaneLabel => "msg.kanban.collapsed_lane_label", 2;
    CollapsedCards => "msg.kanban.collapsed_cards", 1;
    DeleteLane => "msg.kanban.delete_lane", 2;
    DeleteLaneText => "msg.kanban.delete_lane_text", 4;
    DeleteCardsOne => "msg.kanban.delete_cards.one", 0;
    DeleteCardsOther => "msg.kanban.delete_cards.other", 1;
    CardActions => "msg.kanban.card_actions", 1;
    CardActionsEmpty => "msg.kanban.card_actions.empty", 0;
    CardActionsInLane => "msg.kanban.card_actions_in_lane", 1;
    ChecklistProgress => "msg.kanban.checklist_progress", 2;
    ChecklistLabel => "msg.kanban.checklist_label", 2;
    CardLabelInLane => "msg.card_label.in_lane", 3;

    // ---- Sync status and provider errors ----------------------------------
    DeviceCode => "msg.sync.device_code", 2;
    BrowserFailed => "msg.sync.browser_failed", 1;
    SignInFailed => "msg.sync.sign_in_failed", 1;
    TailnetHosting => "msg.sync.tailnet_hosting", 2;
    TailnetHostingReady => "msg.sync.tailnet_hosting_ready", 1;
    SharedKeyCopied => "msg.sync.shared_key_copied", 1;
    SyncPreviousCredential => "msg.sync.previous_credential", 1;
    SyncNothingSaved => "msg.sync.nothing_saved", 1;
    SyncKeyRotationFailed => "msg.sync.key_rotation_failed", 1;
    SyncNowAfterError => "msg.sync.now_after_error", 1;
    SyncOffline => "msg.sync.error.offline", 1;
    SyncRateLimitedAfter => "msg.sync.error.rate_limited", 1;
    SyncProviderError => "msg.sync.error.provider", 2;
    SyncAuthenticationNeeded => "msg.sync.error.authentication_needed", 1;
    SyncPermissionDenied => "msg.sync.error.permission_denied", 1;
    SyncOauthScopeRejected => "msg.sync.error.oauth_scope_rejected", 1;
    SyncRemoteScopeNotFound => "msg.sync.error.remote_scope_not_found", 1;
    SyncRemoteChanged => "msg.sync.error.remote_changed", 1;
    SyncOutcomeUnknown => "msg.sync.error.outcome_unknown", 1;
    SyncListingIncomplete => "msg.sync.error.listing_incomplete", 1;
    SyncUnsupported => "msg.sync.error.unsupported", 2;
    SyncLimitExceeded => "msg.sync.error.limit_exceeded", 1;
    SyncCredentialsUnavailable => "msg.sync.error.credentials_unavailable", 1;
    SyncDirtyBuffer => "msg.sync.error.dirty_buffer", 1;
    SyncLocalChanged => "msg.sync.error.local_changed", 1;
    SyncNotConfigured => "msg.sync.error.not_configured", 1;
    SyncFileError => "msg.sync.error.file_error", 1;

    // ---- Composition fragments --------------------------------------------
    // A composed sentence is built from these in the current language and
    // wrapped once, so a pseudo render has one marker pair around the line.
    RecoveredFor => "msg.recovered.for", 1;
    ImportedFilesOne => "msg.import.imported_files.one", 1;
    ImportedFilesOther => "msg.import.imported_files.other", 1;
    ImportJoin => "msg.import.join", 1;
    ImportAttachmentsNote => "msg.import.attachments_note", 0;
}

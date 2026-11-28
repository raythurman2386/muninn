//! The `en-US` template table for [`MsgKey`].
//!
//! This exhaustive `match` is the only place the English wording of a
//! parameterised sentence is pinned. Adding a [`MsgKey`] without a row here is
//! a compile error, so a missing message cannot reach a draw call. The
//! templates use positional placeholders (`{0}`, `{1}`) that
//! [`crate::phrase`] substitutes; the catalog's static [`crate::Key`] text
//! lives in [`crate::en_us`] and never contains one.

use crate::msg::MsgKey;

/// The English template for `key`.
///
/// Private: callers go through [`crate::phrase`] or [`crate::phrase_current`],
/// which substitute the arguments and pick the language. Keeping the only
/// table behind those functions is what makes a second language a change to
/// this crate rather than to call sites.
pub(crate) fn template(key: MsgKey) -> &'static str {
    match key {
        MsgKey::FilesSkippedOne => "{0} file skipped",
        MsgKey::FilesSkippedOther => "{0} files skipped",
        MsgKey::NoteOne => "{0} note",
        MsgKey::NoteOther => "{0} notes",
        MsgKey::BoardOne => "{0} board",
        MsgKey::BoardOther => "{0} boards",
        MsgKey::TargetOne => "{0} target",
        MsgKey::TargetOther => "{0} targets",
        MsgKey::ReferrerOne => "{0} referrer",
        MsgKey::ReferrerOther => "{0} referrers",
        MsgKey::WordOne => "{0} word",
        MsgKey::WordOther => "{0} words",
        MsgKey::LinkOne => "{0} link",
        MsgKey::LinkOther => "{0} links",
        MsgKey::BacklinkOne => "{0} backlink",
        MsgKey::BacklinkOther => "{0} backlinks",
        MsgKey::GraphOutgoingLinkOne => "{0} outgoing link",
        MsgKey::GraphOutgoingLinkOther => "{0} outgoing links",
        MsgKey::ResultOne => "{0} result",
        MsgKey::ResultOther => "{0} results",
        MsgKey::ElementOne => "{0} element",
        MsgKey::ElementOther => "{0} elements",
        MsgKey::ArchivedCardOne => "{0} archived card",
        MsgKey::ArchivedCardOther => "{0} archived cards",
        MsgKey::LineOne => "{0} line",
        MsgKey::LineOther => "{0} lines",
        MsgKey::FileOne => "{0} file",
        MsgKey::FileOther => "{0} files",

        MsgKey::StatusCounts => "{0} · {1} · {2}",
        MsgKey::TrashSummary => "Trash · {0} · {1}",
        MsgKey::NoteLinks => "{0} · {1}",
        MsgKey::NoLinks => "no links",
        MsgKey::NoteLinksRoot => "vault root · {0}",
        MsgKey::NoteLinksFolder => "{0} · {1}",
        MsgKey::SidebarSummary => "{0} · {1}",
        MsgKey::GraphLinks => "{0} · {1}",
        MsgKey::RecoveredCopies => "{0} and {1}",

        MsgKey::MoreEdits => "+{0} more",
        MsgKey::Deleted => "Deleted {0}",
        MsgKey::ScanMs => "scan {0} ms",
        MsgKey::RestoreTo => "Restore to {0}",
        MsgKey::FilterByTag => "Filter by #{0}",
        MsgKey::NoteTagFilterSummary => "Notes tagged #{0}",
        MsgKey::LinkImpactOne => "{0} note links {1}",
        MsgKey::LinkImpactOther => "{0} notes link {1}",

        MsgKey::CouldNotSave => "Could not save: {0}",
        MsgKey::CouldNotOpenVault => "Could not open the vault: {0}",
        MsgKey::CouldNotReadVault => "Could not read the vault: {0}",
        MsgKey::CouldNotReadNoteState => "Could not read note state: {0}",
        MsgKey::CouldNotCreateVault => "Could not create the vault: {0}",
        MsgKey::CouldNotPersistKanbanRecovery => "Could not persist kanban recovery copy: {0}",
        MsgKey::CouldNotPersistRecovery => "Could not persist recovery copy: {0}",
        MsgKey::CouldNotPersistBoardRecovery => "Could not persist board recovery copy: {0}",
        MsgKey::CouldNotReadRecovery => "Could not read recovery copy: {0}",
        MsgKey::CouldNotRecreateNoteFolder => "Could not recreate note folder: {0}",
        MsgKey::CouldNotRecreateNote => "Could not recreate note: {0}",
        MsgKey::CouldNotReloadNote => "Could not reload the note: {0}",
        MsgKey::CouldNotSaveCopy => "Could not save a copy: {0}",
        MsgKey::CouldNotRenameNote => "Could not rename note: {0}",
        MsgKey::CouldNotRenameFolder => "Could not rename folder: {0}",
        MsgKey::CouldNotDeleteNote => "Could not delete note: {0}",
        MsgKey::CouldNotDeleteFolder => "Could not delete folder: {0}",
        MsgKey::CouldNotEmptyTrash => "Could not empty trash: {0}",
        MsgKey::CouldNotMoveNote => "Could not move note: {0}",
        MsgKey::CouldNotCreateFolder => "Could not create folder: {0}",
        MsgKey::CouldNotCreateNote => "Could not create a note: {0}",
        MsgKey::CouldNotAddNoteTemplate => "Could not add the note template: {0}",
        MsgKey::CouldNotBeRead => "the {0} could not be read",
        MsgKey::CouldNotRestore => "Could not restore {0}: {1}",
        MsgKey::CouldNotOpenFile => "Could not open {0}: {1}. {2}",
        MsgKey::CouldNotRevealFile => "Could not reveal {0}: {1}. {2}",
        MsgKey::CouldNotImportInto => "Could not import into {0}: {1}",
        MsgKey::RefusingUnsafeRecoveryPath => "Refusing unsafe recovery path {0}",
        MsgKey::BoardQuitRefused => {
            "Board not saved and no recovery copy could be written: {0}. Use Save copy or fix the folder, then quit again."
        }
        MsgKey::ScanCancelled => "Scan cancelled after {0} of {1} notes",

        MsgKey::ImportRefused => "Import refused: {0}",
        MsgKey::ImportPartlyRefused => "Imported {0} of {1}; refused: {2}",
        MsgKey::CannotImport => "Cannot import {0}: drop Markdown, image, PDF or office documents",
        MsgKey::ImportProgress => "Importing {0}…",
        MsgKey::ImportDocumentNotice => "Imported {0}",
        MsgKey::ImportUnsupported => "Could not import {0}: this file type cannot be converted.",
        MsgKey::ImportNeedsOcrOne => {
            "Could not import {0}: page {1} of {2} is a scanned image and needs OCR, which Muninn does not do. Export a text version first."
        }
        MsgKey::ImportNeedsOcrOther => {
            "Could not import {0}: pages {1} of {2} are scanned images and need OCR, which Muninn does not do. Export a text version first."
        }
        MsgKey::ImportEncrypted => {
            "Could not import {0}: the document is encrypted. Remove its password and import it again."
        }
        MsgKey::ImportMalformed => {
            "Could not import {0}: the file is damaged, or it is not a readable document."
        }
        MsgKey::ImportResourceLimit => {
            "Could not import {0}: the document is too complex to convert."
        }
        MsgKey::ImportMissingPart => {
            "Could not import {0}: the document is missing a part it needs."
        }
        MsgKey::ImportIo => "Could not import {0}: the file could not be read.",
        MsgKey::ImportFailed => "Could not import {0}: the document could not be converted.",
        MsgKey::ImportTooLarge => "Could not import {0}: the file is {1}, over the {2} limit.",
        MsgKey::ImportBodyTooLarge => {
            "Could not import {0}: the converted note is {1}, over the {2} limit."
        }
        MsgKey::RefusedNotAFile => "{0}: not a file",
        MsgKey::RefusedUnsupportedType => "{0}: unsupported file type",
        MsgKey::RefusedAlreadyExists => "{0}: already exists in this folder",

        MsgKey::ConnectedAs => {
            "Connected as {0}. The token is in the OS credential store. Sync Now when ready."
        }
        MsgKey::LastSync => "Last successful sync {0}.",
        MsgKey::SyncStatus => "Sync: {0}",
        MsgKey::SyncExcluded => "{0} path(s) excluded by the sync contract.",
        MsgKey::SyncConflicts => "{0} conflict(s): {1}. Nothing was overwritten.",
        MsgKey::SyncConflictCopies => {
            "A note changed on two devices; both versions were kept: {0}."
        }
        MsgKey::SyncWebFailed => "Sync failed: {0}",

        MsgKey::BoardNameTaken => "A board named “{0}” is already in this folder",
        MsgKey::NoNoteNamed => "No note named “{0}” in this vault",
        MsgKey::LinkedNoteHint => "Linked note: {0} · ⌘Enter opens",
        MsgKey::BoardCanvas => "Board canvas, {0}, {1} tool",
        MsgKey::BoardCanvasSelected => "Board canvas, {0}, {1} tool, {2} selected",
        MsgKey::CardLabelEmpty => "{0}, empty card",
        MsgKey::CardLabel => "{0}: {1}",

        MsgKey::ThemeLabel => "Theme: {0} — click to cycle (System, Dark, Light)",
        MsgKey::AutosaveHelp => "Autosave writes {0} s after you stop typing.",
        MsgKey::Version => "Version {0}.",
        MsgKey::UpdateAvailable => "Muninn {0} is available",
        MsgKey::UpdateInstalled => "Muninn {0} is installed. Restart Muninn to use it.",
        MsgKey::UpdateFailed => "Update failed: {0}",
        MsgKey::ShowWithChord => "Show Muninn with {0}",
        MsgKey::ShortcutFailed => "{0} could not be registered: {1}",
        MsgKey::ShortcutOn => "On. {0} shows Muninn from any app.",

        MsgKey::RelativeToday => "today",
        MsgKey::RelativeTomorrow => "tomorrow",
        MsgKey::RelativeYesterday => "yesterday",
        MsgKey::RelativeInDays => "in {0} days",
        MsgKey::RelativeDaysAgo => "{0} days ago",
        MsgKey::DayLabel => "Day {0}",
        MsgKey::RelativeDate => "{0} · {1}",

        MsgKey::NewNoteIn => "New note in {0}",
        MsgKey::NewFolderIn => "New folder in {0}",
        MsgKey::WelcomeVaultCreated => "New vault created at {0} — make your first note",

        MsgKey::ScanProgress => "Scanning {0}/{1} notes — Esc to cancel",
        MsgKey::FocusHelp => "Hide the chrome and keep the caret's line centred ({0})",
        MsgKey::FocusExitHelp => "Back to the note's chrome ({0})",
        MsgKey::PaletteKanbanFolder => "Kanban · {0}",

        MsgKey::LinksNotUpdated => "Links in {0} were not updated",
        MsgKey::RenamedLinksFailed => {
            "Renamed, but {0} inbound link file(s) could not be rewritten: {1}"
        }
        MsgKey::MovedLinksFailed => {
            "Moved, but {0} inbound link file(s) could not be rewritten: {1}"
        }
        MsgKey::NoteNotInVault => "Note {0} is no longer in this vault",
        MsgKey::CannotOpenWithoutNote => "Cannot open {0}: no note is selected",
        MsgKey::AttachmentNotInVault => {
            "Attachment {0} is not in this vault — drop it in, or fix the embed"
        }
        MsgKey::BrokenLinksCount => "{0} broken",

        MsgKey::NotAFile => "{0} is not a file",
        MsgKey::PathGone => "{0} no longer exists",
        MsgKey::TrayError => "The tray icon could not be added: {0}",
        MsgKey::LoginItemError => "The login item could not be changed: {0}",
        MsgKey::OutsideVault => "{0} is outside the vault",
        MsgKey::NotABoardInVault => "{0} is not a board inside the vault",
        MsgKey::AlreadyExists => "{0} already exists",
        MsgKey::NotATrashPath => "{0} is not a trash path",
        MsgKey::NoLongerInTrash => "{0} is no longer in the trash",
        MsgKey::NotAFolder => "{0} is not a folder",
        MsgKey::BoardNameInvalid => "a board name {0}",
        MsgKey::BoardSceneKeyObject => "scene {0} must be an object",
        MsgKey::BoardRenamedEmbedsFailed => {
            "Renamed, but {0} note(s) embedding it could not be updated: {1}"
        }
        MsgKey::AutostartRegFailed => "reg.exe could not run: {0}",
        MsgKey::AutostartRegExit => "reg.exe exited with {0}",
        MsgKey::KanbanQuitRefused => {
            "Kanban not saved and no recovery copy could be written: {0}. Use Save copy or fix the folder, then quit again."
        }
        MsgKey::KanbanCouldNotSaveCopy => "could not save a copy: {0}",
        MsgKey::CouldNotDo => "Could not {0}: {1}",
        MsgKey::CouldNotCreateBoard => "Could not create a board: {0}",
        MsgKey::CouldNotCreateKanban => "Could not create a kanban: {0}",
        MsgKey::CouldNotConvertKanban => "Could not convert to a kanban: {0}",

        MsgKey::BoardKeptAsIs => "{0} · {1} kept as-is",
        MsgKey::BoardCouldNotRead => "Could not read: {0}",
        MsgKey::BoardNoteLinkMissing => "{0} (missing)",
        MsgKey::UnsavedSaves => "Unsaved · {0} saves",
        MsgKey::NotSaved => "Not saved: {0}",
        MsgKey::LinkDescription => "Open {0}",
        MsgKey::NoteLabelRoot => "{0} (root)",
        MsgKey::GraphNodeParent => "{0} ({1})",
        MsgKey::GraphStatsWithTags => "{0} notes · {1} tags · {2} edges",

        MsgKey::KanbanFilterFrom => "from {0}",
        MsgKey::KanbanFilterTo => "to {0}",
        MsgKey::KanbanFilterRange => "{0} – {1}",
        MsgKey::LaneActions => "Lane actions: {0}",
        MsgKey::LaneControls => "{0} controls",
        MsgKey::CollapseLane => "Collapse lane: {0}",
        MsgKey::ExpandLane => "Expand lane: {0}",
        MsgKey::LaneCards => "{0} cards",
        MsgKey::AddCardToLane => "Add card to {0}",
        MsgKey::LaneLabel => "{0}, {1} cards",
        MsgKey::LaneOverLimit => ", over limit",
        MsgKey::LaneDone => ", done lane",
        MsgKey::LaneRegionLabel => "{0} lane, {1} cards",
        MsgKey::CollapsedLaneLabel => "{0}, {1} cards, collapsed",
        MsgKey::CollapsedCards => "Collapsed · {0} cards",
        MsgKey::DeleteLane => "Delete “{0}”{1}? Any note text in it stays in the file.",
        MsgKey::DeleteLaneText => {
            "Delete “{0}”{1}? {2} of note text the board doesn't show will be deleted too, starting “{3}”."
        }
        MsgKey::DeleteCardsOne => " and its card",
        MsgKey::DeleteCardsOther => " and its {0} cards",
        MsgKey::CardActions => "Card actions: {0}",
        MsgKey::CardActionsEmpty => "Card actions: empty card",
        MsgKey::CardActionsInLane => "Card actions: empty card in {0}",
        MsgKey::ChecklistProgress => "{0} of {1} done",
        MsgKey::ChecklistLabel => "Checklist, {0} of {1} done",
        MsgKey::CardLabelInLane => "{0} · {1}: {2}",

        MsgKey::DeviceCode => {
            "Enter code {0} at {1}. Your browser is opening it; Muninn finishes when GitHub approves."
        }
        MsgKey::BrowserFailed => {
            "Could not open a browser. Open this address yourself to continue: {0}"
        }
        MsgKey::SignInFailed => "Sign-in did not finish: {0} No token was stored.",
        MsgKey::TailnetHosting => "Hosting on 127.0.0.1:{0}. On this device run: {1}",
        MsgKey::TailnetHostingReady => {
            "Hosting. Run once on this device: {0}. Use Copy key to give the shared key to your other devices."
        }
        MsgKey::SharedKeyCopied => {
            "Shared key copied. The clipboard clears in {0} seconds; paste it on the joining device."
        }
        MsgKey::SyncPreviousCredential => {
            "The previous connection's credential could not be removed: {0}"
        }
        MsgKey::SyncNothingSaved => "{0} Nothing was saved.",
        MsgKey::SyncKeyRotationFailed => "{0} The old key still works.",
        MsgKey::SyncNowAfterError => "{0} Connect again before Sync Now.",
        MsgKey::SyncOffline => "Offline: {0}",
        MsgKey::SyncRateLimitedAfter => "Rate limited; retry after {0} ms",
        MsgKey::SyncProviderError => "Provider error {0}: {1}",
        MsgKey::SyncAuthenticationNeeded => "Authentication needed: {0}",
        MsgKey::SyncPermissionDenied => "Permission denied: {0}",
        MsgKey::SyncOauthScopeRejected => "OAuth scope rejected: {0}",
        MsgKey::SyncRemoteScopeNotFound => "Remote scope not found: {0}",
        MsgKey::SyncRemoteChanged => "Remote changed: {0}",
        MsgKey::SyncOutcomeUnknown => "Outcome unknown, reconcile before retrying: {0}",
        MsgKey::SyncListingIncomplete => {
            "Remote listing incomplete; refusing to treat that as deletion: {0}"
        }
        MsgKey::SyncUnsupported => "Unsupported {0}: {1}",
        MsgKey::SyncLimitExceeded => "Sync limit exceeded: {0}",
        MsgKey::SyncCredentialsUnavailable => "Credentials unavailable: {0}",
        MsgKey::SyncDirtyBuffer => "Save or discard unsaved edits in {0} before sync can change it",
        MsgKey::SyncLocalChanged => "{0} changed during sync; run Sync Now again to replan",
        MsgKey::SyncNotConfigured => "Sync is not configured: {0}",
        MsgKey::SyncFileError => "File error: {0}",

        MsgKey::RecoveredFor => "Recovered unsaved edits for {0}",
        MsgKey::ImportedFilesOne => "Imported {0}",
        MsgKey::ImportedFilesOther => "Imported {0}",
        MsgKey::ImportJoin => " · {0}",
        MsgKey::ImportAttachmentsNote => " · non-Markdown files stay attachments",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `MsgKey::ALL` is complete and in declaration order.
    #[test]
    fn all_is_in_declaration_order() {
        for (position, &key) in MsgKey::ALL.iter().enumerate() {
            assert_eq!(key.index(), position, "{} is out of order", key.id());
        }
        assert!(!MsgKey::ALL.is_empty(), "the message table must seed keys");
    }

    /// The placeholders in `template`: exactly `{0}` through `{arity - 1}`,
    /// each once, and no other braces.
    #[test]
    fn every_template_has_exactly_its_declared_arguments() {
        for &key in MsgKey::ALL {
            let template = template(key);
            assert!(!template.is_empty(), "{} has no template", key.id());
            assert!(
                !template.contains('}') || template.contains('{'),
                "{} has an unbalanced brace: {template:?}",
                key.id()
            );
            for index in 0..key.arity() {
                let placeholder = format!("{{{index}}}");
                assert!(
                    template.contains(&placeholder),
                    "{} is missing {placeholder}: {template:?}",
                    key.id()
                );
                assert_eq!(
                    template.matches(&placeholder).count(),
                    1,
                    "{} repeats {placeholder}: {template:?}",
                    key.id()
                );
            }
            let stray = format!("{{{}}}", key.arity());
            assert!(
                !template.contains(&stray),
                "{} has an argument past its arity: {template:?}",
                key.id()
            );
            assert_eq!(
                template.matches('{').count(),
                key.arity(),
                "{} has an undeclared placeholder: {template:?}",
                key.id()
            );
        }
    }
}

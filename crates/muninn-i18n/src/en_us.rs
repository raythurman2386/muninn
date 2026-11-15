//! The `en-US` catalog.
//!
//! This exhaustive `match` is the only place English copy is pinned. Adding a
//! [`Key`] without a row here is a compile error, so a missing message cannot
//! reach a draw call.

use crate::key::Key;

/// The English text for `key`.
///
/// Private: callers go through [`crate::t`] or [`crate::t_in`], which pick the
/// language. Keeping the only catalog behind those functions is what makes a
/// second language a change to this crate rather than to call sites.
pub(crate) fn text(key: Key) -> &'static str {
    match key {
        Key::AddCard => "Add card",
        Key::Archive => "Archive",
        Key::Today => "Today",
        Key::OverLimit => "Over limit",
        Key::KeyboardShortcuts => "Keyboard shortcuts",
        Key::Close => "Close",
        Key::Save => "Save",
        Key::Cancel => "Cancel",
        Key::LinkedNote => "Linked note",
        Key::NoBoards => "No boards in this vault yet",
        Key::HelpLastLogLines => "last log lines:",
        Key::HelpGroupNavigation => "Navigation",
        Key::HelpNavFocusSearch => "Focus search",
        Key::HelpNavMoveHighlightedResult => "Move the highlighted result",
        Key::HelpNavOpenHighlightedResult => "Open the highlighted result",
        Key::HelpNavBackInNoteHistory => "Back in note history",
        Key::HelpNavForwardInNoteHistory => "Forward in note history",
        Key::HelpNavWriteReadGraphBoardKanban => "Write / Read / Graph / Board / Kanban",
        Key::HelpNavFocusMode => "Focus mode: hide the chrome and centre the caret",
        Key::HelpNavCloseOutermostLayer => "Close the outermost layer",
        Key::HelpNavOpenCommandPalette => "Open the command palette",
        Key::HelpGroupEdit => "Edit",
        Key::HelpEditFindInOpenNote => "Find in the open note",
        Key::HelpEditFindInBoard => "Find in the board",
        Key::HelpEditHideCardsByTextTagDateOrState => "Hide cards by text, tag, date or state",
        Key::HelpEditPerBoardOverrides => "Per-board overrides, one key at a time",
        Key::HelpEditInsertWikilink => "Insert a wikilink",
        Key::HelpEditNewNote => "New note",
        Key::HelpEditSaveNow => "Save now (autosave is on)",
        Key::HelpEditToggleWriteRead => "Toggle Write / Read",
        Key::HelpEditWikilinkCompletion => "Wikilink completion",
        Key::HelpEditVaultTagCompletion => "Vault tag completion",
        Key::HelpEditIndentFourSpaces => "Indent four spaces",
        Key::HelpEditPasteRichText => {
            "Rich text becomes Markdown; a URL over a selection becomes a link"
        }
        Key::HelpEditRenameSelectedNote => "Rename the selected note (on Board: the board)",
        Key::HelpEditDeleteSelectedNote => "Delete the selected note",
        Key::HelpGroupKanban => "Kanban",
        Key::HelpKanbanMoveThroughControls => "Move through the board's controls",
        Key::HelpKanbanMoveBetweenLaneCards => "Move between the focused lane's cards",
        Key::HelpKanbanMoveSelectionAcrossLanes => "Move the selection across lanes",
        Key::HelpKanbanToggleCardDone => "Toggle the selected card's done mark",
        Key::HelpKanbanMoveSelectedCard => "Move the selected card",
        Key::HelpKanbanMoveCardToEdgeOrLane => "Move it to the top, the bottom or another lane",
        Key::HelpKanbanOpenCardMenu => "Open the focused card's menu",
        Key::HelpKanbanEditCard => "Edit the card",
        Key::HelpKanbanCloseMenuOrConfirmation => "Close the menu, confirmation or conflict bar",
        Key::HelpGroupGraph => "Graph",
        Key::HelpGraphToggleGraph => "Toggle Graph",
        Key::HelpGraphToggleGlobalLocal => "Toggle Global / Local",
        Key::HelpGraphMoveFocusRing => "Move the focus ring",
        Key::HelpGraphOpenFocusedNote => "Open the focused note",
        Key::HelpGraphZoomInOut => "Zoom in / out",
        Key::HelpGraphFitVisibleNodes => "Fit the visible nodes",
        Key::HelpGraphToggleIsolate => "Toggle Isolate",
        Key::HelpGraphLeaveGraph => "Leave the graph",
        Key::HelpGroupBoard => "Board",
        Key::HelpBoardPanWithDrag => "Pan the board (or drag with the hand)",
        Key::HelpBoardZoomAtPointer => "Zoom at the pointer",
        Key::HelpBoardZoomAtCanvasCentre => "Zoom in / out at the canvas centre",
        Key::HelpBoardPanTheBoard => "Pan the board",
        Key::HelpBoardFitTheDrawing => "Fit the drawing",
        Key::HelpBoardShapeTools => "Rectangle, diamond, ellipse, arrow, line or freehand tool",
        Key::HelpBoardDrawShape => "Draw that shape; the tool stays picked",
        Key::HelpBoardPlaceText => "Place text, then click and type it in place",
        Key::HelpBoardEditTextLabel => "Write or edit its text label",
        Key::HelpBoardDropTheTool => "Drop the tool (back to selecting)",
        Key::HelpBoardSelectElementOrToggle => "Select an element / add or remove it",
        Key::HelpBoardNudgeSelection => "Nudge the selection 1 unit (Shift: 5)",
        Key::HelpBoardDeleteSelection => "Delete the selection",
        Key::HelpBoardSelectEveryElement => "Select every element",
        Key::HelpBoardLinkToNote => "Link the selected elements to a note (empty clears it)",
        Key::HelpBoardOpenLinkedNote => "Open the selected element's linked note",
        Key::HelpBoardOpenClickedLinkedNote => {
            "Open the clicked element's linked note (a click alone selects)"
        }
        Key::HelpBoardUndoRedo => "Undo / redo drawing, moves and deletes",
        Key::HelpBoardSaveDrawing => "Save the board's drawing",
        Key::HelpBoardEscapeClearsSelection => {
            "Clear the selection, else drop the tool, else leave Board"
        }
        Key::HelpGroupSidebar => "Sidebar",
        Key::HelpSidebarOpenThisHelp => "Open this Help",
        Key::HelpSidebarOpenHelpWhenNotTyping => "Open Help when not typing",
        Key::HelpSidebarMoveBetweenPanes => "Move between panes",
        Key::HelpSidebarMoveThroughAList => "Move through a list",
        Key::HelpSidebarOpenFocusedRow => "Open the focused row",
        Key::HelpSidebarNoteAndFolderActions => "Note and folder actions",
        Key::HelpSidebarSettings => "Settings (vaults, theme, graph effects, Kanban defaults)",
        Key::SettingsTabVaults => "Vaults",
        Key::SettingsTabEditor => "Editor",
        Key::SettingsTabAppearance => "Appearance",
        Key::SettingsTabGraph => "Graph",
        Key::SettingsTabSync => "Sync",
        Key::SettingsTabKanban => "Kanban",
        Key::SettingsVaultsOpenTitle => "Open a vault",
        Key::SettingsVaultsOpenCaption => {
            "Paste a folder path, or create a new vault in a new folder."
        }
        Key::SettingsVaultsRecentTitle => "Recent vaults",
        Key::SettingsVaultsRecentCaption => "Vaults you have opened recently, most recent first.",
        Key::SettingsVaultsRecentEmpty => "No recent vaults yet",
        Key::SettingsVaultsPathLabel => "Vault folder path",
        Key::SettingsVaultsPathHint => "Paste a folder path; ~ expands to your home folder.",
        Key::SettingsVaultsOpenAction => "Open vault",
        Key::SettingsVaultsNewAction => "New vault",
        Key::SettingsEditorAutosaveTitle => "Autosave delay",
        Key::SettingsEditorAutosaveCaption => "Idle seconds before the open note is written.",
        Key::SettingsEditorTemplateTitle => "Default note template",
        Key::SettingsEditorTemplateCaption => {
            "Markdown inserted at the top of every new note. Leave empty for a plain heading."
        }
        Key::SettingsEditorTemplateLabel => "Default note template",
        Key::SettingsEditorTemplatePlaceholder => "Markdown inserted at the top of each new note",
        Key::SettingsAppearanceThemeTitle => "Theme",
        Key::SettingsAppearanceThemeFollowing => "Following Omarchy system theme",
        Key::SettingsAppearanceUnavailable => "Not available on macOS yet.",
        Key::SettingsAppearanceStateOn => "On.",
        Key::SettingsAppearanceStateOff => "Off.",
        Key::SettingsAppearanceTrayTitle => "Tray icon",
        Key::SettingsAppearanceTrayCaption => {
            "A tray icon with Show Muninn, New note, Sync now, the sync status and Quit."
        }
        Key::SettingsAppearanceTrayShow => "Show tray icon",
        Key::SettingsAppearanceTrayStatusOn => {
            "On. Linux shows it where a tray host runs; GNOME needs an AppIndicator extension."
        }
        Key::SettingsAppearanceCloseToTrayTitle => "Close to tray",
        Key::SettingsAppearanceCloseToTrayNeedsTray => {
            "Needs the tray icon. Closing the window quits Muninn."
        }
        Key::SettingsAppearanceCloseToTrayOff => "Off. Closing the window quits Muninn.",
        Key::SettingsAppearanceCloseToTrayOn => {
            "On. Closing the window keeps Muninn in the tray; Quit Muninn there quits. If no tray host shows the icon, leave this off."
        }
        Key::SettingsAppearanceCloseToTrayNoIcon => {
            "On, but there is no tray icon right now, so closing the window quits Muninn."
        }
        Key::SettingsAppearanceStartupTitle => "Startup and shortcut",
        Key::SettingsAppearanceLoginTitle => "Start Muninn at login",
        Key::SettingsAppearanceLoginOn => "On. Muninn opens when you log in.",
        Key::SettingsAppearanceShortcutRegistering => "Registering…",
        Key::SettingsGraphReduceMotionTitle => "Reduce motion",
        Key::SettingsGraphReduceMotionCaption => {
            "Freezes the graph layout, skips camera glides and makes hover, selection and graph effects static. System follows your OS setting."
        }
        Key::SettingsGraphReduceMotionEnv => {
            "Currently on: MUNINN_REDUCED_MOTION is set, which overrides this choice."
        }
        Key::SettingsGraphReduceMotionSystemOn => "Currently on, following the system setting.",
        Key::SettingsGraphReduceMotionSystemOff => "Currently off, following the system setting.",
        Key::SettingsGraphReduceMotionOn => "Currently on.",
        Key::SettingsGraphReduceMotionOff => "Currently off.",
        Key::SettingsGraphEffectsTitle => "Graph effects",
        Key::SettingsGraphEffectsCaption => {
            "The same effect family draws the selection's border and the lines it touches: Classic keeps today's plain lines, Electric arcs both, Off draws plain lines with no effect layers. Reduced motion freezes whichever you pick."
        }
        Key::SettingsGraphIntensityTitle => "Effect intensity",
        Key::SettingsGraphIntensityCaption => {
            "Scales the Electric arcs and the delete burn. Balanced is the original look. Classic draws no arcs and Off draws nothing, so the arc choice is only visible with Electric; the burn still scales under either. Reduced motion still freezes the result."
        }
        Key::SyncHeadingProvider => "Sync provider",
        Key::SyncHeadingProviderCaption => {
            "Manual Sync Now only. One provider for this vault. Disconnect removes credentials, not notes."
        }
        Key::SyncProviderGithubName => "GitHub",
        Key::SyncProviderGithubDescription => {
            "GitHub selected. Use a dedicated repository and branch, not the Muninn source repository."
        }
        Key::SyncProviderDriveName => "Google Drive",
        Key::SyncProviderDriveShortName => "Drive",
        Key::SyncProviderDriveDescription => {
            "Google Drive selected. Sync keeps an app-managed commit history in Muninn's private app data in your Drive, not editable Markdown files."
        }
        Key::SyncProviderDriveCaption => {
            "Kept in Muninn's private app data in your Drive, not in a visible folder. Use the same store name on each device."
        }
        Key::SyncProviderTailscaleName => "Tailscale",
        Key::SyncProviderTailscaleDescription => {
            "Tailscale selected. One device hosts a commit store; the others join it by MagicDNS name over your tailnet."
        }
        Key::SyncFieldGithubOwner => "GitHub owner",
        Key::SyncFieldRepository => "Repository",
        Key::SyncFieldBranch => "Branch",
        Key::SyncFieldPrefix => "Prefix",
        Key::SyncFieldGithubClientId => "GitHub client id (optional)",
        Key::SyncFieldStoreName => "Store name",
        Key::SyncFieldGoogleClientId => "Google client id (optional)",
        Key::SyncFieldLoopbackPort => "Loopback port",
        Key::SyncFieldAllowedLogins => "Allowed Tailscale logins (optional, comma-separated)",
        Key::SyncFieldMagicDnsName => "Host MagicDNS name (device.tailnet.ts.net)",
        Key::SyncFieldSharedKey => "Shared key from the host",
        Key::SyncActionConnect => "Connect",
        Key::SyncActionSyncNow => "Sync now",
        Key::SyncActionDisconnect => "Disconnect",
        Key::SyncTailnetRoleLabel => "Tailscale role",
        Key::SyncTailnetRoleHost => "Host here",
        Key::SyncTailnetRoleJoin => "Join host",
        Key::PaletteTitle => "Command palette",
        Key::PaletteEmpty => "No notes or actions match",
        Key::PaletteFooter => "↑↓ move · Enter open · Esc close",
        Key::PaletteActionOpenLocalGraph => "Open local graph",
        Key::PaletteActionSwitchVault => "Switch vault",
        Key::PaletteActionOpenSettings => "Open settings",
        Key::PaletteActionNewNote => "New note",
        Key::PaletteActionTogglePreview => "Toggle Write / Read",
        Key::PaletteActionToggleGraph => "Toggle Graph",
        Key::PaletteActionToggleBoard => "Toggle Board",
        Key::PaletteActionToggleKanban => "Toggle Kanban",
        Key::PaletteActionToggleFocusMode => "Toggle focus mode",
        Key::PaletteActionNewKanban => "New Kanban",
        Key::PaletteActionConvertToKanban => "Convert empty note to Kanban",
        Key::PaletteHintGraphLocal => "Graph · Local",
        Key::PaletteHintVaults => "Vaults",
        Key::PaletteHintHelp => "Help",
        Key::PaletteHintKanban => "Kanban",
        Key::PaletteKeywordsOpenLocalGraph => "graph local focus neighbours scope",
        Key::PaletteKeywordsSwitchVault => "vault open recent folder switch",
        Key::PaletteKeywordsOpenSettings => "settings preferences options config help",
        Key::PaletteKeywordsNewNote => "new note create add",
        Key::PaletteKeywordsTogglePreview => "write read preview editor mode",
        Key::PaletteKeywordsToggleGraph => "graph global local mode",
        Key::PaletteKeywordsToggleBoard => "board whiteboard excalidraw drawing canvas sketch mode",
        Key::PaletteKeywordsToggleKanban => "kanban lanes cards tasks mode",
        Key::PaletteKeywordsToggleFocusMode => {
            "focus zen distraction free writing concentrate hide chrome dim"
        }
        Key::PaletteKeywordsNewKanban => "kanban create add lanes cards tasks",
        Key::PaletteKeywordsConvertToKanban => "kanban empty blank turn lanes cards tasks",
        Key::PaletteSectionRecent => "Recent",
        Key::PaletteSectionNotes => "Notes",
        Key::PaletteSectionActions => "Actions",
        Key::PaletteSectionKanbans => "Kanbans",
        Key::KanbanSectionDatesTitle => "Dates and times",
        Key::KanbanSectionDatesCaption => "How dates and times are typed, written and shown.",
        Key::KanbanSectionTagsTitle => "Tags",
        Key::KanbanSectionTagsCaption => "Tag chips and the kept tag-colour list.",
        Key::KanbanSectionMetadataTitle => "Linked note metadata",
        Key::KanbanSectionMetadataCaption => {
            "Keys drawn at a card's foot from its first linked note."
        }
        Key::KanbanSectionArchiveTitle => "Archive",
        Key::KanbanSectionArchiveCaption => {
            "What archiving writes and how many cards the archive keeps."
        }
        Key::KanbanSectionBoardTitle => "Board and cards",
        Key::KanbanSectionBoardCaption => "Lane width, card editing and the header buttons.",
        Key::KanbanSectionAppTitle => "App",
        Key::KanbanSectionAppCaption => "Choices for every board; there is no per-board override.",
        Key::KanbanChoiceEnter => "Enter",
        Key::KanbanChoiceShiftEnter => "Shift+Enter",
        Key::KanbanChoiceStart => "Start",
        Key::KanbanChoiceEnd => "End",
        Key::KanbanChoicePrepend => "Prepend",
        Key::KanbanChoiceAppend => "Append",
        Key::KanbanUiDefaultsNote => {
            "A board's own settings footer wins; otherwise these apply to every board."
        }
        Key::KanbanUiBoardSettings => "Board settings",
        Key::KanbanUiChangesWriteOnly => {
            "Changes write only that key to this note's settings footer and can be undone."
        }
        Key::KanbanUiNoMetadataKeys => "No metadata keys",
        Key::KanbanUiNoTagColours => "No tag colours",
        Key::KanbanUiKeptAsWritten => "Kept as written. Viewing never rewrites it.",
        Key::KanbanUiSourceBoardIgnored => "Board value ignored",
        Key::KanbanUiSourceBoardOverride => "Board override",
        Key::KanbanUiSourceAppIgnored => "App value ignored",
        Key::KanbanUiSourceAppDefault => "App default",
        Key::KanbanUiSourceDefault => "Default",
        Key::KanbanUiResetBuiltIn => "Reset to built-in",
        Key::KanbanUiResetBoard => "Reset board",
        Key::KanbanUiResetAppDefault => "Reset app default",
        Key::KanbanUiRemove => "Remove",
        Key::KanbanUiAdd => "Add",
        Key::KanbanUiContainsMarkdown => "Contains markdown",
        Key::KanbanUiNewMetadataKey => "New metadata key",
        Key::KanbanUiUnlimited => "Unlimited",
        Key::KanbanSettingDateTriggerLabel => "Date trigger",
        Key::KanbanSettingDateTriggerHelp => "Typed before a date, as in @{2026-11-12}.",
        Key::KanbanSettingTimeTriggerLabel => "Time trigger",
        Key::KanbanSettingTimeTriggerHelp => "Typed before a time, as in @@{14:30}.",
        Key::KanbanSettingDateFormatLabel => "Date format",
        Key::KanbanSettingDateFormatHelp => {
            "Written inside a new date token; YYYY, MM, DD, MMMM and Do are tokens."
        }
        Key::KanbanSettingTimeFormatLabel => "Time format",
        Key::KanbanSettingTimeFormatHelp => {
            "Written inside a new time token; HH, mm, h and A are tokens."
        }
        Key::KanbanSettingDateDisplayFormatLabel => "Date display format",
        Key::KanbanSettingDateDisplayFormatHelp => "How a date is shown at the card's foot.",
        Key::KanbanSettingShowRelativeDateLabel => "Show relative date",
        Key::KanbanSettingShowRelativeDateHelp => "Adds today or in N days beside the date.",
        Key::KanbanSettingHideDateInTitleLabel => "Hide date in title",
        Key::KanbanSettingHideDateInTitleHelp => {
            "Keeps date and time tokens out of the card's main text."
        }
        Key::KanbanSettingLinkDateToDailyNoteLabel => "Link date to daily note",
        Key::KanbanSettingLinkDateToDailyNoteHelp => {
            "Makes the shown date a link to its daily note."
        }
        Key::KanbanSettingHideTagsInTitleLabel => "Hide tags in title",
        Key::KanbanSettingHideTagsInTitleHelp => "Keeps #tag tokens out of the card's main text.",
        Key::KanbanSettingShowTagsAtBottomLabel => "Show tags at bottom",
        Key::KanbanSettingShowTagsAtBottomHelp => "Draws tag chips at the card's foot.",
        Key::KanbanSettingTagColorsLabel => "Tag colours",
        Key::KanbanSettingTagColorsHelp => "Kept as written. Muninn has no colour editor yet.",
        Key::KanbanSettingLinkedPageMetadataLabel => "Linked note metadata",
        Key::KanbanSettingLinkedPageMetadataHelp => {
            "Keys shown at the card's foot from its first linked note."
        }
        Key::KanbanSettingShowMissingMetadataLabel => "Show missing metadata",
        Key::KanbanSettingShowMissingMetadataHelp => {
            "Draws key: — when the linked note or the key is missing."
        }
        Key::KanbanSettingArchiveWithDateLabel => "Archive with date",
        Key::KanbanSettingArchiveWithDateHelp => {
            "Adds the date to a card's text when it is archived."
        }
        Key::KanbanSettingArchiveDateFormatLabel => "Archive date format",
        Key::KanbanSettingArchiveDateFormatHelp => {
            "The date format used when archiving; the same tokens as Date format."
        }
        Key::KanbanSettingArchiveDatePositionLabel => "Archive date position",
        Key::KanbanSettingArchiveDatePositionHelp => "Start or end of the archived card's text.",
        Key::KanbanSettingArchiveDateSeparatorLabel => "Archive date separator",
        Key::KanbanSettingArchiveDateSeparatorHelp => {
            "Text between the card and its date; an empty value joins them."
        }
        Key::KanbanSettingMaxArchiveCardsLabel => "Max archive cards",
        Key::KanbanSettingMaxArchiveCardsHelp => {
            "Oldest archive cards beyond this many go. 0 is unlimited."
        }
        Key::KanbanSettingLaneWidthLabel => "Lane width",
        Key::KanbanSettingLaneWidthHelp => {
            "Lane width in points; the board clamps what it draws to 120–720."
        }
        Key::KanbanSettingNewLineTriggerLabel => "New line trigger",
        Key::KanbanSettingNewLineTriggerHelp => {
            "In the card editor, Enter commits and Shift+Enter breaks the line, or the reverse."
        }
        Key::KanbanSettingNewCardInsertionLabel => "New card position",
        Key::KanbanSettingNewCardInsertionHelp => "Where + Add card puts the new card.",
        Key::KanbanSettingShowCheckboxesLabel => "Show checkboxes",
        Key::KanbanSettingShowCheckboxesHelp => "Draws each card's done checkbox.",
        Key::KanbanSettingShowFilterButtonLabel => "Filter button",
        Key::KanbanSettingShowFilterButtonHelp => "Shows the header's Filter button.",
        Key::KanbanSettingShowArchiveButtonLabel => "Archive button",
        Key::KanbanSettingShowArchiveButtonHelp => "Shows the header's Archive button.",
        Key::KanbanSettingShowOpenMarkdownButtonLabel => "Open as markdown button",
        Key::KanbanSettingShowOpenMarkdownButtonHelp => {
            "Shows the header's Open as markdown button."
        }
        Key::KanbanSettingNewNoteFolderLabel => "New note folder",
        Key::KanbanSettingNewNoteFolderHelp => {
            "Folder for New note from card; empty uses the folder New note uses."
        }
        Key::KanbanSettingNewNoteTemplateLabel => "New note template",
        Key::KanbanSettingNewNoteTemplateHelp => {
            "A note path whose text starts the new note; empty uses the app note template."
        }
        Key::KanbanSettingKanbanAutosaveLabel => "Autosave kanban edits",
        Key::KanbanSettingKanbanAutosaveHelp => {
            "Saves unsaved kanban edits once they rest. Off leaves saving to ⌘S."
        }
    }
}

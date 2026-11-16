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
        Key::Clear => "Clear",
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
        Key::SyncTailnetKeyLabel => "Shared key",
        Key::SyncTailnetKeyCopy => "Copy key",
        Key::SyncTailnetKeyShow => "Show key",
        Key::SyncTailnetKeyHide => "Hide key",
        Key::SyncTailnetKeyRotate => "Rotate key",
        Key::SyncStatusNotConnected => {
            "Not connected. Choose a provider and an explicit remote, then Connect."
        }
        Key::SyncStatusTailnetHost => {
            "This device hosts. Muninn serves the commit store on 127.0.0.1 while it is open; tailscale serve publishes it to your tailnet."
        }
        Key::SyncStatusTailnetJoin => {
            "Join a host. Enter the host's full MagicDNS name and paste the key from Copy key on the host."
        }
        Key::SyncStatusConnectedSharedKey => {
            "Connected. The shared key is in the OS credential store. Sync Now when ready."
        }
        Key::SyncStatusConnectedToken => {
            "Connected. The token is in the OS credential store. Sync Now when ready."
        }
        Key::SyncStatusAskingGithubCode => "Asking GitHub for a sign-in code…",
        Key::SyncStatusOpeningGoogle => "Opening Google sign-in in your browser…",
        Key::SyncStatusWaitingGoogle => {
            "Finish signing in to Google in your browser. Muninn is waiting on 127.0.0.1 for the answer."
        }
        Key::SyncStatusSignedInAfterDisconnect => {
            "Sign-in finished after Disconnect; its token was removed."
        }
        Key::SyncStatusTailnetJoined => {
            "Joined. The shared key is in the OS credential store. Sync Now reaches the host over your tailnet."
        }
        Key::SyncStatusSyncStarted => "Sync started. The window stays responsive.",
        Key::SyncStatusCancelling => {
            "Cancelling. A change the provider already accepted is kept and reconciled."
        }
        Key::SyncStatusNoteChangedWhileSyncing => {
            "The open note changed while sync ran; its text was not overwritten."
        }
        Key::SyncStatusNothingConnected => "Nothing is connected. Local notes were not changed.",
        Key::SyncStatusDisconnected => {
            "Disconnected. Credentials for this binding were removed. Local and remote notes were not deleted."
        }
        Key::SyncStatusSaveBeforeSync => {
            "Save or resolve the open note before Sync Now. Unsaved text is not uploaded or overwritten."
        }
        Key::SyncStatusAlreadyRunning => "A sync is already running.",
        Key::SyncStatusFinishSignIn => "Finish signing in first, or Disconnect to cancel.",
        Key::SyncStatusConnectProvider => "Connect a provider for this vault before Sync Now.",
        Key::SyncStatusWaitForRun => {
            "Wait for the running sync or sign-in to finish, or Disconnect to cancel it."
        }
        Key::SyncStatusWaitToRotateKey => {
            "Wait for the running sync to finish before rotating the key."
        }
        Key::SyncStatusKeyRotated => {
            "Key rotated; the old key no longer works. On each joined device, Disconnect and join again with the new key (Copy key)."
        }
        Key::SyncStatusCurrentConnectionKept => "The current connection was not changed.",
        Key::SyncErrorVaultNotCanonical => {
            "This vault folder cannot be canonicalized, so it cannot be bound."
        }
        Key::SyncErrorGithubScope => "Enter the GitHub owner and repository before connecting.",
        Key::SyncErrorLoopbackPort => "Enter a loopback port between 1024 and 65535 for the host.",
        Key::SyncErrorPasteSharedKey => {
            "Paste the shared key shown on the host. It is stored in the OS credential store, not in settings."
        }
        Key::SyncErrorNotTailnetHost => "This vault does not host a Tailscale store.",
        Key::SyncErrorSharedKeyMissing => "The shared key is missing from the credential store.",
        Key::SyncErrorTailnetKeyMissing => {
            "The Tailscale shared key is missing from the credential store. Disconnect and connect again."
        }
        Key::SyncErrorCancelled => "Sync cancelled",
        Key::SyncErrorTimedOutAfterSend => {
            "Timed out after a change was sent; reconcile before retrying"
        }
        Key::SyncErrorTimedOutBeforeResponse => "Timed out before the provider responded",
        Key::SyncErrorRateLimited => "Rate limited",
        Key::SyncErrorLeaseBusy => "Another Muninn instance is writing this vault",
        Key::SyncErrorSettingsNotObject => "Sync settings were not an object, so sync is off.",
        Key::SyncErrorSettingsVersion => "Sync settings version is not supported, so sync is off.",
        Key::SyncErrorSettingsVaultList => "Sync vault list was missing, so sync is off.",
        Key::SyncErrorSettingsBindingInvalid => {
            "A sync binding was invalid and was not loaded. Sync is off until it is replaced."
        }
        Key::KanbanUiNotInsideVault => "the kanban is not inside the open vault",
        Key::KanbanUiFieldRefused => {
            "Not quitting: the kanban could not be saved. Fix what stopped the save, or undo the edits, then quit again."
        }
        Key::ErrorSaveKanbanBeforeRestore => {
            "Save or discard the current kanban before restoring another"
        }
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
        Key::BoardPageTitle => "Boards",
        Key::BoardPageNew => "New board",
        Key::BoardPageEmptyTitle => "No boards yet",
        Key::BoardPageEmptyDetail => "Make one with New board",
        Key::BoardPageEmptyHint => {
            "A board is an Excalidraw file in your vault. Make one to start drawing."
        }
        Key::BoardPageNoSelectionTitle => "No board selected",
        Key::BoardPageNoSelectionDetail => "Pick a board on the left to see what it holds",
        Key::BoardPageTitleLabel => "Board name; double-click to rename",
        Key::BoardPageUnreadableTitle => "This board could not be read",
        Key::BoardPageNoOpenTitle => "No board open",
        Key::BoardPageNoOpenDetail => {
            "Pick a board on the left, or make a new one. Boards are Excalidraw files in your vault."
        }
        Key::BoardPageEmptyCanvas => "Nothing drawn on this board yet",
        Key::BoardPageHint => {
            "Click/drag select · Del delete · ⌘Z undo · R D O A L P draw · H pan · ⌘L link note · ⌘S save · 0 fit"
        }
        Key::BoardToolToolbar => "Board tools",
        Key::BoardToolHand => "Hand",
        Key::BoardToolHandHelp => "Pan the board (H)",
        Key::BoardToolSelect => "Select",
        Key::BoardToolSelectHelp => "Select and move elements (V)",
        Key::BoardToolRectangle => "Rectangle",
        Key::BoardToolRectangleHelp => "Draw a rectangle (R)",
        Key::BoardToolDiamond => "Diamond",
        Key::BoardToolDiamondHelp => "Draw a diamond (D)",
        Key::BoardToolEllipse => "Ellipse",
        Key::BoardToolEllipseHelp => "Draw an ellipse (O)",
        Key::BoardToolArrow => "Arrow",
        Key::BoardToolArrowHelp => "Draw an arrow (A)",
        Key::BoardToolLine => "Line",
        Key::BoardToolLineHelp => "Draw a line (L)",
        Key::BoardToolFreedraw => "Freehand",
        Key::BoardToolFreedrawHelp => "Draw freehand (P or X)",
        Key::BoardToolText => "Text",
        Key::BoardToolTextHelp => "Place text, then type (T)",
        Key::BoardLinkTitle => "Link note",
        Key::BoardLinkHelp => "Link the selected elements to a note (⌘L)",
        Key::BoardLinkName => "Note name or path",
        Key::BoardLinkHint => "a name or vault path · empty clears",
        Key::BoardRenameName => "Board name",
        Key::BoardPaletteTitle => "Board colours",
        Key::BoardPaletteHelp => "Stroke and fill colours",
        Key::BoardPaletteOpen => "Colour palette",
        Key::BoardPaletteClose => "Close colour palette",
        Key::BoardPaletteStroke => "Stroke",
        Key::BoardPaletteFill => "Fill",
        Key::BoardPaletteSwatchBlack => "black",
        Key::BoardPaletteSwatchGrey => "grey",
        Key::BoardPaletteSwatchRed => "red",
        Key::BoardPaletteSwatchOrange => "orange",
        Key::BoardPaletteSwatchYellow => "yellow",
        Key::BoardPaletteSwatchGreen => "green",
        Key::BoardPaletteSwatchBlue => "blue",
        Key::BoardPaletteSwatchPurple => "purple",
        Key::BoardPaletteSwatchTransparent => "transparent",
        Key::GraphPageEmpty => "No notes to draw",
        Key::GraphPageFindNode => "Find graph node",
        Key::GraphPageFindPlaceholder => "Find node · Enter",
        Key::GraphPageSelected => "SELECTED",
        Key::GraphPageHint => {
            "Arrows focus · Enter open · +/- zoom · 0/F fit · I isolate · Drag pan · Scroll zoom"
        }
        Key::GraphScopeGlobal => "Global",
        Key::GraphScopeLocal => "Local",
        Key::GraphActionFit => "Fit",
        Key::GraphActionReset => "Reset",
        Key::GraphActionIsolate => "Isolate",
        Key::GraphActionOpenNote => "Open note",
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
        Key::KanbanPageTitle => "Kanban",
        Key::KanbanPageViewSwitcher => "Kanban view",
        Key::KanbanPageFilter => "Filter",
        Key::KanbanPageClearFilters => "Clear filters",
        Key::KanbanPageOpenMarkdown => "Open as markdown",
        Key::KanbanPageResolveConflict => "Resolve conflict",
        Key::KanbanPageEmptyTitle => "No kanban open",
        Key::KanbanPageEmptyDetail => {
            if cfg!(target_os = "macos") {
                "Select a kanban note, then press ⌘5 to see it as a board."
            } else {
                "Select a kanban note, then press Ctrl+5 to see it as a board."
            }
        }
        Key::KanbanPageInvalidTitle => "This note can't be shown as a kanban",
        Key::KanbanPageInvalidDetail => {
            "Its settings footer is unfinished or is not valid JSON. The file is left as it is."
        }
        Key::KanbanLaneComplete => "Complete",
        Key::KanbanLaneNew => "New lane",
        Key::KanbanLaneAdd => "Add lane",
        Key::KanbanLaneTitle => "Lane title",
        Key::KanbanLaneCardLimit => "Card limit",
        Key::KanbanLaneNoLimit => "No limit",
        Key::KanbanCardText => "Card text",
        Key::KanbanCardOpenLink => "Open link",
        Key::KanbanViewBoard => "Board",
        Key::KanbanViewList => "List",
        Key::KanbanViewTable => "Table",
        Key::KanbanViewBoardTab => "Board view",
        Key::KanbanViewListTab => "List view",
        Key::KanbanViewTableTab => "Table view",
        Key::KanbanViewNoLanes => "No lanes on this board",
        Key::KanbanViewNoCards => "No cards",
        Key::KanbanViewNoMatchingCards => "No matching cards",
        Key::KanbanViewCardsByLane => "Cards by lane",
        Key::KanbanViewColumns => "Lane, Card, Date, Tags, Progress",
        Key::KanbanViewColumnLane => "Lane",
        Key::KanbanViewColumnCard => "Card",
        Key::KanbanViewColumnDate => "Date",
        Key::KanbanViewColumnTags => "Tags",
        Key::KanbanViewColumnProgress => "Progress",
        Key::KanbanFilterFind => "Find in board",
        Key::KanbanFilterTitle => "Filter cards",
        Key::KanbanFilterText => "Text",
        Key::KanbanFilterDate => "Date",
        Key::KanbanFilterAnyDate => "Any date",
        Key::KanbanFilterOverdue => "Overdue",
        Key::KanbanFilterStatus => "Status",
        Key::KanbanFilterAny => "Any",
        Key::KanbanFilterOpen => "Open",
        Key::KanbanFilterDone => "Done",
        Key::KanbanFilterClearAll => "Clear all",
        Key::KanbanFilterDateError => "A date filter is YYYY-MM-DD; the range is not applied.",
        Key::KanbanFilterPreviousMatch => "Previous match",
        Key::KanbanFilterNextMatch => "Next match",
        Key::KanbanFilterCloseFind => "Close find",
        Key::KanbanArchiveEmpty => "No archived cards.",
        Key::KanbanArchiveDelete => "Delete",
        Key::KanbanArchiveRestore => "Restore",
        Key::KanbanArchiveRestoreTo => "Restore to…",
        Key::KanbanPickerAddDate => "Add date",
        Key::KanbanPickerAddTime => "Add time",
        Key::KanbanPickerAddADate => "Add a date",
        Key::KanbanPickerAddATime => "Add a time",
        Key::KanbanPickerHour => "Hour",
        Key::KanbanPickerMinute => "Minute",
        Key::KanbanPickerNow => "Now",
        Key::KanbanPickerPreviousMonth => "Previous month",
        Key::KanbanPickerNextMonth => "Next month",
        Key::KanbanPickerDays => "Days",
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
        Key::StatusSaving => "Saving…",
        Key::StatusSaved => "Saved",
        Key::StatusAutosaveOn => "Autosave on",
        Key::StatusRetry => "Retry",
        Key::StatusUndo => "Undo",
        Key::RecoveryRestore => "Restore",
        Key::RecoveryRestoreAll => "Restore all",
        Key::RecoveryDismiss => "Dismiss",
        Key::RecoveryWriteEdits => "— Write edits",
        Key::RecoveryKanbanEdits => "— Kanban edits",
        Key::BrokenAllResolve => "All links resolve",
        Key::BrokenTitle => "Broken links",
        Key::TopbarNoNoteSelected => "No note selected",
        Key::TopbarOpenAsKanban => "Open as kanban",
        Key::TrashTitle => "Trash",
        Key::TrashEmptyAction => "Empty trash",
        Key::TrashExplainer => {
            "Deleted notes stay here until undo or permanent removal. At most 500 files or 100 MB are kept; the oldest timestamp folder is pruned first."
        }
        Key::TrayShow => "Show Muninn",
        Key::TrayNewNote => "New note",
        Key::TraySyncNow => "Sync now",
        Key::TrayQuit => "Quit Muninn",
        Key::TrayNotConnected => "Not connected.",
        Key::TrayTooltip => "Muninn",
        Key::ErrorEnterVaultPath => "Enter a vault folder path",
        Key::ErrorEnterNewVaultPath => "Enter a folder path for the new vault",
        Key::ErrorSaveNoteBeforeSwitch => {
            "Save or resolve the current note before switching vaults"
        }
        Key::ErrorSaveBoardBeforeSwitch => "Save the current board before switching vaults",
        Key::ErrorSaveKanbanBeforeSwitch => "Save the current kanban before switching vaults",
        Key::ErrorSaveNoteBeforeRestore => {
            "Save or discard the current note before restoring another"
        }
        Key::ErrorSaveBoardBeforeRestore => {
            "Save or discard the current board before restoring another"
        }
        Key::ErrorSaveBoardBeforeRescan => "Save the current board before rescanning the vault",
        Key::ErrorOpenerHintUnix => "Install xdg-utils (for xdg-open) or open the file manually.",
        Key::ErrorOpenerHintMacos => "The system `open` command failed; open the file from Finder.",
        Key::ErrorOpenerHintWindows => "The system opener failed; open the file from Explorer.",
        Key::ErrorOpenerHintOther => "Open the file manually.",
        Key::ErrorDocumentBoard => "board",
        Key::ErrorDocumentKanban => "kanban",
        Key::StatusTrashEmptied => "Trash emptied",
        Key::StatusLinkCopied => "Copied a link to the card",
        Key::ConflictExternalChange => "External change detected",
        Key::ConflictFileDeleted => "file deleted externally",
        Key::ConflictChangedOnDisk => "Changed on disk by another app",
        Key::ConflictMissingOnDisk => "Removed or renamed on disk",
        Key::ConflictKeepMine => "Keep mine",
        Key::ConflictLoadTheirs => "Load theirs",
        Key::ConflictSaveCopy => "Save copy",
        Key::ConflictDiscard => "Discard",
        Key::CommonAppName => "Muninn",
        Key::CommonDelete => "Delete",
        Key::CommonEdit => "Edit",
        Key::CommonMove => "Move",
        Key::CommonNewNote => "New note",
        Key::CommonOpen => "Open",
        Key::CommonRename => "Rename",
        Key::CommonResults => "Results",
        Key::CommonVaultRoot => "Vault root",
        Key::CommonDismiss => "Dismiss",
        Key::CommonOn => "On",
        Key::CommonOff => "Off",
        Key::SidebarNoNotesTitle => "No notes yet",
        Key::SidebarNoNotesHint => "Add .md files here, or make one below.",
        Key::SidebarNoMatchTitle => "No notes match",
        Key::SidebarNoMatchHint => "Try a different search.",
        Key::SidebarSearch => "Search notes",
        Key::SidebarLinksOk => "links ok",
        Key::SidebarKanbans => "KANBANS",
        Key::SidebarBoards => "BOARDS",
        Key::SidebarTheVaultRoot => "the vault root",
        Key::SettingsTitle => "Settings",
        Key::TopbarSettingsHelp => "Settings (Ctrl+,)",
        Key::TopbarHideSidebar => "Hide sidebar",
        Key::TopbarShowSidebar => "Show sidebar",
        Key::SettingsChangesSave => "Changes save automatically.",
        Key::SettingsClose => "Close settings (Esc)",
        Key::SettingsSection => "Settings section",
        Key::SettingsAutosave => "Autosave",
        Key::SettingsDiagnostics => "Diagnostics",
        Key::SettingsCopyDiagnostics => "Copy diagnostics",
        Key::SettingsReportIssue => "Report an issue on GitHub",
        Key::SettingsThemeOmarchy => "Omarchy",
        Key::SettingsThemeSystem => "System",
        Key::SettingsThemeDark => "Dark",
        Key::SettingsThemeLight => "Light",
        Key::SettingsMotionSystem => "System",
        Key::SettingsMotionOn => "On",
        Key::SettingsMotionOff => "Off",
        Key::SettingsEffectsClassic => "Classic",
        Key::SettingsEffectsElectric => "Electric",
        Key::SettingsEffectsOff => "Off / Minimal",
        Key::SettingsIntensitySubtle => "Subtle",
        Key::SettingsIntensityBalanced => "Balanced",
        Key::SettingsIntensityIntense => "Intense",
        Key::WelcomeGraphLabel => "Open the link graph",
        Key::WelcomeGraphHelp => "See how notes connect through [[links]]",
        Key::WelcomeGraphPill => "[[a note]] → graph",
        Key::PreviewEmptyNote => "This note is empty. Write something in the editor.",
        Key::PreviewStartWriting => "Make a note to start writing.",
        Key::PreviewEditor => "Note editor",
        Key::PreviewEditorPlaceholder => "Start writing. Type [[ to link another note.",
        Key::PreviewCompletions => "Completions",
        Key::PreviewAttachments => "ATTACHMENTS",
        Key::PreviewAttachmentsNote => "Files stay outside the note index",
        Key::PreviewOpen => "Open",
        Key::PreviewOpenBoard => "Open board",
        Key::PreviewMissingFile => "missing file",
        Key::PreviewBoardKind => "board",
        Key::PreviewAttachmentKind => "attachment",
        Key::PreviewMissing => "missing",
        Key::PreviewNoOutgoing => "No outgoing links",
        Key::PreviewNoBacklinks => "No backlinks yet",
        Key::PreviewLinks => "LINKS",
        Key::PreviewOutgoing => "OUTGOING",
        Key::PreviewBacklinks => "BACKLINKS",
        Key::PreviewGraph => "GRAPH",
        Key::PreviewGraphHint => "See this note and everything it touches.",
        Key::PreviewFind => "Find in note",
        Key::StartupScanning => "Scanning your vault",
        Key::StartupScanCancelled => "Scan cancelled",
        Key::StartupCouldNotOpen => "Could not open the vault",
        Key::StartupRetryHint => {
            "Retry scans the vault again, or pick another folder in Settings (⌘,)."
        }
        Key::StartupPickFolderHint => {
            "Pick another folder in Settings (⌘,), or pass --vault / set MUNINN_VAULT."
        }
        Key::ModeWrite => "Write",
        Key::ModeRead => "Read",
        Key::ModeGraph => "Graph",
        Key::ModeBoard => "Board",
        Key::ModeKanban => "Kanban",
        Key::FocusExit => "Exit focus mode",
        Key::FocusEnter => "Focus mode",
        Key::MenuOpenExternally => "Open externally",
        Key::MenuReveal => "Reveal in file manager",
        Key::MenuMoveTo => "Move to…",
        Key::MenuNewNoteHere => "New note here",
        Key::MenuNewFolder => "New folder…",
        Key::MenuInsertBefore => "Insert card before",
        Key::MenuInsertAfter => "Insert card after",
        Key::MenuDuplicate => "Duplicate",
        Key::MenuMarkOpen => "Mark open",
        Key::MenuMarkDone => "Mark done",
        Key::MenuChangeDate => "Change date…",
        Key::MenuAddDate => "Add a date…",
        Key::MenuChangeTime => "Change time…",
        Key::MenuAddTime => "Add a time…",
        Key::MenuSplitCard => "Split card",
        Key::MenuCopyLink => "Copy link to card",
        Key::MenuNewNoteFromCard => "New note from card",
        Key::MenuMarkAllDone => "Mark all cards done",
        Key::MenuNoteActions => "Note actions",
        Key::MenuFolderActions => "Folder actions",
        Key::MenuBoardActions => "Board actions",
        Key::MenuCardActions => "Card actions",
        Key::MenuLaneActions => "Lane actions",
        Key::MenuArchiveActions => "Archive actions",
        Key::MenuMoveToLane => "Move to lane",
        Key::LinkImpactHere => "here",
        Key::LinkImpactIntoFolder => "into this folder",
        Key::LinkThisNote => "This note",
        Key::LinkNameFallback => "Link",
        Key::RenameRewriteLinks => "Rewrite links to new name",
        Key::RenameLeaveDangling => "Leave links dangling",
        Key::DeleteNoteQuestion => "Delete this note?",
        Key::DeleteFolderQuestion => "Delete folder? (empty only)",
        Key::NewNoteName => "New note name",
        Key::NoteNamePlaceholder => "Note name",
        Key::RenameNoteName => "Rename note",
        Key::FolderNamePlaceholder => "Folder name",
        Key::NewFolderName => "New folder name",
        Key::RenameFolderName => "Rename folder",
        Key::PaletteMissingNote => "Missing note",
        Key::PaletteKanbanRoot => "Kanban · Vault root",
        Key::BoardRenameSaveFirst => "Save the board first: it could not be saved",
        Key::KanbanFilterToday => "today",
        Key::KanbanFilterOverdueWord => "overdue",
        Key::KanbanFilterOpenWord => "open",
        Key::KanbanFilterDoneWord => "done",
        Key::KanbanDailyNoteLink => "Open the date's daily note",
        Key::KanbanLaneMoveUp => "Move up",
        Key::KanbanLaneMoveTop => "Move to top",
        Key::KanbanLaneMoveDown => "Move down",
        Key::KanbanLaneMoveBottom => "Move to bottom",
        Key::KanbanLaneMovePreviousList => "Move to previous list",
        Key::KanbanLaneMoveNextList => "Move to next list",
        Key::KanbanLaneMoveToList => "Move to list…",
        Key::KanbanLaneInsertBefore => "Insert lane before",
        Key::KanbanLaneInsertAfter => "Insert lane after",
        Key::KanbanLaneMoveLeft => "Move lane left",
        Key::KanbanLaneMoveRight => "Move lane right",
        Key::KanbanLaneRename => "Rename lane",
        Key::KanbanLaneLimit => "Card limit…",
        Key::KanbanLaneMarkNotDone => "Mark as not done lane",
        Key::KanbanLaneMarkDone => "Mark as done lane",
        Key::KanbanLaneSortText => "Sort by text",
        Key::KanbanLaneSortDate => "Sort by date",
        Key::KanbanLaneSortTags => "Sort by tags",
        Key::KanbanLaneDelete => "Delete lane",
        Key::KanbanLaneDeleteCards => "Delete the lane and its cards",
        Key::KanbanLaneDeleteText => "Delete the lane and its note text",
        Key::KanbanCardFallback => "Card",
        Key::KanbanCardMarkDone => "Mark done",
        Key::KanbanCardMarkOpen => "Mark not done",
        Key::KanbanCardChecklistItem => "Checklist item",
        Key::KanbanEditAddCard => "add the card",
        Key::KanbanEditEditCard => "edit the card",
        Key::KanbanEditDeleteCard => "delete the card",
        Key::KanbanEditDuplicateCard => "duplicate the card",
        Key::KanbanEditSplitCard => "split the card",
        Key::KanbanEditChangeDoneMark => "change the done mark",
        Key::KanbanEditToggleChecklistItem => "toggle the checklist item",
        Key::KanbanEditMarkCardsDone => "mark the cards done",
        Key::KanbanEditAddLane => "add the lane",
        Key::KanbanEditRenameLane => "rename the lane",
        Key::KanbanEditClearLimit => "clear the limit",
        Key::KanbanEditSetLimit => "set the limit",
        Key::KanbanEditDeleteLane => "delete the lane",
        Key::KanbanEditChangeDoneLane => "change the done lane",
        Key::KanbanEditSortLane => "sort the lane",
        Key::KanbanEditCollapseLane => "collapse the lane",
        Key::KanbanEditMoveCard => "move the card",
        Key::KanbanEditChangeDate => "change the date",
        Key::KanbanEditChangeTime => "change the time",
        Key::KanbanEditClearDate => "clear the date",
        Key::KanbanEditClearTime => "clear the time",
        Key::KanbanEditChangeBoardSetting => "change the board setting",
        Key::KanbanEditResetBoardSetting => "reset the board setting",
        Key::KanbanEditCopyCardLink => "copy the card link",
        Key::KanbanEditMakeNoteFromCard => "make a note from the card",
        Key::KanbanEditArchiveCard => "archive the card",
        Key::KanbanEditRestoreCard => "restore the archived card",
        Key::KanbanEditDeleteArchivedCard => "delete the archived card",
        Key::KanbanStatusLaneRefused => "the lane being edited was refused",
        Key::KanbanStatusCardRefused => "the card being edited was refused",
        Key::KanbanStatusNotInsideVault => "the kanban is not inside the open vault",
        Key::KanbanStatusNoteGone => "the note is no longer in the vault",
        Key::KanbanStatusFileUnchecked => "the file could not be checked",
        Key::KanbanStatusFileRemoved => "the file was removed",
        Key::KanbanStatusFileChanged => "the file changed on disk",
        Key::KanbanTimeInvalid => "A time needs an hour 0–23 and a minute 0–59",
        Key::KanbanLaneLimitInvalid => "A card limit is a whole number",
        Key::VaultEmptyFolderName => "empty folder name",
        Key::VaultCannotRenameRoot => "cannot rename the root",
        Key::VaultCannotDeleteRoot => "cannot delete the root",
        Key::VaultDestinationExists => "destination exists",
        Key::BoardParseNotObject => "a board must be a JSON object",
        Key::BoardParseNotExcalidraw => "not an Excalidraw scene (type is not \"excalidraw\")",
        Key::BoardParseVersionNumber => "scene version must be a whole number",
        Key::BoardParseSourceString => "scene source must be a string",
        Key::BoardParseElementsArray => "scene elements must be an array",
        Key::BoardStatusUnreadable => "the board could not be read, so it is not written",
        Key::BoardStatusOtherVault => "the board belongs to another vault",
        Key::BoardStatusInsideVault => "the board is not inside the open vault",
        Key::KanbanEditMoveLane => "move the lane",
        Key::AutostartPathNotUtf8 => "the executable path is not UTF-8",
        Key::AutostartPathLineBreak => "the executable path has a line break",
        Key::NoteNoLongerIndexed => "note is no longer indexed",
        Key::CoreBoardNameEmpty => "cannot be empty",
        Key::CoreBoardNameSeparators => "cannot contain / or \\",
        Key::CoreBoardNameHidden => "cannot start with a dot",
        Key::CoreBoardNameForbidden => "cannot contain : * ? \" < > | or control characters",
        Key::CoreBoardNameTrailingDot => "cannot end with a dot",
        Key::CoreBoardNameReserved => "cannot be a reserved device name",
    }
}

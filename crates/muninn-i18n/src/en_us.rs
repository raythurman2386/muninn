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
    }
}

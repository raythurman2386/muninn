//! The catalog key type.
//!
//! `keys!` is a local declarative macro rather than a proc macro: the key set
//! is small, and a macro keeps `Key`, `Key::ALL` and `Key::id` in one place
//! with no build-time dependency. Adding a key here without English text in
//! `en_us` fails `cargo check`, because that match is exhaustive.

/// Declares [`Key`], its [`Key::ALL`] slice and its stable ids.
///
/// One row per user-visible string: `Variant => "dotted.id"`. The id is
/// deliberately not the English text, so a copy edit never changes it.
macro_rules! keys {
    ($($variant:ident => $id:literal,)*) => {
        /// The identity of one piece of user-visible copy.
        ///
        /// Call sites name a key; [`crate::t`] resolves it to text in the
        /// current language. A key never carries the text itself, so the same
        /// call site can render any language.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Key {
            $(
                /// See [`Key::id`] for the stable id and [`crate::t`] for the text.
                $variant,
            )*
        }

        impl Key {
            /// Every key in the catalog, in declaration order.
            ///
            /// Tests use this to check for duplicate ids and missing text
            /// without naming each key.
            pub const ALL: &'static [Key] = &[
                $(Key::$variant,)*
            ];

            /// The stable dotted id, for debugging and future catalogs.
            ///
            /// Use this in log lines and test failure messages; use
            /// [`crate::t`] for anything a user reads. The id is part of the
            /// catalog's contract: renaming one is a catalog change, not a
            /// copy edit.
            pub const fn id(self) -> &'static str {
                match self {
                    $(Key::$variant => $id,)*
                }
            }
        }
    };
}

keys! {
    AddCard => "kanban.add_card",
    Archive => "kanban.archive",
    Today => "date.today",
    OverLimit => "kanban.over_limit",
    KeyboardShortcuts => "help.keyboard_shortcuts",
    Close => "common.close",
    Save => "common.save",
    Cancel => "common.cancel",
    LinkedNote => "link.linked_note",
    NoBoards => "board.no_boards",
    HelpLastLogLines => "help.last_log_lines",
    HelpGroupNavigation => "help.group.navigation",
    HelpNavFocusSearch => "help.nav.focus_search",
    HelpNavMoveHighlightedResult => "help.nav.move_highlighted_result",
    HelpNavOpenHighlightedResult => "help.nav.open_highlighted_result",
    HelpNavBackInNoteHistory => "help.nav.back_in_note_history",
    HelpNavForwardInNoteHistory => "help.nav.forward_in_note_history",
    HelpNavWriteReadGraphBoardKanban => "help.nav.write_read_graph_board_kanban",
    HelpNavFocusMode => "help.nav.focus_mode",
    HelpNavCloseOutermostLayer => "help.nav.close_outermost_layer",
    HelpNavOpenCommandPalette => "help.nav.open_command_palette",
    HelpGroupEdit => "help.group.edit",
    HelpEditFindInOpenNote => "help.edit.find_in_open_note",
    HelpEditFindInBoard => "help.edit.find_in_board",
    HelpEditHideCardsByTextTagDateOrState => "help.edit.hide_cards_by_text_tag_date_or_state",
    HelpEditPerBoardOverrides => "help.edit.per_board_overrides",
    HelpEditInsertWikilink => "help.edit.insert_wikilink",
    HelpEditNewNote => "help.edit.new_note",
    HelpEditSaveNow => "help.edit.save_now",
    HelpEditToggleWriteRead => "help.edit.toggle_write_read",
    HelpEditWikilinkCompletion => "help.edit.wikilink_completion",
    HelpEditVaultTagCompletion => "help.edit.vault_tag_completion",
    HelpEditIndentFourSpaces => "help.edit.indent_four_spaces",
    HelpEditPasteRichText => "help.edit.paste_rich_text",
    HelpEditRenameSelectedNote => "help.edit.rename_selected_note",
    HelpEditDeleteSelectedNote => "help.edit.delete_selected_note",
    HelpGroupKanban => "help.group.kanban",
    HelpKanbanMoveThroughControls => "help.kanban.move_through_controls",
    HelpKanbanMoveBetweenLaneCards => "help.kanban.move_between_lane_cards",
    HelpKanbanMoveSelectionAcrossLanes => "help.kanban.move_selection_across_lanes",
    HelpKanbanToggleCardDone => "help.kanban.toggle_card_done",
    HelpKanbanMoveSelectedCard => "help.kanban.move_selected_card",
    HelpKanbanMoveCardToEdgeOrLane => "help.kanban.move_card_to_edge_or_lane",
    HelpKanbanOpenCardMenu => "help.kanban.open_card_menu",
    HelpKanbanEditCard => "help.kanban.edit_card",
    HelpKanbanCloseMenuOrConfirmation => "help.kanban.close_menu_or_confirmation",
    HelpGroupGraph => "help.group.graph",
    HelpGraphToggleGraph => "help.graph.toggle_graph",
    HelpGraphToggleGlobalLocal => "help.graph.toggle_global_local",
    HelpGraphMoveFocusRing => "help.graph.move_focus_ring",
    HelpGraphOpenFocusedNote => "help.graph.open_focused_note",
    HelpGraphZoomInOut => "help.graph.zoom_in_out",
    HelpGraphFitVisibleNodes => "help.graph.fit_visible_nodes",
    HelpGraphToggleIsolate => "help.graph.toggle_isolate",
    HelpGraphLeaveGraph => "help.graph.leave_graph",
    HelpGroupBoard => "help.group.board",
    HelpBoardPanWithDrag => "help.board.pan_with_drag",
    HelpBoardZoomAtPointer => "help.board.zoom_at_pointer",
    HelpBoardZoomAtCanvasCentre => "help.board.zoom_at_canvas_centre",
    HelpBoardPanTheBoard => "help.board.pan_the_board",
    HelpBoardFitTheDrawing => "help.board.fit_the_drawing",
    HelpBoardShapeTools => "help.board.shape_tools",
    HelpBoardDrawShape => "help.board.draw_shape",
    HelpBoardPlaceText => "help.board.place_text",
    HelpBoardEditTextLabel => "help.board.edit_text_label",
    HelpBoardDropTheTool => "help.board.drop_the_tool",
    HelpBoardSelectElementOrToggle => "help.board.select_element_or_toggle",
    HelpBoardNudgeSelection => "help.board.nudge_selection",
    HelpBoardDeleteSelection => "help.board.delete_selection",
    HelpBoardSelectEveryElement => "help.board.select_every_element",
    HelpBoardLinkToNote => "help.board.link_to_note",
    HelpBoardOpenLinkedNote => "help.board.open_linked_note",
    HelpBoardOpenClickedLinkedNote => "help.board.open_clicked_linked_note",
    HelpBoardUndoRedo => "help.board.undo_redo",
    HelpBoardSaveDrawing => "help.board.save_drawing",
    HelpBoardEscapeClearsSelection => "help.board.escape_clears_selection",
    HelpGroupSidebar => "help.group.sidebar",
    HelpSidebarOpenThisHelp => "help.sidebar.open_this_help",
    HelpSidebarOpenHelpWhenNotTyping => "help.sidebar.open_help_when_not_typing",
    HelpSidebarMoveBetweenPanes => "help.sidebar.move_between_panes",
    HelpSidebarMoveThroughAList => "help.sidebar.move_through_a_list",
    HelpSidebarOpenFocusedRow => "help.sidebar.open_focused_row",
    HelpSidebarNoteAndFolderActions => "help.sidebar.note_and_folder_actions",
    HelpSidebarSettings => "help.sidebar.settings",
    SettingsTabVaults => "settings.tab.vaults",
    SettingsTabEditor => "settings.tab.editor",
    SettingsTabAppearance => "settings.tab.appearance",
    SettingsTabGraph => "settings.tab.graph",
    SettingsTabSync => "settings.tab.sync",
    SettingsTabKanban => "settings.tab.kanban",
    PaletteTitle => "palette.title",
    PaletteEmpty => "palette.empty",
    PaletteFooter => "palette.footer",
    PaletteActionOpenLocalGraph => "palette.action.open_local_graph",
    PaletteActionSwitchVault => "palette.action.switch_vault",
    PaletteActionOpenSettings => "palette.action.open_settings",
    PaletteActionNewNote => "palette.action.new_note",
    PaletteActionTogglePreview => "palette.action.toggle_preview",
    PaletteActionToggleGraph => "palette.action.toggle_graph",
    PaletteActionToggleBoard => "palette.action.toggle_board",
    PaletteActionToggleKanban => "palette.action.toggle_kanban",
    PaletteActionToggleFocusMode => "palette.action.toggle_focus_mode",
    PaletteActionNewKanban => "palette.action.new_kanban",
    PaletteActionConvertToKanban => "palette.action.convert_to_kanban",
    PaletteHintGraphLocal => "palette.hint.graph_local",
    PaletteHintVaults => "palette.hint.vaults",
    PaletteHintHelp => "palette.hint.help",
    PaletteHintKanban => "palette.hint.kanban",
    PaletteKeywordsOpenLocalGraph => "palette.keywords.open_local_graph",
    PaletteKeywordsSwitchVault => "palette.keywords.switch_vault",
    PaletteKeywordsOpenSettings => "palette.keywords.open_settings",
    PaletteKeywordsNewNote => "palette.keywords.new_note",
    PaletteKeywordsTogglePreview => "palette.keywords.toggle_preview",
    PaletteKeywordsToggleGraph => "palette.keywords.toggle_graph",
    PaletteKeywordsToggleBoard => "palette.keywords.toggle_board",
    PaletteKeywordsToggleKanban => "palette.keywords.toggle_kanban",
    PaletteKeywordsToggleFocusMode => "palette.keywords.toggle_focus_mode",
    PaletteKeywordsNewKanban => "palette.keywords.new_kanban",
    PaletteKeywordsConvertToKanban => "palette.keywords.convert_to_kanban",
    PaletteSectionRecent => "palette.section.recent",
    PaletteSectionNotes => "palette.section.notes",
    PaletteSectionActions => "palette.section.actions",
    PaletteSectionKanbans => "palette.section.kanbans",
}

//! Board slice 8: the collapsible notes sidebar, and New board.

use std::path::Path;

use crate::{Mode, State};

/// The name a new board starts with; `Untitled board 2`… when it is taken.
pub(crate) const NEW_BOARD_TITLE: &str = "Untitled board";

impl State {
    /// Whether the notes sidebar shows in the current mode.
    pub fn sidebar_shown(&self) -> bool {
        if self.mode == Mode::Board {
            self.sidebar_open_board
        } else {
            self.sidebar_open
        }
    }

    /// Show or hide the notes sidebar in the current mode. Board keeps its
    /// own state, so entering Board collapses it (until it is reopened there)
    /// and leaving Board restores whatever the other modes had.
    pub fn toggle_sidebar(&mut self) {
        let open = if self.mode == Mode::Board {
            &mut self.sidebar_open_board
        } else {
            &mut self.sidebar_open
        };
        *open = !*open;
        if !self.sidebar_shown() {
            self.release_sidebar_fields();
        }
    }

    /// The notes sidebar just went out of sight (its toggle, or a mode
    /// switch that collapses it): its fields go with it, so none is left
    /// holding the keyboard. Search lets go of focus and the inline
    /// prompts (new note, rename note, new folder, rename folder) are
    /// cancelled, creating and renaming nothing.
    pub(crate) fn release_sidebar_fields(&mut self) {
        self.search.focused = false;
        self.cancel_new_note();
        self.cancel_rename_note();
        self.cancel_new_folder();
        self.cancel_rename_folder();
    }

    /// The folder a new board goes in: the open board's folder, else the
    /// folder New note would use (the selected note's), else the vault root.
    pub(crate) fn new_board_folder(&self) -> String {
        if let Some(open) = &self.board_open
            && let Ok(rel) = open.path.strip_prefix(&self.vault.root)
        {
            return rel
                .parent()
                .map(Path::to_string_lossy)
                .unwrap_or_default()
                .replace('\\', "/");
        }
        self.current_folder()
    }

    /// New board: write an empty `Untitled board.excalidraw` (never over an
    /// existing file) and open it on Board. Nothing is written until this
    /// runs; opening Board alone never creates a file.
    pub fn new_board(&mut self) {
        self.commit_board_text();
        // Unsaved drawing on the open board is saved first, as switching
        // boards does; a failed save keeps it open and creates nothing.
        if !self.save_board() {
            return;
        }
        let folder = self.new_board_folder();
        match self.vault.create_board_in(NEW_BOARD_TITLE, &folder) {
            Ok(path) => {
                let rel = path
                    .strip_prefix(&self.vault.root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                self.log_event("board", &format!("created {rel}"));
                self.set_mode(Mode::Board);
                self.relist_boards();
                self.select_board(path);
            }
            Err(e) => {
                self.log_event("board", &format!("new board not created: {e}"));
                self.notice = Some(format!("Could not create a board: {e}"));
            }
        }
    }
}

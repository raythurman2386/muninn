//! Mode state machine: which main-column page is shown (Write, Read, Graph,
//! Board or Kanban), the graph's scope (Global or Local), and the transitions
//! between them.
//!
//! `Mode` and `Scope` are plain enums whose fields live on
//! [`crate::State`](crate::State); this module owns their transitions so the
//! "what does Graph assume" rules (reheat, focus fallback, last-note memory)
//! sit in one place.

use crate::State;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Editor,
    Preview,
    Graph,
    /// The whiteboard page (`.excalidraw` files). Like Graph, it is not a
    /// note mode: leaving it returns to the last of Write/Read.
    Board,
    /// A kanban note shown as a read-only board. An app mode like Board:
    /// leaving it returns to the last of Write/Read.
    Kanban,
}

impl Mode {
    /// Stable settings-file spelling. Array-indexed so serialising the
    /// variants does not repeat the same match shape used elsewhere. Older
    /// settings files only ever contain the first three keys.
    pub(crate) fn as_key(self) -> &'static str {
        const KEYS: [&str; 5] = ["editor", "preview", "graph", "board", "kanban"];
        KEYS[self as usize]
    }

    /// Write and Read show the open note; Graph, Board and Kanban replace it.
    #[must_use]
    pub fn is_note_mode(self) -> bool {
        matches!(self, Mode::Editor | Mode::Preview)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Global,
    Local,
}

impl Scope {
    pub(crate) fn as_key(self) -> &'static str {
        const KEYS: [&str; 2] = ["global", "local"];
        KEYS[self as usize]
    }
}

impl State {
    pub fn set_mode(&mut self, mode: Mode) {
        if mode == self.mode {
            return;
        }
        // Slice 6: leaving Board commits an open text editor.
        self.commit_board_text();
        // #43: a rename field in the Board panel or title goes with them.
        self.board_rename_left_board(mode);
        self.pending_heading = None;
        self.pending_top = false;
        self.complete = None;
        if mode.is_note_mode() && self.mode.is_note_mode() {
            self.last_note_mode = mode;
        }
        if mode == Mode::Graph {
            self.needs_fit = true;
            self.auto_fit = true;
            self.smooth_fit = false;
            self.graph.reheat();
            if self
                .graph_focus
                .is_none_or(|id| id >= self.vault.notes.len())
            {
                self.graph_focus = self
                    .selected
                    .or_else(|| self.vault.notes.first().map(|note| note.id));
            }
        } else {
            // A glide belongs to the visible graph; leaving it must not resume
            // mid-flight when the user comes back.
            self.camera_glide = None;
        }
        if mode == Mode::Board {
            // The file is the source of truth: list what is on disk now.
            self.refresh_boards();
        }
        if mode == Mode::Kanban {
            // Typed text reaches the file first, then the board is read from it.
            self.flush();
            self.refresh_kanban();
        }
        if mode == Mode::Editor {
            self.focus_editor();
        } else {
            // Leaving the editor must not leave a stale focused flag behind:
            // F2/Delete read it, and the field is no longer mounted.
            self.editor.focused = false;
        }
        self.mode = mode;
        // Slice 8 (GK MF1): entering Board collapses the sidebar, so a field
        // in it must not keep the keyboard while out of sight.
        if !self.sidebar_shown() {
            self.release_sidebar_fields();
        }
        // LIF-01: the last mode is part of the restored session.
        self.save_settings();
    }

    pub fn toggle_graph(&mut self) {
        if self.mode == Mode::Graph {
            self.set_mode(self.last_note_mode);
        } else {
            self.set_mode(Mode::Graph);
        }
    }

    /// The palette's Toggle Board: enter Board, or go back to the last note
    /// mode. ⌘4 only enters Board (like ⌘3 for Graph); Esc leaves it.
    pub fn toggle_board(&mut self) {
        if self.mode == Mode::Board {
            self.set_mode(self.last_note_mode);
        } else {
            self.set_mode(Mode::Board);
        }
    }

    /// The palette's Toggle Kanban, as Toggle Board. ⌘5 only enters Kanban;
    /// Esc leaves it.
    pub fn toggle_kanban(&mut self) {
        if self.mode == Mode::Kanban {
            self.set_mode(self.last_note_mode);
        } else {
            self.set_mode(Mode::Kanban);
        }
    }

    pub fn toggle_preview(&mut self) {
        if !self.mode.is_note_mode() {
            self.set_mode(self.last_note_mode);
        } else if self.mode == Mode::Editor {
            self.set_mode(Mode::Preview);
        } else {
            self.set_mode(Mode::Editor);
        }
    }

    pub fn open_local_graph(&mut self) {
        self.scope = Scope::Local;
        self.set_mode(Mode::Graph);
    }

    pub fn set_scope(&mut self, scope: Scope) {
        if self.scope == scope {
            return;
        }
        let was_graph = self.mode == Mode::Graph;
        self.scope = scope;
        self.graph.reheat();
        if was_graph {
            // Ease to the new scope's fit instead of swapping the whole
            // canvas in one frame, and keep following the settling layout so
            // the glide lands on the final framing. The nodes that leave or
            // dim fade through Rune's transitions on the same press commit.
            self.needs_fit = true;
            self.auto_fit = true;
            self.smooth_fit = true;
            self.glide_graph_fit();
        } else {
            self.needs_fit = true;
            self.auto_fit = true;
            self.smooth_fit = false;
        }
        // LIF-01: restore Global/Local with the rest of the session.
        self.save_settings();
    }

    pub fn toggle_scope(&mut self) {
        self.set_scope(if self.scope == Scope::Global {
            Scope::Local
        } else {
            Scope::Global
        });
    }
}

//! Mode state machine: which main-column page is shown (Write, Read or
//! Graph), the graph's scope (Global or Local), and the transitions between
//! them.
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
}

impl Mode {
    /// Stable settings-file spelling. Array-indexed so serialising the three
    /// variants does not repeat the same match shape used elsewhere.
    pub(crate) fn as_key(self) -> &'static str {
        const KEYS: [&str; 3] = ["editor", "preview", "graph"];
        KEYS[self as usize]
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
        self.pending_heading = None;
        self.pending_top = false;
        self.complete = None;
        if mode != Mode::Graph && self.mode != Mode::Graph {
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
        if mode == Mode::Editor {
            self.focus_editor();
        } else {
            // Leaving the editor must not leave a stale focused flag behind:
            // F2/Delete read it, and the field is no longer mounted.
            self.editor.focused = false;
        }
        self.mode = mode;
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

    pub fn toggle_preview(&mut self) {
        if self.mode == Mode::Graph {
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

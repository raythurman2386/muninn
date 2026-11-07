//! Application state: the open vault, note, editor, graph and camera.

use std::path::PathBuf;

use muninn_core::Vault;
use muninn_graph::{Graph, WORLD};
use rune::{Drag, Phase, TextEdit};

use crate::camera::Camera;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Editor,
    Preview,
    Graph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Global,
    Local,
}

pub struct State {
    pub vault: Vault,
    pub selected: Option<usize>,
    pub selected_path: Option<PathBuf>,
    pub editor: TextEdit,
    /// The text last read from or written to disk.
    pub saved: String,
    pub search: TextEdit,
    pub mode: Mode,
    pub last_note_mode: Mode,
    pub scope: Scope,
    pub graph: Graph,
    pub cam: Camera,
    /// The graph canvas's measured size, in points.
    pub canvas: [f32; 2],
    /// The last measured window width, so narrow layouts can drop the links rail.
    pub window_width: f32,
    pub needs_fit: bool,
    /// Keep the settling layout centred and in view until the camera is touched.
    pub auto_fit: bool,
    pub pan_at_press: [f32; 2],
    pub dragging: Option<(usize, [f32; 2])>,
    pub dark: Option<bool>,
    /// Scroll the sidebar to this note on the next frame.
    pub reveal: Option<usize>,
    pub error: Option<String>,
    pub last_save: Option<f64>,
    pub dirty_at: Option<f64>,
    pub now: f64,
}

impl State {
    pub fn new(root: PathBuf) -> State {
        let vault = Vault::load(&root).unwrap_or_else(|e| {
            eprintln!("muninn: cannot read {}: {e}", root.display());
            Vault::empty(root.clone())
        });
        let first = vault.notes.first().map(|n| n.id);
        let graph = Graph::new(&vault);
        let mut state = State {
            vault,
            selected: None,
            selected_path: None,
            editor: TextEdit::new(""),
            saved: String::new(),
            search: TextEdit::new(""),
            mode: Mode::Editor,
            last_note_mode: Mode::Editor,
            scope: Scope::Global,
            graph,
            cam: Camera::new(),
            canvas: [900.0, 620.0],
            window_width: 1180.0,
            needs_fit: true,
            auto_fit: true,
            pan_at_press: [0.0; 2],
            dragging: None,
            dark: None,
            reveal: None,
            error: None,
            last_save: None,
            dirty_at: None,
            now: 0.0,
        };
        if let Some(id) = first {
            state.load_note(id, false);
            state.focus_editor();
        }
        state
    }

    pub fn load_note(&mut self, id: usize, focus: bool) {
        if self.selected == Some(id) {
            if focus && self.mode == Mode::Editor {
                self.focus_editor();
            }
            return;
        }
        self.flush();
        let note = &self.vault.notes[id];
        let (source, path) = (note.source.clone(), note.path.clone());
        self.editor.replace_text(source.clone());
        // Open at the top; a new note moves the caret to its end when it is created.
        self.editor.anchor = 0;
        self.editor.focus = 0;
        self.saved = source;
        self.selected = Some(id);
        self.selected_path = Some(path);
        self.reveal = Some(id);
        self.dirty_at = None;
        self.error = None;
        if focus && self.mode == Mode::Editor {
            self.focus_editor();
        }
    }

    pub fn open_from_list(&mut self, id: usize) {
        if self.mode == Mode::Graph {
            self.mode = Mode::Editor;
        }
        self.load_note(id, true);
    }

    pub fn open_from_link(&mut self, id: usize) {
        self.load_note(id, self.mode == Mode::Editor);
    }

    pub fn open_note(&mut self, id: usize) {
        self.mode = Mode::Editor;
        self.load_note(id, true);
    }

    pub fn select_graph_node(&mut self, id: usize) {
        self.load_note(id, false);
    }

    pub fn focus_editor(&mut self) {
        self.editor.focused = true;
        self.search.focused = false;
    }

    pub fn focus_search(&mut self) {
        self.search.focused = true;
        self.editor.focused = false;
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if mode == self.mode {
            return;
        }
        if mode != Mode::Graph && self.mode != Mode::Graph {
            self.last_note_mode = mode;
        }
        if mode == Mode::Graph {
            self.needs_fit = true;
            self.auto_fit = true;
            self.graph.reheat();
        }
        if mode == Mode::Editor {
            self.focus_editor();
        }
        self.mode = mode;
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
        self.scope = scope;
        self.needs_fit = true;
        self.auto_fit = true;
        self.graph.reheat();
    }

    pub fn toggle_scope(&mut self) {
        self.set_scope(if self.scope == Scope::Global {
            Scope::Local
        } else {
            Scope::Global
        });
    }

    /// Fit the visible nodes without giving up the "follow the layout" mode.
    pub fn refit(&mut self) {
        let visible = self.visible_notes();
        if let Some(bounds) = self.graph.bounds(&visible) {
            self.cam.fit(bounds, self.canvas);
        }
        self.needs_fit = false;
    }

    pub fn fit_graph(&mut self) {
        self.auto_fit = false;
        self.refit();
    }

    pub fn reset_graph(&mut self) {
        self.graph.reset();
        self.needs_fit = true;
        self.auto_fit = true;
    }

    #[must_use]
    pub fn local_notes(&self) -> Vec<usize> {
        let Some(id) = self.selected else {
            return Vec::new();
        };
        let Some(note) = self.vault.notes.get(id) else {
            return Vec::new();
        };
        let mut ids = Vec::with_capacity(note.links.len() + note.backlinks.len() + 1);
        ids.push(id);
        ids.extend(note.links.iter().copied());
        ids.extend(note.backlinks.iter().copied());
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    #[must_use]
    pub fn visible_notes(&self) -> Vec<usize> {
        if self.scope == Scope::Global {
            return self.vault.notes.iter().map(|n| n.id).collect();
        }
        self.local_notes()
    }

    #[must_use]
    pub fn visible_edge_count(&self) -> usize {
        self.graph.visible_edges(&self.visible_notes()).len()
    }

    #[must_use]
    pub fn dirty(&self) -> bool {
        self.selected.is_some() && self.editor.text != self.saved
    }

    #[must_use]
    pub fn words(&self) -> usize {
        self.editor.text.split_whitespace().count()
    }

    #[must_use]
    pub fn link_total(&self) -> usize {
        self.vault.notes.iter().map(|n| n.links.len()).sum()
    }

    /// Write the open note if its text changed, and refresh the vault's links.
    pub fn save_now(&mut self) -> bool {
        let Some(id) = self.selected else {
            return false;
        };
        let text = self.editor.text.clone();
        if text == self.saved {
            self.dirty_at = None;
            return false;
        }
        match self.vault.write_note(id, &text) {
            Ok(()) => {
                self.saved = text;
                self.last_save = Some(self.now);
                self.dirty_at = None;
                self.error = None;
                self.graph.sync(&self.vault);
                true
            }
            Err(e) => {
                self.error = Some(format!("Could not save: {e}"));
                false
            }
        }
    }

    fn flush(&mut self) {
        self.save_now();
    }

    fn reload(&mut self, select: Option<PathBuf>) {
        let root = self.vault.root.clone();
        let path = select.or_else(|| self.selected_path.clone());
        match Vault::load(&root) {
            Ok(v) => {
                self.vault = v;
                self.graph.sync(&self.vault);
                self.selected = None;
                self.selected_path = None;
                let id = path
                    .and_then(|p| self.vault.notes.iter().find(|n| n.path == p).map(|n| n.id))
                    .or_else(|| self.vault.notes.first().map(|n| n.id));
                if let Some(id) = id {
                    self.load_note(id, true);
                } else {
                    self.editor.replace_text("");
                    self.saved.clear();
                }
                self.needs_fit = true;
                self.search.focused = false;
            }
            Err(e) => self.error = Some(format!("Could not read the vault: {e}")),
        }
    }

    pub fn new_note(&mut self) {
        self.create_note_named("Untitled");
    }

    pub fn create_note_named(&mut self, title: &str) {
        self.flush();
        match self.vault.create_note(title) {
            Ok(path) => {
                self.reload(Some(path));
                self.mode = Mode::Editor;
                self.focus_editor();
                let end = self.editor.text.len();
                self.editor.anchor = end;
                self.editor.focus = end;
            }
            Err(e) => self.error = Some(format!("Could not create a note: {e}")),
        }
    }

    pub fn open_link(&mut self, url: &str, tx: &mut rune::Tx) {
        if let Some(title) = url.strip_prefix("note:") {
            if let Some(id) = self.vault.resolve(title) {
                self.open_from_link(id);
            }
        } else if let Some(title) = url.strip_prefix("missing:") {
            self.create_note_named(title);
        } else if tx.open_url(url).is_err() {
            // External links are best-effort: the platform may have no opener.
        }
    }

    pub fn drag_node(&mut self, id: usize, drag: &Drag) {
        let zoom = self.cam.zoom().max(0.01);
        let start = self
            .dragging
            .and_then(|(held, start)| (held == id).then_some(start));
        match drag.phase {
            Phase::Down => {
                self.auto_fit = false;
                if let Some(node) = self.graph.node_mut(id) {
                    node.fixed = true;
                    node.vel = [0.0; 2];
                    self.dragging = Some((id, node.pos));
                }
            }
            Phase::Move => {
                if let Some((_, start)) = self.dragging.filter(|(held, _)| *held == id)
                    && let Some(node) = self.graph.node_mut(id)
                {
                    node.pos = [
                        (start[0] + drag.dx / zoom).clamp(70.0, WORLD[0] - 70.0),
                        (start[1] + drag.dy / zoom).clamp(60.0, WORLD[1] - 60.0),
                    ];
                }
            }
            Phase::Up => {
                if let Some(node) = self.graph.node_mut(id) {
                    node.fixed = false;
                }
                self.dragging = None;
                if drag.cancelled()
                    && let Some(start) = start
                    && let Some(node) = self.graph.node_mut(id)
                {
                    node.pos = start;
                }
                self.graph.reheat();
            }
        }
    }

    pub fn drag_canvas(&mut self, drag: &Drag) {
        if drag.phase == Phase::Down {
            self.auto_fit = false;
            self.pan_at_press = self.cam.pan();
        }
        let pan = if drag.cancelled() {
            self.pan_at_press
        } else {
            [
                self.pan_at_press[0] + drag.dx,
                self.pan_at_press[1] + drag.dy,
            ]
        };
        self.cam.set(pan, self.cam.zoom());
    }
}

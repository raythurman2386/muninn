//! Application state: the open vault, note, editor, graph and camera.

use std::collections::HashSet;
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

/// What a sidebar context menu acts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuTarget {
    Note(usize),
    Folder(String),
}

impl MenuTarget {
    fn note(id: usize) -> Self {
        Self::Note(id)
    }
    fn folder(folder: &str) -> Self {
        Self::Folder(folder.to_owned())
    }
}

/// Open right-click menu, anchored in window points.
#[derive(Clone, Debug)]
pub struct ContextMenu {
    pub target: MenuTarget,
    pub x: f32,
    pub y: f32,
}

/// Name being typed for a new note in `folder`.
#[derive(Clone, Debug)]
pub struct NamePrompt {
    pub folder: String,
}

/// Name being typed for an existing note's rename.
#[derive(Clone, Debug)]
pub struct RenamePrompt {
    pub id: usize,
}

/// Name being typed for a new folder under `parent`.
#[derive(Clone, Debug)]
pub struct FolderPrompt {
    pub parent: String,
}

/// New last segment being typed for an existing folder.
#[derive(Clone, Debug)]
pub struct FolderRename {
    pub folder: String,
}

/// Inline delete confirmation for a note or folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeleteConfirm {
    Note(usize),
    Folder(String),
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
    /// Collapsed sidebar folders (vault-relative, `""` for the root).
    pub collapsed: HashSet<String>,
    /// Open right-click menu, if any.
    pub menu: Option<ContextMenu>,
    /// Inline name field for a note being created. Empty/cancel creates nothing.
    pub new_note_prompt: Option<NamePrompt>,
    pub new_note_field: TextEdit,
    /// Inline rename field for a note.
    pub rename_prompt: Option<RenamePrompt>,
    pub rename_field: TextEdit,
    /// Inline name field for a folder being created.
    pub new_folder_prompt: Option<FolderPrompt>,
    pub new_folder_field: TextEdit,
    /// Inline rename field for a folder.
    pub rename_folder_prompt: Option<FolderRename>,
    pub rename_folder_field: TextEdit,
    /// Inline delete confirmation.
    pub delete_confirm: Option<DeleteConfirm>,
    /// Note being moved: the sidebar shows folder targets under its row.
    pub move_picker: Option<usize>,
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
            collapsed: HashSet::new(),
            menu: None,
            new_note_prompt: None,
            new_note_field: TextEdit::new(""),
            rename_prompt: None,
            rename_field: TextEdit::new(""),
            new_folder_prompt: None,
            new_folder_field: TextEdit::new(""),
            rename_folder_prompt: None,
            rename_folder_field: TextEdit::new(""),
            delete_confirm: None,
            move_picker: None,
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
    /// Returns true when the camera pan/zoom actually changed beyond a
    /// sub-pixel threshold, so pollers can report only visible changes.
    /// The threshold absorbs the layout's invisible spring-vs-separation
    /// creep (a few thousandths of a point per step).
    pub fn refit(&mut self) -> bool {
        let before = (self.cam.pan(), self.cam.zoom());
        let visible = self.visible_notes();
        if let Some(bounds) = self.graph.bounds(&visible) {
            self.cam.fit(bounds, self.canvas);
        }
        self.needs_fit = false;
        let after = (self.cam.pan(), self.cam.zoom());
        (before.0[0] - after.0[0]).abs() > 0.5
            || (before.0[1] - after.0[1]).abs() > 0.5
            || (before.1 - after.1).abs() > 0.005
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

    /// Begin naming a note in the current folder. Nothing is created until
    /// the field is submitted with a non-empty name.
    pub fn new_note(&mut self) {
        let folder = self.current_folder();
        self.begin_new_note(&folder);
    }

    /// Begin naming a note in `folder`. Nothing is created until the field
    /// is submitted with a non-empty name.
    pub fn new_note_in(&mut self, folder: &str) {
        self.begin_new_note(folder);
    }

    /// Open the inline name field for `folder`, expanding it so the field
    /// stays visible.
    pub fn begin_new_note(&mut self, folder: &str) {
        self.menu = None;
        self.collapsed.remove(folder);
        self.new_note_field.replace_text("");
        self.new_note_field.focused = true;
        self.editor.focused = false;
        self.search.focused = false;
        self.new_note_prompt = Some(NamePrompt {
            folder: folder.to_owned(),
        });
    }

    /// Create the prompted note if the name is non-empty; an empty field
    /// cancels and creates nothing.
    pub fn submit_new_note(&mut self) {
        let Some(prompt) = self.new_note_prompt.take() else {
            return;
        };
        let name = self.new_note_field.text.trim().to_owned();
        self.new_note_field.focused = false;
        if name.is_empty() {
            return;
        }
        self.create_note_named_in(&name, &prompt.folder);
    }

    pub fn cancel_new_note(&mut self) {
        self.new_note_prompt = None;
        self.new_note_field.focused = false;
    }

    pub fn begin_rename_note(&mut self, id: usize) {
        self.menu = None;
        let title = self.vault.notes.get(id).map(|n| n.title.clone()).unwrap_or_default();
        self.rename_field.replace_text(title.as_str());
        self.rename_field.focused = true;
        self.editor.focused = false;
        self.search.focused = false;
        self.rename_prompt = Some(RenamePrompt { id });
    }

    pub fn cancel_rename_note(&mut self) {
        self.rename_prompt = None;
        self.rename_field.focused = false;
    }

    pub fn submit_rename_note(&mut self) {
        let Some(prompt) = self.rename_prompt.take() else {
            return;
        };
        let name = self.rename_field.text.trim().to_owned();
        self.rename_field.focused = false;
        if name.is_empty() {
            return;
        }
        self.flush();
        let was_selected = self.selected == Some(prompt.id);
        match self.vault.rename_note(prompt.id, &name) {
            Ok(path) => {
                self.error = None;
                // The vault updated in place; reload resyncs ids (tests use
                // paths) and rebuilds graph titles keyed by rel.
                let select = if was_selected { Some(path) } else { self.selected_path.clone() };
                self.reload(select);
            }
            Err(e) => self.error = Some(format!("Could not rename note: {e}")),
        }
    }

    pub fn delete_note(&mut self, id: usize) {
        self.delete_confirm = Some(DeleteConfirm::Note(id));
        self.menu = None;
    }

    pub fn confirm_delete(&mut self) {
        let Some(target) = self.delete_confirm.take() else {
            return;
        };
        match target {
            DeleteConfirm::Note(id) => {
                let was_selected = self.selected == Some(id);
                self.flush();
                match self.vault.delete_note(id) {
                    Ok(()) => {
                        self.error = None;
                        if was_selected {
                            self.selected = None;
                            self.selected_path = None;
                            self.editor.replace_text("");
                            self.saved.clear();
                        }
                        self.reload(None);
                    }
                    Err(e) => self.error = Some(format!("Could not delete note: {e}")),
                }
            }
            DeleteConfirm::Folder(folder) => match self.vault.delete_folder(&folder) {
                Ok(()) => self.reload(self.selected_path.clone()),
                Err(e) => self.error = Some(format!("Could not delete folder: {e}")),
            },
        }
    }

    pub fn cancel_delete(&mut self) {
        self.delete_confirm = None;
    }

    pub fn begin_move_note(&mut self, id: usize) {
        self.menu = None;
        self.move_picker = Some(id);
    }

    pub fn cancel_move(&mut self) {
        self.move_picker = None;
    }

    pub fn move_note_to(&mut self, id: usize, folder: &str) {
        self.move_picker = None;
        self.flush();
        match self.vault.move_note(id, folder) {
            Ok(path) => self.reload(Some(path)),
            Err(e) => self.error = Some(format!("Could not move note: {e}")),
        }
    }

    pub fn begin_new_folder(&mut self, parent: &str) {
        self.menu = None;
        self.collapsed.remove(parent);
        self.new_folder_field.replace_text("");
        self.new_folder_field.focused = true;
        self.editor.focused = false;
        self.search.focused = false;
        self.new_folder_prompt = Some(FolderPrompt {
            parent: parent.to_owned(),
        });
    }

    pub fn submit_new_folder(&mut self) {
        let Some(prompt) = self.new_folder_prompt.take() else {
            return;
        };
        let name = self.new_folder_field.text.trim().to_owned();
        self.new_folder_field.focused = false;
        if name.is_empty() {
            return;
        }
        match self.vault.create_folder(&prompt.parent, &name) {
            Ok(_) => {
                self.error = None;
                // An empty folder has no notes so grouping won't show it;
                // keep the parent expanded so the next inline field is visible.
            }
            Err(e) => self.error = Some(format!("Could not create folder: {e}")),
        }
    }

    pub fn cancel_new_folder(&mut self) {
        self.new_folder_prompt = None;
        self.new_folder_field.focused = false;
    }

    pub fn begin_rename_folder(&mut self, folder: &str) {
        self.menu = None;
        let current = folder.rsplit('/').next().unwrap_or(folder).to_owned();
        self.rename_folder_field.replace_text(current.as_str());
        self.rename_folder_field.focused = true;
        self.editor.focused = false;
        self.search.focused = false;
        self.rename_folder_prompt = Some(FolderRename {
            folder: folder.to_owned(),
        });
    }

    pub fn cancel_rename_folder(&mut self) {
        self.rename_folder_prompt = None;
        self.rename_folder_field.focused = false;
    }

    pub fn submit_rename_folder(&mut self) {
        let Some(prompt) = self.rename_folder_prompt.take() else {
            return;
        };
        let name = self.rename_folder_field.text.trim().to_owned();
        self.rename_folder_field.focused = false;
        if name.is_empty() {
            return;
        }
        match self.vault.rename_folder(&prompt.folder, &name) {
            Ok(_) => {
                self.error = None;
                self.reload(self.selected_path.clone());
            }
            Err(e) => self.error = Some(format!("Could not rename folder: {e}")),
        }
    }

    pub fn delete_folder(&mut self, folder: &str) {
        self.delete_confirm = Some(DeleteConfirm::Folder(folder.to_owned()));
        self.menu = None;
    }

    pub fn open_note_menu(&mut self, id: usize, x: f32, y: f32) {
        self.menu = Some(ContextMenu {
            target: MenuTarget::note(id),
            x,
            y,
        });
    }

    pub fn open_folder_menu(&mut self, folder: &str, x: f32, y: f32) {
        self.menu = Some(ContextMenu {
            target: MenuTarget::folder(folder),
            x,
            y,
        });
    }

    pub fn close_menu(&mut self) {
        self.menu = None;
    }

    pub fn pick_menu(&mut self, action: &str) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        match (menu.target, action) {
            (MenuTarget::Note(id), "rename") => self.begin_rename_note(id),
            (MenuTarget::Note(id), "delete") => self.delete_note(id),
            (MenuTarget::Note(id), "move") => self.begin_move_note(id),
            (MenuTarget::Folder(folder), "new_note") => self.begin_new_note(&folder),
            (MenuTarget::Folder(folder), "new_folder") => self.begin_new_folder(&folder),
            (MenuTarget::Folder(folder), "rename") => self.begin_rename_folder(&folder),
            (MenuTarget::Folder(folder), "delete") => self.delete_folder(&folder),
            _ => {}
        }
    }

    /// Folder new notes target by default: the selected note's folder, or
    /// the vault root when nothing is selected.
    #[must_use]
    pub fn current_folder(&self) -> String {
        self.selected
            .and_then(|id| self.vault.notes.get(id))
            .map(|n| n.folder().to_owned())
            .unwrap_or_default()
    }

    /// Collapse or expand one sidebar folder.
    pub fn toggle_folder(&mut self, folder: &str) {
        if !self.collapsed.remove(folder) {
            self.collapsed.insert(folder.to_owned());
        }
    }

    #[must_use]
    pub fn is_collapsed(&self, folder: &str) -> bool {
        self.collapsed.contains(folder)
    }

    pub fn create_note_named(&mut self, title: &str) {
        let folder = self.current_folder();
        self.create_note_named_in(title, &folder);
    }

    /// Create `title` directly in `folder`, then open it in the editor.
    pub fn create_note_named_in(&mut self, title: &str, folder: &str) {
        self.flush();
        match self.vault.create_note_in(title, folder) {
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
        } else if let Some(raw) = url.strip_prefix("missing:") {
            // A path-qualified missing link names its folder; a bare one is
            // created in the current note's folder (see `create_note_named`).
            let normalised = raw.replace('\\', "/");
            if let Some((folder, title)) = normalised.rsplit_once('/')
                && !title.is_empty()
            {
                self.create_note_named_in(title, folder);
            } else {
                self.create_note_named(raw);
            }
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

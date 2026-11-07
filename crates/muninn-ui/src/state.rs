//! Application state: the open vault, note, editor, graph and camera.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

use muninn_core::Vault;
use muninn_graph::{Graph, WORLD};
use rune::{Drag, Phase, TextEdit};

use crate::camera::Camera;
use crate::recovery::{RecoveryEntry, RecoveryStore};
use crate::watch::{self, VaultSnapshot};

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

/// A note file's on-disk identity: `(mtime, length)`.
pub type DiskState = (Option<SystemTime>, u64);

/// Autosave backoff sentinel used while a conflict is pending. It is larger
/// than any runtime timestamp, so `poll` never treats it as a due save.
const CONFLICT_PAUSED: f64 = f64::MAX;

/// SAF-03: how often `poll` compares vault metadata with the last snapshot.
/// 1.5 s keeps external edits visible within roughly two seconds while the
/// ~2,000 raw `metadata` calls per tick stay imperceptible (~1–3 ms warm).
pub(crate) const WATCH_INTERVAL: f64 = 1.5;

/// SAF-05: trailing idle window before a dirty buffer is copied to the XDG
/// recovery file. Shorter than the 0.55 s autosave window so a crash during
/// that window still leaves the text recoverable.
const RECOVERY_DEBOUNCE: f64 = 0.2;

/// SAF-05: a continuous typist never goes idle, so force a copy at least this
/// often. Combined with the idle debounce this bounds crash loss to about a
/// second of typing.
const RECOVERY_MAX_AGE: f64 = 1.0;

/// Why a guarded save refused to overwrite the open note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictReason {
    /// Another writer changed the file after it was loaded or last saved.
    Modified,
    /// The file disappeared after it was loaded or last saved.
    Deleted,
}

/// A pending external change to the open note.
///
/// While this is present autosave is paused: the user must choose a
/// resolution, so the buffer is never silently replaced and the other
/// writer's file is never clobbered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictState {
    pub note_id: usize,
    /// The identity this editor last saw. `None` when it could not be
    /// captured (for example the file was already gone at load time).
    pub expected: Option<DiskState>,
    /// What the disk looked like when the save was refused.
    pub observed: Option<DiskState>,
    pub reason: ConflictReason,
}

impl ConflictState {
    #[must_use]
    pub fn message(&self) -> &'static str {
        match self.reason {
            ConflictReason::Modified => "External change detected",
            ConflictReason::Deleted => "file deleted externally",
        }
    }
}

/// The last note moved to the vault-local trash this session.
///
/// `trash_rel` is vault-relative (`.muninn-trash/<timestamp>/<original path>`)
/// so Undo still works after a reload. The toast expires on the next note
/// action; the file itself stays in the trash until restored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrashUndo {
    /// Human label shown in the status bar (`Alpha` or `folder/Alpha`).
    pub label: String,
    pub trash_rel: String,
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
    /// Identity `(mtime, length)` this editor expects on disk for the open
    /// note. Captured on load and after every successful save.
    pub expected_disk: Option<DiskState>,
    /// Pending external change; autosave pauses until the user resolves it.
    pub conflict: Option<ConflictState>,
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
    /// Why the vault could not be scanned at startup, if it could not. The
    /// view shows this instead of "No notes yet" and offers Retry.
    pub startup_error: Option<String>,
    /// Last note moved to the trash, offered as Undo in the status bar.
    pub trash_undo: Option<TrashUndo>,
    /// SAF-05 recovery files for this vault (XDG state, never the vault).
    recovery: RecoveryStore,
    /// Dirty buffers found at startup; the banner lists and restores these.
    pub recovered: Vec<RecoveryEntry>,
    /// Trailing-debounce clock for the next recovery write.
    recovery_at: Option<f64>,
    /// When the current dirty spell was first seen or last copied, so a
    /// continuous typist still gets a fresh copy at least every second.
    recovery_written_at: Option<f64>,
    /// Text the debounce timer is currently counting down for.
    recovery_pending_text: Option<String>,
    /// `rel` and text of the newest persisted recovery copy, so unchanged
    /// buffers are not rewritten every tick.
    recovery_rel: Option<String>,
    recovery_text: Option<String>,
    /// SAF-03 baseline: metadata picture of the vault at the last check.
    watched: Option<VaultSnapshot>,
    last_watch: f64,
}

impl State {
    pub fn new(root: PathBuf) -> State {
        Self::with_state_home(root, crate::recovery::state_home())
    }

    /// Build a state rooted in `root` with an explicit XDG state home. The
    /// application always uses [`State::new`]; tests use this to keep recovery
    /// copies inside their own temp directory instead of the user's.
    pub fn with_state_home(root: PathBuf, state_home: PathBuf) -> State {
        let (vault, startup_error) = match Vault::load(&root) {
            Ok(vault) => (vault, None),
            Err(e) => {
                eprintln!("muninn: cannot read {}: {e}", root.display());
                (
                    Vault::empty(root.clone()),
                    Some(format!("Could not read the vault: {e}")),
                )
            }
        };
        // Key recovery by the canonical root *after* the load above: a missing
        // root is created by the load, and the next launch must derive the same
        // vault id (canonicalization succeeds then). A symlinked path would
        // otherwise write copies under one id and look for them under another.
        let recovery = RecoveryStore::at(&state_home, &root);
        let recovered = recovery.scan();
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
            expected_disk: None,
            conflict: None,
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
            startup_error,
            trash_undo: None,
            recovery,
            recovered,
            recovery_at: None,
            recovery_written_at: None,
            recovery_pending_text: None,
            recovery_rel: None,
            recovery_text: None,
            watched: None,
            last_watch: 0.0,
        };
        if let Some(id) = first {
            state.load_note(id, false);
            state.focus_editor();
        }
        state.refresh_watch();
        state
    }

    pub fn load_note(&mut self, id: usize, focus: bool) {
        // Opening or switching notes is the next note action: a pending Undo
        // toast expires here (the file stays in the trash).
        self.trash_undo = None;
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
        self.conflict = None;
        match self.vault.disk_state(id) {
            Ok(state) => self.expected_disk = Some(state),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // The index still lists it but the file vanished. Surface the
                // same conflict the save path would, instead of writing blind.
                self.expected_disk = None;
                self.conflict = Some(ConflictState {
                    note_id: id,
                    expected: None,
                    observed: None,
                    reason: ConflictReason::Deleted,
                });
                self.dirty_at = Some(CONFLICT_PAUSED);
            }
            Err(e) => {
                self.expected_disk = None;
                self.error = Some(format!("Could not read note state: {e}"));
            }
        }
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
        self.set_text_focus(true, false);
    }

    pub fn focus_search(&mut self) {
        self.set_text_focus(false, true);
    }

    /// Clear both text-field focus flags in the same event as a shell blur.
    ///
    /// EDT-06: Escape made the runtime blur its focus, but `editor.focused`
    /// stayed true until the next frame, so the typing gate in `keys.rs` kept
    /// F2/Delete disabled. The editor's input handler calls this first.
    pub fn blur_text(&mut self) {
        self.set_text_focus(false, false);
    }

    /// One owner for the mutually exclusive editor/search focus flags.
    fn set_text_focus(&mut self, editor: bool, search: bool) {
        self.editor.focused = editor;
        self.search.focused = search;
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
        } else {
            // Leaving the editor must not leave a stale focused flag behind:
            // F2/Delete read it, and the field is no longer mounted.
            self.editor.focused = false;
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
    ///
    /// The write is guarded by the file identity captured when the note was
    /// loaded (or last saved). If another writer changed or deleted the file,
    /// nothing is overwritten: autosave pauses and [`State::conflict`] holds a
    /// typed, user-resolvable conflict instead of a generic error.
    pub fn save_now(&mut self) -> bool {
        let Some(id) = self.selected else {
            return false;
        };
        let text = self.editor.text.clone();
        if text == self.saved {
            self.dirty_at = None;
            return false;
        }
        if self.conflict.is_some() {
            // Autosave must stay quiet until the user picks a resolution. The
            // sentinel keeps `poll` from treating an explicit save shortcut
            // (which lands here) as a reason to resume retrying.
            self.dirty_at = Some(CONFLICT_PAUSED);
            return false;
        }
        let expected = match self.expected_disk {
            Some(state) => state,
            None => match self.vault.disk_state(id) {
                Ok(state) => {
                    self.expected_disk = Some(state);
                    state
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    self.raise_conflict(id, ConflictReason::Deleted, None);
                    return true;
                }
                Err(e) => {
                    self.error = Some(format!("Could not save: {e}"));
                    return false;
                }
            },
        };
        match self.vault.write_note_if_unmodified(id, &text, expected) {
            Ok(true) => {
                self.saved = text;
                self.last_save = Some(self.now);
                self.dirty_at = None;
                self.error = None;
                self.expected_disk = self.vault.disk_state(id).ok();
                self.graph.sync(&self.vault);
                if let Some(rel) = self.selected_rel().map(str::to_owned) {
                    self.clear_recovery(&rel);
                }
                self.refresh_watch();
                true
            }
            // Another writer got there first. Keep the buffer and let the bar
            // offer keep-mine / load-theirs / save-copy.
            Ok(false) => {
                let observed = self.vault.disk_state(id).ok();
                self.raise_conflict(id, ConflictReason::Modified, observed);
                true
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.raise_conflict(id, ConflictReason::Deleted, None);
                true
            }
            // The save failed before touching `saved`, so the editor keeps its
            // dirty state and the in-memory note still matches the old file.
            Err(e) => {
                self.error = Some(format!("Could not save: {e}"));
                false
            }
        }
    }

    /// TRU-01: user-initiated retry after an IO error. Resets the autosave
    /// backoff so the attempt happens now rather than after another idle
    /// window.
    pub fn retry_save(&mut self) -> bool {
        self.dirty_at = None;
        self.save_now()
    }

    // ---- SAF-05: dirty-buffer crash recovery ---------------------------------

    /// Where the recovery copy for `rel` lives. Exposed for tests and support.
    #[must_use]
    pub fn recovery_path(&self, rel: &str) -> PathBuf {
        self.recovery.path_for(rel)
    }

    /// One recovery tick: once the buffer has been idle for
    /// [`RECOVERY_DEBOUNCE`], copy it to the state directory. Clean buffers
    /// only clear their own file; they are never written.
    pub(crate) fn recovery_tick(&mut self, t: f64) {
        if !self.dirty() {
            self.recovery_at = None;
            self.recovery_written_at = None;
            self.recovery_pending_text = None;
            if let Some(rel) = self.recovery_rel.clone()
                && self.selected_rel() == Some(rel.as_str())
            {
                self.clear_recovery(&rel);
            }
            return;
        }
        let Some(rel) = self.selected_rel().map(str::to_owned) else {
            return;
        };
        let text = self.editor.text.clone();
        if self.recovery_rel.as_deref() == Some(rel.as_str())
            && self.recovery_text.as_deref() == Some(text.as_str())
        {
            self.recovery_at = None;
            self.recovery_pending_text = None;
            return;
        }
        // A changed buffer restarts the idle window. While it keeps changing,
        // RECOVERY_MAX_AGE guarantees a copy at least once a second.
        if self.recovery_pending_text.as_deref() != Some(text.as_str()) {
            if self.recovery_written_at.is_none() {
                self.recovery_written_at = Some(t);
            }
            self.recovery_pending_text = Some(text);
            self.recovery_at = Some(t);
        }
        let idle = self
            .recovery_at
            .is_some_and(|at| t - at >= RECOVERY_DEBOUNCE);
        let stale = self
            .recovery_written_at
            .is_some_and(|at| t - at >= RECOVERY_MAX_AGE);
        if idle || stale {
            self.recovery_at = None;
            match self.persist_recovery() {
                Ok(()) => self.recovery_written_at = Some(t),
                Err(e) => {
                    // Retry at the max-age cadence, not every frame.
                    self.recovery_written_at = Some(t);
                    self.error = Some(format!("Could not persist recovery copy: {e}"));
                }
            }
        }
    }

    fn persist_recovery(&mut self) -> io::Result<()> {
        let Some(rel) = self.selected_rel().map(str::to_owned) else {
            return Ok(());
        };
        if !self.dirty() {
            return Ok(());
        }
        let text = self.editor.text.clone();
        self.recovery.save(&rel, &text)?;
        self.recovery_rel = Some(rel);
        self.recovery_text = Some(text);
        Ok(())
    }

    fn clear_recovery(&mut self, rel: &str) {
        self.recovery.clear(rel);
        self.recovered.retain(|entry| entry.rel != rel);
        if self.recovery_rel.as_deref() == Some(rel) {
            self.recovery_rel = None;
            self.recovery_text = None;
        }
    }

    /// Dismiss the startup banner by discarding every listed copy. This is the
    /// explicit "I do not want these edits" action; it never touches the notes.
    pub fn dismiss_recovery(&mut self) -> bool {
        let rels: Vec<String> = self
            .recovered
            .iter()
            .map(|entry| entry.rel.clone())
            .collect();
        for rel in rels {
            self.clear_recovery(&rel);
        }
        self.recovered.clear();
        true
    }

    /// Throw away one recovery copy (and its banner row).
    pub fn discard_recovery(&mut self, rel: &str) -> bool {
        self.clear_recovery(rel);
        true
    }

    /// Load one recovered buffer into the editor as dirty. If the note no
    /// longer exists it is recreated at its original vault-relative path.
    pub fn restore_recovery(&mut self, rel: &str) -> bool {
        let Some(id) = self.vault.notes.iter().find(|n| n.rel == rel).map(|n| n.id) else {
            return self.restore_missing_note(rel);
        };
        // Read the copy before flushing: a flush of the same note may save the
        // current buffer and clear the file we are about to load.
        let text = match self.recovery.load(rel) {
            Ok(text) => text,
            Err(e) => {
                self.error = Some(format!("Could not read recovery copy: {e}"));
                return false;
            }
        };
        // Never silently replace a different dirty buffer that is still open.
        self.flush();
        if self.dirty() {
            self.error =
                Some("Save or discard the current note before restoring another".to_owned());
            return false;
        }
        let path = self.vault.notes[id].path.clone();
        let source =
            fs::read_to_string(&path).unwrap_or_else(|_| self.vault.notes[id].source.clone());
        self.selected = Some(id);
        self.selected_path = Some(path);
        self.editor.replace_text(text.clone());
        self.editor.anchor = 0;
        self.editor.focus = 0;
        self.saved = source;
        self.expected_disk = self.vault.disk_state(id).ok();
        if self.conflict.as_ref().is_some_and(|c| c.note_id == id) {
            self.conflict = None;
        }
        self.dirty_at = Some(self.now);
        self.error = None;
        // The persisted copy and the buffer now agree; do not rewrite it just
        // because the debounce clock restarted.
        self.recovery_rel = Some(rel.to_owned());
        self.recovery_text = Some(text);
        self.recovery_at = None;
        self.recovery_written_at = Some(self.now);
        self.recovery_pending_text = None;
        // A flush above may have cleared the copy for this very note. Keep the
        // restored buffer recoverable until it is saved again.
        let buffer = self.editor.text.clone();
        if let Err(e) = self.recovery.save(rel, &buffer) {
            self.error = Some(format!("Could not persist recovery copy: {e}"));
        }
        self.recovered.retain(|entry| entry.rel != rel);
        true
    }

    /// Restore every listed buffer. Each restored note is flushed to disk
    /// before the next one opens, so the last one stays in the editor.
    pub fn restore_all_recovery(&mut self) -> bool {
        let rels: Vec<String> = self
            .recovered
            .iter()
            .map(|entry| entry.rel.clone())
            .collect();
        let mut restored = true;
        for rel in rels {
            restored &= self.restore_recovery(&rel);
        }
        restored
    }

    fn restore_missing_note(&mut self, rel: &str) -> bool {
        let text = match self.recovery.load(rel) {
            Ok(text) => text,
            Err(e) => {
                self.error = Some(format!("Could not read recovery copy: {e}"));
                return false;
            }
        };
        if !safe_rel(rel) {
            self.error = Some(format!("Refusing unsafe recovery path {rel}"));
            return false;
        }
        let path = self.vault.root.join(rel);
        if let Some(parent) = path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            self.error = Some(format!("Could not recreate note folder: {e}"));
            return false;
        }
        if let Err(e) = fs::write(&path, &text) {
            self.error = Some(format!("Could not recreate note: {e}"));
            return false;
        }
        // The text is on disk now, so this copy has served its purpose.
        self.clear_recovery(rel);
        self.reload(Some(path));
        true
    }

    // ---- SAF-03: external-change watcher (metadata polling) ------------------

    /// Re-baseline the watcher after this process writes to the vault, so our
    /// own atomic writes are not mistaken for somebody else's edits.
    fn refresh_watch(&mut self) {
        self.watched = Some(watch::snapshot(&self.vault));
    }

    /// Compare vault metadata with the last snapshot every
    /// [`WATCH_INTERVAL`] seconds.
    ///
    /// Safe behaviour:
    /// - A dirty note that changed under us raises a conflict and keeps the
    ///   buffer untouched.
    /// - Any other change (new/deleted note, folder change, a clean note's
    ///   content) reloads the vault and re-scans the sidebar/graph while
    ///   preserving selection, mode, scope and collapsed folders.
    /// - While the open buffer is dirty, an unrelated change first flushes
    ///   that buffer through the normal guarded save and then reloads. If the
    ///   save cannot complete, the old baseline is kept and the reload waits
    ///   for a clean tick (or a conflict resolution) instead of discarding the
    ///   buffer.
    pub(crate) fn watch_tick(&mut self, t: f64) -> bool {
        if t - self.last_watch < WATCH_INTERVAL {
            return false;
        }
        self.last_watch = t;
        let snapshot = watch::snapshot(&self.vault);
        let Some(previous) = self.watched.take() else {
            self.watched = Some(snapshot);
            return false;
        };
        if previous == snapshot {
            self.watched = Some(snapshot);
            return false;
        }
        // A pending conflict already owns the next user action; do not stack
        // another one or reload underneath the resolution bar.
        if self.conflict.is_some() {
            self.watched = Some(snapshot);
            return false;
        }
        if let Some(id) = self.selected {
            let rel = self.vault.notes[id].rel.clone();
            let before = previous.note(&rel);
            let after = snapshot.note(&rel);
            if before != after && self.dirty() {
                let reason = if after.is_none() {
                    ConflictReason::Deleted
                } else {
                    ConflictReason::Modified
                };
                self.raise_conflict(id, reason, after);
                self.watched = Some(snapshot);
                return true;
            }
        }
        if self.dirty() {
            // The change is elsewhere (the selected note was checked above).
            // Save the buffer first, then reload; if the save cannot complete
            // (conflict or IO error) keep the old baseline so the pending
            // change is still picked up once the buffer is clean.
            self.flush();
            if self.dirty() {
                self.watched = Some(previous);
                return false;
            }
        }
        let select = self.selected_path.clone();
        let (editor_focused, search_focused) = (self.editor.focused, self.search.focused);
        self.reload(select);
        self.editor.focused = editor_focused;
        self.search.focused = search_focused;
        true
    }

    fn selected_rel(&self) -> Option<&str> {
        let id = self.selected?;
        self.vault.notes.get(id).map(|note| note.rel.as_str())
    }

    /// Conflict resolution 1/3: overwrite the other writer's file with the
    /// editor buffer through the atomic write path.
    pub fn resolve_conflict_keep_mine(&mut self) -> bool {
        let Some(conflict) = self.conflict.take() else {
            return false;
        };
        let text = self.editor.text.clone();
        match self.vault.write_note(conflict.note_id, &text) {
            Ok(()) => {
                self.saved = text;
                self.last_save = Some(self.now);
                self.dirty_at = None;
                self.error = None;
                self.expected_disk = self.vault.disk_state(conflict.note_id).ok();
                self.graph.sync(&self.vault);
                if let Some(rel) = self
                    .vault
                    .notes
                    .get(conflict.note_id)
                    .map(|n| n.rel.clone())
                {
                    self.clear_recovery(&rel);
                }
                self.refresh_watch();
                true
            }
            Err(e) => {
                self.error = Some(format!("Could not save: {e}"));
                self.conflict = Some(conflict);
                false
            }
        }
    }

    /// Conflict resolution 2/3: discard the buffer and load the disk bytes.
    /// If the file was deleted, leave the empty no-note state.
    pub fn resolve_conflict_load_theirs(&mut self) -> bool {
        let Some(conflict) = self.conflict.take() else {
            return false;
        };
        match self.load_disk_into_editor(conflict.note_id) {
            Ok(()) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.clear_open_note();
                self.refresh_index_only();
                true
            }
            Err(e) => {
                self.error = Some(format!("Could not reload the note: {e}"));
                self.conflict = Some(conflict);
                false
            }
        }
    }

    /// Conflict resolution 3/3: write the buffer to a sibling
    /// `Title (conflict YYYYMMDD-HHMMSS).md`, then load the original disk
    /// bytes into the editor.
    pub fn resolve_conflict_save_copy(&mut self) -> bool {
        let Some(conflict) = self.conflict.take() else {
            return false;
        };
        let Some(note) = self.vault.notes.get(conflict.note_id) else {
            self.clear_open_note();
            return true;
        };
        let mine = self.editor.text.clone();
        let title = format!("{} (conflict {})", note.title, conflict_stamp());
        let folder = note.folder().to_owned();
        let original = note.path.clone();
        let path = match self.vault.create_note_in(&title, &folder) {
            Ok(path) => path,
            Err(e) => {
                self.error = Some(format!("Could not save a copy: {e}"));
                self.conflict = Some(conflict);
                return false;
            }
        };
        if let Err(e) = fs::write(&path, mine) {
            self.error = Some(format!("Could not save a copy: {e}"));
            self.conflict = Some(conflict);
            return false;
        }
        // The copy is now the buffer's home. Show what the original holds; if
        // it disappeared, fall back to the empty state and index the copy.
        match self.load_disk_into_editor(conflict.note_id) {
            Ok(()) => {
                // Re-scan so the copy appears in the sidebar, keeping the
                // original open (its bytes now equal the editor buffer).
                self.reload(Some(original));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.clear_open_note();
                self.refresh_index_only();
            }
            Err(e) => {
                self.error = Some(format!("Could not reload the note: {e}"));
                self.conflict = Some(conflict);
                return false;
            }
        }
        true
    }

    /// Read the note file back into the editor, replacing the buffer and the
    /// expected disk identity.
    fn load_disk_into_editor(&mut self, note_id: usize) -> io::Result<()> {
        let path = self
            .vault
            .notes
            .get(note_id)
            .map(|note| note.path.clone())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "note is no longer indexed"))?;
        let source = fs::read_to_string(&path)?;
        self.editor.replace_text(source.clone());
        self.editor.anchor = 0;
        self.editor.focus = 0;
        self.saved = source.clone();
        self.expected_disk = self.vault.disk_state(note_id).ok();
        if let Some(note) = self.vault.notes.get_mut(note_id) {
            note.source = source;
        }
        self.vault.reindex();
        self.graph.sync(&self.vault);
        self.dirty_at = None;
        self.error = None;
        if let Some(rel) = self.vault.notes.get(note_id).map(|n| n.rel.clone()) {
            self.clear_recovery(&rel);
        }
        self.refresh_watch();
        Ok(())
    }

    fn raise_conflict(
        &mut self,
        note_id: usize,
        reason: ConflictReason,
        observed: Option<DiskState>,
    ) {
        self.conflict = Some(ConflictState {
            note_id,
            expected: self.expected_disk,
            observed,
            reason,
        });
        self.dirty_at = Some(CONFLICT_PAUSED);
        self.error = None;
    }

    fn clear_open_note(&mut self) {
        self.selected = None;
        self.selected_path = None;
        self.editor.replace_text("");
        self.saved.clear();
        self.expected_disk = None;
        self.conflict = None;
        self.dirty_at = None;
    }

    /// Re-scan the vault without changing what the editor shows or selects.
    fn refresh_index_only(&mut self) {
        if let Ok(vault) = Vault::load(&self.vault.root) {
            self.vault = vault;
            self.graph.sync(&self.vault);
            self.refresh_watch();
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
                // A later successful scan proves the startup failure is gone.
                self.startup_error = None;
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
                self.refresh_watch();
            }
            Err(e) => self.error = Some(format!("Could not read the vault: {e}")),
        }
    }

    /// Retry the startup scan after [`State::startup_error`]. On success the
    /// vault and graph are rebuilt and the first note opens; on failure the
    /// error is refreshed and the empty fallback stays in place. The folder
    /// picker is still pending (LIF-01), so this is the only recovery path.
    pub fn retry_startup(&mut self) -> bool {
        match Vault::load(&self.vault.root) {
            Ok(vault) => {
                self.vault = vault;
                self.graph.sync(&self.vault);
                self.startup_error = None;
                self.error = None;
                self.needs_fit = true;
                if let Some(id) = self.vault.notes.first().map(|n| n.id) {
                    self.load_note(id, false);
                }
                self.refresh_watch();
                true
            }
            Err(e) => {
                self.startup_error = Some(format!("Could not read the vault: {e}"));
                false
            }
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
        let Some(name) = finish_name_field(&mut self.new_note_field) else {
            return;
        };
        self.create_note_named_in(&name, &prompt.folder);
    }

    pub fn cancel_new_note(&mut self) {
        self.new_note_prompt = None;
        self.new_note_field.focused = false;
    }

    pub fn begin_rename_note(&mut self, id: usize) {
        self.menu = None;
        let title = self
            .vault
            .notes
            .get(id)
            .map(|n| n.title.clone())
            .unwrap_or_default();
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
        let Some(name) = finish_name_field(&mut self.rename_field) else {
            return;
        };
        self.flush();
        let was_selected = self.selected == Some(prompt.id);
        match self.vault.rename_note(prompt.id, &name) {
            Ok(path) => {
                self.error = None;
                // The vault updated in place; reload resyncs ids (tests use
                // paths) and rebuilds graph titles keyed by rel.
                let select = if was_selected {
                    Some(path)
                } else {
                    self.selected_path.clone()
                };
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
                let Some(label) = self
                    .vault
                    .notes
                    .get(id)
                    .map(|_| self.vault.display_label(id))
                else {
                    return;
                };
                let was_selected = self.selected == Some(id);
                self.flush();
                match self.vault.delete_note_to_trash(id) {
                    Ok(trash_rel) => {
                        self.error = None;
                        if was_selected {
                            self.selected = None;
                            self.selected_path = None;
                            self.editor.replace_text("");
                            self.saved.clear();
                        }
                        // Reload first: opening the next note expires any
                        // previous Undo, so record this deletion afterwards.
                        self.reload(None);
                        self.trash_undo = Some(TrashUndo { label, trash_rel });
                    }
                    Err(e) => self.error = Some(format!("Could not delete note: {e}")),
                }
            }
            DeleteConfirm::Folder(folder) => {
                self.trash_undo = None;
                match self.vault.delete_folder(&folder) {
                    Ok(()) => self.reload(self.selected_path.clone()),
                    Err(e) => self.error = Some(format!("Could not delete folder: {e}")),
                }
            }
        }
    }

    /// Restore the last trashed note and reload. Fails (keeping the toast) if
    /// a note with the same path was recreated after the delete; restoring
    /// never overwrites.
    pub fn undo_trash(&mut self) -> bool {
        let Some(undo) = self.trash_undo.take() else {
            return false;
        };
        match self.vault.restore_note(&undo.trash_rel) {
            Ok(path) => {
                self.error = None;
                self.reload(Some(path));
                true
            }
            Err(e) => {
                self.error = Some(format!("Could not restore {}: {e}", undo.label));
                self.trash_undo = Some(undo);
                false
            }
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
        let Some(name) = finish_name_field(&mut self.new_folder_field) else {
            return;
        };
        match self.vault.create_folder(&prompt.parent, &name) {
            Ok(_) => {
                self.error = None;
                self.refresh_watch();
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
        let Some(name) = finish_name_field(&mut self.rename_folder_field) else {
            return;
        };
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
        if let Some(id) = self.selected
            && let Some(note) = self.vault.notes.get(id)
        {
            return note.folder().to_owned();
        }
        String::new()
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

/// Trim a submitted inline name and clear its focus. `None` means the field
/// was empty (cancel), so no note or folder is created.
fn finish_name_field(field: &mut TextEdit) -> Option<String> {
    let name = field.text.trim().to_owned();
    field.focused = false;
    if name.is_empty() { None } else { Some(name) }
}

/// A vault-relative note path that cannot escape the vault root. Used only
/// when recreating a note whose file disappeared while its recovery copy
/// survived.
fn safe_rel(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.starts_with('/')
        && !rel.contains('\\')
        && !rel
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

/// UTC `YYYYMMDD-HHMMSS`, used to give a conflict copy a unique, sortable name.
fn conflict_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let (hour, minute, second) = (rem / 3_600, (rem % 3_600) / 60, rem % 60);
    // Howard Hinnant's `civil_from_days`.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

//! Application state: the open vault, note, editor, graph and camera.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

use muninn_core::{Vault, heading_offset, unclosed_link};
use muninn_graph::{Graph, WORLD, world_centre};
use rune::{Drag, Key, KeyEvent, Phase, TextEdit};

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

/// M2 NAV-03: one point in the note-navigation history.
///
/// History is path-based, not id-based, so it survives a vault reload (ids are
/// positions in the scan and change when files appear or disappear).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub path: PathBuf,
    /// Caret byte offset when the note was left; restored on Back/Forward.
    pub caret: usize,
}

/// M2 EDT-01: the in-note find bar.
#[derive(Clone, Debug, Default)]
pub struct FindState {
    /// The query the matches were computed for (mirrors `field.text`).
    pub query: String,
    /// Byte offsets of each match start in `editor.text`, in document order.
    pub matches: Vec<usize>,
    /// Match end offsets, parallel to `matches` (needed to move the selection).
    end: Vec<usize>,
    /// Index of the current match in `matches`.
    pub index: usize,
    /// The bar's single-line field; focused while the bar is open.
    pub field: TextEdit,
    /// Editor text the matches were computed against.
    hay: String,
    /// Whether the query has been non-empty at least once. Clearing it then
    /// closes the bar (opening it empty must not close it immediately).
    had_query: bool,
}

/// M2 EDT-03: inline `[[` autocomplete, or the explicit insert-link picker.
#[derive(Clone, Debug)]
pub struct LinkComplete {
    /// Byte offset of the `[[` being completed; `None` for the Cmd+P picker.
    pub open: Option<usize>,
    /// Where the picker inserts on accept (the caret when it opened).
    pub at: usize,
    /// Text after `[[`, or the picker's typed filter.
    pub query: String,
    /// Candidate note ids, best first, capped at eight.
    pub items: Vec<usize>,
    pub index: usize,
}

/// M2 SAF-06: rename confirmation with the inbound-link choice.
#[derive(Clone, Debug)]
pub struct RenameConfirm {
    pub id: usize,
    pub name: String,
    /// Notes whose links currently resolve to this note.
    pub referrers: usize,
    /// Rewrite inbound wikilinks to the new stem (default on).
    pub rewrite: bool,
}

/// M2 SAF-06: move confirmation with the inbound-link choice.
#[derive(Clone, Debug)]
pub struct MoveConfirm {
    pub id: usize,
    pub folder: String,
    pub referrers: usize,
    pub rewrite: bool,
}

/// M2 SAF-06: folder rename confirmation with the inbound-link choice.
#[derive(Clone, Debug)]
pub struct FolderRenameConfirm {
    pub folder: String,
    pub name: String,
    /// Notes whose links currently resolve to a note under this folder.
    pub referrers: usize,
    /// Rewrite inbound wikilinks to the renamed stems (default on).
    pub rewrite: bool,
}

/// M2 NAV-03: how many history entries are kept. Deep enough for a session's
/// reading path, bounded so a long-running window cannot grow without limit.
pub(crate) const HISTORY_CAP: usize = 100;

/// M2 EDT-02: how many notes keep their editor state (text, caret and Rune's
/// undo stack). Rune bounds one note's history at 128 steps or 1 MiB, so this
/// caps the aggregate snapshot cache at about 24 MiB worst case while covering
/// a normal session's visits.
const NOTE_EDITOR_CAP: usize = 24;

pub struct State {
    pub vault: Vault,
    pub selected: Option<usize>,
    pub selected_path: Option<PathBuf>,
    pub editor: TextEdit,
    /// M2 EDT-02: per-note editor state keyed by vault path. Cloning a
    /// [`TextEdit`] shares Rune's immutable history (`TextEdit: Clone` keeps
    /// one history until the next edit), so restoring the clone brings back
    /// undo/redo as well as text and caret.
    note_editors: HashMap<PathBuf, TextEdit>,
    /// Least-recently-used order for [`State::note_editors`].
    note_editors_order: Vec<PathBuf>,
    /// The text last read from or written to disk.
    pub saved: String,
    pub search: TextEdit,
    /// M2 NAV-01: highlighted row in the search results, an index into
    /// [`State::search_results`]. `None` means Enter opens the first result.
    pub search_selected: Option<usize>,
    /// The query `search_selected` belongs to; a change clears the selection.
    search_query: String,
    /// M2 EDT-01: open in-note find bar, if any.
    pub find: Option<FindState>,
    /// M2 EDT-03: open wikilink completion or insert-link picker.
    pub complete: Option<LinkComplete>,
    /// Inline completion dismissed with Escape at `(open offset, caret)`; it
    /// stays closed until either moves.
    complete_dismissed: Option<(usize, usize)>,
    /// M2 NAV-02: the broken-links dashboard is open.
    pub broken_panel: bool,
    /// M2 EDT-04: heading a Read-mode open should scroll to on the next frame.
    pub pending_heading: Option<String>,
    /// M2 EDT-04: a heading link whose heading is absent scrolls Read mode to
    /// the top instead of doing nothing.
    pub pending_top: bool,
    /// M2 SAF-06: rename waiting on the link-impact choice.
    pub rename_confirm: Option<RenameConfirm>,
    /// M2 SAF-06: move waiting on the link-impact choice.
    pub move_confirm: Option<MoveConfirm>,
    /// M2 SAF-06: folder rename waiting on the link-impact choice.
    pub folder_rename_confirm: Option<FolderRenameConfirm>,
    /// M2 NAV-03: note-navigation history. Tabs were considered for M2 and
    /// deliberately rejected: one editor plus Back/Forward covers the
    /// "return to what I was reading" case without a tab-strip state model,
    /// per-tab dirty buffers, or a second focus owner.
    pub history: Vec<HistoryEntry>,
    /// Index of the current note in [`State::history`].
    pub history_index: usize,
    pub mode: Mode,
    pub last_note_mode: Mode,
    pub scope: Scope,
    pub graph: Graph,
    pub cam: Camera,
    /// Keyboard focus in the graph, deliberately separate from `selected`:
    /// arrows move the focus ring, Enter opens the focused note.
    pub graph_focus: Option<usize>,
    /// Search-to-node field in the graph controls.
    pub graph_query: TextEdit,
    /// Isolate mode: non-neighbours stay as faint dots.
    pub graph_isolate: bool,
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
            note_editors: HashMap::new(),
            note_editors_order: Vec::new(),
            saved: String::new(),
            search: TextEdit::new(""),
            search_selected: None,
            search_query: String::new(),
            find: None,
            complete: None,
            complete_dismissed: None,
            broken_panel: false,
            pending_heading: None,
            pending_top: false,
            rename_confirm: None,
            move_confirm: None,
            folder_rename_confirm: None,
            history: Vec::new(),
            history_index: 0,
            mode: Mode::Editor,
            last_note_mode: Mode::Editor,
            scope: Scope::Global,
            graph,
            cam: Camera::new(),
            graph_focus: None,
            graph_query: TextEdit::new(""),
            graph_isolate: false,
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

    /// M2 EDT-02: keep the live editor under the note being left. The clone
    /// shares Rune's immutable history, so the next `replace_text` on the live
    /// editor cannot erase the snapshot's undo stack.
    fn stash_editor(&mut self) {
        let Some(path) = self.selected_path.clone() else {
            return;
        };
        let mut snapshot = self.editor.clone();
        // Focus belongs to the window, not to a note parked in the cache.
        snapshot.focused = false;
        self.note_editors.insert(path.clone(), snapshot);
        self.note_editors_order.retain(|p| p != &path);
        self.note_editors_order.push(path);
        while self.note_editors_order.len() > NOTE_EDITOR_CAP {
            let oldest = self.note_editors_order.remove(0);
            self.note_editors.remove(&oldest);
        }
    }

    /// Take the snapshot for `path`, but only when its text still matches the
    /// bytes the vault is about to show. A mismatch means the file changed
    /// under us (external edit, conflict resolution, a discarded dirty buffer),
    /// so restoring the old stack would resurrect history for text that is no
    /// longer there.
    fn take_editor(&mut self, path: &PathBuf, source: &str) -> Option<TextEdit> {
        let snapshot = self.note_editors.remove(path)?;
        (snapshot.text == source).then_some(snapshot)
    }

    /// Move a parked snapshot when the open note itself is renamed or moved.
    fn rekey_editor(&mut self, from: Option<&PathBuf>, to: &PathBuf) {
        let Some(from) = from.filter(|old| *old != to) else {
            return;
        };
        let Some(snapshot) = self.note_editors.remove(from) else {
            return;
        };
        self.note_editors.insert(to.clone(), snapshot);
        for path in &mut self.note_editors_order {
            if path.as_path() == from.as_path() {
                *path = to.clone();
            }
        }
    }

    /// Drop one note's snapshot (conflict reload or a closed note).
    fn forget_editor(&mut self, path: &PathBuf) {
        self.note_editors.remove(path);
        self.note_editors_order.retain(|p| p != path);
    }

    /// Drop snapshots whose files are no longer in the vault: deleted notes and
    /// paths left behind by a rename.
    fn prune_editors(&mut self) {
        let live: HashSet<PathBuf> = self.vault.notes.iter().map(|n| n.path.clone()).collect();
        self.note_editors.retain(|path, _| live.contains(path));
        self.note_editors_order.retain(|path| live.contains(path));
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
        // Remember where the caret was in the note we are leaving before the
        // editor starts showing the next one.
        self.sync_history_caret();
        self.flush();
        // M2 EDT-02: keep the note's text, caret and Rune undo stack for the
        // next visit. Stashing after `flush` means a successful autosave has
        // already made the snapshot match the file the vault will reload.
        self.stash_editor();
        self.load_note_at(id, focus);
        if let Some(path) = self.selected_path.clone() {
            self.record_history(path);
        }
    }

    /// The actual note swap, shared by [`State::load_note`] and history
    /// navigation. History-recording callers push afterwards; Back/Forward
    /// call this directly so moving through history does not rewrite it.
    fn load_note_at(&mut self, id: usize, focus: bool) {
        self.complete = None;
        self.complete_dismissed = None;
        self.pending_heading = None;
        self.pending_top = false;
        let note = &self.vault.notes[id];
        let (source, path) = (note.source.clone(), note.path.clone());
        // M2 EDT-02: a revisited note comes back with its text, caret and undo
        // stack. Fresh or externally changed notes start with empty history.
        if let Some(mut snapshot) = self.take_editor(&path, &source) {
            snapshot.focused = false;
            self.editor = snapshot;
        } else {
            self.editor.replace_text(source.clone());
            // Open at the top; a new note moves the caret to its end when it is created.
            self.editor.anchor = 0;
            self.editor.focus = 0;
        }
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
        // A pointer selection is also where the keyboard focus ring sits.
        self.graph_focus = Some(id);
        self.load_note(id, false);
    }

    // ---- M2 NAV-03: Back / Forward note history -----------------------------

    /// Save the live caret into the current history entry. Called before the
    /// editor loads another note (and before Back/Forward moves away).
    fn sync_history_caret(&mut self) {
        let Some(path) = self.selected_path.as_ref() else {
            return;
        };
        if let Some(entry) = self.history.get_mut(self.history_index)
            && &entry.path == path
        {
            entry.caret = floor_char_boundary(&self.editor.text, self.editor.focus);
        }
    }

    /// Push `path` as the newest history point.
    ///
    /// Consecutive visits to the same note collapse (a reload keeps its place),
    /// a new open drops the abandoned forward branch, and the list is capped at
    /// [`HISTORY_CAP`] entries.
    fn record_history(&mut self, path: PathBuf) {
        if let Some(entry) = self.history.get_mut(self.history_index)
            && entry.path == path
        {
            entry.caret = floor_char_boundary(&self.editor.text, self.editor.focus);
            return;
        }
        self.history.truncate(self.history_index + 1);
        self.history.push(HistoryEntry { path, caret: 0 });
        if self.history.len() > HISTORY_CAP {
            let excess = self.history.len() - HISTORY_CAP;
            self.history.drain(0..excess);
        }
        self.history_index = self.history.len() - 1;
    }

    /// Open the previous note in history. Returns false when there is none.
    pub fn back(&mut self) -> bool {
        self.navigate_history(-1)
    }

    /// Open the next note in history. Returns false when there is none.
    pub fn forward(&mut self) -> bool {
        self.navigate_history(1)
    }

    #[must_use]
    pub fn can_back(&self) -> bool {
        self.history.get(..self.history_index).is_some_and(|past| {
            past.iter()
                .any(|entry| self.note_at_path(&entry.path).is_some())
        })
    }

    #[must_use]
    pub fn can_forward(&self) -> bool {
        self.history
            .get(self.history_index + 1..)
            .is_some_and(|rest| {
                rest.iter()
                    .any(|entry| self.note_at_path(&entry.path).is_some())
            })
    }

    fn note_at_path(&self, path: &PathBuf) -> Option<usize> {
        self.vault
            .notes
            .iter()
            .find(|note| &note.path == path)
            .map(|note| note.id)
    }

    /// Move `delta` entries through history, skipping notes whose files are
    /// gone (a deleted note must not trap Back/Forward).
    fn navigate_history(&mut self, delta: isize) -> bool {
        if self.history.is_empty() {
            return false;
        }
        self.sync_history_caret();
        let len = self.history.len() as isize;
        let mut cursor = self.history_index as isize + delta;
        while (0..len).contains(&cursor) {
            let (path, caret) = {
                let entry = &self.history[cursor as usize];
                (entry.path.clone(), entry.caret)
            };
            if let Some(id) = self.note_at_path(&path) {
                self.history_index = cursor as usize;
                if self.selected != Some(id) {
                    self.flush();
                    self.stash_editor();
                    self.load_note_at(id, true);
                }
                let caret = floor_char_boundary(&self.editor.text, caret);
                self.editor.anchor = caret;
                self.editor.focus = caret;
                return true;
            }
            cursor += delta;
        }
        false
    }

    // ---- M2 NAV-01: keyboard search results ---------------------------------

    /// The current search result order. Ranking stays title-first in
    /// [`Vault::search`]; this is only the id list the selection indexes into.
    ///
    /// M2 deliberately keeps the row subtitle (link counts / duplicate folder)
    /// instead of adding a body snippet: the sidebar is not query-aware enough
    /// to make an excerpt worth the extra rebuild work yet.
    #[must_use]
    pub fn search_results(&self) -> Vec<usize> {
        self.vault.search(&self.search.text)
    }

    /// Move the search highlight by `delta`, wrapping at both ends. The first
    /// Down selects the first row; the first Up selects the last.
    pub fn search_step(&mut self, delta: isize) {
        self.search_tick();
        let len = self.search_results().len();
        if len == 0 {
            self.search_selected = None;
            return;
        }
        let next = match self.search_selected {
            Some(index) if index < len => index as isize + delta,
            _ if delta < 0 => len as isize - 1,
            _ => 0,
        };
        self.search_selected = Some(next.rem_euclid(len as isize) as usize);
    }

    /// Enter in the search field: open the highlighted row, or the first
    /// result when nothing is highlighted.
    pub fn open_search_selected(&mut self) {
        self.search_tick();
        let results = self.search_results();
        if let Some(index) = self
            .search_selected
            .filter(|index| *index < results.len())
            .or(if results.is_empty() { None } else { Some(0) })
        {
            self.search_selected = Some(index);
            self.open_from_list(results[index]);
        }
    }

    /// Drop the highlight when the query changes. `poll` calls this once a
    /// frame, so typing (which runs after the key event) resets on the next
    /// frame instead of pointing at a row from the previous query.
    pub fn search_tick(&mut self) -> bool {
        if self.search_query == self.search.text {
            return false;
        }
        self.search_query.clone_from(&self.search.text);
        self.search_selected.take().is_some()
    }

    // ---- M2 EDT-01: find in the open note -----------------------------------

    /// Cmd/Ctrl+F: open the find bar, or refocus it when it is already open.
    pub fn open_find(&mut self) {
        self.complete = None;
        self.complete_dismissed = None;
        if let Some(find) = self.find.as_mut() {
            find.field.focused = true;
        } else {
            let mut field = TextEdit::new("");
            field.focused = true;
            self.find = Some(FindState {
                field,
                ..FindState::default()
            });
        }
        self.editor.focused = false;
        self.search.focused = false;
        self.find_tick();
    }

    /// Escape: close the bar, clear its highlight and hand typing back to the
    /// editor. A later Cmd/Ctrl+F reopens an empty bar (M2 keeps no query
    /// history).
    pub fn close_find(&mut self) {
        self.find = None;
        self.set_text_focus(true, false);
    }

    pub fn find_next(&mut self) {
        self.find_step(1);
    }

    pub fn find_prev(&mut self) {
        self.find_step(-1);
    }

    fn find_step(&mut self, delta: isize) {
        self.find_tick();
        let Some(find) = self.find.as_mut() else {
            return;
        };
        if find.matches.is_empty() {
            return;
        }
        let len = find.matches.len() as isize;
        find.index = (find.index as isize + delta).rem_euclid(len) as usize;
        let (start, end) = (find.matches[find.index], find.end[find.index]);
        self.editor.anchor = start;
        self.editor.focus = end;
    }

    /// Recompute matches when the query or the note's text changed.
    ///
    /// This runs from `poll` (once a frame, after the field has committed the
    /// typed character) rather than from the view, which only borrows `State`.
    /// That keeps a text edit and its match list in the same state object and
    /// avoids threading an owned match list through every sidebar rebuild.
    /// Returns true when something visible changed.
    pub fn find_tick(&mut self) -> bool {
        let Some(current) = self.find.as_ref() else {
            return false;
        };
        let query = current.field.text.clone();
        if query.trim().is_empty() && current.had_query {
            self.close_find();
            return true;
        }
        let Some(find) = self.find.as_mut() else {
            return false;
        };
        if query.trim().is_empty() {
            let changed = !find.matches.is_empty();
            find.query.clear();
            find.matches.clear();
            find.end.clear();
            find.index = 0;
            find.hay.clone_from(&self.editor.text);
            return changed;
        }
        find.had_query = true;
        if find.query == query && find.hay == self.editor.text {
            return false;
        }
        let (matches, ends) = find_matches(&self.editor.text, &query);
        find.query = query;
        find.hay.clone_from(&self.editor.text);
        find.matches = matches;
        find.end = ends;
        find.index = 0;
        if let Some((&start, &end)) = find.matches.first().zip(find.end.first()) {
            // Highlight by moving the editor's selection to the match.
            self.editor.anchor = start;
            self.editor.focus = end;
        }
        true
    }

    // ---- M2 EDT-03: wikilink completion -------------------------------------

    /// Detect an open `[[…` at the caret or refresh the candidate list.
    ///
    /// Runs from `poll` once a frame, like the find bar's match list, because
    /// the view only borrows `State`. The explicit Cmd+P picker is not
    /// detected here: its query is typed into the picker, not the note.
    /// Returns true when something the user can see changed.
    pub fn complete_tick(&mut self) -> bool {
        if let Some(c) = &self.complete {
            let Some(open) = c.open else {
                return false;
            };
            let query = c.query.clone();
            if !self.editor.focused || self.find.is_some() {
                self.complete = None;
                return true;
            }
            let caret = self.editor.focus.min(self.editor.text.len());
            return match unclosed_link(&self.editor.text, caret) {
                Some((mark, current)) if mark == open => {
                    if current == query {
                        false
                    } else {
                        let items = self.vault.suggest_links(&current, 8);
                        if let Some(c) = self.complete.as_mut() {
                            c.query = current;
                            c.items = items;
                            c.index = 0;
                        }
                        true
                    }
                }
                _ => {
                    self.complete = None;
                    true
                }
            };
        }
        if !self.editor.focused || self.find.is_some() || self.selected.is_none() {
            return false;
        }
        let caret = self.editor.focus.min(self.editor.text.len());
        let Some((open, query)) = unclosed_link(&self.editor.text, caret) else {
            return false;
        };
        if self.complete_dismissed == Some((open, caret)) {
            return false;
        }
        self.complete_dismissed = None;
        let items = self.vault.suggest_links(&query, 8);
        if items.is_empty() {
            return false;
        }
        self.complete = Some(LinkComplete {
            open: Some(open),
            at: caret,
            query,
            items,
            index: 0,
        });
        true
    }

    /// Cmd/Ctrl+P: show the insert-link picker at the caret even without `[[`.
    /// This is deliberately not a command palette or feed search: it lists
    /// notes and inserts a wikilink.
    pub fn open_link_picker(&mut self) {
        if self.selected.is_none() {
            return;
        }
        // Inserting edits the note, so a picker opened from Read or Graph
        // returns to Write first.
        self.set_mode(Mode::Editor);
        self.find = None;
        self.focus_editor();
        let at = self.editor.focus.min(self.editor.text.len());
        let items = self.vault.suggest_links("", 8);
        self.complete = Some(LinkComplete {
            open: None,
            at,
            query: String::new(),
            items,
            index: 0,
        });
        self.complete_dismissed = None;
    }

    /// Keys claimed while a completion or the picker is open. Up/Down move the
    /// highlight, Enter/Tab accept, Escape dismisses. Typing filters only in
    /// the explicit picker; in inline mode the note itself receives the text
    /// and `complete_tick` follows the query.
    pub fn complete_key(&mut self, e: &KeyEvent) -> bool {
        let Some((picker, current)) = self
            .complete
            .as_ref()
            .map(|c| (c.open.is_none(), c.items.get(c.index).copied()))
        else {
            return false;
        };
        match &e.key {
            Key::Escape => {
                self.dismiss_complete();
                true
            }
            Key::Enter | Key::Tab => {
                if let Some(id) = current {
                    self.complete_choose(id);
                } else {
                    self.dismiss_complete();
                }
                true
            }
            Key::Up => {
                self.complete_step(-1);
                true
            }
            Key::Down => {
                self.complete_step(1);
                true
            }
            Key::Backspace if picker => {
                if let Some(c) = self.complete.as_mut() {
                    c.query.pop();
                    c.index = 0;
                    let query = c.query.clone();
                    c.items = self.vault.suggest_links(&query, 8);
                }
                true
            }
            Key::Text(text) if picker && !e.mods.cmd && !e.mods.ctrl && !e.mods.alt => {
                if let Some(c) = self.complete.as_mut() {
                    c.query.push_str(text);
                    c.index = 0;
                    let query = c.query.clone();
                    c.items = self.vault.suggest_links(&query, 8);
                }
                true
            }
            _ => false,
        }
    }

    /// Move the highlighted suggestion, wrapping at both ends.
    pub fn complete_step(&mut self, delta: isize) -> bool {
        let Some(c) = self.complete.as_mut() else {
            return false;
        };
        if c.items.is_empty() {
            return false;
        }
        let len = c.items.len() as isize;
        c.index = (c.index as isize + delta).rem_euclid(len) as usize;
        true
    }

    /// Insert `[[stem]]` for `id`, replacing the open `[[query` or inserting at
    /// the picker's caret. Pointer clicks use this too.
    pub fn complete_choose(&mut self, id: usize) -> bool {
        let Some(c) = self.complete.take() else {
            return false;
        };
        let Some(note) = self.vault.notes.get(id) else {
            return false;
        };
        let text = format!("[[{}]]", note.stem());
        match c.open {
            Some(open) => {
                let caret = self.editor.focus.min(self.editor.text.len());
                self.editor.set_selection(open.min(caret), caret);
                self.editor.insert(&text);
            }
            None => {
                self.editor.set_caret(c.at.min(self.editor.text.len()));
                self.editor.insert(&text);
            }
        }
        self.complete_dismissed = None;
        self.dirty_at.get_or_insert(self.now);
        true
    }

    /// Escape: close the list. Inline completion remembers where it was
    /// dismissed so the next frame does not immediately reopen it.
    pub fn dismiss_complete(&mut self) {
        if let Some(c) = self.complete.take()
            && let Some(open) = c.open
        {
            let caret = self.editor.focus.min(self.editor.text.len());
            self.complete_dismissed = Some((open, caret));
        }
    }

    pub fn focus_editor(&mut self) {
        self.set_text_focus(true, false);
    }

    pub fn focus_search(&mut self) {
        self.complete = None;
        self.complete_dismissed = None;
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

    /// M2 EDT-05: Enter inside the editor.
    ///
    /// An unclosed `[[` is closed first (`]]`, caret left inside), matching the
    /// completion flow that the user is still typing. Then a Markdown list
    /// item continues its marker with the same indentation (numbered items
    /// advance by one); Enter on an empty item removes the marker and ends the
    /// list. Everywhere else it inserts a plain newline, as Rune's plain field
    /// did before.
    pub fn editor_enter(&mut self) {
        let (anchor, focus) = self.editor.range();
        if anchor != focus {
            self.editor.insert("\n");
            return;
        }
        let caret = floor_char_boundary(&self.editor.text, focus);
        if unclosed_link(&self.editor.text, caret).is_some() {
            self.editor.insert("]]");
            self.editor.anchor = caret;
            self.editor.focus = caret;
            return;
        }
        let line_start = self.editor.text[..caret].rfind('\n').map_or(0, |at| at + 1);
        let line_end = self.editor.text[caret..]
            .find('\n')
            .map_or(self.editor.text.len(), |at| caret + at);
        let line = self.editor.text[line_start..line_end].to_owned();
        let Some((indent_len, marker_len, next_marker)) = list_prefix(&line) else {
            self.editor.insert("\n");
            return;
        };
        let content_start = line_start + indent_len + marker_len;
        // Enter before the marker is an ordinary paragraph break.
        if caret < content_start {
            self.editor.insert("\n");
            return;
        }
        if self.editor.text[content_start..line_end].trim().is_empty() {
            // Empty item: drop the marker (and its indent) and stay on a blank
            // line. That is how a Markdown list ends without a trailing item.
            self.editor.set_selection(line_start, content_start);
            self.editor.insert("");
            self.editor.anchor = line_start;
            self.editor.focus = line_start;
            return;
        }
        let indent = &line[..indent_len];
        self.editor.insert(&format!("\n{indent}{next_marker}"));
    }

    /// M2 EDT-05: Tab indents by four spaces at the caret (replacing any
    /// selection) instead of moving focus out of the note.
    pub fn editor_tab(&mut self) {
        self.editor.insert("    ");
    }

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
            self.graph.reheat();
            if self
                .graph_focus
                .is_none_or(|id| id >= self.vault.notes.len())
            {
                self.graph_focus = self
                    .selected
                    .or_else(|| self.vault.notes.first().map(|note| note.id));
            }
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

    /// M3 GRP-02: graph-only keyboard commands. Kept on `State` so the window
    /// shortcut table stays a table and the canvas's keys are testable without
    /// a window.
    pub fn graph_key(&mut self, e: &KeyEvent) -> bool {
        if self.mode != Mode::Graph || e.mods.cmd || e.mods.ctrl || e.mods.alt {
            return false;
        }
        match &e.key {
            Key::Left => self.graph_focus_step(-1.0, 0.0),
            Key::Right => self.graph_focus_step(1.0, 0.0),
            Key::Up => self.graph_focus_step(0.0, -1.0),
            Key::Down => self.graph_focus_step(0.0, 1.0),
            Key::Enter => {
                let Some(id) = self.graph_focus else {
                    return false;
                };
                self.open_note(id);
            }
            Key::Text(t) => match t.as_str() {
                "+" | "=" => self.zoom_graph(1.25),
                "-" | "_" => self.zoom_graph(0.8),
                "0" | "f" | "F" => self.fit_graph(),
                "i" | "I" => self.toggle_graph_isolate(),
                _ => return false,
            },
            _ => return false,
        }
        true
    }

    /// Move the focus ring to the nearest visible node in a screen direction.
    ///
    /// The score is squared distance plus twice the squared sideways offset,
    /// so a modestly farther but on-axis node wins over a near off-axis one.
    /// Ties go to the lower note id. With no node in that half-plane (e.g.
    /// Left from the leftmost node) the stable sorted order wraps instead, as
    /// documented in the README.
    pub fn graph_focus_step(&mut self, dx: f32, dy: f32) {
        let visible = self.visible_notes();
        if visible.is_empty() {
            self.graph_focus = None;
            return;
        }
        let current = self
            .graph_focus
            .filter(|id| visible.contains(id))
            .or_else(|| self.selected.filter(|id| visible.contains(id)));
        let Some(current) = current else {
            self.graph_focus = Some(visible[0]);
            return;
        };
        let Some(origin) = self.graph.node(current).map(|node| node.pos) else {
            self.graph_focus = Some(visible[0]);
            return;
        };
        let mut best: Option<(f32, usize)> = None;
        for &id in &visible {
            if id == current {
                continue;
            }
            let Some(node) = self.graph.node(id) else {
                continue;
            };
            let ddx = node.pos[0] - origin[0];
            let ddy = node.pos[1] - origin[1];
            let along = ddx * dx + ddy * dy;
            if along <= 0.5 {
                continue;
            }
            let sideways = (ddx * dy - ddy * dx).abs();
            let score = ddx * ddx + ddy * ddy + 2.0 * sideways * sideways;
            let better = match best {
                None => true,
                Some((best_score, best_id)) => match score.total_cmp(&best_score) {
                    std::cmp::Ordering::Less => true,
                    std::cmp::Ordering::Equal => id < best_id,
                    std::cmp::Ordering::Greater => false,
                },
            };
            if better {
                best = Some((score, id));
            }
        }
        let next = if let Some((_, id)) = best {
            id
        } else {
            let mut sorted = visible;
            sorted.sort_unstable();
            let index = sorted.binary_search(&current).unwrap_or(0);
            let step = if dx + dy < 0.0 { sorted.len() - 1 } else { 1 };
            sorted[(index + step) % sorted.len()]
        };
        self.graph_focus = Some(next);
        self.keep_graph_focus_visible();
    }

    /// Pan (and minimally zoom in) so `id` is comfortably on screen.
    pub fn center_graph_on(&mut self, id: usize) {
        let Some(pos) = self.graph.node(id).map(|node| node.pos) else {
            return;
        };
        let centre = world_centre();
        let zoom = self.cam.zoom().max(0.9);
        self.cam.set(
            [
                self.canvas[0] / 2.0 - centre[0] - (pos[0] - centre[0]) * zoom,
                self.canvas[1] / 2.0 - centre[1] - (pos[1] - centre[1]) * zoom,
            ],
            zoom,
        );
        self.auto_fit = false;
    }

    fn keep_graph_focus_visible(&mut self) {
        let Some(id) = self.graph_focus else {
            return;
        };
        let Some(node) = self.graph.node(id) else {
            return;
        };
        let point = self.cam.screen(node.pos);
        let margin = 48.0;
        if point[0] >= margin
            && point[0] <= self.canvas[0] - margin
            && point[1] >= margin
            && point[1] <= self.canvas[1] - margin
        {
            return;
        }
        self.center_graph_on(id);
    }

    pub fn zoom_graph(&mut self, factor: f32) {
        self.auto_fit = false;
        self.cam
            .zoom_at([self.canvas[0] / 2.0, self.canvas[1] / 2.0], factor);
    }

    pub fn toggle_graph_isolate(&mut self) {
        self.graph_isolate = !self.graph_isolate;
    }

    /// The note the graph search field points at: title-exact, then title
    /// prefix, then title substring, then path substring; ties by note id.
    #[must_use]
    pub fn graph_search_match(&self) -> Option<usize> {
        let query = self.graph_query.text.trim().to_lowercase();
        if query.is_empty() {
            return None;
        }
        let mut best: Option<(u8, usize)> = None;
        for note in &self.vault.notes {
            let title = note.title.to_lowercase();
            let rank = if title == query {
                0
            } else if title.starts_with(&query) {
                1
            } else if title.contains(&query) {
                2
            } else if note.rel.to_lowercase().contains(&query) {
                3
            } else {
                continue;
            };
            let better = best.is_none_or(|(best_rank, best_id)| {
                rank < best_rank || (rank == best_rank && note.id < best_id)
            });
            if better {
                best = Some((rank, note.id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// GRP-03: search-to-node. Selects the match, focuses it, and centres it.
    pub fn reveal_graph_search(&mut self) -> bool {
        let Some(id) = self.graph_search_match() else {
            return false;
        };
        self.select_graph_node(id);
        self.graph_focus = Some(id);
        self.center_graph_on(id);
        true
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
        // EDT-02: resolving a conflict by loading the disk bytes discards the
        // buffer's undo stack; the parked snapshot must go with it.
        self.forget_editor(&path);
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
        if let Some(path) = self.selected_path.clone() {
            self.forget_editor(&path);
        }
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
        // EDT-02: park the editor before the selection resets below; a same-note
        // reload replaces it only if the reloaded text still matches.
        self.stash_editor();
        match Vault::load(&root) {
            Ok(v) => {
                self.vault = v;
                self.graph.sync(&self.vault);
                // A later successful scan proves the startup failure is gone.
                self.startup_error = None;
                // Reloads keep the note by path identity; save its caret before
                // the id-based selection is rebuilt below.
                self.sync_history_caret();
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
                self.prune_editors();
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
        // SAF-06: count inbound links before mutating; the rewrite itself reads
        // the pre-mutation resolution so duplicate bare titles stay correct.
        let referrers = self.vault.link_referrers(prompt.id).len();
        if referrers == 0 {
            self.apply_rename(prompt.id, &name, false);
        } else {
            self.rename_confirm = Some(RenameConfirm {
                id: prompt.id,
                name,
                referrers,
                rewrite: true,
            });
        }
    }

    /// SAF-06: flip the rename dialog's rewrite choice.
    pub fn toggle_rename_rewrite(&mut self) {
        if let Some(confirm) = self.rename_confirm.as_mut() {
            flip_rewrite(&mut confirm.rewrite);
        }
    }

    pub fn cancel_rename_confirm(&mut self) {
        self.rename_confirm = None;
    }

    /// SAF-06: rename now, rewriting (or deliberately leaving) inbound links.
    pub fn confirm_rename_note(&mut self) {
        if let Some(RenameConfirm {
            id, name, rewrite, ..
        }) = self.rename_confirm.take()
        {
            self.apply_rename(id, &name, rewrite);
        }
    }

    fn apply_rename(&mut self, id: usize, name: &str, rewrite: bool) {
        let was_selected = self.selected == Some(id);
        let old_path = was_selected.then(|| self.selected_path.clone()).flatten();
        let inbound = if rewrite {
            self.vault.link_referrers(id)
        } else {
            Vec::new()
        };
        match self.vault.rename_note(id, name) {
            Ok(path) => {
                self.error = None;
                let failures = if rewrite {
                    let stem = self
                        .vault
                        .notes
                        .get(id)
                        .map(|note| note.stem().to_owned())
                        .unwrap_or_default();
                    self.vault.rewrite_inbound_links(&inbound, &stem)
                } else {
                    Vec::new()
                };
                // The vault updated in place; reload resyncs ids (tests use
                // paths) and rebuilds graph titles keyed by rel.
                let select = if was_selected {
                    // EDT-02: the open note's parked undo stack follows its new
                    // path instead of being dropped as a stale entry.
                    self.rekey_editor(old_path.as_ref(), &path);
                    self.selected_path = Some(path.clone());
                    Some(path)
                } else {
                    self.selected_path.clone()
                };
                self.reload(select);
                if !failures.is_empty() {
                    self.error = Some(format!(
                        "Renamed, but {} inbound link file(s) could not be rewritten: {}",
                        failures.len(),
                        self.format_rewrite_failures(&failures)
                    ));
                }
            }
            Err(e) => self.error = Some(format!("Could not rename note: {e}")),
        }
    }

    fn format_rewrite_failures(&self, failures: &[(usize, io::Error)]) -> String {
        failures
            .iter()
            .map(|(id, error)| {
                let rel = self
                    .vault
                    .notes
                    .get(*id)
                    .map_or("(unknown note)", |note| note.rel.as_str());
                format!("{rel}: {error}")
            })
            .collect::<Vec<_>>()
            .join("; ")
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
        let referrers = self.vault.link_referrers(id).len();
        if referrers == 0 {
            self.apply_move(id, folder, false);
        } else {
            self.move_confirm = Some(MoveConfirm {
                id,
                folder: folder.to_owned(),
                referrers,
                rewrite: true,
            });
        }
    }

    /// SAF-06: flip the move dialog's rewrite choice.
    pub fn toggle_move_rewrite(&mut self) {
        if let Some(confirm) = self.move_confirm.as_mut() {
            flip_rewrite(&mut confirm.rewrite);
        }
    }

    pub fn cancel_move_confirm(&mut self) {
        self.move_confirm = None;
    }

    /// SAF-06: move now, rewriting (or deliberately leaving) inbound links.
    pub fn confirm_move_note(&mut self) {
        if let Some(MoveConfirm {
            id,
            folder,
            rewrite,
            ..
        }) = self.move_confirm.take()
        {
            self.apply_move(id, &folder, rewrite);
        }
    }

    fn apply_move(&mut self, id: usize, folder: &str, rewrite: bool) {
        let was_selected = self.selected == Some(id);
        let old_path = was_selected.then(|| self.selected_path.clone()).flatten();
        let inbound = if rewrite {
            self.vault.link_referrers(id)
        } else {
            Vec::new()
        };
        match self.vault.move_note(id, folder) {
            Ok(path) => {
                self.error = None;
                let failures = if rewrite {
                    let stem = self
                        .vault
                        .notes
                        .get(id)
                        .map(|note| note.stem().to_owned())
                        .unwrap_or_default();
                    self.vault.rewrite_inbound_links(&inbound, &stem)
                } else {
                    Vec::new()
                };
                if was_selected {
                    self.rekey_editor(old_path.as_ref(), &path);
                    self.selected_path = Some(path.clone());
                }
                self.reload(Some(path));
                if !failures.is_empty() {
                    self.error = Some(format!(
                        "Moved, but {} inbound link file(s) could not be rewritten: {}",
                        failures.len(),
                        self.format_rewrite_failures(&failures)
                    ));
                }
            }
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
        self.flush();
        // SAF-06: count links that resolve under this folder before mutating;
        // the rewrite itself replays pre-rename resolution after the move.
        let referrers = self.vault.link_referrers_for_folder(&prompt.folder).len();
        if referrers == 0 {
            self.apply_rename_folder(&prompt.folder, &name, false);
        } else {
            self.folder_rename_confirm = Some(FolderRenameConfirm {
                folder: prompt.folder,
                name,
                referrers,
                rewrite: true,
            });
        }
    }

    /// SAF-06: flip the folder-rename dialog's rewrite choice.
    pub fn toggle_folder_rename_rewrite(&mut self) {
        if let Some(confirm) = self.folder_rename_confirm.as_mut() {
            flip_rewrite(&mut confirm.rewrite);
        }
    }

    pub fn cancel_folder_rename_confirm(&mut self) {
        self.folder_rename_confirm = None;
    }

    /// SAF-06: rename the folder now, rewriting (or deliberately leaving)
    /// inbound links. The rename is not rolled back if a link file write
    /// fails; the failure is reported instead.
    pub fn confirm_folder_rename(&mut self) {
        if let Some(FolderRenameConfirm {
            folder,
            name,
            rewrite,
            ..
        }) = self.folder_rename_confirm.take()
        {
            self.apply_rename_folder(&folder, &name, rewrite);
        }
    }

    fn apply_rename_folder(&mut self, folder: &str, name: &str, rewrite: bool) {
        let folder = folder.trim_matches('/');
        let old_path = self.selected_path.clone();
        match self.vault.rename_folder(folder, name) {
            Ok(path) => {
                self.error = None;
                let new_folder = path
                    .strip_prefix(&self.vault.root)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                let failures = if rewrite && !new_folder.is_empty() {
                    self.vault
                        .rewrite_inbound_links_for_folder(folder, &new_folder)
                } else {
                    Vec::new()
                };
                // Keep the open note selected across the folder move by mapping
                // its old path through the same prefix change.
                let select = old_path.as_ref().map(|old| {
                    let rel = old
                        .strip_prefix(&self.vault.root)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_default();
                    match rel.strip_prefix(&format!("{folder}/")) {
                        Some(rest) => self.vault.root.join(format!("{new_folder}/{rest}")),
                        None => old.clone(),
                    }
                });
                if let (Some(old), Some(new)) = (old_path.as_ref(), select.as_ref()) {
                    self.rekey_editor(Some(old), new);
                }
                // Format before reload: reopening can reorder ids, and these
                // failures name notes in the rewritten (pre-reload) vault.
                let failure_message = (!failures.is_empty()).then(|| {
                    format!(
                        "Renamed, but {} inbound link file(s) could not be rewritten: {}",
                        failures.len(),
                        self.format_rewrite_failures(&failures)
                    )
                });
                self.reload(select);
                if failure_message.is_some() {
                    self.error = failure_message;
                }
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
        if let Some(rest) = url.strip_prefix("note:") {
            // EDT-04: `note:stem#Heading` opens the note and then places the
            // caret (Write) or scrolls the reader (Read) to that heading.
            let (target, heading) = match rest.split_once('#') {
                Some((target, heading)) => (target, Some(heading)),
                None => (rest, None),
            };
            if let Some(id) = self.vault.resolve(target) {
                self.open_from_link(id);
                if let Some(heading) = heading.filter(|h| !h.trim().is_empty()) {
                    self.goto_heading(heading, tx);
                }
            }
        } else if let Some(raw) = url.strip_prefix("missing:") {
            self.create_missing_link(raw);
        } else if tx.open_url(url).is_err() {
            // External links are best-effort: the platform may have no opener.
        }
    }

    /// Create a note for an unresolved link. A path-qualified target names its
    /// folder; a bare one is created in the current note's folder (see
    /// `create_note_named`). Shared by reader clicks and the broken-links
    /// dashboard.
    pub fn create_missing_link(&mut self, raw: &str) {
        let normalised = raw.replace('\\', "/");
        if let Some((folder, title)) = normalised.rsplit_once('/')
            && !title.is_empty()
        {
            self.create_note_named_in(title, folder);
        } else {
            self.create_note_named(raw);
        }
    }

    /// EDT-04: place the caret on `heading` in Write mode, or scroll the reader
    /// in Read mode. A missing heading is not an error: the note opens at the
    /// top instead.
    fn goto_heading(&mut self, heading: &str, tx: &mut rune::Tx) {
        self.pending_heading = None;
        self.pending_top = false;
        let offset = heading_offset(&self.editor.text, heading);
        match self.mode {
            Mode::Preview => {
                // The reader shows the preview-rewritten source, whose byte
                // offsets differ from the note's, so resolve the key against
                // that source. Block keys are `heading-<preview offset>`.
                let key = self.selected.and_then(|id| {
                    let source = self.vault.preview(&self.editor.text, id);
                    heading_offset(&source, heading).map(|offset| format!("heading-{offset}"))
                });
                match key {
                    Some(key) => {
                        self.pending_heading = Some(heading.to_owned());
                        tx.scroll_into_view_in("preview-scroll", key);
                    }
                    None => {
                        self.pending_top = true;
                        tx.scroll_to_y("preview-scroll", 0.0);
                    }
                }
            }
            Mode::Editor => {
                let offset = offset.unwrap_or(0);
                self.editor.anchor = offset;
                self.editor.focus = offset;
            }
            Mode::Graph => {}
        }
    }

    // ---- M2 NAV-02: broken-links dashboard ----------------------------------

    #[must_use]
    pub fn broken_count(&self) -> usize {
        self.vault.broken_links().len()
    }

    pub fn open_broken_links(&mut self) {
        self.broken_panel = true;
        self.menu = None;
        self.blur_text();
    }

    pub fn close_broken_links(&mut self) {
        self.broken_panel = false;
    }

    /// Create the note a missing target names, then leave the dashboard for it.
    pub fn open_broken_target(&mut self, target: &str) {
        self.broken_panel = false;
        self.create_missing_link(target);
    }

    /// Open a note that refers to a missing target.
    pub fn open_broken_referrer(&mut self, id: usize) {
        self.broken_panel = false;
        self.open_from_list(id);
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

/// Case-insensitive, non-overlapping match starts and ends, in bytes of
/// `text`. The haystack is folded character by character with a byte-origin
/// map so Unicode case folding (including a character that lowers to more than
/// one) still reports offsets into the original note.
fn find_matches(text: &str, query: &str) -> (Vec<usize>, Vec<usize>) {
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut folded = String::with_capacity(text.len());
    let mut origin = Vec::with_capacity(text.len() + 1);
    let mut buf = [0u8; 4];
    for (offset, ch) in text.char_indices() {
        for lower in ch.to_lowercase() {
            let bytes = lower.encode_utf8(&mut buf);
            for _ in 0..bytes.len() {
                origin.push(offset);
            }
            folded.push_str(bytes);
        }
    }
    origin.push(text.len());
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let mut from = 0;
    while let Some(found) = folded[from..].find(&needle) {
        let start = from + found;
        let stop = start + needle.len();
        starts.push(origin[start]);
        ends.push(origin[stop]);
        from = stop;
    }
    (starts, ends)
}

/// Clamp a byte offset to `text` and step back to the nearest character
/// boundary. Carets kept in history can outlive a shorter reloaded note.
fn floor_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// M2 EDT-05: a Markdown list prefix at the start of `line`: indentation byte
/// length, marker byte length, and the marker a continued item repeats
/// (numbered items advance by one). Supports `- `, `* `, `+ ` and `N. `.
fn list_prefix(line: &str) -> Option<(usize, usize, String)> {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent_len..];
    for marker in ["- ", "* ", "+ "] {
        if rest.starts_with(marker) {
            return Some((indent_len, marker.len(), marker.to_owned()));
        }
    }
    let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0 && rest[digits..].starts_with(". ") {
        let number = rest[..digits].parse::<u64>().unwrap_or(0);
        return Some((
            indent_len,
            digits + 2,
            format!("{}. ", number.saturating_add(1)),
        ));
    }
    None
}

/// SAF-06: flip one confirmation dialog's rewrite checkbox.
fn flip_rewrite(rewrite: &mut bool) {
    *rewrite = !*rewrite;
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

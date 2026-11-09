//! Application state: the open vault, note, editor, graph and camera.

use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime};

use muninn_core::{ScanJob, Vault, atomic_copy, heading_offset, unclosed_link};
use muninn_graph::{Camera as GraphCamera, Graph, WORLD};
use rune::{Drag, Key, KeyEvent, Phase, TextEdit};

use crate::camera::{Camera, CameraGlide};
use crate::mode::{Mode, Scope};
use crate::recovery::{RecoveryEntry, RecoveryStore, state_home};
use crate::settings::{ReduceMotion, Settings, ThemeMode, clamp_note_template, config_home};
use crate::watch::{self, VaultSnapshot};

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

/// How many recently opened notes the empty palette shows.
pub(crate) const PALETTE_RECENT_MAX: usize = 6;

/// Overlay types live in [`crate::overlay`]; this re-export keeps the
/// pre-extraction import paths stable for `lib.rs` `pub use` consumers.
use crate::overlay::{PaletteAction, PaletteItem, PaletteState};

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

/// Up to this many note files a scan is synchronous: a progress bar for a
/// sub-frame load would flash for no reason. Larger vaults read their bodies in
/// bounded chunks from [`crate::poll`], one frame at a time.
pub(crate) const SCAN_SYNC_LIMIT: usize = 100;

/// Note bodies read per poll tick. Reading is the expensive part (the walk is
/// metadata only); 50 keeps a tick far below one frame on a cold cache while a
/// 2,000-note vault finishes in about forty ticks.
pub(crate) const SCAN_CHUNK: usize = 50;

/// File types drag-and-drop imports: Markdown becomes a note, the rest are
/// copied as attachments. Only the `.md` files reach the sidebar/index; the
/// vault scan deliberately ignores every other extension.
pub const IMPORT_EXTENSIONS: &[&str] = &["md", "png", "jpg", "jpeg", "gif", "webp", "pdf"];

/// Which system handler a path should reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAction {
    /// Hand the file to its default application.
    Open,
    /// Open the containing folder in the file manager.
    Reveal,
}

/// What one [`State::import_files`] call did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Destination paths written into the vault, in input order.
    pub imported: Vec<PathBuf>,
    /// How many of [`ImportReport::imported`] are Markdown notes.
    pub notes: usize,
    /// Why individual files were refused (`name: reason`), never overwritten.
    pub refused: Vec<String>,
}

impl ImportReport {
    /// A successful write happened.
    #[must_use]
    pub fn changed(&self) -> bool {
        !self.imported.is_empty()
    }
}

/// X-01: the Rune revision pinned in the workspace `Cargo.toml`. Logged at
/// startup and shown in diagnostics; update it with the pin together.
pub(crate) const RUNE_REV: &str = "b39c24f";

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
    pub(crate) complete_dismissed: Option<(usize, usize)>,
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
    /// An in-flight camera glide. Kept in `State` (not an `El`) so `poll` can step it
    /// and a headless test can watch it move; `None` in reduced motion.
    pub(crate) camera_glide: Option<CameraGlide>,
    /// A scope switch follows the settling layout by retargeting its glide.
    /// Entering the graph and Reset still fit at once (snapshot-stable first frame).
    pub(crate) smooth_fit: bool,
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
    /// Which palette source the window follows (see [`ThemeMode`]).
    pub theme: ThemeMode,
    /// Omarchy palette file watched for live theme switches.
    pub omarchy_path: PathBuf,
    /// Content hash of the palette file at the last check (0 when missing).
    pub omarchy_rev: u64,
    /// Last seen palette metadata: the cheap trigger so `poll` only re-reads
    /// and re-hashes the file when it actually changed.
    pub omarchy_mtime: Option<SystemTime>,
    pub omarchy_len: u64,
    /// Skip the palette file entirely (tests, and `MUNINN_OMARCHY_COLORS=""`).
    pub omarchy_disabled: bool,
    /// Scroll the sidebar to this note on the next frame.
    pub reveal: Option<usize>,
    pub error: Option<String>,
    /// A successful but non-note-changing action worth one status line, such as
    /// importing images (which the sidebar cannot show). Cleared by the next
    /// import; errors still outrank it in the status bar.
    pub notice: Option<String>,
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
    /// XDG state root, kept so a vault switch can re-key crash recovery.
    pub(crate) state_home: PathBuf,
    /// XDG config root; `None` in tests means settings are never read or written.
    settings_home: Option<PathBuf>,
    /// Loaded preferences. Fields mirrored by `State` are refreshed on every
    /// save, so this stays the source for values the UI does not own (recent
    /// vaults, autosave delay).
    pub(crate) settings: Settings,
    /// Wall-clock cost of the startup scan, shown in the status bar. The fast
    /// path is synchronous; a large vault is scanned in chunks (see
    /// [`State::scan`]) and records the whole load when the last chunk lands.
    load_ms: f64,
    /// Chunked startup scan still reading note bodies. While this is `Some`,
    /// `vault` is an empty placeholder — a partial index is never exposed.
    scan: Option<ScanJob>,
    /// Start of the chunked scan, so `load_ms` covers the whole load.
    scan_started: Option<Instant>,
    /// Note to reopen when the chunked scan completes (session restore).
    restore_after_scan: Option<String>,
    /// Invisible, focused key catcher for the loading screen: a focused text
    /// field sees Escape before the window shortcut table, so the progress view
    /// can cancel without owning `keys.rs`.
    pub(crate) scan_field: TextEdit,
    /// The vault folder did not exist before startup and is still empty.
    pub welcome: bool,
    /// DES-01: in-window keyboard-shortcuts overlay.
    pub help: bool,
    /// v0.5: in-window settings modal. It sits above every other layer and
    /// owns the keyboard while open.
    pub settings_modal: bool,
    /// The Settings modal's selected tab. Ephemeral: never persisted, and
    /// reset to the first tab whenever Settings opens.
    pub settings_tab: crate::overlay::SettingsTab,
    /// Where focus was when the settings modal opened, so closing it returns
    /// typing to the same field.
    pub(crate) settings_return_editor: bool,
    pub(crate) settings_return_search: bool,
    /// v0.5: the settings modal's default-note-template textarea buffer. It
    /// mirrors [`Settings::note_template`]; edits persist through
    /// [`State::set_note_template`].
    pub note_template_field: TextEdit,
    /// v0.5: command palette (notes and actions in one input).
    pub palette: Option<PaletteState>,
    /// The palette's persistent input field; focused while it is open.
    pub palette_field: TextEdit,
    /// Editor/search focus at the moment the palette opened, restored on Esc.
    pub(crate) palette_focus: (bool, bool),
    /// The Settings Vaults path field.
    pub vault_field: TextEdit,
    /// X-01: append-only session log (`$XDG_STATE_HOME/muninn/session.log`).
    pub(crate) log_path: PathBuf,
    /// X-01: `--trace` logs every autosave tick as well as the normal events.
    pub trace: bool,
    /// DES-03: `MUNINN_REDUCED_MOTION=1` was set at startup. It forces
    /// reduced motion on whatever the Settings choice is; read the resolved
    /// value through [`State::reduced_motion`], never this field.
    pub reduced_motion_env: bool,
    /// The OS reduce-motion preference as of the last view, which is the only
    /// place Rune reports it. `Cell` because `view` borrows `State` shared.
    pub(crate) system_reduced_motion: std::cell::Cell<bool>,
    /// Whether the last layout tick ran with reduced motion, so
    /// [`State::graph_step`] can reheat once when it turns off.
    graph_was_reduced: bool,
    pub sync_ui: crate::sync_ui::SyncUi,
}

impl State {
    pub fn new(root: PathBuf) -> State {
        let settings = Settings::load();
        let mut state = Self::with_settings(root, state_home(), config_home(), settings);
        let github = state.settings.sync.github_client_id.clone();
        let google = state.settings.sync.google_client_id.clone();
        state.sync_ui = crate::sync_ui::SyncUi::native();
        state.sync_ui.github_client_id.replace_text(&github);
        state.sync_ui.google_client_id.replace_text(&google);
        state.resume_tailnet_host();
        state
    }

    /// Turn verbose session logging on or off.
    pub fn set_trace(&mut self, trace: bool) {
        self.trace = trace;
        if trace {
            self.log_event("trace", "enabled");
        }
    }

    /// Build a state rooted in `root` with an explicit XDG state home. The
    /// application always uses [`State::new`]; tests use this to keep recovery
    /// copies inside their own temp directory instead of the user's.
    ///
    /// Settings persistence is deliberately disabled here: tests must never
    /// read or overwrite the user's real `~/.config/muninn/settings.json`.
    pub fn with_state_home(root: PathBuf, state_home: PathBuf) -> State {
        Self::build(root, state_home, None, Settings::default())
    }

    /// Build a state with explicit config and state roots, applying `settings`.
    /// This is the constructor behind [`State::new`] and the one tests use to
    /// exercise session restore without touching the user's files.
    pub fn with_settings(
        root: PathBuf,
        state_home: PathBuf,
        config_home: PathBuf,
        settings: Settings,
    ) -> State {
        Self::build(root, state_home, Some(config_home), settings)
    }

    fn build(
        root: PathBuf,
        state_home: PathBuf,
        settings_home: Option<PathBuf>,
        settings: Settings,
    ) -> State {
        // A vault folder that had to be created is the first-run signal. Capture
        // it before the scan, which creates a missing root.
        let existed = root.is_dir();
        let restore_vault = settings
            .last_vault
            .as_deref()
            .is_some_and(|path| same_path(path, &root));
        let restore_rel = settings.selected_rel.clone();
        let restore_after_scan = if restore_vault {
            restore_rel.clone()
        } else {
            None
        };
        let theme = settings.theme;
        let collapsed: HashSet<String> = settings.collapsed.iter().cloned().collect();
        let restore_mode = settings.mode;
        let restore_scope = settings.scope;
        let reduced_motion_env = reduced_motion_from_env();
        let log_path = state_home.join("muninn").join("session.log");
        let note_template_field = TextEdit::new(&settings.note_template);
        let started = Instant::now();
        let mut scan = None;
        let (vault, startup_error) = match Vault::scan_start(&root) {
            Ok(mut job) if job.total() <= SCAN_SYNC_LIMIT => {
                job.read_chunk(usize::MAX);
                (job.finish(), None)
            }
            Ok(job) => {
                // Large vault: keep the placeholder empty and read in `poll`.
                scan = Some(job);
                (Vault::empty(root.clone()), None)
            }
            Err(e) => {
                eprintln!("muninn: cannot read {}: {e}", root.display());
                (
                    Vault::empty(root.clone()),
                    Some(format!("Could not read the vault: {e}")),
                )
            }
        };
        let loading = scan.is_some();
        let load_ms = if loading {
            0.0
        } else {
            started.elapsed().as_secs_f64() * 1_000.0
        };
        let welcome = !existed && startup_error.is_none() && !loading && vault.notes.is_empty();
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
            camera_glide: None,
            smooth_fit: false,
            graph_focus: None,
            graph_query: TextEdit::new(""),
            graph_isolate: false,
            canvas: [900.0, 620.0],
            window_width: 1180.0,
            needs_fit: true,
            auto_fit: true,
            pan_at_press: [0.0; 2],
            dragging: None,
            theme,
            omarchy_path: crate::omarchy::colors_path(),
            omarchy_rev: 0,
            omarchy_mtime: None,
            omarchy_len: 0,
            omarchy_disabled: false,
            reveal: None,
            error: None,
            notice: None,
            expected_disk: None,
            conflict: None,
            last_save: None,
            dirty_at: None,
            now: 0.0,
            collapsed,
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
            state_home,
            settings_home,
            settings,
            load_ms,
            scan,
            scan_started: loading.then_some(started),
            restore_after_scan,
            scan_field: {
                let mut field = TextEdit::new("");
                field.focused = loading;
                field
            },
            welcome,
            help: false,
            settings_modal: false,
            settings_tab: crate::overlay::SettingsTab::Vaults,
            settings_return_editor: false,
            settings_return_search: false,
            note_template_field,
            palette: None,
            palette_field: TextEdit::new(""),
            palette_focus: (false, false),
            vault_field: TextEdit::new(""),
            log_path,
            trace: false,
            reduced_motion_env,
            system_reduced_motion: std::cell::Cell::new(false),
            graph_was_reduced: false,
            sync_ui: crate::sync_ui::SyncUi::memory(),
        };
        // X-01: one startup line with the build, the vault path and counts.
        // Keys only: note bodies and titles never reach the log.
        state.log_event(
            "startup",
            &format!(
                "muninn={} rune={} vault={} notes={} dirs={} skipped={} load_ms={:.1}{}",
                env!("CARGO_PKG_VERSION"),
                RUNE_REV,
                state.vault.root.display(),
                state.vault.notes.len(),
                state.vault.dirs.len(),
                state.vault.skipped_count(),
                state.load_ms,
                if state.startup_error.is_some() {
                    " error=unreadable"
                } else if state.loading() {
                    " scan=pending"
                } else {
                    ""
                }
            ),
        );
        if let Some(job) = state.scan.as_ref() {
            state.log_event("scan-start", &format!("total={}", job.total()));
        }
        // LIF-01: reopen the note that was open last time, but only when this is
        // the same vault. A `--vault` override gets that vault's first note
        // instead of a path that happens to exist in another vault.
        let restored = restore_vault
            .then_some(restore_rel)
            .flatten()
            .and_then(|rel| {
                state
                    .vault
                    .notes
                    .iter()
                    .find(|note| note.rel == rel)
                    .map(|note| note.id)
            });
        if let Some(id) = restored.or(first) {
            state.load_note(id, false);
        }
        state.set_scope(restore_scope);
        state.set_mode(restore_mode);
        if state.mode == Mode::Editor && !state.loading() {
            state.focus_editor();
        }
        if state.settings_home.is_some() {
            let root = state.vault.root.clone();
            state.settings.push_recent(&root);
            state.settings.last_vault = Some(root);
            state.save_settings();
        }
        if !state.loading() {
            state.refresh_watch();
        }
        state
    }

    /// Persist the current theme, folder collapse, vault, note and mode/scope.
    ///
    /// Saves are immediate and best-effort: the file is a few hundred bytes,
    /// and a failed settings write must never interrupt editing. Callers are
    /// the mutators below plus the app's `on_close` hook.
    pub fn save_settings(&self) -> bool {
        let Some(config_home) = self.settings_home.as_ref() else {
            return false;
        };
        let mut settings = self.settings.clone();
        settings.theme = self.theme;
        settings.collapsed = {
            let mut folders: Vec<String> = self.collapsed.iter().cloned().collect();
            folders.sort();
            folders
        };
        settings.last_vault = Some(self.vault.root.clone());
        // While a chunked scan runs there is no selection yet; keep the note the
        // session would restore instead of overwriting it with `None`.
        settings.selected_rel = self
            .selected_rel()
            .map(str::to_owned)
            .or_else(|| self.restore_after_scan.clone());
        settings.mode = self.mode;
        settings.scope = self.scope;
        match settings.save_to(config_home) {
            Ok(()) => true,
            Err(e) => {
                // Settings are a convenience; logging keeps the failure visible
                // without turning it into an editor error bar.
                eprintln!("muninn: could not save settings: {e}");
                false
            }
        }
    }

    // ---- X-01: session log and diagnostics --------------------------------

    /// The append-only session log path.
    #[must_use]
    pub fn session_log_path(&self) -> &Path {
        &self.log_path
    }

    /// Append one timestamped event line. Best-effort: a read-only state
    /// directory must never interrupt editing or saving.
    ///
    /// Callers pass keys (vault-relative names, counts, results), never note
    /// bodies, so the log is safe to attach to a bug report.
    fn log_event(&self, event: &str, detail: &str) {
        let line = if detail.is_empty() {
            format!("{} {event}", conflict_stamp())
        } else {
            format!("{} {event} {detail}", conflict_stamp())
        };
        if let Some(parent) = self.log_path.parent()
            && fs::create_dir_all(parent).is_err()
        {
            return;
        }
        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            let _ = writeln!(file, "{line}");
        }
    }

    /// Log a verbose event only with `--trace`.
    pub fn trace_log(&self, event: &str, detail: &str) {
        if self.trace {
            self.log_event(event, detail);
        }
    }

    /// Log a successful vault open with counts and the synchronous scan time.
    fn log_open(&self, started: Instant) {
        self.log_event(
            "open",
            &format!(
                "{} notes={} dirs={} skipped={} load_ms={:.1}",
                self.vault.root.display(),
                self.vault.notes.len(),
                self.vault.dirs.len(),
                self.vault.skipped_count(),
                started.elapsed().as_secs_f64() * 1_000.0,
            ),
        );
    }

    /// Log a failed save with the note key and the OS error, never its text.
    fn log_save_failure(&self, error: &io::Error) {
        let rel = self.selected_rel().unwrap_or("(no note)");
        self.log_event("save-failed", &format!("{rel}: {error}"));
    }

    /// Shared post-write bookkeeping: drop the recovery copy and trace the
    /// autosave tick with the note key only, never its text.
    fn after_save(&mut self, rel: &str) {
        self.clear_recovery(rel);
        self.trace_log("autosave", rel);
    }

    /// The newest `count` complete log lines (oldest first).
    #[must_use]
    pub fn log_tail(&self, count: usize) -> Vec<String> {
        let Ok(text) = fs::read_to_string(&self.log_path) else {
            return Vec::new();
        };
        let lines: Vec<&str> = text.lines().collect();
        lines[lines.len().saturating_sub(count)..]
            .iter()
            .map(|line| (*line).to_owned())
            .collect()
    }

    /// X-01 diagnostics shown in the Help overlay (and copied to the
    /// clipboard): build, vault counts, log path and the last five lines.
    #[must_use]
    pub fn diagnostics(&self) -> String {
        let mut out = format!(
            "Muninn {} · rune {}\nvault {} · {} notes · {} folders · {} skipped\nlog {}\n",
            env!("CARGO_PKG_VERSION"),
            RUNE_REV,
            self.vault.root.display(),
            self.vault.notes.len(),
            self.vault.folders().len(),
            self.vault.skipped_count(),
            self.log_path.display(),
        );
        let tail = self.log_tail(5);
        if tail.is_empty() {
            out.push_str("(no log lines yet)");
        } else {
            out.push_str("last log lines:\n");
            for line in tail {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out
    }

    /// Idle seconds before autosave. Preferences may store any finite value in
    /// the documented range; the default is 0.55 s.
    #[must_use]
    pub fn autosave_delay(&self) -> f64 {
        if self.settings.autosave_delay.is_finite() {
            self.settings.autosave_delay.clamp(0.05, 10.0)
        } else {
            crate::settings::DEFAULT_AUTOSAVE_DELAY
        }
    }

    /// Startup scan duration, in milliseconds.
    #[must_use]
    pub fn load_ms(&self) -> f64 {
        self.load_ms
    }

    /// Focus the vault-path field in the Settings modal's Vaults section.
    pub fn begin_vault_prompt(&mut self) {
        self.begin_inline_prompt();
        self.vault_field.replace_text("");
        self.vault_field.focused = true;
    }

    pub fn cancel_vault_prompt(&mut self) {
        self.vault_field.focused = false;
    }

    /// Open the path typed into [`State::vault_field`].
    pub fn submit_vault_path(&mut self) {
        let raw = self.vault_field.text.clone();
        if self.open_vault(&raw) {
            self.cancel_vault_prompt();
            self.close_settings();
        }
    }

    /// Create the typed folder when missing and open it as an empty vault.
    /// `Vault::load` already creates a missing root, so this is the same path
    /// as Open… with a name that tells the user what will happen.
    pub fn submit_new_vault(&mut self) {
        let raw = self.vault_field.text.clone();
        if self.create_vault(&raw) {
            self.cancel_vault_prompt();
            self.close_settings();
        }
    }

    /// Open a vault by a path typed by the user (`~` and `~/x` are expanded).
    pub fn open_vault(&mut self, raw: &str) -> bool {
        let raw = raw.trim();
        if raw.is_empty() {
            self.error = Some("Enter a vault folder path".to_owned());
            return false;
        }
        self.switch_vault(expand_path(raw))
    }

    /// Create (when missing) and open a folder as a vault.
    pub fn create_vault(&mut self, raw: &str) -> bool {
        let raw = raw.trim();
        if raw.is_empty() {
            self.error = Some("Enter a folder path for the new vault".to_owned());
            return false;
        }
        let path = expand_path(raw);
        if let Err(e) = fs::create_dir_all(&path) {
            self.error = Some(format!("Could not create the vault: {e}"));
            return false;
        }
        self.switch_vault(path)
    }

    /// Replace the open vault, resetting the graph, history and recovery store.
    /// The old vault is only abandoned after the new one scans successfully and
    /// only when the current buffer is clean: switching must never silently
    /// discard an unresolved conflict or a save that failed.
    pub fn switch_vault(&mut self, root: PathBuf) -> bool {
        self.flush();
        if self.dirty() {
            self.error =
                Some("Save or resolve the current note before switching vaults".to_owned());
            return false;
        }
        let started = Instant::now();
        match Vault::load(&root) {
            Ok(vault) => {
                // A successful explicit switch abandons any chunked startup scan
                // for the old vault; otherwise its completion would overwrite
                // the newly installed vault.
                self.scan = None;
                self.scan_started = None;
                self.restore_after_scan = None;
                self.scan_field.focused = false;
                self.vault = vault;
                self.log_open(started);
                self.selected = None;
                self.selected_path = None;
                self.history.clear();
                self.history_index = 0;
                self.note_editors.clear();
                self.note_editors_order.clear();
                self.graph = Graph::new(&self.vault);
                self.cam = Camera::new();
                self.graph_focus = None;
                self.needs_fit = true;
                self.auto_fit = true;
                self.dragging = None;
                self.conflict = None;
                self.expected_disk = None;
                self.dirty_at = None;
                self.trash_undo = None;
                self.startup_error = None;
                self.error = None;
                self.welcome = false;
                self.broken_panel = false;
                let recovery = RecoveryStore::at(&self.state_home, &self.vault.root);
                self.recovered = recovery.scan();
                self.recovery = recovery;
                let root = self.vault.root.clone();
                self.settings.push_recent(&root);
                self.settings.last_vault = Some(root);
                self.settings.selected_rel = None;
                let first = self.vault.notes.first().map(|note| note.id);
                if let Some(id) = first {
                    self.load_note(id, false);
                } else {
                    self.clear_open_note();
                }
                self.save_settings();
                self.refresh_watch();
                true
            }
            Err(e) => {
                self.log_event("open-failed", &format!("{}: {e}", root.display()));
                self.error = Some(format!("Could not open the vault: {e}"));
                false
            }
        }
    }

    /// The recent-vault MRU, newest first.
    #[must_use]
    pub fn recent_vaults(&self) -> &[PathBuf] {
        &self.settings.recent_vaults
    }

    /// Open the recent vault at `index` (as shown in Settings).
    pub fn open_recent_vault(&mut self, index: usize) -> bool {
        let Some(path) = self.settings.recent_vaults.get(index).cloned() else {
            return false;
        };
        self.switch_vault(path)
    }

    /// The Markdown inserted at the top of every new note. Empty (the
    /// default) keeps the plain `# Title` body.
    #[must_use]
    pub fn note_template(&self) -> &str {
        &self.settings.note_template
    }

    /// Persist a new default note template, clamped to 4096 characters. The
    /// settings textarea is kept in sync, so a pasted over-long value shows
    /// exactly what will be stored. Called after each edit; there is no Save
    /// button.
    pub fn set_note_template(&mut self, template: &str) {
        let template = clamp_note_template(template);
        if self.note_template_field.text != template {
            self.note_template_field.replace_text(template.as_str());
        }
        self.settings.note_template = template;
        self.save_settings();
    }

    // ---- v0.5: command palette ---------------------------------------------

    /// Is the palette open? (The field itself is `palette.is_some()`.)
    #[must_use]
    pub fn palette_open(&self) -> bool {
        self.palette.is_some()
    }

    /// The highlighted row, clamped to the current list.
    #[must_use]
    pub fn palette_selected(&self) -> usize {
        self.palette.as_ref().map_or(0, |palette| palette.selected)
    }

    /// The palette's current rows: actions and notes, searched by one query.
    ///
    /// An empty query deliberately shows only the fixed action list and notes
    /// opened this session (the history's newest entries), never the whole
    /// vault. Typing searches notes through [`State::search_results_for`], so
    /// titles rank above body-only matches, then the matching actions.
    #[must_use]
    pub fn palette_items(&self) -> Vec<PaletteItem> {
        if self.palette.is_none() {
            return Vec::new();
        }
        let query = self.palette_field.text.trim().to_lowercase();
        let mut items = Vec::new();
        if query.is_empty() {
            items.extend(PaletteAction::ALL.into_iter().map(PaletteItem::Action));
            items.extend(self.recent_note_ids().into_iter().map(PaletteItem::Recent));
        } else {
            items.extend(
                self.search_results_for(&query)
                    .into_iter()
                    .map(PaletteItem::Note),
            );
            items.extend(
                PaletteAction::ALL
                    .into_iter()
                    .filter(|action| action.matches(&query))
                    .map(PaletteItem::Action),
            );
        }
        items
    }

    /// Newest-first notes from this session's history, de-duplicated.
    fn recent_note_ids(&self) -> Vec<usize> {
        let Some(last) = self.history.len().checked_sub(1) else {
            return Vec::new();
        };
        let mut seen = HashSet::new();
        let mut ids = Vec::new();
        for entry in self.history[..=self.history_index.min(last)].iter().rev() {
            let Some(id) = self.note_at_path(&entry.path) else {
                continue;
            };
            if seen.insert(id) {
                ids.push(id);
                if ids.len() == PALETTE_RECENT_MAX {
                    break;
                }
            }
        }
        ids
    }

    /// Keep the highlight inside the current list and reset it when the query
    /// changes. `poll` calls this after the field has committed a character.
    pub fn palette_tick(&mut self) -> bool {
        if self.palette.is_none() {
            return false;
        }
        let mut changed = false;
        {
            let palette = self.palette.as_mut().expect("palette checked above");
            if palette.query != self.palette_field.text {
                palette.query.clone_from(&self.palette_field.text);
                palette.selected = 0;
                palette.reveal = true;
                changed = true;
            }
        }
        let len = self.palette_items().len();
        let palette = self.palette.as_mut().expect("palette checked above");
        if len == 0 {
            if palette.selected != 0 {
                palette.selected = 0;
                changed = true;
            }
        } else if palette.selected >= len {
            palette.selected = len - 1;
            changed = true;
        }
        changed
    }

    /// Up/Down: move the highlight by `delta`, wrapping at both ends.
    pub fn palette_step(&mut self, delta: isize) {
        self.palette_tick();
        let len = self.palette_items().len();
        if len == 0 {
            return;
        }
        let palette = self.palette.as_mut().expect("palette is open");
        palette.selected = (palette.selected as isize + delta).rem_euclid(len as isize) as usize;
        palette.reveal = true;
    }

    /// Offer the highlighted row to `poll` so it can scroll it into view.
    pub(crate) fn palette_take_reveal(&mut self) -> Option<String> {
        let palette = self.palette.as_mut()?;
        if !std::mem::take(&mut palette.reveal) {
            return None;
        }
        Some(format!("palette-item-{}", palette.selected))
    }

    /// Enter in the palette: run the highlighted row.
    pub fn palette_run(&mut self) {
        self.palette_tick();
        self.palette_run_index(self.palette_selected());
    }

    /// Run the row at `index` (also used by pointer presses on a row).
    pub fn palette_run_index(&mut self, index: usize) {
        let Some(item) = self.palette_items().get(index).cloned() else {
            return;
        };
        self.close_palette();
        match item {
            PaletteItem::Recent(id) | PaletteItem::Note(id) => self.open_note(id),
            PaletteItem::Action(action) => self.palette_run_action(action),
        }
    }

    fn palette_run_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::OpenLocalGraph => self.open_local_graph(),
            PaletteAction::SwitchVault => {
                self.open_settings();
                self.begin_vault_prompt();
            }
            PaletteAction::OpenSettings => self.open_settings(),
            PaletteAction::NewNote => self.new_note(),
            PaletteAction::TogglePreview => self.toggle_preview(),
            PaletteAction::ToggleGraph => self.toggle_graph(),
        }
    }

    /// Dismiss the first-run welcome banner without creating a note.
    pub fn dismiss_welcome(&mut self) {
        self.welcome = false;
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
        // LIF-01: session restore is path-based, so record the note as soon as
        // it is showing. History navigation and recovery also land here.
        self.save_settings();
    }

    pub fn open_from_list(&mut self, id: usize) {
        if self.mode == Mode::Graph {
            self.mode = Mode::Editor;
            self.camera_glide = None;
        }
        self.load_note(id, true);
    }

    pub fn open_from_link(&mut self, id: usize) {
        self.load_note(id, self.mode == Mode::Editor);
    }

    pub fn open_note(&mut self, id: usize) {
        self.mode = Mode::Editor;
        self.camera_glide = None;
        self.load_note(id, true);
    }

    pub fn select_graph_node(&mut self, id: usize) {
        // A pointer selection is also where the keyboard focus ring sits, and
        // the camera glides to the note instead of jumping there.
        self.graph_focus = Some(id);
        self.load_note(id, false);
        self.center_graph_on(id);
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
        self.search_results_for(&self.search.text)
    }

    /// Rank notes for an arbitrary query. The sidebar passes its field and the
    /// command palette passes its own, so both share one title-first order
    /// without either field borrowing the other's text.
    #[must_use]
    pub fn search_results_for(&self, query: &str) -> Vec<usize> {
        self.vault.search(query)
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
        self.dismiss_palette();
        self.set_text_focus(true, false);
    }

    pub fn focus_search(&mut self) {
        self.complete = None;
        self.complete_dismissed = None;
        self.dismiss_palette();
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
    pub(crate) fn set_text_focus(&mut self, editor: bool, search: bool) {
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

    /// DES-03: one layout tick. Reduced motion pins heat to zero and never
    /// moves a node; the deterministic seed positions stay where they are.
    ///
    /// When the resolved value turns off (the Settings choice, or the OS
    /// preference under System), the layout is reheated once. Without that, a
    /// vault over the dense-layout limit would stay dormant on its seed
    /// positions, because pinned heat and zero velocity read as "settled".
    pub(crate) fn graph_step(&mut self) -> bool {
        let reduced = self.reduced_motion();
        let resumed = self.graph_was_reduced && !reduced;
        self.graph_was_reduced = reduced;
        if reduced {
            self.graph.heat = 0.0;
            false
        } else {
            if resumed {
                self.graph.reheat();
            }
            self.graph.step()
        }
    }

    /// Set the palette source and persist the choice. The theme button calls
    /// this (cycling through the sources); startup applies it before the first
    /// frame.
    pub fn set_theme(&mut self, theme: ThemeMode) {
        if self.theme == theme {
            // Still repaint: a pinned or followed palette may have changed
            // underneath (e.g. a system appearance flip), and the button is
            // the user's cue that the mode was confirmed.
            self.save_settings();
            return;
        }
        self.theme = theme;
        self.save_settings();
    }

    /// The Reduce motion choice from Settings (not the resolved value).
    #[must_use]
    pub fn reduce_motion(&self) -> ReduceMotion {
        self.settings.reduce_motion
    }

    /// Set the Reduce motion choice and persist it. Takes effect on the next
    /// frame: the layout stops stepping, a camera glide in flight lands, and
    /// Rune transitions (hover, selection, presence) stop travelling.
    pub fn set_reduce_motion(&mut self, mode: ReduceMotion) {
        self.settings.reduce_motion = mode;
        self.save_settings();
    }

    /// Whether motion is reduced right now: the single resolved value that the
    /// graph layout, the camera, graph press colours, Rune's own transitions
    /// (and any future graph-effects style) all read. See
    /// [`ReduceMotion::resolve`] for how `MUNINN_REDUCED_MOTION`, the Settings
    /// choice and the OS preference combine.
    #[must_use]
    pub fn reduced_motion(&self) -> bool {
        self.settings
            .reduce_motion
            .resolve(self.reduced_motion_env, self.system_reduced_motion.get())
    }

    /// Record the OS reduce-motion preference. `view` calls this every frame
    /// with what Rune reports; a host without a Rune view may call it too.
    pub fn set_system_reduced_motion(&self, reduced: bool) {
        self.system_reduced_motion.set(reduced);
    }

    /// Advance the theme button's cycle (Omarchy → System → Dark → Light).
    pub fn cycle_theme(&mut self) {
        self.set_theme(self.theme.next());
    }

    /// The Omarchy palette file that `poll` watches. Overridable in tests and
    /// by `MUNINN_OMARCHY_COLORS` (which `crate::omarchy::colors_path` reads
    /// once at startup); a test points this at a fixture file instead.
    pub fn set_omarchy_path(&mut self, path: PathBuf) {
        self.omarchy_path = path;
    }

    /// Never look up the shared Omarchy palette (tests, embedded use).
    pub fn disable_omarchy_theme(&mut self) {
        self.omarchy_disabled = true;
    }

    /// `poll` hook: cheap mtime/length check, then content-hash compare.
    /// Returns true when the palette content actually changed, so the theme
    /// switch is a visible change (`omarchy-theme-set` rewrites the file).
    /// The first check only adopts the current signature as the baseline:
    /// first paint reads the file fresh anyway, so boot is not a "change"
    /// (and must not wake the window for `poll`'s callers).
    pub fn omarchy_tick(&mut self) -> bool {
        if self.omarchy_disabled || self.theme != ThemeMode::Omarchy {
            return false;
        }
        let (mtime, len, hash) = crate::omarchy::signature(&self.omarchy_path);
        if self.omarchy_mtime.is_none() && self.omarchy_len == 0 && self.omarchy_rev == 0 {
            self.omarchy_mtime = mtime;
            self.omarchy_len = len;
            self.omarchy_rev = hash;
            return false;
        }
        if mtime == self.omarchy_mtime && len == self.omarchy_len {
            return false;
        }
        self.omarchy_mtime = mtime;
        self.omarchy_len = len;
        if hash == self.omarchy_rev {
            // Some tools rewrite the file with identical bytes; nothing to show.
            return false;
        }
        self.omarchy_rev = hash;
        true
    }

    /// Fit the visible nodes without giving up the "follow the layout" mode.
    /// Returns true when the camera pan/zoom actually changed beyond a
    /// sub-pixel threshold, so pollers can report only visible changes.
    /// The threshold absorbs the layout's invisible spring-vs-separation
    /// creep (a few thousandths of a point per step).
    pub fn refit(&mut self) -> bool {
        self.refit_with(false)
    }

    /// The auto-fit path: retarget a running glide instead of snapping, so a
    /// scope switch eases into its final framing as the layout settles.
    pub(crate) fn refit_glide(&mut self) -> bool {
        self.refit_with(true)
    }

    fn refit_with(&mut self, smooth: bool) -> bool {
        let before = (self.cam.pan(), self.cam.zoom());
        let visible = self.visible_notes();
        if let Some(bounds) = self.graph.bounds(&visible)
            && let Some(target) = GraphCamera::fitted_to(bounds, self.canvas)
        {
            if smooth {
                self.ease_camera_to(target);
            } else {
                self.cam.fit(bounds, self.canvas);
            }
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
        self.smooth_fit = false;
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
                if self.selected == Some(id) {
                    // The focused note is already open: Enter brings it back
                    // to the centre with a glide instead of reloading it and
                    // leaving the graph.
                    self.center_graph_on(id);
                } else {
                    self.open_note(id);
                }
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

    /// Glide the camera so `id` is comfortably on screen (zooming in to at
    /// least 0.9, never out). Reduced motion snaps at once.
    pub fn center_graph_on(&mut self, id: usize) {
        let Some(pos) = self.graph.node(id).map(|node| node.pos) else {
            return;
        };
        let target = self.cam.inner().focused_on(pos, self.canvas, 0.9);
        self.glide_camera_to(target);
    }

    /// Ease the camera toward a fit of the visible notes, used when the scope
    /// or the visible set changes without a layout reset.
    pub(crate) fn glide_graph_fit(&mut self) {
        let visible = self.visible_notes();
        let Some(bounds) = self.graph.bounds(&visible) else {
            return;
        };
        let Some(target) = GraphCamera::fitted_to(bounds, self.canvas) else {
            return;
        };
        self.ease_camera_to(target);
    }

    /// Start a camera glide to `target`, or land it at once when motion is
    /// reduced, the graph is not on screen, or there is nowhere to go. Explicit
    /// navigation (a click, search reveal or scope button) stops auto-fit.
    fn glide_camera_to(&mut self, target: GraphCamera) {
        self.auto_fit = false;
        self.smooth_fit = false;
        self.ease_camera_to(target);
    }

    /// Move the camera to `target`: a spring glide in normal motion, or an
    /// immediate set under reduced motion. An in-flight glide is retargeted,
    /// which lets `refit` follow a settling layout without restarting.
    fn ease_camera_to(&mut self, target: GraphCamera) {
        let current = self.cam.inner();
        let still = (current.pan[0] - target.pan[0]).abs() < 0.5
            && (current.pan[1] - target.pan[1]).abs() < 0.5
            && (current.zoom - target.zoom).abs() < 0.005;
        if self.reduced_motion() || self.mode != Mode::Graph || still {
            self.camera_glide = None;
            self.cam.set(target.pan, target.zoom);
        } else if let Some(glide) = &mut self.camera_glide {
            glide.retarget(target);
        } else {
            self.camera_glide = Some(CameraGlide::new(current, target, self.now));
        }
    }

    /// Step an in-flight camera glide at `now`. Returns true when the visible
    /// camera moved, so `poll` only reports a frame the user can see.
    pub(crate) fn advance_camera_glide(&mut self) -> bool {
        let Some(mut glide) = self.camera_glide else {
            return false;
        };
        // A wheel, drag or direct camera set wins over a glide in flight.
        if !same_camera(self.cam.inner(), glide.pose()) {
            self.camera_glide = None;
            return false;
        }
        // A reduced-motion switch mid-flight must not keep waking the window.
        if self.reduced_motion() {
            let target = glide.target();
            self.camera_glide = None;
            let before = (self.cam.pan(), self.cam.zoom());
            self.cam.set(target.pan, target.zoom);
            let after = (self.cam.pan(), self.cam.zoom());
            return camera_moved(before, after);
        }
        let (next, done) = glide.advance(self.now);
        let before = (self.cam.pan(), self.cam.zoom());
        self.cam.set(next.pan, next.zoom);
        if done {
            self.camera_glide = None;
        } else {
            self.camera_glide = Some(glide);
        }
        let after = (self.cam.pan(), self.cam.zoom());
        camera_moved(before, after)
    }

    /// Whether a camera glide still has frames left to draw. `poll` asks Rune
    /// for the next frame only while this is true, so reduced motion (which
    /// never starts a glide) leaves the window asleep.
    #[must_use]
    pub fn camera_gliding(&self) -> bool {
        self.camera_glide.is_some()
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
        if self.sync_ui.phase.is_running() {
            return false;
        }
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
                    self.after_save(&rel);
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
                self.log_save_failure(&e);
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
        self.log_event("recovery", &format!("restored {rel}"));
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
    pub(crate) fn refresh_watch(&mut self) {
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
        if self.sync_ui.phase.is_running() {
            return false;
        }
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
        self.log_event("watch", &format!("reload notes={}", self.vault.notes.len()));
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
        let kind = match reason {
            ConflictReason::Modified => "modified",
            ConflictReason::Deleted => "deleted",
        };
        let rel = self
            .vault
            .notes
            .get(note_id)
            .map_or("(missing)", |note| note.rel.as_str())
            .to_owned();
        self.log_event("conflict", &format!("{rel} {kind}"));
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

    pub(crate) fn reload(&mut self, select: Option<PathBuf>) {
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

    /// Retry the startup scan after [`State::startup_error`]. A small vault is
    /// scanned synchronously; a large one starts the same chunked path as
    /// startup, and `poll` completes it. On failure the error is refreshed.
    pub fn retry_startup(&mut self) -> bool {
        match Vault::scan_start(&self.vault.root) {
            Ok(mut job) => {
                let total = job.total();
                if total <= SCAN_SYNC_LIMIT {
                    job.read_chunk(usize::MAX);
                    self.install_scanned_vault(job.finish());
                } else {
                    self.startup_error = None;
                    self.error = None;
                    self.restore_after_scan = None;
                    self.scan_field.focused = true;
                    self.scan = Some(job);
                    self.scan_started = Some(Instant::now());
                    self.log_event("scan-start", &format!("total={total}"));
                }
                true
            }
            Err(e) => {
                self.startup_error = Some(format!("Could not read the vault: {e}"));
                false
            }
        }
    }

    /// True while the startup scan is still reading note bodies. The view shows
    /// the progress page and `poll` advances the scan; no partial vault is
    /// exposed.
    #[must_use]
    pub fn loading(&self) -> bool {
        self.scan.is_some()
    }

    /// Progress as `(files done, files total)` while loading.
    #[must_use]
    pub fn load_progress(&self) -> Option<(usize, usize)> {
        self.scan.as_ref().map(|job| (job.done(), job.total()))
    }

    /// Read the next chunk of a large startup scan, installing the vault when
    /// the last file lands. Returns true when something visible changed
    /// (progress or the finished vault), so `poll` can wake the window.
    pub(crate) fn scan_tick(&mut self) -> bool {
        let Some(job) = self.scan.as_mut() else {
            return false;
        };
        job.read_chunk(SCAN_CHUNK);
        if !job.is_done() {
            return true;
        }
        let Some(job) = self.scan.take() else {
            return false;
        };
        let total = job.total();
        let vault = job.finish();
        self.load_ms = self
            .scan_started
            .take()
            .map_or(0.0, |started| started.elapsed().as_secs_f64() * 1_000.0);
        self.install_scanned_vault(vault);
        self.log_event(
            "scan-complete",
            &format!(
                "notes={} total={} load_ms={:.1}",
                self.vault.notes.len(),
                total,
                self.load_ms
            ),
        );
        true
    }

    /// Escape on the loading screen: abandon the scan and leave the app on the
    /// startup error page with Retry. The placeholder vault stays empty — a
    /// half-read index is never installed.
    pub fn cancel_scan(&mut self) -> bool {
        let Some(job) = self.scan.take() else {
            return false;
        };
        let (done, total) = (job.done(), job.total());
        self.scan_started = None;
        self.restore_after_scan = None;
        self.scan_field.focused = false;
        self.startup_error = Some(format!("Scan cancelled after {done} of {total} notes"));
        self.log_event("scan-cancelled", &format!("done={done} total={total}"));
        true
    }

    /// Replace the placeholder with a finished scan: build the graph, reopen
    /// the session's note (or the first one) and start watching the folder.
    fn install_scanned_vault(&mut self, vault: Vault) {
        self.vault = vault;
        self.scan_field.focused = false;
        self.startup_error = None;
        self.error = None;
        self.selected = None;
        self.selected_path = None;
        self.history.clear();
        self.history_index = 0;
        self.note_editors.clear();
        self.note_editors_order.clear();
        self.graph = Graph::new(&self.vault);
        self.cam = Camera::new();
        self.graph_focus = None;
        self.needs_fit = true;
        self.auto_fit = true;
        self.dragging = None;
        self.conflict = None;
        self.expected_disk = None;
        self.dirty_at = None;
        self.trash_undo = None;
        self.broken_panel = false;
        let restore = self.restore_after_scan.take();
        let id = restore
            .and_then(|rel| {
                self.vault
                    .notes
                    .iter()
                    .find(|note| note.rel == rel)
                    .map(|note| note.id)
            })
            .or_else(|| self.vault.notes.first().map(|note| note.id));
        if let Some(id) = id {
            self.load_note(id, false);
        } else {
            self.clear_open_note();
        }
        let root = self.vault.root.clone();
        self.settings.push_recent(&root);
        self.settings.last_vault = Some(root);
        self.save_settings();
        self.refresh_watch();
    }

    /// Begin naming a note in the current folder. Nothing is created until
    /// the field is submitted with a non-empty name; the new note then starts
    /// with the configured default template (see [`Self::set_note_template`]).
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
        self.begin_inline_prompt();
        self.collapsed.remove(folder);
        self.new_note_field.replace_text("");
        self.new_note_field.focused = true;
        self.new_note_prompt = Some(NamePrompt {
            folder: folder.to_owned(),
        });
    }

    /// Shared start for the sidebar's inline fields: close the pointer menu and
    /// hand the keyboard to a prompt instead of the editor or search box.
    fn begin_inline_prompt(&mut self) {
        self.dismiss_palette();
        self.menu = None;
        self.vault_field.focused = false;
        self.editor.focused = false;
        self.search.focused = false;
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
                        self.log_event("trash", &trash_rel);
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
                self.log_event("restore", &undo.trash_rel);
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

    /// Files and bytes currently in the vault-local trash, shown in Help and
    /// used by the core's fixed 500-file / 100 MB budget.
    #[must_use]
    pub fn trash_size(&self) -> (usize, u64) {
        self.vault.trash_size()
    }

    /// Permanently empty the vault-local trash. Live notes are untouched, and
    /// the Undo toast is dropped because its file no longer exists.
    pub fn empty_trash(&mut self) -> bool {
        let (files, bytes) = self.vault.trash_size();
        match self.vault.empty_trash() {
            Ok(()) => {
                self.trash_undo = None;
                self.error = None;
                self.notice = Some("Trash emptied".to_owned());
                self.log_event("trash-empty", &format!("files={files} bytes={bytes}"));
                true
            }
            Err(e) => {
                self.error = Some(format!("Could not empty trash: {e}"));
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
        self.begin_inline_prompt();
        self.collapsed.remove(parent);
        self.new_folder_field.replace_text("");
        self.new_folder_field.focused = true;
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
        self.begin_inline_prompt();
        let current = folder.rsplit('/').next().unwrap_or(folder).to_owned();
        self.rename_folder_field.replace_text(current.as_str());
        self.rename_folder_field.focused = true;
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
            (MenuTarget::Note(id), "reveal") => {
                self.reveal_note(id);
            }
            (MenuTarget::Note(id), "open_external") => {
                self.open_note_externally(id);
            }
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
        self.save_settings();
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
                if let Err(e) = self.apply_note_template(&path) {
                    self.error = Some(format!("Could not add the note template: {e}"));
                }
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

    /// Prepend the default note template to a just-created note file. The
    /// template is inserted verbatim before the heading; a missing trailing
    /// newline is added so the two do not run together. An empty template
    /// writes nothing, keeping the default note body byte-for-byte unchanged.
    fn apply_note_template(&self, path: &Path) -> io::Result<()> {
        let template = self.settings.note_template.as_str();
        if template.is_empty() {
            return Ok(());
        }
        let mut body = template.to_owned();
        if !body.ends_with('\n') {
            body.push('\n');
        }
        body.push_str(&fs::read_to_string(path)?);
        fs::write(path, body)
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

    // ---- Attachments: open, reveal and drag-drop import ----------------------

    /// Open a file with the platform handler (`xdg-open` on Linux).
    ///
    /// Rune's `Tx::open_url` refuses `file:` links, so the OS opener is spawned
    /// directly. The exported path is canonicalised first: only an existing
    /// file is handed out, and failures become a status error instead of a
    /// panic or a silent no-op.
    pub fn open_externally(&mut self, path: &Path) -> bool {
        self.run_file_action(FileAction::Open, path)
    }

    /// Open the containing folder in the file manager.
    pub fn reveal_in_file_manager(&mut self, path: &Path) -> bool {
        self.run_file_action(FileAction::Reveal, path)
    }

    /// Open a note with its default external application.
    pub fn open_note_externally(&mut self, id: usize) -> bool {
        self.note_file_action(id, FileAction::Open)
    }

    /// Reveal a note in the file manager.
    pub fn reveal_note(&mut self, id: usize) -> bool {
        self.note_file_action(id, FileAction::Reveal)
    }

    fn note_file_action(&mut self, id: usize, action: FileAction) -> bool {
        match self.vault.notes.get(id).map(|note| note.path.clone()) {
            Some(path) => self.run_file_action(action, &path),
            None => {
                self.error = Some(format!("Note {id} is no longer in this vault"));
                false
            }
        }
    }

    /// Open an attachment named by a note embed (`![[assets/pic.png]]`). The
    /// target is resolved against the selected note before the platform opener
    /// runs, so a stale embed reports a clear error.
    pub fn open_attachment(&mut self, target: &str) -> bool {
        let Some(current) = self.selected else {
            self.error = Some(format!("Cannot open {target}: no note is selected"));
            return false;
        };
        let Some(path) = self.vault.attachment_path(target, current) else {
            self.error = Some(format!(
                "Attachment {target} is not in this vault — drop it in, or fix the embed"
            ));
            return false;
        };
        self.open_externally(&path)
    }

    fn run_file_action(&mut self, action: FileAction, path: &Path) -> bool {
        let absolute = match fs::canonicalize(path) {
            Ok(path) if path.is_file() => path,
            Ok(_) => {
                self.error = Some(format!("{} is not a file", path.display()));
                return false;
            }
            Err(_) => {
                self.error = Some(format!("{} no longer exists", path.display()));
                return false;
            }
        };
        let (program, args) = match file_action_command(action, &absolute) {
            Ok(command) => command,
            Err(message) => {
                self.error = Some(message);
                return false;
            }
        };
        let verb = match action {
            FileAction::Open => "open",
            FileAction::Reveal => "reveal",
        };
        // Fire and forget: a spawned opener returns at once and must not block a
        // frame. A missing program is the one failure worth surfacing.
        match Command::new(&program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(_) => {
                self.trace_log(&format!("{verb}-external"), &absolute.display().to_string());
                true
            }
            Err(error) => {
                self.error = Some(format!(
                    "Could not {verb} {}: {error}. {}",
                    absolute.display(),
                    opener_hint()
                ));
                false
            }
        }
    }

    /// Import dropped files into the current folder, then reload the vault.
    ///
    /// Markdown becomes notes; images and PDFs are copied as attachments the
    /// scan deliberately ignores, so the sidebar shows no row for them. Every
    /// destination is written through a hidden temp file + rename (the same
    /// crash-safe pattern as note saves) and an existing file is never
    /// overwritten: the refusal names the file.
    pub fn import_files(&mut self, paths: &[PathBuf]) -> ImportReport {
        let mut report = ImportReport::default();
        self.notice = None;
        if paths.is_empty() {
            return report;
        }
        let folder = self.current_folder();
        let dest_dir = if folder.is_empty() {
            self.vault.root.clone()
        } else {
            self.vault.root.join(&folder)
        };
        if let Err(error) = fs::create_dir_all(&dest_dir) {
            self.error = Some(format!(
                "Could not import into {}: {error}",
                dest_dir.display()
            ));
            return report;
        }
        let mut first_note: Option<PathBuf> = None;
        for source in paths {
            let name = source
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| source.display().to_string());
            if !source.is_file() {
                report.refused.push(format!("{name}: not a file"));
                continue;
            }
            let extension = source
                .extension()
                .and_then(OsStr::to_str)
                .map(str::to_ascii_lowercase);
            let Some(extension) =
                extension.filter(|extension| IMPORT_EXTENSIONS.contains(&extension.as_str()))
            else {
                report
                    .refused
                    .push(format!("{name}: unsupported file type"));
                continue;
            };
            let some_name = source.file_name().unwrap_or(OsStr::new(""));
            let dest = dest_dir.join(some_name);
            if dest.exists() {
                report
                    .refused
                    .push(format!("{name}: already exists in this folder"));
                continue;
            }
            match atomic_copy(source, &dest) {
                Ok(()) => {
                    report.imported.push(dest.clone());
                    if extension == "md" {
                        report.notes += 1;
                        if first_note.is_none() {
                            first_note = Some(dest);
                        }
                    }
                }
                Err(error) => report.refused.push(format!("{name}: {error}")),
            }
        }
        if let Some(first_note) = first_note {
            // Save the open buffer before the index is rebuilt, then land on the
            // first imported note so the drop has a visible result.
            self.flush();
            self.reload(Some(first_note));
        }
        if report.refused.is_empty() {
            self.notice = Some(import_notice(&report));
            self.trace_log(
                "import",
                &format!("files={} notes={}", report.imported.len(), report.notes),
            );
        } else {
            self.notice = report.changed().then(|| import_notice(&report));
            self.error = Some(if report.imported.is_empty() {
                format!("Import refused: {}", report.refused.join("; "))
            } else {
                format!(
                    "Imported {} of {}; refused: {}",
                    report.imported.len(),
                    report.imported.len() + report.refused.len(),
                    report.refused.join("; ")
                )
            });
        }
        report
    }

    /// Explain a drag full of files the drop target does not accept.
    pub fn reject_drop(&mut self, paths: &[PathBuf]) {
        let names: Vec<String> = paths
            .iter()
            .take(3)
            .map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            })
            .collect();
        self.notice = None;
        self.error = Some(format!(
            "Cannot import {}: drop .md, png, jpg, jpeg, gif, webp or pdf files",
            names.join(", ")
        ));
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
                    let source = self.vault.preview_with_attachments(&self.editor.text, id);
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

/// LIF-01: compare two vault paths, following symlinks when both exist. A
/// missing path compares literally, which is the best available answer before
/// the first scan creates it.
fn same_path(a: &Path, b: &Path) -> bool {
    let canonical = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical(a) == canonical(b)
}

/// Expand a user-typed vault path: `~` and `~/…` use `$HOME` (or
/// `%USERPROFILE%`); everything else is left as typed, so relative paths stay
/// relative to the process directory.
fn expand_path(raw: &str) -> PathBuf {
    let raw = raw.trim();
    let home = || {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    };
    if raw == "~"
        && let Some(home) = home()
    {
        return home;
    }
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = home()
    {
        return home.join(rest);
    }
    PathBuf::from(raw)
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

/// The platform command that opens or reveals `path`.
///
/// Kept separate from the runner so the command shape is
/// testable without launching anything. Linux/BSD: `xdg-open` (the file for
/// Open, its parent folder for Reveal). macOS: `open` / `open -R`. Windows:
/// `cmd /C start ""` / `explorer /select,`. Any other target returns a message
/// rather than guessing.
pub fn file_action_command(
    action: FileAction,
    path: &Path,
) -> Result<(String, Vec<OsString>), String> {
    #[cfg(target_os = "macos")]
    {
        let mut args = match action {
            FileAction::Open => Vec::new(),
            FileAction::Reveal => vec![OsString::from("-R")],
        };
        args.push(path.as_os_str().to_owned());
        Ok(("open".to_owned(), args))
    }
    #[cfg(target_os = "windows")]
    {
        match action {
            FileAction::Open => Ok((
                "cmd".to_owned(),
                vec![
                    OsString::from("/C"),
                    OsString::from("start"),
                    OsString::from(""),
                    path.as_os_str().to_owned(),
                ],
            )),
            FileAction::Reveal => Ok((
                "explorer".to_owned(),
                vec![OsString::from(format!("/select,{}", path.display()))],
            )),
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = match action {
            FileAction::Open => path.to_path_buf(),
            FileAction::Reveal => path
                .parent()
                .map_or_else(|| path.to_path_buf(), Path::to_path_buf),
        };
        Ok(("xdg-open".to_owned(), vec![target.into_os_string()]))
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = (action, path);
        Err("No file opener is documented for this platform; open the file manually".to_owned())
    }
}

/// The install hint appended when spawning the opener fails.
fn opener_hint() -> &'static str {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "Install xdg-utils (for xdg-open) or open the file manually."
    }
    #[cfg(target_os = "macos")]
    {
        "The system `open` command failed; open the file from Finder."
    }
    #[cfg(target_os = "windows")]
    {
        "The system opener failed; open the file from Explorer."
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        "Open the file manually."
    }
}

/// Copy `source` to `dest` through a hidden sibling temp file + rename, so a
/// failed or interrupted import never leaves a partial attachment behind and
/// never replaces an existing file.
/// One status line for an import: counts, and why the sidebar may stay empty.
fn import_notice(report: &ImportReport) -> String {
    let files = report.imported.len();
    let mut text = format!("Imported {files} file{}", if files == 1 { "" } else { "s" });
    if report.notes > 0 {
        text.push_str(&format!(
            " · {} note{}",
            report.notes,
            if report.notes == 1 { "" } else { "s" }
        ));
    }
    if files > report.notes {
        text.push_str(" · non-Markdown files stay attachments");
    }
    text
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

/// Whether a camera pose change is big enough to count as visible motion.
fn camera_moved(before: ([f32; 2], f32), after: ([f32; 2], f32)) -> bool {
    (before.0[0] - after.0[0]).abs() > 0.05
        || (before.0[1] - after.0[1]).abs() > 0.05
        || (before.1 - after.1).abs() > 0.0005
}

/// Whether two poses are the same within sub-pixel travel tolerance.
fn same_camera(a: GraphCamera, b: GraphCamera) -> bool {
    (a.pan[0] - b.pan[0]).abs() < 0.01
        && (a.pan[1] - b.pan[1]).abs() < 0.01
        && (a.zoom - b.zoom).abs() < 0.0001
}

/// DES-03: whether `MUNINN_REDUCED_MOTION` asks for the reduced-motion mode.
fn reduced_motion_from_env() -> bool {
    flag_on(std::env::var_os("MUNINN_REDUCED_MOTION").as_deref())
}

/// A documented boolean environment flag: `1`, `true`, `yes` or `on`
/// (case-insensitive) is on; anything else, including unset, is off.
fn flag_on(value: Option<&OsStr>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.to_string_lossy().trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_flag_accepts_documented_values() {
        for on in ["1", "true", "TRUE", " yes ", "On"] {
            assert!(flag_on(Some(OsStr::new(on))), "{on:?} should be on");
        }
        for off in ["", "0", "false", "no", "off", "2"] {
            assert!(!flag_on(Some(OsStr::new(off))), "{off:?} should be off");
        }
        assert!(!flag_on(None));
    }
}

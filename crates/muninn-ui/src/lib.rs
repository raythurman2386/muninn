//! Muninn's state and Rune views: sidebar, editor, Markdown reader, graph canvas and boards.
//!
//! The binary in `muninn-app` only wires these into `rune::run`; everything that can be
//! tested headlessly lives here.

mod autostart;
mod board_camera;
mod board_edit;
mod board_new;
mod board_note_link;
mod board_palette;
mod board_rename;
mod board_safety;
mod board_scene;
mod board_sync;
mod board_text;
mod board_tools;
mod board_view;
mod camera;
pub mod contrast;
mod doc_sync;
mod focus;
mod global_shortcut;
mod graph_view;
mod import_document;
mod kanban_archive;
mod kanban_dates;
mod kanban_drag;
mod kanban_edit;
mod kanban_lanes;
mod kanban_links;
mod kanban_metadata;
mod kanban_search;
mod kanban_settings;
mod kanban_sync;
mod kanban_tags;
mod kanban_view;
mod kanban_views;
mod kanban_wrap;
mod keys;
mod lock_session;
mod mode;
mod omarchy;
mod overlay;
mod paste;
mod poll;
mod recovery;
mod settings;
mod state;
mod sync_ui;
mod tray;
pub mod update;
mod view;
mod watch;

pub use autostart::{
    AUTOSTART_NAME, AutostartPort, NativeAutostart, WINDOWS_RUN_KEY, autostart_dir,
    autostart_supported, desktop_entry, desktop_entry_path, remove_desktop_entry,
    windows_disable_args, windows_enable_args, write_desktop_entry,
};
pub use board_camera::{BoardCam, BoardView};
pub use board_edit::{BoardEdit, Gesture, NUDGE, NUDGE_BIG};
pub use board_note_link::BoardNoteLink;
pub use board_rename::{BoardPlace, BoardRename};
pub use board_scene::{BoardScene, SceneText};
pub use board_text::{BoardTextTarget, measure_text};
pub use board_tools::{BoardTool, Draft, tool_for_key};
pub use board_view::{
    BoardConflict, BoardCounts, BoardSession, BoardStatus, BoardSummary, OpenBoard,
};
pub use contrast::{TEXT, UI, at_least, board_stroke, meets, over, ratio as contrast_ratio};
pub use doc_sync::DocConflict;
pub use global_shortcut::{SHOW_SHORTCUT, ShortcutPhase, ShortcutPort, show_shortcut_label};
pub use import_document::DOCUMENT_EXTENSIONS;
pub use kanban_dates::{KanbanPicker, PickerInsert, PickerKind};
pub use kanban_edit::{CardSlot, KanbanHistory};
pub use kanban_lanes::{CardStep, LaneField, LaneSlot};
pub use kanban_search::{
    KanbanDateFilter, KanbanDatePreset, KanbanDateRange, KanbanFilter, KanbanFindState, KanbanMatch,
};
pub use kanban_view::ShownKanban;
pub use keys::{on_key, shortcut};
pub use lock_session::UnlockError;
pub use mode::{Mode, Scope};
pub use omarchy::resolve_kit;
pub use overlay::{Layer, PaletteAction, PaletteItem, PaletteState, SettingsTab};
pub use poll::poll;
pub use recovery::{RecoveryEntry, RecoverySlot};
pub use settings::{
    DEFAULT_AUTOSAVE_DELAY, GraphEffectIntensity, GraphEffects, LanguageChoice, RECENT_VAULTS_MAX,
    ReduceMotion, Settings, ThemeMode, config_home,
};
pub use state::{
    BurnMark, BurnSpace, ConflictReason, ConflictState, ContextMenu, DeleteConfirm, DiskState,
    FileAction, FindState, FolderPrompt, FolderRename, FolderRenameConfirm, HistoryEntry,
    IMPORT_EXTENSIONS, ImportReport, LinkComplete, LockPrompt, LockPromptKind, MenuTarget,
    MoveConfirm, NamePrompt, RenameConfirm, RenamePrompt, State, TagComplete, TrashUndo,
    file_action_command,
};
pub use tray::{
    TRAY_NEW_NOTE, TRAY_QUIT, TRAY_SHOW, TRAY_SYNC_NOW, TRAY_SYNC_STATUS, TrayAction, TrayFactory,
    TrayPort, tray_action, tray_icon, tray_menu, tray_spec, tray_status_label, tray_supported,
};
pub use update::{AvailableUpdate, CheckOutcome, InstallMethod, InstallProbe, ReleasesClient};
pub use view::view;

/// Effect-slot marks the graph view mints and the `muninn-app` compositor paints.
///
/// Each is the high 16 bits of an `El::effect` mark; the low half carries the
/// node id or the slot's rank as the effect's seed.
pub mod fx_marks {
    pub use crate::graph_view::{FX_BURN, FX_EDGE_COLD, FX_EDGE_HOT, FX_NODE_NEAR, FX_NODE_SEL};
}

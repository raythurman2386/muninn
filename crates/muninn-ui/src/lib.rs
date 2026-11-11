//! Muninn's state and Rune views: sidebar, editor, Markdown reader, graph canvas and boards.
//!
//! The binary in `muninn-app` only wires these into `rune::run`; everything that can be
//! tested headlessly lives here.

mod autostart;
mod board_camera;
mod board_scene;
mod board_view;
mod camera;
mod global_shortcut;
mod graph_view;
mod keys;
mod mode;
mod omarchy;
mod overlay;
mod poll;
mod recovery;
mod settings;
mod state;
mod sync_ui;
mod tray;
mod view;
mod watch;

pub use autostart::{
    AUTOSTART_NAME, AutostartPort, NativeAutostart, WINDOWS_RUN_KEY, autostart_dir,
    autostart_supported, desktop_entry, desktop_entry_path, remove_desktop_entry,
    windows_disable_args, windows_enable_args, write_desktop_entry,
};
pub use board_camera::{BoardCam, BoardView};
pub use board_scene::{BoardScene, SceneText};
pub use board_view::{BoardCounts, BoardSession, BoardSummary, OpenBoard};
pub use global_shortcut::{SHOW_SHORTCUT, ShortcutPhase, ShortcutPort, show_shortcut_label};
pub use keys::{on_key, shortcut};
pub use mode::{Mode, Scope};
pub use overlay::{Layer, PaletteAction, PaletteItem, PaletteState, SettingsTab};
pub use poll::poll;
pub use recovery::RecoveryEntry;
pub use settings::{
    DEFAULT_AUTOSAVE_DELAY, RECENT_VAULTS_MAX, ReduceMotion, Settings, ThemeMode, config_home,
};
pub use state::{
    ConflictReason, ConflictState, ContextMenu, DeleteConfirm, DiskState, FileAction, FindState,
    FolderPrompt, FolderRename, FolderRenameConfirm, HistoryEntry, ImportReport, LinkComplete,
    MenuTarget, MoveConfirm, NamePrompt, RenameConfirm, RenamePrompt, State, TrashUndo,
    file_action_command,
};
pub use tray::{
    TRAY_NEW_NOTE, TRAY_QUIT, TRAY_SHOW, TRAY_SYNC_NOW, TRAY_SYNC_STATUS, TrayAction, TrayFactory,
    TrayPort, tray_action, tray_icon, tray_menu, tray_spec, tray_status_label, tray_supported,
};
pub use view::view;

//! Muninn's state and Rune views: sidebar, editor, Markdown reader and graph canvas.
//!
//! The binary in `muninn-app` only wires these into `rune::run`; everything that can be
//! tested headlessly lives here.

mod camera;
mod graph_view;
mod keys;
mod mode;
mod omarchy;
mod overlay;
mod poll;
mod recovery;
mod settings;
mod state;
mod view;
mod watch;

pub use keys::{on_key, shortcut};
pub use mode::{Mode, Scope};
pub use overlay::{Layer, PaletteAction, PaletteItem, PaletteState, SettingsTab};
pub use poll::poll;
pub use recovery::RecoveryEntry;
pub use settings::{DEFAULT_AUTOSAVE_DELAY, RECENT_VAULTS_MAX, Settings, ThemeMode, config_home};
pub use state::{
    ConflictReason, ConflictState, ContextMenu, DeleteConfirm, DiskState, FileAction, FindState,
    FolderPrompt, FolderRename, FolderRenameConfirm, HistoryEntry, ImportReport, LinkComplete,
    MenuTarget, MoveConfirm, NamePrompt, RenameConfirm, RenamePrompt, State, TrashUndo,
    file_action_command,
};
pub use view::view;

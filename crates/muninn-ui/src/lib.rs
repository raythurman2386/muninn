//! Muninn's state and Rune views: sidebar, editor, Markdown reader and graph canvas.
//!
//! The binary in `muninn-app` only wires these into `rune::run`; everything that can be
//! tested headlessly lives here.

mod camera;
mod graph_view;
mod keys;
mod omarchy;
mod poll;
mod recovery;
mod settings;
mod state;
mod view;
mod watch;

pub use keys::{on_key, shortcut};
pub use poll::poll;
pub use recovery::RecoveryEntry;
pub use settings::{
    DEFAULT_AUTOSAVE_DELAY, GraphEffects, RECENT_VAULTS_MAX, Settings, ThemeMode, config_home,
};
pub use state::{
    ConflictReason, ConflictState, ContextMenu, DeleteConfirm, DiskState, FileAction, FindState,
    FolderPrompt, FolderRename, FolderRenameConfirm, HistoryEntry, ImportReport, LinkComplete,
    MenuTarget, Mode, MoveConfirm, NamePrompt, PaletteAction, PaletteItem, PaletteState,
    RenameConfirm, RenamePrompt, Scope, State, TrashUndo, file_action_command,
};
pub use view::view;

/// Effect-slot mark prefixes used by the graph FX compositor.
pub mod fx_marks {
    pub use crate::graph_view::{FX_EDGE_COLD, FX_EDGE_HOT, FX_GLOW};
}

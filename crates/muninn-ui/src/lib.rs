//! Muninn's state and Rune views: sidebar, editor, Markdown reader and graph canvas.
//!
//! The binary in `muninn-app` only wires these into `rune::run`; everything that can be
//! tested headlessly lives here.

mod camera;
mod graph_view;
mod keys;
mod poll;
mod recovery;
mod state;
mod view;
mod watch;

pub use keys::{on_key, shortcut};
pub use poll::poll;
pub use recovery::RecoveryEntry;
pub use state::{
    ConflictReason, ConflictState, ContextMenu, DeleteConfirm, DiskState, FindState, FolderPrompt,
    FolderRename, FolderRenameConfirm, HistoryEntry, LinkComplete, MenuTarget, Mode, MoveConfirm,
    NamePrompt, RenameConfirm, RenamePrompt, Scope, State, TrashUndo,
};
pub use view::view;

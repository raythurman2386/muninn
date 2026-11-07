//! Muninn's state and Rune views: sidebar, editor, Markdown reader and graph canvas.
//!
//! The binary in `muninn-app` only wires these into `rune::run`; everything that can be
//! tested headlessly lives here.

mod camera;
mod graph_view;
mod keys;
mod poll;
mod state;
mod view;

pub use keys::{on_key, shortcut};
pub use poll::poll;
pub use state::{ContextMenu, DeleteConfirm, FolderPrompt, FolderRename, MenuTarget, Mode, NamePrompt, RenamePrompt, Scope, State};
pub use view::view;

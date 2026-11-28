//! The browser host seam for vault export and import (#240 slice 6e).
//!
//! The palette rows and the import guard are shared UI; the file picker, the
//! download and the archive bytes are the browser's. A host registers two
//! one-shot callbacks at boot (`muninn-web` does, before `rune::run`), and the
//! palette offers the two rows only while a host has them. The native build
//! registers nothing: its palette is unchanged and it keeps the whole
//! filesystem it already has, so export/import stays a web-only surface.
//!
//! The import posture is the ordinary save guard. [`State::begin_vault_import`]
//! refuses while the open note, board or kanban still holds unsaved edits or an
//! unresolved conflict — an import never writes over work the vault has not
//! persisted — and [`State::finish_vault_import`] re-indexes through the same
//! `install_store` path the browser boot uses (slice 6a/6b).

use std::sync::Arc;
use std::sync::OnceLock;

use muninn_core::VaultStore;
use muninn_i18n::{Key as TextKey, t};

use crate::State;

/// The browser callbacks the palette's two vault actions need.
#[derive(Clone, Copy, Debug)]
pub struct VaultTransferHooks {
    /// Start the archive download. Export only reads, so a read-only tab may
    /// use it.
    pub export: fn(),
    /// Open the file picker. The picked bytes reach the shared UI through the
    /// host's next poll, which calls [`State::begin_vault_import`].
    pub import: fn(),
}

static HOOKS: OnceLock<VaultTransferHooks> = OnceLock::new();

/// Register this session's browser callbacks. The first registration wins; a
/// host boots once.
pub fn install(hooks: VaultTransferHooks) {
    let _ = HOOKS.set(hooks);
}

/// Whether a browser host can export and import in this session.
#[must_use]
pub fn available() -> bool {
    HOOKS.get().is_some()
}

fn request_export() -> bool {
    HOOKS.get().map(|hooks| (hooks.export)()).is_some()
}

fn request_import() -> bool {
    HOOKS.get().map(|hooks| (hooks.import)()).is_some()
}

/// Why an import could not be applied.
///
/// The reason is deliberately coarse: the host logs the store's own error, and
/// the status bar says which user action is needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultImportError {
    /// The open note, board or kanban still has unsaved edits, or a conflict is
    /// waiting for a resolution. Nothing was written.
    Dirty,
    /// The archive's files were written but the vault could not be re-indexed.
    Reload,
}

impl VaultImportError {
    /// The catalogued status line for this refusal.
    #[must_use]
    pub fn message(self) -> &'static str {
        t(match self {
            VaultImportError::Dirty => TextKey::WebImportRefusedDirty,
            VaultImportError::Reload => TextKey::WebImportFailed,
        })
    }
}

impl State {
    /// Start a browser vault export through the registered host.
    pub fn export_vault(&mut self) -> bool {
        if request_export() {
            return true;
        }
        self.error = Some(t(TextKey::WebExportUnavailable).to_owned());
        false
    }

    /// Ask the registered host to open its file picker for an import.
    pub fn import_vault(&mut self) -> bool {
        if request_import() {
            return true;
        }
        self.error = Some(t(TextKey::WebImportUnavailable).to_owned());
        false
    }

    /// Refuse while the vault is dirty and hand back the store an import may
    /// write through.
    ///
    /// The check is the save guard's own state: [`State::dirty`], the open
    /// board's unsaved status, [`State::kanban_dirty`] and an unresolved
    /// [`State::conflict`]. An import does not autosave implicitly: the notice
    /// tells the user to save or discard first, so the archived bytes never
    /// land on top of edits the vault has not persisted. Once the vault is
    /// clean, autosave or `⌘S` has already written the buffer through the same
    /// guarded store path.
    pub fn begin_vault_import(&mut self) -> Result<Arc<dyn VaultStore>, VaultImportError> {
        if self.conflict.is_some() || self.dirty() || self.board_dirty() || self.kanban_dirty() {
            return Err(VaultImportError::Dirty);
        }
        Ok(self.vault.store())
    }

    /// Re-index the vault after the host wrote an archive's files through
    /// `store`. This is the same install the browser boot uses, so the sidebar,
    /// graph, boards and the first note all match a fresh scan.
    pub fn finish_vault_import(
        &mut self,
        store: Arc<dyn VaultStore>,
    ) -> Result<(), VaultImportError> {
        self.install_store(store)
            .map_err(|_| VaultImportError::Reload)
    }
}

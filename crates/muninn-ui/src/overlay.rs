//! Overlay state machine: the command palette, Settings modal and Help page,
//! plus the one dismissal stack every overlay shares.
//!
//! `Layer` names the order Escape dismisses them in; `State::topmost_layer`
//! reports the topmost open layer and `State::dismiss_layer` closes it. The
//! fields behind each variant stay on [`crate::State`](crate::State) so its
//! invariants (focus, persistence) do not scatter across files; only the
//! layer's *types* and the *push*/peel behaviour live here.

use crate::State;
use crate::settings::{AUTOSAVE_MAX_DELAY, AUTOSAVE_MIN_DELAY};

/// One dismissible layer of the UI, in Escape-dismissal order. See
/// [`State::topmost_layer`] / [`State::dismiss_layer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Palette,
    Settings,
    Help,
    Complete,
    Find,
    BrokenPanel,
    RenameConfirm,
    MoveConfirm,
    FolderRenameConfirm,
    Menu,
    NewNote,
    RenameNote,
    NewFolder,
    RenameFolder,
    DeleteConfirm,
    MovePicker,
}

/// The command palette's own state. The input text lives on
/// [`State::palette_field`] so the text field's `on_edit` callback can always
/// hand Rune a valid field, even in the frame after Escape unmounts the card.
/// `selected` indexes [`State::palette_items`]; `query` is the snapshot the
/// highlight belongs to, so typing resets it to the first row.
#[derive(Clone, Debug)]
pub struct PaletteState {
    /// Highlighted row, an index into the current item list.
    pub selected: usize,
    /// Query the highlight belongs to; a change resets it to row zero.
    pub(crate) query: String,
    /// Ask `poll` to scroll the highlighted row into view next frame.
    pub reveal: bool,
}

/// v0.5: one action the command palette can run.
///
/// Actions are a fixed list: the palette searches them with the same input as
/// notes, but they never move with the vault. Every action is a thin call into
/// an existing state method so the palette stays a view over the app, not a
/// second command layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteAction {
    OpenLocalGraph,
    SwitchVault,
    /// Opens the settings modal (v0.5).
    OpenSettings,
    NewNote,
    TogglePreview,
    ToggleGraph,
}

impl PaletteAction {
    /// Every action, in the order shown before a query is typed.
    pub const ALL: [PaletteAction; 6] = [
        PaletteAction::OpenLocalGraph,
        PaletteAction::SwitchVault,
        PaletteAction::OpenSettings,
        PaletteAction::NewNote,
        PaletteAction::TogglePreview,
        PaletteAction::ToggleGraph,
    ];

    /// The row's main label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            PaletteAction::OpenLocalGraph => "Open local graph",
            PaletteAction::SwitchVault => "Switch vault",
            PaletteAction::OpenSettings => "Open settings",
            PaletteAction::NewNote => "New note",
            PaletteAction::TogglePreview => "Toggle Write / Read",
            PaletteAction::ToggleGraph => "Toggle Graph",
        }
    }

    /// The shortcut or affordance the action mirrors, drawn on the right.
    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            PaletteAction::OpenLocalGraph => "Graph · Local",
            PaletteAction::SwitchVault => "Vaults",
            PaletteAction::OpenSettings => "Help",
            PaletteAction::NewNote => "⌘N",
            PaletteAction::TogglePreview => "⌘E",
            PaletteAction::ToggleGraph => "⌘G",
        }
    }

    /// Extra words that should find the action even when they are not in its
    /// label (`prefs`, `create`, `editor`, …).
    fn keywords(self) -> &'static str {
        match self {
            PaletteAction::OpenLocalGraph => "graph local focus neighbours scope",
            PaletteAction::SwitchVault => "vault open recent folder switch",
            PaletteAction::OpenSettings => "settings preferences options config help",
            PaletteAction::NewNote => "new note create add",
            PaletteAction::TogglePreview => "write read preview editor mode",
            PaletteAction::ToggleGraph => "graph global local mode",
        }
    }

    pub(crate) fn matches(self, query: &str) -> bool {
        self.label().to_lowercase().contains(query)
            || self.hint().to_lowercase().contains(query)
            || self.keywords().contains(query)
    }
}

/// One row in the command palette. Notes keep their vault id; the view looks
/// up the title, folder and link counts from the live vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteItem {
    /// A note already opened this session, shown while the input is empty.
    Recent(usize),
    /// A note matching the typed query (title-first, via `Vault::search`).
    Note(usize),
    Action(PaletteAction),
}

impl PaletteItem {
    /// The group header this item belongs under.
    #[must_use]
    pub fn section(&self) -> &'static str {
        match self {
            PaletteItem::Recent(_) => "Recent",
            PaletteItem::Note(_) => "Notes",
            PaletteItem::Action(_) => "Actions",
        }
    }
}

impl State {
    /// Whether a text field owns the keyboard right now.
    ///
    /// F2/Delete (and bare `?`) are suppressed while this is true so Delete
    /// edits text instead of prompting, and a literal question mark stays one
    /// inside the editor. One name for what `keys.rs` used to enumerate
    /// inline in a thirteen-condition chain.
    #[must_use]
    pub fn is_typing(&self) -> bool {
        self.editor.focused
            || self.search.focused
            || self.palette.is_some()
            || self.find.is_some()
            || self.complete.is_some()
            || self.broken_panel
            || self.rename_confirm.is_some()
            || self.move_confirm.is_some()
            || self.folder_rename_confirm.is_some()
            || self.new_note_prompt.is_some()
            || self.rename_prompt.is_some()
            || self.new_folder_prompt.is_some()
            || self.rename_folder_prompt.is_some()
    }

    /// DES-01: help overlay.
    pub fn open_help(&mut self) {
        self.help = true;
        self.dismiss_palette();
        self.menu = None;
        self.vault_field.focused = false;
        // The overlay owns the keyboard while it is up; typing must not edit the
        // hidden note underneath it.
        self.editor.focused = false;
        self.search.focused = false;
    }

    pub fn close_help(&mut self) {
        self.help = false;
    }

    pub fn toggle_help(&mut self) {
        if self.help {
            self.close_help();
        } else {
            self.open_help();
        }
    }

    /// v0.5: open the in-window settings modal.
    ///
    /// The modal is a Rune dialog on the overlay layer, so it sits above the
    /// sidebar and the help page. It records which text field was typing
    /// before taking the keyboard and gives it back on [`Self::close_settings`].
    pub fn open_settings(&mut self) {
        if self.settings_modal {
            return;
        }
        self.settings_return_editor = self.editor.focused;
        self.settings_return_search = self.search.focused;
        self.settings_modal = true;
        // Settings is the top layer; a help page or sidebar menu cannot stay
        // live underneath it.
        self.help = false;
        self.menu = None;
        self.vault_field.focused = false;
        self.set_text_focus(false, false);
    }

    /// Close the settings modal and return typing to the field that opened it.
    /// The vault-path field never keeps focus while hidden.
    pub fn close_settings(&mut self) {
        if !self.settings_modal {
            return;
        }
        self.settings_modal = false;
        self.vault_field.focused = false;
        if self.settings_return_search {
            self.focus_search();
        } else if self.settings_return_editor {
            self.focus_editor();
        }
        self.settings_return_editor = false;
        self.settings_return_search = false;
    }

    pub fn toggle_settings(&mut self) {
        if self.settings_modal {
            self.close_settings();
        } else {
            self.open_settings();
        }
    }

    /// Persist a new autosave idle window, clamped to the documented range.
    /// The segmented theme choices and the stepper both write immediately.
    pub fn set_autosave_delay(&mut self, seconds: f64) {
        if !seconds.is_finite() {
            return;
        }
        self.settings.autosave_delay = seconds.clamp(AUTOSAVE_MIN_DELAY, AUTOSAVE_MAX_DELAY);
        self.save_settings();
    }

    /// Nudge the autosave delay by `delta` seconds, rounded to the stepper's
    /// 0.05 s grid so repeated clicks do not accumulate float noise.
    pub fn nudge_autosave_delay(&mut self, delta: f64) {
        let next = (self.autosave_delay() + delta) * 20.0;
        self.set_autosave_delay(next.round() / 20.0);
    }

    /// Cmd/Ctrl+Shift+P: open the palette, or close it when it is already up.
    pub fn toggle_palette(&mut self) {
        if self.palette.is_some() {
            self.close_palette();
        } else {
            self.open_palette();
        }
    }

    /// Show the palette over whatever is open. The field owns typing until the
    /// palette closes; the focus that was active is remembered for Escape.
    pub fn open_palette(&mut self) {
        self.palette_focus = (self.editor.focused, self.search.focused);
        // The palette is a modal layer: nothing else may keep the keyboard or
        // sit above it.
        self.help = false;
        self.menu = None;
        self.vault_field.focused = false;
        self.complete = None;
        self.complete_dismissed = None;
        if let Some(find) = self.find.as_mut() {
            find.field.focused = false;
        }
        self.palette_field.replace_text("");
        self.palette_field.focused = true;
        self.set_text_focus(false, false);
        self.palette = Some(PaletteState {
            selected: 0,
            query: String::new(),
            reveal: false,
        });
    }

    /// Escape: drop the palette and hand the keyboard back to the field that
    /// had it (the editor or search box). Opening a prompt first clears both
    /// flags, so a prompt that replaces the palette keeps its own focus.
    pub fn close_palette(&mut self) {
        if self.palette.take().is_none() {
            return;
        }
        self.palette_field.focused = false;
        let (editor, search) = self.palette_focus;
        self.set_text_focus(editor, search);
    }

    /// Drop the palette without restoring focus, for a layer that is taking the
    /// keyboard instead (Help, an inline prompt, or a focus command).
    pub(crate) fn dismiss_palette(&mut self) {
        self.palette = None;
        self.palette_field.focused = false;
    }

    /// The topmost open [`Layer`], or `None` when nothing is dismissed by
    /// Escape. This is the single ordering `keys.rs` follows; it mirrors the
    /// stacking the view draws (modals above pages, pages above inline
    /// prompts) so Escape always peels exactly one layer.
    #[must_use]
    pub fn topmost_layer(&self) -> Option<Layer> {
        if self.palette.is_some() {
            return Some(Layer::Palette);
        }
        if self.settings_modal {
            return Some(Layer::Settings);
        }
        if self.help {
            return Some(Layer::Help);
        }
        if self.complete.is_some() {
            return Some(Layer::Complete);
        }
        if self.find.is_some() {
            return Some(Layer::Find);
        }
        if self.broken_panel {
            return Some(Layer::BrokenPanel);
        }
        if self.rename_confirm.is_some() {
            return Some(Layer::RenameConfirm);
        }
        if self.move_confirm.is_some() {
            return Some(Layer::MoveConfirm);
        }
        if self.folder_rename_confirm.is_some() {
            return Some(Layer::FolderRenameConfirm);
        }
        if self.menu.is_some() {
            return Some(Layer::Menu);
        }
        if self.new_note_prompt.is_some() {
            return Some(Layer::NewNote);
        }
        if self.rename_prompt.is_some() {
            return Some(Layer::RenameNote);
        }
        if self.new_folder_prompt.is_some() {
            return Some(Layer::NewFolder);
        }
        if self.rename_folder_prompt.is_some() {
            return Some(Layer::RenameFolder);
        }
        if self.delete_confirm.is_some() {
            return Some(Layer::DeleteConfirm);
        }
        if self.move_picker.is_some() {
            return Some(Layer::MovePicker);
        }
        None
    }

    /// Dismiss the given layer with the same semantics as pressing Escape on
    /// it (focus returns to the field that opened it where that was the
    /// behaviour). `Layer` is exhaustive, so a new dismissible layer forces a
    /// match arm here and a `topmost_layer` branch.
    pub fn dismiss_layer(&mut self, layer: Layer) {
        match layer {
            Layer::Palette => self.close_palette(),
            Layer::Settings => self.close_settings(),
            Layer::Help => self.close_help(),
            Layer::Complete => self.dismiss_complete(),
            Layer::Find => self.close_find(),
            Layer::BrokenPanel => self.close_broken_links(),
            Layer::RenameConfirm => self.cancel_rename_confirm(),
            Layer::MoveConfirm => self.cancel_move_confirm(),
            Layer::FolderRenameConfirm => self.cancel_folder_rename_confirm(),
            Layer::Menu => self.close_menu(),
            Layer::NewNote => self.cancel_new_note(),
            Layer::RenameNote => self.cancel_rename_note(),
            Layer::NewFolder => self.cancel_new_folder(),
            Layer::RenameFolder => self.cancel_rename_folder(),
            Layer::DeleteConfirm => self.cancel_delete(),
            Layer::MovePicker => self.cancel_move(),
        }
    }
}

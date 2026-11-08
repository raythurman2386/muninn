//! Keyboard shortcuts shared by the shell and the text fields.

use rune::{Key, KeyEvent, Tx};

use crate::{Mode, State};

pub fn on_key(s: &mut State, e: &KeyEvent, tx: &mut Tx) -> bool {
    shortcut(s, e, tx)
}

pub fn shortcut(s: &mut State, e: &KeyEvent, _tx: &mut Tx) -> bool {
    // Escape dismisses inline prompts/menus before falling through to the
    // graph escape below. Text fields handle Escape first and never reach here.
    if e.key == Key::Escape {
        // v0.5: the settings modal is the topmost layer.
        if s.settings_modal {
            s.close_settings();
            return true;
        }
        // DES-01: the help overlay and the vault menu are the outermost layers.
        if s.help {
            s.close_help();
            return true;
        }
        if s.vault_prompt {
            s.cancel_vault_prompt();
            return true;
        }
        if s.vault_menu {
            s.close_vault_menu();
            return true;
        }
        // M2 EDT-03: autocomplete dismisses before find, prompts and menus.
        if s.complete.is_some() {
            s.dismiss_complete();
            return true;
        }
        // M2 EDT-01: the find bar closes first and returns typing to the
        // editor; the find field itself handles its own Escape before this.
        if s.find.is_some() {
            s.close_find();
            return true;
        }
        if s.broken_panel {
            s.close_broken_links();
            return true;
        }
        if s.rename_confirm.is_some() {
            s.cancel_rename_confirm();
            return true;
        }
        if s.move_confirm.is_some() {
            s.cancel_move_confirm();
            return true;
        }
        if s.folder_rename_confirm.is_some() {
            s.cancel_folder_rename_confirm();
            return true;
        }
        if s.menu.is_some() {
            s.close_menu();
            return true;
        }
        if s.new_note_prompt.is_some() {
            s.cancel_new_note();
            return true;
        }
        if s.rename_prompt.is_some() {
            s.cancel_rename_note();
            return true;
        }
        if s.new_folder_prompt.is_some() {
            s.cancel_new_folder();
            return true;
        }
        if s.rename_folder_prompt.is_some() {
            s.cancel_rename_folder();
            return true;
        }
        if s.delete_confirm.is_some() {
            s.cancel_delete();
            return true;
        }
        if s.move_picker.is_some() {
            s.cancel_move();
            return true;
        }
    }
    if e.key == Key::Escape && s.mode == Mode::Graph {
        s.toggle_graph();
        return true;
    }
    // The settings modal owns the keyboard while it is mounted: only the
    // toggle chord is read here, everything else belongs to its controls.
    if s.settings_modal {
        if e.is_command(',') {
            s.close_settings();
        }
        return true;
    }
    // M3 GRP-02: arrows, Enter and the zoom/fit/isolate keys are graph-only;
    // State owns them so the canvas can be tested without a window.
    if s.mode == Mode::Graph && s.graph_key(e) {
        return true;
    }
    // F2 renames the selected note, Delete asks to delete it. When a text
    // field is focused, Delete edits text instead, so skip both then.
    let typing = s.editor.focused
        || s.search.focused
        || s.find.is_some()
        || s.complete.is_some()
        || s.broken_panel
        || s.rename_confirm.is_some()
        || s.move_confirm.is_some()
        || s.folder_rename_confirm.is_some()
        || s.new_note_prompt.is_some()
        || s.rename_prompt.is_some()
        || s.new_folder_prompt.is_some()
        || s.rename_folder_prompt.is_some();
    // DES-01: Cmd/Ctrl+? opens Help from anywhere, including the editor (it is
    // a command chord, not typed text). Plain `?` also works when no text field
    // owns the keyboard; inside the editor `?` stays a literal question mark.
    if e.is_command('?') {
        s.toggle_help();
        return true;
    }
    if !typing {
        if e.key == Key::Text("?".to_owned()) {
            s.toggle_help();
            return true;
        }
        if e.key == Key::Function(2)
            && let Some(id) = s.selected
        {
            s.begin_rename_note(id);
            return true;
        }
        if e.key == Key::Delete
            && let Some(id) = s.selected
        {
            s.delete_note(id);
            return true;
        }
    }
    if e.is_command('k') {
        s.focus_search();
    } else if e.is_command('p') {
        // M2 EDT-03: the insert-link picker (NOT feed find or a command
        // palette). Cmd/Ctrl+K stays search focus.
        s.open_link_picker();
    } else if e.is_command('f') {
        // M2 EDT-01: find inside the open note. Cmd/Ctrl+F is the
        // platform-standard chord and leaves Vim-style Ctrl+[ free.
        s.open_find();
    } else if e.is_command('[') {
        // M2 NAV-03: browser-standard Back. Chosen over Alt+Left because
        // Option+Left is word-wise caret movement inside text fields on macOS.
        s.back();
    } else if e.is_command(']') {
        s.forward();
    } else if e.is_command('n') {
        s.new_note();
    } else if e.is_command('s') {
        s.save_now();
    } else if e.is_command('e') {
        s.toggle_preview();
    } else if e.is_command('g') {
        s.toggle_graph();
    } else if e.is_command('l') {
        s.toggle_scope();
    } else if e.is_command(',') {
        // v0.5: platform-standard preferences chord (VSCode style).
        s.open_settings();
    } else if e.is_command('1') {
        s.set_mode(Mode::Editor);
    } else if e.is_command('2') {
        s.set_mode(Mode::Preview);
    } else if e.is_command('3') {
        s.set_mode(Mode::Graph);
    } else {
        return false;
    }
    true
}

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
    // F2 renames the selected note, Delete asks to delete it. When a text
    // field is focused, Delete edits text instead, so skip both then.
    let typing = s.editor.focused
        || s.search.focused
        || s.new_note_prompt.is_some()
        || s.rename_prompt.is_some()
        || s.new_folder_prompt.is_some()
        || s.rename_folder_prompt.is_some();
    if !typing {
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

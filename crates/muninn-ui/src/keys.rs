//! Keyboard shortcuts shared by the shell and the text fields.

use rune::{Key, KeyEvent, Tx};

use crate::{Mode, State};

pub fn on_key(s: &mut State, e: &KeyEvent, tx: &mut Tx) -> bool {
    shortcut(s, e, tx)
}

pub fn shortcut(s: &mut State, e: &KeyEvent, _tx: &mut Tx) -> bool {
    if e.key == Key::Escape && s.mode == Mode::Graph {
        s.toggle_graph();
        return true;
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

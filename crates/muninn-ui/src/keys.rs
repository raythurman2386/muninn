//! Keyboard shortcuts shared by the shell and the text fields.

use rune::{Key, KeyEvent, Tx};

use crate::{Mode, State};

pub fn on_key(s: &mut State, e: &KeyEvent, tx: &mut Tx) -> bool {
    // #40 recheck B1: a Board key's edit reaches the recovery copy without
    // waiting for more input.
    if s.mode == crate::Mode::Board {
        tx.wake_after(crate::board_safety::BOARD_EDIT_WAKE);
    }
    shortcut(s, e, tx)
}

pub fn shortcut(s: &mut State, e: &KeyEvent, tx: &mut Tx) -> bool {
    // Escape dismisses the topmost layer (see `Layer` / `State::topmost_layer`)
    // before falling through to the graph escape below. Text fields handle
    // Escape first and never reach here.
    if e.key == Key::Escape
        && let Some(layer) = s.topmost_layer()
    {
        s.dismiss_layer(layer);
        return true;
    }
    if e.key == Key::Escape && s.mode == Mode::Graph {
        s.toggle_graph();
        return true;
    }
    if e.key == Key::Escape && s.mode == Mode::Kanban {
        // A card editor or lane field that lost its focus is still open: Esc
        // cancels it and the page stays. Esc only leaves with none open,
        // never committing the field's text by leaving.
        if s.cancel_kanban_field() {
            return true;
        }
        s.toggle_kanban();
        return true;
    }
    // Esc on Board drops the drawing tool; with none picked it leaves Board
    // the way it leaves the graph.
    if e.key == Key::Escape && s.mode == Mode::Board {
        if !s.board_escape() {
            s.toggle_board();
        }
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
    // State owns them so the canvas can be tested without a window. The
    // palette takes precedence while it is open (its input owns arrows).
    // A focused text field (the sidebar search, a prompt) owns typing, so
    // neither canvas reads letters, digits or arrows meant for it.
    if !s.is_typing() && s.mode == Mode::Graph && s.graph_key(e) {
        return true;
    }
    // Board slice 5: undo, redo and select all on the open board, before
    // the note chords (⌘Z is the editor's otherwise, ⌘A its select-all).
    if !s.is_typing() && crate::board_edit::board_command(s, e) {
        return true;
    }
    // Kanban slice 4: undo and redo on the Kanban page, the same way.
    if !s.is_typing() && crate::kanban_edit::kanban_command(s, e) {
        return true;
    }
    // Kanban slice 5: arrows pick a card, Alt+arrows move it.
    if !s.is_typing() && crate::kanban_lanes::kanban_key(s, e) {
        return true;
    }
    // Board slice 3: zoom, fit and arrow-pan keys on the open board; slice 4
    // adds the tool letters, slice 5 Delete, arrows nudging and Esc.
    if !s.is_typing() && s.mode == Mode::Board && s.board_key(e) {
        return true;
    }
    // v0.5: Cmd/Ctrl+Shift+P opens the command palette from anywhere,
    // including the editor. Cmd/Ctrl+P stays the insert-link picker below.
    if e.is_command('p') && e.mods.shift {
        s.toggle_palette();
        return true;
    }
    // The palette owns typing, but a different command chord means the user is
    // done searching: close it before that command runs.
    if s.palette.is_some() && e.mods.cmd {
        s.close_palette();
    }
    // F2 renames the selected note, Delete asks to delete it. When a text
    // field is focused, Delete edits text instead, so skip both then.
    let typing = s.is_typing();
    // DES-01: Cmd/Ctrl+? opens Help from anywhere, including the editor (it is
    // a command chord, not typed text). Plain `?` also works when no text field
    // owns the keyboard; inside the editor `?` stays a literal question mark.
    if e.is_command('?') {
        s.toggle_help();
        return true;
    }
    if !typing {
        // Slice 8: Cmd/Ctrl+\\ shows or hides the notes sidebar (Board keeps
        // its own state). Not from a text field: there it is the field's.
        if e.is_command('\\') {
            s.toggle_sidebar();
            // The canvas and its chrome move at once, not with a glide.
            tx.instant();
            return true;
        }
        if e.key == Key::Text("?".to_owned()) {
            s.toggle_help();
            return true;
        }
        // #43: on Board, F2 renames the open board (in its panel row).
        if e.key == Key::Function(2)
            && s.mode == Mode::Board
            && s.begin_rename_open_board(crate::BoardPlace::Panel)
        {
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
        // Board slice 4: on Board the chord also saves the open board;
        // Kanban slice 4: on the Kanban page, the kanban.
        if s.mode == Mode::Board {
            s.save_board();
        } else if s.mode == Mode::Kanban {
            s.save_kanban();
        }
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
    } else if e.is_command('4') {
        s.set_mode(Mode::Board);
    } else if e.is_command('5') {
        s.set_mode(Mode::Kanban);
    } else {
        return false;
    }
    true
}

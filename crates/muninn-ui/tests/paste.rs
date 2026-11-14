//! #111: pasting into the Write editor through Rune's real clipboard seam.
//!
//! These drive the focused `text_input` and the in-memory clipboard, so the
//! `on_input_paste` hook, the editor model and the autosave path are the ones
//! the window uses.

use std::fs;

use muninn_core::testing::TempVault;
use muninn_ui::{State, on_key, poll, view};
use rune::{ClipboardContent, Env, Key, KeyEvent, Mods, Runtime, clipboard};

fn temp_vault() -> TempVault {
    TempVault::new(&[("Alpha.md", "# Alpha\n\nSee [[Beta]].\n")])
}

fn runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = Runtime::new(
        State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state")),
        view,
        (1180.0, 780.0),
        Env::new(true, 2.0),
    );
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn command(c: char) -> KeyEvent {
    KeyEvent {
        key: Key::Command(c),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    }
}

fn paste(rt: &mut Runtime<State>) {
    rt.update(|_, tx| tx.request_paste());
    rt.frame(0.0);
}

#[test]
fn rich_clipboard_text_pastes_as_markdown_and_autosaves() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let path = rt.state.vault.notes[rt.state.selected.expect("open note")]
        .path
        .clone();
    let board = clipboard::memory();
    rt.use_clipboard(&board);

    rt.state.editor.replace_text("Intro\n\n");
    rt.state.editor.set_caret(rt.state.editor.text.len());
    rt.state.focus_editor();
    rt.frame(0.0);

    board.set_content(ClipboardContent::new("Plan\nShip it.\nStep one").with_html(
        "<h2>Plan</h2><p>Ship it with <strong>care</strong>.</p><ul><li>Step one</li></ul>",
    ));
    paste(&mut rt);

    assert_eq!(
        rt.state.editor.text,
        "Intro\n\n## Plan\n\nShip it with **care**.\n\n- Step one"
    );

    // One undo step, through the field's own ⌘Z.
    rt.key(command('z'));
    rt.frame(0.0);
    assert_eq!(rt.state.editor.text, "Intro\n\n");

    // Redo, then let autosave write it like any other edit.
    rt.key(KeyEvent {
        key: Key::Command('z'),
        mods: Mods {
            cmd: true,
            shift: true,
            ..Mods::default()
        },
    });
    rt.frame(0.0);
    assert!(rt.state.editor.text.contains("## Plan"));
    rt.frame(0.1);
    rt.frame(0.8);
    let saved = fs::read_to_string(path).expect("saved note");
    assert!(saved.contains("## Plan"), "{saved}");
}

#[test]
fn a_url_pasted_over_a_selection_becomes_a_link_in_one_undo_step() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let board = clipboard::memory();
    rt.use_clipboard(&board);

    rt.state.editor.replace_text("Read the docs here");
    rt.state.editor.set_selection(9, 13);
    rt.state.focus_editor();
    rt.frame(0.0);

    board.set_content(ClipboardContent::new("https://example.com/docs"));
    paste(&mut rt);
    assert_eq!(
        rt.state.editor.text,
        "Read the [docs](https://example.com/docs) here"
    );

    rt.key(command('z'));
    rt.frame(0.0);
    assert_eq!(rt.state.editor.text, "Read the docs here");
}

#[test]
fn plain_clipboards_keep_todays_paste() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let board = clipboard::memory();
    rt.use_clipboard(&board);

    rt.state.editor.replace_text("before ");
    rt.state.editor.set_caret(rt.state.editor.text.len());
    rt.state.focus_editor();
    rt.frame(0.0);

    // A URL with no selection stays plain text.
    board.set_content(ClipboardContent::new("https://example.com"));
    paste(&mut rt);
    assert_eq!(rt.state.editor.text, "before https://example.com");
    rt.key(command('z'));
    rt.frame(0.0);

    // Already-Markdown plain text is neither escaped nor converted.
    board.set_content(ClipboardContent::new("**bold** [link](https://x.dev)"));
    paste(&mut rt);
    assert_eq!(
        rt.state.editor.text,
        "before **bold** [link](https://x.dev)"
    );
}

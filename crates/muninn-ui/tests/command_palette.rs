//! v0.5 headless tests: the command palette.
//!
//! One input searches notes and actions together. These drive the real Rune
//! runtime by keys only: open the palette, type, filter, move the highlight,
//! run a row, and Escape back to the editor.

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, PaletteAction, PaletteItem, Scope, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn palette_vault() -> TempVault {
    TempVault::new(&[
        ("Apple.md", "An orchard grows here.\n"),
        ("Orchard.md", "Apple trees.\n"),
        ("Banana.md", "Nothing to see.\n"),
    ])
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

fn frame(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

fn key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
}

fn text(s: &str) -> KeyEvent {
    key(Key::Text(s.to_owned()))
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

fn palette_chord() -> KeyEvent {
    KeyEvent {
        key: Key::Command('p'),
        mods: Mods {
            cmd: true,
            shift: true,
            ..Mods::default()
        },
    }
}

/// Type one character at a time into the focused field, then settle a frame so
/// the palette's query-derived list is current.
fn type_text(rt: &mut Runtime<State>, typed: &str) {
    for ch in typed.chars() {
        rt.key(text(&ch.to_string()));
        frame(rt);
    }
    frame(rt);
}

#[test]
fn palette_filters_notes_title_first_and_enter_opens() {
    let tmp = palette_vault();
    let mut rt = runtime(&tmp);
    assert!(rt.state.editor.focused, "the editor starts focused");

    rt.key(palette_chord());
    frame(&mut rt);
    assert!(
        rt.state.palette_open(),
        "Ctrl/Cmd+Shift+P opens the palette"
    );
    assert!(
        rt.state.palette_field.focused,
        "the palette input owns typing"
    );
    assert!(!rt.state.editor.focused, "the hidden editor is not focused");
    assert!(rt.find("palette").is_some(), "the palette card is mounted");
    assert!(rt.find("palette-input").is_some());

    type_text(&mut rt, "orchard");
    let orchard = rt.state.vault.resolve("Orchard").expect("Orchard");
    let apple = rt.state.vault.resolve("Apple").expect("Apple");
    assert_eq!(
        rt.state.palette_items(),
        vec![PaletteItem::Note(orchard), PaletteItem::Note(apple)],
        "the title match ranks above the body-only match"
    );

    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert!(!rt.state.palette_open(), "Enter closes the palette");
    assert_eq!(
        rt.state.selected,
        Some(orchard),
        "the highlighted note opens"
    );
    assert_eq!(rt.state.mode, Mode::Editor);
    assert!(rt.state.editor.focused, "focus returns to the editor");
    assert!(rt.state.editor.text.contains("Apple trees"));
    assert!(rt.find("palette").is_none(), "the card unmounts");

    // Cmd/Ctrl+P is still the insert-link picker, not this palette.
    rt.key(command('p'));
    frame(&mut rt);
    assert!(!rt.state.palette_open());
    assert!(rt.state.complete.is_some(), "Ctrl+P stays the link picker");
}

#[test]
fn palette_arrows_wrap_and_escape_restores_focus() {
    let tmp = palette_vault();
    let mut rt = runtime(&tmp);

    rt.key(palette_chord());
    frame(&mut rt);
    let len = rt.state.palette_items().len();
    assert!(len > 1, "the empty palette lists actions and recents");
    assert_eq!(rt.state.palette_selected(), 0);

    // Up from the first row wraps to the last; Down from the last wraps back.
    rt.key(key(Key::Up));
    frame(&mut rt);
    assert_eq!(rt.state.palette_selected(), len - 1);
    rt.key(key(Key::Down));
    frame(&mut rt);
    assert_eq!(rt.state.palette_selected(), 0);

    let before = rt.state.editor.text.clone();
    rt.key(key(Key::Escape));
    frame(&mut rt);
    assert!(!rt.state.palette_open(), "Escape closes the palette");
    assert!(rt.state.editor.focused, "Escape restores editor focus");
    assert!(rt.find("palette").is_none());

    // The restored editor really owns typing again.
    rt.key(text("Z"));
    frame(&mut rt);
    assert_ne!(rt.state.editor.text, before, "typing reaches the note");
    assert!(rt.state.editor.text.contains('Z'));
}

#[test]
fn palette_empty_query_is_actions_and_recent_notes_only() {
    let tmp = palette_vault();
    let mut rt = runtime(&tmp);
    let alpha = rt.state.vault.resolve("Apple").expect("Apple");
    let beta = rt.state.vault.resolve("Orchard").expect("Orchard");
    let gamma = rt.state.vault.resolve("Banana").expect("Banana");
    // Startup opens one note; open a second so the recent list has an order.
    rt.state.open_note(beta);
    frame(&mut rt);

    rt.key(palette_chord());
    frame(&mut rt);
    let items = rt.state.palette_items();
    assert_eq!(
        items
            .iter()
            .filter(|item| matches!(item, PaletteItem::Action(_)))
            .count(),
        PaletteAction::ALL.len(),
        "every action is offered before typing"
    );
    assert_eq!(
        items.first(),
        Some(&PaletteItem::Action(PaletteAction::OpenLocalGraph))
    );
    assert!(
        items.contains(&PaletteItem::Recent(beta)),
        "the note just opened is recent"
    );
    assert!(
        items.contains(&PaletteItem::Recent(alpha)),
        "the startup note is recent"
    );
    assert!(
        !items.contains(&PaletteItem::Recent(gamma)),
        "an unvisited note is not recent"
    );
    assert!(
        !items
            .iter()
            .any(|item| matches!(item, PaletteItem::Note(_))),
        "an empty query never dumps the whole vault"
    );
}

#[test]
fn palette_runs_actions_and_closes_itself() {
    let tmp = palette_vault();
    let mut rt = runtime(&tmp);

    // Open local graph.
    rt.key(palette_chord());
    frame(&mut rt);
    type_text(&mut rt, "local graph");
    assert_eq!(
        rt.state.palette_items(),
        vec![PaletteItem::Action(PaletteAction::OpenLocalGraph)]
    );
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert!(!rt.state.palette_open());
    assert_eq!(rt.state.mode, Mode::Graph);
    assert_eq!(rt.state.scope, Scope::Local);

    // Switch vault opens the sidebar's path field.
    rt.key(palette_chord());
    frame(&mut rt);
    type_text(&mut rt, "switch vault");
    assert_eq!(
        rt.state.palette_items(),
        vec![PaletteItem::Action(PaletteAction::SwitchVault)]
    );
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert!(!rt.state.palette_open());
    assert!(rt.state.vault_prompt);
    assert!(rt.state.vault_field.focused);

    // Opening the palette dismisses the vault prompt; Open settings opens the
    // v0.5 settings modal.
    rt.key(palette_chord());
    frame(&mut rt);
    assert!(!rt.state.vault_prompt);
    type_text(&mut rt, "settings");
    assert_eq!(
        rt.state.palette_items(),
        vec![PaletteItem::Action(PaletteAction::OpenSettings)]
    );
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert!(!rt.state.palette_open());
    assert!(
        rt.state.settings_modal,
        "Open settings opens the settings modal"
    );

    // New note opens the inline name field.
    rt.state.close_settings();
    rt.key(palette_chord());
    frame(&mut rt);
    type_text(&mut rt, "new note");
    rt.key(key(Key::Enter));
    frame(&mut rt);
    assert!(!rt.state.palette_open());
    assert!(rt.state.new_note_prompt.is_some());
    assert!(rt.state.new_note_field.focused);
}

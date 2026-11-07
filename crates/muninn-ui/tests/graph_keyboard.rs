//! M3 GRP-02/03: headless graph keyboard, search-to-node and isolate.

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, State, on_key, poll, view};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn temp_vault() -> TempVault {
    TempVault::new(&[
        ("Alpha.md", "# Alpha\n\nSee [[Beta]] and [[Gamma]].\n"),
        ("Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
        ("Gamma.md", "# Gamma\n\nA quiet note.\n"),
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

fn key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
}

fn text(value: &str) -> KeyEvent {
    KeyEvent {
        key: Key::Text(value.to_owned()),
        mods: Mods::default(),
    }
}

fn graph_runtime(tmp: &TempVault) -> Runtime<State> {
    let mut rt = runtime(tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    rt.state.load_note(alpha, false);
    rt.state.set_mode(Mode::Graph);
    rt.frame(1.0 / 60.0);
    rt
}

#[test]
fn arrows_move_the_focus_ring_and_enter_opens_the_focused_note() {
    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    let alpha = rt.state.vault.resolve("Alpha").expect("Alpha");
    // Entering the graph focuses the selection, so the ring is already drawn.
    assert_eq!(rt.state.graph_focus, Some(alpha));
    assert!(
        rt.find("graph-focus-ring").is_some(),
        "the focus ring must be mounted"
    );

    rt.key(key(Key::Right));
    rt.frame(1.0 / 60.0);
    let focused = rt.state.graph_focus.expect("focus moved");
    assert_ne!(focused, alpha, "Right must move the focus ring");
    // Focus is not selection: the note stays selected but the ring moved.
    assert_eq!(rt.state.selected, Some(alpha));
    assert_eq!(rt.state.mode, Mode::Graph);

    rt.key(key(Key::Enter));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.mode, Mode::Editor, "Enter opens the focused note");
    assert_eq!(rt.state.selected, Some(focused));
}

#[test]
fn graph_zoom_fit_and_escape_are_keyboard_only() {
    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    let before = rt.state.cam.zoom();
    rt.key(text("+"));
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state.cam.zoom() > before,
        "+ must zoom in: {} -> {}",
        before,
        rt.state.cam.zoom()
    );
    let zoomed = rt.state.cam.zoom();
    rt.key(text("-"));
    rt.frame(1.0 / 60.0);
    assert!(rt.state.cam.zoom() < zoomed, "- must zoom out");

    rt.key(text("f"));
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.auto_fit, "F fits without resuming auto-fit");

    rt.key(key(Key::Escape));
    rt.frame(1.0 / 60.0);
    assert_ne!(rt.state.mode, Mode::Graph, "Esc leaves the graph");
}

#[test]
fn search_to_node_selects_centres_and_isolate_toggles() {
    let tmp = temp_vault();
    let mut rt = graph_runtime(&tmp);
    let gamma = rt.state.vault.resolve("Gamma").expect("Gamma");
    // Type in the real field and submit, not just the state method.
    let (x, y) = rt.find("graph-search-box").expect("search box");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    for c in "Gam".chars() {
        rt.key(KeyEvent {
            key: Key::Text(c.to_string()),
            mods: Mods::default(),
        });
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(rt.state.graph_query.text, "Gam");
    rt.key(key(Key::Enter));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.selected, Some(gamma));
    assert_eq!(rt.state.graph_focus, Some(gamma));
    assert_eq!(rt.state.mode, Mode::Graph, "search keeps the graph open");
    assert!(!rt.state.auto_fit, "revealing a node stops auto-fit");

    assert!(rt.find("graph-search-box").is_some());
    assert!(rt.find("graph-isolate").is_some());
    rt.key(text("i"));
    rt.frame(1.0 / 60.0);
    assert!(rt.state.graph_isolate, "I toggles isolate");
    rt.key(text("i"));
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.graph_isolate);
}

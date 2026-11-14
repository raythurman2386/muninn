//! #112 Focus mode: chrome hidden, the caret's paragraph bright and centred,
//! everything restored on the way out.
//!
//! These drive the real Rune runtime headlessly, like `ui.rs`: no window and
//! no snapshots, only the mounted element tree and `State`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use muninn_core::testing::TempVault;
use muninn_ui::{
    Mode, PaletteAction, PaletteItem, ReduceMotion, Settings, State, on_key, poll, view,
};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muninn-focus-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// A three-paragraph note, so dimming has something to leave out.
const BREATHES: &str = "# Breathes\n\nFirst paragraph, the one before.\n\nSecond paragraph holds the caret.\n\nThird paragraph, the one after.\n";

fn temp_vault() -> TempVault {
    TempVault::new(&[
        ("Breathes.md", BREATHES),
        ("Alpha.md", "# Alpha\n\nSee [[Beta]].\n"),
        ("Beta.md", "# Beta\n\nBack to [[Alpha]].\n"),
    ])
}

fn runtime_with(tmp: &TempVault, mode: ReduceMotion, config: &Path) -> Runtime<State> {
    let settings = Settings {
        reduce_motion: mode,
        ..Settings::default()
    };
    let mut state = State::with_settings(
        tmp.path().to_path_buf(),
        tmp.path().with_extension("state"),
        config.to_path_buf(),
        settings,
    );
    state.reduced_motion_env = false;
    let mut rt = Runtime::new(state, view, (1180.0, 780.0), Env::new(true, 2.0));
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn runtime(tmp: &TempVault) -> Runtime<State> {
    let config = unique_dir("config");
    runtime_with(tmp, ReduceMotion::System, &config)
}

fn frames(rt: &mut Runtime<State>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
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

fn focus_chord() -> KeyEvent {
    KeyEvent {
        key: Key::Command('f'),
        mods: Mods {
            cmd: true,
            shift: true,
            ..Mods::default()
        },
    }
}

fn text(s: &str) -> KeyEvent {
    KeyEvent {
        key: Key::Text(s.to_owned()),
        mods: Mods::default(),
    }
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} must be mounted"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 2);
}

/// Open the long note and put the caret at `at` (a byte offset), as a click or
/// an arrow would.
fn open_at(rt: &mut Runtime<State>, at: usize) {
    let id = rt.state.vault.resolve("Breathes").expect("Breathes");
    rt.update(|s, _| {
        s.load_note(id, true);
        s.editor.set_caret(at);
    });
    frames(rt, 2);
}

fn caret_offset(text: &str) -> usize {
    text.find("holds").expect("the second paragraph's word")
}

/// How far the editor's content is scrolled inside its viewport.
fn scrolled(rt: &Runtime<State>) -> f32 {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("editor"))
        .expect("the editor")
        .scrolled[1]
}

/// The caret's drawn top, and the editor viewport's rect, in window points.
fn caret_in_viewport(rt: &Runtime<State>) -> (f32, [f32; 4]) {
    let elements = rt.elements();
    let viewport = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("editor-scroll"))
        .expect("the editor viewport");
    let caret = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("editor"))
        .and_then(|e| e.caret)
        .expect("a focused editor draws a caret");
    (caret[1], viewport.rect)
}

#[test]
fn the_shortcut_hides_the_chrome_and_the_note_stays_editable() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    open_at(&mut rt, 0);

    for key in ["topbar-title", "mode-editor", "rail", "note-list", "editor"] {
        assert!(rt.find(key).is_some(), "{key} is part of the ordinary page");
    }

    rt.key(focus_chord());
    frames(&mut rt, 4);
    assert!(rt.state.focus_active(), "⇧⌘F enters Focus mode");
    for key in [
        "topbar-title",
        "mode-editor",
        "mode-graph",
        "rail",
        "note-list",
    ] {
        assert!(rt.find(key).is_none(), "{key} is hidden in Focus mode");
    }
    assert!(rt.find("editor").is_some(), "the note stays on screen");
    assert!(
        rt.find("focus-exit").is_some(),
        "the exit chip is the one control left"
    );

    // The hidden chrome did not take the keyboard with it.
    assert!(rt.state.editor.focused);
    let before = rt.state.editor.text.clone();
    rt.key(text("Z"));
    frames(&mut rt, 1);
    assert_ne!(
        rt.state.editor.text, before,
        "typing still reaches the note"
    );
    assert!(rt.state.editor.text.contains('Z'));

    rt.key(focus_chord());
    frames(&mut rt, 4);
    assert!(!rt.state.focus_active(), "the same chord leaves it");
    for key in ["topbar-title", "mode-editor", "rail", "note-list"] {
        assert!(rt.find(key).is_some(), "{key} comes back");
    }
    assert!(rt.find("focus-exit").is_none());
}

#[test]
fn leaving_focus_restores_the_sidebar_state_it_had() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    open_at(&mut rt, 0);
    assert!(rt.state.sidebar_shown(), "the sidebar starts shown");

    // Enter with the button beside the sidebar toggle, leave with the chip.
    click(&mut rt, "focus-toggle");
    assert!(rt.state.focus_active());
    frames(&mut rt, 60);
    assert!(!rt.state.sidebar_visible());
    assert!(
        rt.state.sidebar_shown(),
        "Focus never changes the sidebar's own choice"
    );
    assert!(rt.find("note-list").is_none());
    click(&mut rt, "focus-exit");
    assert!(!rt.state.focus_active());
    frames(&mut rt, 60);
    assert!(rt.state.sidebar_visible());
    assert!(rt.find("note-list").is_some(), "the sidebar is back");

    // Now hide it first: Focus brings it back hidden, not shown.
    rt.state.editor.focused = false;
    rt.key(command('\\'));
    frames(&mut rt, 2);
    assert!(!rt.state.sidebar_shown());
    rt.key(focus_chord());
    frames(&mut rt, 4);
    assert!(rt.state.focus_active());
    rt.key(text("Q"));
    rt.key(focus_chord());
    frames(&mut rt, 4);
    assert!(!rt.state.focus_active());
    assert!(
        !rt.state.sidebar_shown(),
        "the pre-Focus choice is restored"
    );
    assert!(rt.find("note-list").is_none());
}

#[test]
fn cmd_backslash_still_only_toggles_the_sidebar() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    open_at(&mut rt, 0);
    rt.key(focus_chord());
    frames(&mut rt, 4);
    assert!(rt.state.focus_active());

    rt.state.editor.focused = false;
    rt.key(command('\\'));
    frames(&mut rt, 4);
    assert!(
        rt.state.focus_active(),
        "⌘\\ does not enter or leave Focus mode"
    );
    assert!(
        !rt.state.sidebar_shown(),
        "it toggled the sidebar's own choice"
    );
    assert!(!rt.state.sidebar_visible(), "which Focus still hides");
}

#[test]
fn the_palette_offers_the_same_toggle() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    open_at(&mut rt, 0);

    rt.key(palette_chord());
    frames(&mut rt, 2);
    for c in "focus mode".chars() {
        rt.key(text(&c.to_string()));
        frames(&mut rt, 1);
    }
    assert_eq!(
        rt.state.palette_items(),
        vec![PaletteItem::Action(PaletteAction::ToggleFocusMode)]
    );
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    frames(&mut rt, 4);
    assert!(!rt.state.palette_open());
    assert!(rt.state.focus_active(), "the action enters Focus mode");

    rt.key(palette_chord());
    frames(&mut rt, 2);
    for c in "focus mode".chars() {
        rt.key(text(&c.to_string()));
        frames(&mut rt, 1);
    }
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    frames(&mut rt, 4);
    assert!(!rt.state.focus_active(), "and leaves it");
}

#[test]
fn the_shortcut_waits_for_the_note_editor() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    open_at(&mut rt, 0);

    // The sidebar search owns typing: the chord is not the note's to take.
    rt.key(command('k'));
    frames(&mut rt, 2);
    assert!(rt.state.search.focused);
    rt.key(focus_chord());
    frames(&mut rt, 2);
    assert!(!rt.state.focus_active(), "search keeps its keyboard");
    assert_eq!(rt.state.search.text, "", "and the chord types nothing");

    // The find bar too.
    rt.update(|s, _| s.blur_text());
    frames(&mut rt, 2);
    rt.key(command('f'));
    frames(&mut rt, 2);
    assert!(rt.state.find.is_some(), "⌘F still opens find");
    rt.key(focus_chord());
    frames(&mut rt, 2);
    assert!(!rt.state.focus_active(), "find keeps its keyboard");

    // No field at all: the chord works.
    rt.update(|s, _| {
        s.close_find();
        s.blur_text();
    });
    frames(&mut rt, 2);
    rt.key(focus_chord());
    frames(&mut rt, 2);
    assert!(rt.state.focus_active());
}

#[test]
fn only_the_caret_paragraph_keeps_full_contrast() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    let caret = caret_offset(BREATHES);
    open_at(&mut rt, caret);
    assert!(!rt.state.focus_active());
    assert!(
        rt.state.focus_dim_spans(rune::Color::WHITE).is_empty(),
        "nothing is dimmed outside Focus mode"
    );

    rt.key(focus_chord());
    frames(&mut rt, 2);
    let spans = rt.state.focus_dim_spans(rune::Color::WHITE);
    assert!(!spans.is_empty(), "the other paragraphs are dimmed");
    let active_start = BREATHES[..caret].rfind("\n\n").map_or(0, |at| at + 2);
    let active_end = BREATHES[caret..]
        .find("\n\n")
        .map_or(BREATHES.len(), |at| caret + at + 1);
    for span in &spans {
        let (start, end) = (span.start as usize, span.end as usize);
        assert!(
            end <= active_start || start >= active_end,
            "the caret's paragraph {active_start}..{active_end} is not dimmed: {start}..{end}"
        );
        assert_eq!(
            span.color,
            Some(rune::Color::WHITE),
            "each dimmed block carries the fainter ink"
        );
    }
    // The title above and the paragraph below are both in the dimmed set.
    assert!(spans.iter().any(|s| s.start == 0), "the title is dimmed");
    assert!(
        spans.iter().any(|s| s.end as usize == BREATHES.len()),
        "the last paragraph is dimmed"
    );
}

#[test]
fn the_caret_line_stays_centred_while_it_moves() {
    let lines: Vec<String> = (0..120)
        .map(|i| format!("Line {i} of a long note.\n"))
        .collect();
    let note = lines.join("\n");
    let tmp = TempVault::new(&[("Long.md", note.as_str()), ("Other.md", "# Other\n\nHi.\n")]);
    let mut rt = runtime(&tmp);
    let long = rt.state.vault.resolve("Long").expect("Long");
    rt.update(|s, _| {
        s.load_note(long, true);
        s.editor.set_caret(note.len());
    });
    frames(&mut rt, 2);
    rt.key(focus_chord());
    frames(&mut rt, 60);
    assert!(rt.state.focus_active());

    // The caret starts at the note's end, so the viewport is at its bottom
    // edge: the last line stands where the note ends.
    assert!(scrolled(&rt) > 0.0, "the viewport moved to the caret");

    // A line in the middle of the note is centred in the viewport, its line's
    // own middle on the viewport's.
    let at = note.find("Line 60").expect("line 60");
    rt.update(|s, _| s.editor.set_caret(at));
    frames(&mut rt, 60);
    let (caret_y, rect) = caret_in_viewport(&rt);
    let middle = rect[1] + rect[3] * 0.5;
    let line_middle = caret_y + 12.5;
    assert!(
        (line_middle - middle).abs() < 20.0,
        "the caret line is centred: {line_middle} vs {middle}"
    );

    // Moving again re-centres the new line.
    let before = scrolled(&rt);
    let at = note.find("Line 30").expect("line 30");
    rt.update(|s, _| s.editor.set_caret(at));
    frames(&mut rt, 60);
    assert!(scrolled(&rt) < before, "the viewport followed the caret");
    let (caret_y, rect) = caret_in_viewport(&rt);
    let middle = rect[1] + rect[3] * 0.5;
    assert!(
        (caret_y + 12.5 - middle).abs() < 20.0,
        "the new line is centred too"
    );

    // The note's first line cannot be centred higher than the top.
    rt.update(|s, _| s.editor.set_caret(0));
    frames(&mut rt, 60);
    assert_eq!(scrolled(&rt), 0.0, "the top stops where the note starts");
}

#[test]
fn focus_mode_never_applies_to_graph_board_or_kanban() {
    let tmp = temp_vault();
    let mut rt = runtime(&tmp);
    open_at(&mut rt, 0);
    rt.key(focus_chord());
    frames(&mut rt, 4);
    assert!(rt.state.focus_active());

    rt.key(command('3'));
    frames(&mut rt, 4);
    assert_eq!(rt.state.mode, Mode::Graph);
    assert!(!rt.state.focus_active(), "Focus leaves with the note page");
    assert!(
        rt.find("mode-editor").is_some(),
        "the graph keeps its chrome"
    );
    rt.key(command('1'));
    frames(&mut rt, 4);
    assert!(!rt.state.focus_active(), "and does not come back by itself");
    assert!(rt.find("topbar-title").is_some());
}

#[test]
fn reduced_motion_makes_the_focus_chrome_change_land_at_once() {
    for mode in [ReduceMotion::Off, ReduceMotion::On] {
        let tmp = temp_vault();
        let config = unique_dir("motion");
        let mut rt = runtime_with(&tmp, mode, &config);
        open_at(&mut rt, 0);
        rt.key(focus_chord());
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.reduced_motion(), mode == ReduceMotion::On);
        let moving: Vec<String> = rt
            .elements()
            .into_iter()
            .filter(|e| e.moving)
            .filter_map(|e| e.key)
            .collect();
        if mode == ReduceMotion::On {
            assert!(
                moving.is_empty(),
                "reduced motion skips the slide and the fade: {moving:?}"
            );
        } else {
            assert!(
                moving.iter().any(|key| key == "editor")
                    && moving.iter().any(|key| key == "focus-exit"),
                "with motion allowed the layout slides and the chip fades: {moving:?}"
            );
        }
        let _ = fs::remove_dir_all(&config);
    }
}

#[test]
fn focus_is_session_only_and_never_saved() {
    let tmp = temp_vault();
    let config = unique_dir("settings");
    let mut rt = runtime_with(&tmp, ReduceMotion::System, &config);
    open_at(&mut rt, 0);
    rt.key(focus_chord());
    frames(&mut rt, 4);
    assert!(rt.state.focus_active());
    // A save happens while Focus is on (autosave, or a settings write).
    rt.state.save_settings();
    let raw = fs::read_to_string(config.join("muninn").join("settings.json")).expect("settings");
    assert!(
        !raw.contains("focus_mode"),
        "Focus mode is not part of settings.json: {raw}"
    );
    let _ = fs::remove_dir_all(&config);
}

#[test]
fn an_empty_note_is_editable_without_a_mark_to_centre() {
    let tmp = TempVault::new(&[("Blank.md", ""), ("Other.md", "# Other\n")]);
    let mut rt = runtime(&tmp);
    let id = rt.state.vault.resolve("Blank").expect("Blank");
    rt.update(|s, _| s.load_note(id, true));
    rt.key(focus_chord());
    frames(&mut rt, 4);
    assert!(rt.state.focus_active());
    assert!(rt.state.focus_caret_mark().is_none());
    rt.key(text("A"));
    frames(&mut rt, 2);
    assert_eq!(rt.state.editor.text, "A");
    assert!(rt.state.focus_caret_mark().is_some());
}

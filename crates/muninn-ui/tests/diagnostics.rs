//! X-01 session log/diagnostics and DES-03 reduced-motion tests.
//!
//! Every test uses a private XDG state home beside the temp vault so the
//! developer's real `$XDG_STATE_HOME/muninn/session.log` is untouched.

use std::fs;
use std::path::PathBuf;

use muninn_core::testing::TempVault;
use muninn_ui::{Mode, State, poll};
use rune::Tx;

/// A state whose session log lives in `vault`'s sibling `<vault>.state`.
fn state_at(vault: &TempVault) -> (State, PathBuf) {
    let home = vault.path().with_extension("state");
    let state = State::with_state_home(vault.path().to_path_buf(), home.clone());
    (state, home)
}

fn log_text(home: &std::path::Path) -> String {
    fs::read_to_string(home.join("muninn").join("session.log")).expect("session log")
}

#[test]
fn session_log_records_open_and_save_failure_without_note_bodies() {
    let secret = "TOP-SECRET-BODY-TEXT";
    let tmp = TempVault::new(&[("Alpha.md", &format!("# Alpha\n\n{secret}\n"))]);
    let (mut s, home) = state_at(&tmp);

    let startup_log = log_text(&home);
    assert!(startup_log.contains("startup"), "{startup_log}");
    assert!(startup_log.contains("notes=1"), "{startup_log}");
    assert!(
        !startup_log.contains("Alpha.md"),
        "the startup line has counts, not note paths: {startup_log}"
    );
    assert!(
        !startup_log.contains(secret),
        "note bodies never reach the log"
    );
    assert!(
        !startup_log.contains("# Alpha"),
        "note bodies never reach the log"
    );

    let id = s.vault.resolve("Alpha").expect("Alpha");
    s.load_note(id, false);
    s.editor.replace_text("edited but unmovable\n");
    // Make the note path a directory so the atomic rename fails; a real IO
    // failure is the only way to reach the save-failed branch.
    let path = s.vault.notes[id].path.clone();
    fs::remove_file(&path).expect("remove note");
    fs::create_dir(&path).expect("block path");
    s.expected_disk = None;
    assert!(!s.save_now(), "the write must fail");

    let log = log_text(&home);
    assert!(log.contains("save-failed"), "{log}");
    assert!(log.contains("Alpha.md"), "{log}");
    assert!(
        !log.contains("edited but unmovable"),
        "buffers never reach the log"
    );
    drop(s);
    drop(fs::remove_dir_all(&home));
}

#[test]
fn trace_logs_autosave_ticks() {
    let tmp = TempVault::new(&[("Alpha.md", "# Alpha\n")]);
    let (mut s, home) = state_at(&tmp);
    let id = s.vault.resolve("Alpha").expect("Alpha");
    s.load_note(id, false);
    s.set_trace(true);
    s.editor.replace_text("# Alpha\n\nchanged\n");
    assert!(s.save_now(), "the buffered text reaches disk");

    let log = log_text(&home);
    assert!(log.contains("trace enabled"), "{log}");
    assert!(log.contains("autosave Alpha.md"), "{log}");
    drop(s);
    drop(fs::remove_dir_all(&home));
}

#[test]
fn reduced_motion_freezes_the_graph_layout() {
    let tmp = TempVault::new(&[
        ("Alpha.md", "[[Beta]]"),
        ("Beta.md", "[[Gamma]]"),
        ("Gamma.md", "[[Alpha]]"),
    ]);
    let (mut s, home) = state_at(&tmp);
    s.reduced_motion_env = true;
    s.set_mode(Mode::Graph);
    let before: Vec<[f32; 2]> = s.graph.nodes.iter().map(|node| node.pos).collect();
    let mut tx = Tx::default();
    for frame in 0..5 {
        poll(&mut s, f64::from(frame) / 60.0, &mut tx);
    }
    let after: Vec<[f32; 2]> = s.graph.nodes.iter().map(|node| node.pos).collect();
    assert_eq!(before, after, "reduced motion keeps every node still");
    assert_eq!(
        s.graph.heat, 0.0,
        "heat is pinned so nothing wakes the window"
    );

    // The same layout does settle when reduced motion is off.
    s.reduced_motion_env = false;
    s.graph.reheat();
    for frame in 0..5 {
        poll(&mut s, 0.1 + f64::from(frame) / 60.0, &mut tx);
    }
    let moved: Vec<[f32; 2]> = s.graph.nodes.iter().map(|node| node.pos).collect();
    assert_ne!(before, moved, "the normal layout still steps");
    drop(s);
    drop(fs::remove_dir_all(&home));
}

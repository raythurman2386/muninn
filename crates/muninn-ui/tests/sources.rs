//! The source seam a browser host uses: a `State` whose vault is built from
//! caller-supplied bytes instead of a folder scan (#240 slice 5).
//!
//! The demo crate (`muninn-web`) drives this on wasm; this keeps the seam
//! honest on the native suite, where a folder is available but deliberately
//! not used for the vault.

use muninn_core::{NoteSource, testing::TempVault};
use muninn_ui::State;

#[test]
fn a_state_installs_a_vault_built_from_sources() {
    // An existing empty folder: the placeholder startup scan succeeds cleanly,
    // so the install below is what supplies every note.
    let home = TempVault::new(&[]);
    let sources = [
        NoteSource {
            rel: "Welcome.md",
            bytes: b"# Welcome\n\nSee [[Graph]].\n",
        },
        NoteSource {
            rel: "Graph.md",
            bytes: b"# Graph\n",
        },
        // Not a note: `from_sources` ignores it, as the filesystem walk would.
        NoteSource {
            rel: "Board.excalidraw",
            bytes: b"{}",
        },
    ];
    let mut state = State::with_state_home(
        home.path().to_path_buf(),
        home.path().with_extension("state"),
    );
    assert_eq!(state.vault.notes.len(), 0);
    assert!(state.startup_error.is_none());

    assert!(state.install_sources(&sources), "no board can refuse here");

    assert_eq!(state.vault.notes.len(), 2);
    assert_eq!(state.vault.skipped_count(), 0);
    assert!(state.startup_error.is_none());
    let welcome = state.vault.resolve("Welcome").expect("Welcome resolves");
    let graph = state.vault.resolve("Graph").expect("Graph resolves");
    assert!(state.vault.notes[welcome].links.contains(&graph));

    // The install runs the tail of a scan: a note is open and the editor
    // holds its source, which is what an unseamed folder scan would show.
    let selected = state.selected.expect("a note is open after install");
    assert_eq!(state.vault.notes[selected].rel, "Graph.md");
    assert_eq!(state.editor.text, state.vault.notes[selected].source);
}

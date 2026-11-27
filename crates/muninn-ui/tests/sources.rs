//! The source seam a browser host uses: a `State` whose vault is built from
//! caller-supplied bytes instead of a folder scan (#240 slice 5).
//!
//! The demo crate (`muninn-web`) drives this on wasm; this keeps the seam
//! honest on the native suite, where a folder is available but deliberately
//! not used for the vault.

use std::sync::Arc;

use muninn_core::{MemoryStore, NoteSource, VaultStore, testing::TempVault};
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

#[test]
fn a_store_backed_state_creates_and_reloads_through_its_own_store() {
    // A different root from the placeholder scan above, so the store's root is
    // the only owner of these paths.
    let home = TempVault::new(&[]);
    let root = home.path().join("browser-vault");
    let store = Arc::new(MemoryStore::new(&root));
    store.put_file("Welcome.md", b"# Welcome\n\nA [store](Note.md).\n");
    let store: Arc<dyn VaultStore> = store.clone();
    let mut state = State::with_state_home(root.clone(), home.path().with_extension("state"));
    state
        .install_store(Arc::clone(&store))
        .expect("store installs");

    // Creating a note writes through the vault, then `reload` re-reads through
    // the same store. Before 6b it called `Vault::load`, which on web would
    // have replaced the browser vault with an empty native one.
    state.create_note_named_in("Fresh", "");
    assert!(
        state.vault.notes.iter().any(|note| note.title == "Fresh"),
        "the new note is in the reloaded index"
    );
    assert!(
        store.is_file(&root.join("Fresh.md")),
        "the new note is in the same store"
    );

    // A second install sees the same bytes: the reload round-trip.
    let mut again = State::with_state_home(root.clone(), home.path().with_extension("state-2"));
    again
        .install_store(Arc::clone(&store))
        .expect("reinstall the same store");
    assert!(again.vault.notes.iter().any(|note| note.title == "Fresh"));
    assert!(again.vault.notes.iter().any(|note| note.title == "Welcome"));
}

//! The browser store seen through the real vault API (#240 slice 6b).
//!
//! The store's own unit tests pin the mirror and the journal; this drives the
//! same operations the in-browser gate performs — create, edit, rename, move,
//! delete — through `Vault`, then opens the store again as a reload would.

use std::sync::Arc;

use muninn_core::{Vault, VaultStore};
use muninn_web::{OpfsStore, SAMPLE_ROOT, SAMPLE_SOURCES};

fn sample_store() -> Arc<OpfsStore> {
    let store = Arc::new(OpfsStore::new(SAMPLE_ROOT, true));
    for source in SAMPLE_SOURCES {
        store.load(source.rel, source.bytes);
    }
    store
}

#[test]
fn create_edit_rename_move_and_delete_all_land_in_the_store() {
    let store = sample_store();
    let handle: Arc<dyn VaultStore> = store.clone();
    let mut vault = Vault::open(Arc::clone(&handle)).expect("the sample opens");

    let welcome = vault.resolve("Welcome").expect("Welcome resolves");
    vault
        .write_note(welcome, "# Welcome\n\nEdited in the browser.\n")
        .expect("edit an existing note");

    let fresh = vault
        .create_note_in("Fresh note", "notes")
        .expect("create in a new folder");
    // `create_note_in` writes the file; the index picks it up on the next
    // open, exactly as a browser reload does.
    vault = Vault::open(Arc::clone(&handle)).expect("reopen after create");
    let fresh_id = vault
        .notes
        .iter()
        .find(|note| note.path == fresh)
        .map(|note| note.id)
        .expect("the new note is indexed");

    // A case-only rename takes the two-hop route the store contract pins.
    vault.rename_note(fresh_id, "fresh NOTE").expect("rename");
    vault
        .create_folder("", "Archive")
        .expect("create the destination folder");
    vault.move_note(fresh_id, "Archive").expect("move");

    let old = vault.resolve("Backlinks").expect("Backlinks resolves");
    vault
        .delete_note_to_trash(old, 1_700_000_000)
        .expect("delete to trash");

    let plan = store.take_flush_plan();
    assert!(
        plan.files.contains(&"Welcome.md".to_owned()),
        "the edit is dirty: {plan:?}"
    );
    assert!(
        plan.files.contains(&"Archive/fresh NOTE.md".to_owned()),
        "the rename and move are dirty: {plan:?}"
    );
    assert!(
        plan.removals.contains(&"Backlinks.md".to_owned()),
        "the delete is dirty: {plan:?}"
    );
    assert!(
        plan.files
            .iter()
            .any(|rel| rel.starts_with(".muninn-trash/")),
        "the trash copy is written too: {plan:?}"
    );

    // A reload is another `Vault::open` over the same store, which is what the
    // boot does after walking OPFS back into the mirror.
    let reloaded = Vault::open(handle).expect("reload");
    assert!(
        reloaded
            .notes
            .iter()
            .any(|note| note.rel == "Archive/fresh NOTE.md"),
        "the renamed and moved note is in the reloaded index"
    );
    let reopened = reloaded.resolve("Welcome").expect("Welcome survives");
    assert!(
        reloaded.notes[reopened]
            .source
            .contains("Edited in the browser.")
    );
    assert!(reloaded.resolve("Backlinks").is_none());
    assert!(
        reloaded.trash_size().0 > 0,
        "the deleted note is in the trash, not the index"
    );
    let trash = reloaded
        .store()
        .read_to_string(
            &reloaded
                .root
                .join(Vault::TRASH_DIR)
                .join("1700000000")
                .join("Backlinks.md"),
        )
        .expect("the trash copy is readable through the store");
    assert!(trash.contains("Backlinks"));
}

/// The sample's board is listed and readable through the store, the shape the
/// board page's `read_text` path now uses.
#[test]
fn the_seeded_board_is_listed_and_its_bytes_are_readable() {
    let store = sample_store();
    let handle: Arc<dyn VaultStore> = store.clone();
    let vault = Vault::open(handle).expect("the sample opens");
    let boards = vault.boards();
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].title, "Muninn board");
    let text = store
        .read_to_string(&boards[0].path)
        .expect("the board's bytes come from the store");
    assert!(text.contains("\"type\": \"excalidraw\""));
}

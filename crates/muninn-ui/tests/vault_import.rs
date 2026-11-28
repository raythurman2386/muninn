//! HEADLESS tests for the browser vault's import seam (#240 slice 6e).
//!
//! Export and import exist only where a browser host registers its picker and
//! download. These drive the shared half: the save guard that keeps an import
//! from overwriting unsaved work, the re-index after the bytes are written, and
//! the palette rows that appear only once a host is installed.

use muninn_core::testing::TempVault;
use muninn_ui::vault_transfer::{self, VaultImportError, VaultTransferHooks};
use muninn_ui::{PaletteAction, PaletteItem, State};

fn state(tmp: &TempVault) -> State {
    State::with_state_home(tmp.path().to_path_buf(), tmp.path().with_extension("state"))
}

#[test]
fn an_import_refuses_while_the_open_note_is_dirty() {
    let tmp = TempVault::new(&[("A.md", "# A\n")]);
    let mut state = state(&tmp);
    assert!(!state.dirty(), "the startup note is clean");

    state.editor.replace_text("# A changed\n");
    assert!(state.dirty());
    assert_eq!(
        state.begin_vault_import().err(),
        Some(VaultImportError::Dirty),
        "an unsaved note refuses the import"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("A.md")).unwrap(),
        "# A\n",
        "the refusal wrote nothing"
    );
    assert_eq!(
        VaultImportError::Dirty.message(),
        muninn_i18n::t(muninn_i18n::Key::WebImportRefusedDirty)
    );
}

#[test]
fn an_unresolved_conflict_refuses_the_import() {
    let tmp = TempVault::new(&[("A.md", "# A\n")]);
    let mut state = state(&tmp);
    let id = state.selected.expect("startup selects a note");
    state.conflict = Some(muninn_ui::ConflictState {
        note_id: id,
        expected: None,
        observed: None,
        reason: muninn_ui::ConflictReason::Modified,
    });
    assert_eq!(
        state.begin_vault_import().err(),
        Some(VaultImportError::Dirty),
        "a conflict outranks an import: resolve it first"
    );
}

#[test]
fn a_clean_vault_imports_and_reindexes() {
    let tmp = TempVault::new(&[("A.md", "# A\n")]);
    let mut state = state(&tmp);
    let before = state.vault.notes.len();

    let store = state.begin_vault_import().expect("a clean vault imports");
    assert_eq!(
        store.root(),
        state.vault.root.as_path(),
        "the import writes through the vault's own store"
    );
    store
        .write_atomic(&store.root().join("B.md"), b"# B\n")
        .expect("write B");
    store
        .create_dir_all(&store.root().join("Folder"))
        .expect("make the imported folder");
    store
        .write_atomic(&store.root().join("Folder/C.md"), b"# C\n")
        .expect("write C");

    state.finish_vault_import(store).expect("re-index");
    assert_eq!(state.vault.notes.len(), before + 2);
    assert!(state.vault.resolve("B").is_some());
    assert!(state.vault.resolve("C").is_some());
    assert!(
        state.vault.dirs.iter().any(|dir| dir == "Folder"),
        "the imported folder is in the index: {:?}",
        state.vault.dirs
    );
}

#[test]
fn the_palette_offers_export_and_import_only_with_a_host() {
    let tmp = TempVault::new(&[("A.md", "# A\n")]);
    let mut state = state(&tmp);
    state.open_palette();
    assert!(
        !vault_transfer::available(),
        "no browser host is registered in this test process yet"
    );
    assert!(
        !state
            .palette_items()
            .contains(&PaletteItem::Action(PaletteAction::ExportVault)),
        "the native palette has no browser-only rows"
    );

    fn noop() {}
    vault_transfer::install(VaultTransferHooks {
        export: noop,
        import: noop,
    });
    assert!(vault_transfer::available());
    let items = state.palette_items();
    assert!(items.contains(&PaletteItem::Action(PaletteAction::ExportVault)));
    assert!(items.contains(&PaletteItem::Action(PaletteAction::ImportVault)));
    assert!(state.export_vault(), "the registered export hook answers");
    assert!(state.import_vault(), "the registered import hook answers");
}

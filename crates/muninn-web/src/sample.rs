//! The committed sample vault, embedded as bytes.
//!
//! The demo reads its vault through [`Vault::from_sources`], the same pure
//! seam a browser store will use in slice 6, instead of a folder scan. Every
//! file is compiled into the wasm binary with [`include_bytes!`], so the bundle
//! is self-contained: no fetch, no manifest, no build script.
//!
//! The sample is the repository's `vault/` folder verbatim (17 Markdown notes
//! plus one `.excalidraw`). `Vault::from_sources` indexes `.md` notes and
//! seeds every non-note source into its memory store (slice 6a), so Board
//! mode lists the embedded board on web; opening it still needs the store's
//! read path, which the OPFS slice (6b) supplies.

use muninn_core::NoteSource;

/// The label stored on [`muninn_core::Vault::root`]. It is a path, not a folder
/// on disk: nothing in the demo writes to it.
pub const SAMPLE_ROOT: &str = "/Muninn sample";

/// The whole sample vault: vault-relative path and committed bytes.
///
/// A `const` slice of [`NoteSource`] (the struct is `Copy` and every reference
/// is `'static`), so startup allocates nothing to describe the vault; the
/// index itself is built once by `Vault::from_sources`.
pub const SAMPLE_SOURCES: &[NoteSource<'static>] = &[
    NoteSource {
        rel: "Backlinks.md",
        bytes: include_bytes!("../../../vault/Backlinks.md"),
    },
    NoteSource {
        rel: "Code Blocks.md",
        bytes: include_bytes!("../../../vault/Code Blocks.md"),
    },
    NoteSource {
        rel: "Daily Notes.md",
        bytes: include_bytes!("../../../vault/Daily Notes.md"),
    },
    NoteSource {
        rel: "Design Principles.md",
        bytes: include_bytes!("../../../vault/Design Principles.md"),
    },
    NoteSource {
        rel: "Focus.md",
        bytes: include_bytes!("../../../vault/Focus.md"),
    },
    NoteSource {
        rel: "Force Layout.md",
        bytes: include_bytes!("../../../vault/Force Layout.md"),
    },
    NoteSource {
        rel: "Getting Started.md",
        bytes: include_bytes!("../../../vault/Getting Started.md"),
    },
    NoteSource {
        rel: "Knowledge Graph.md",
        bytes: include_bytes!("../../../vault/Knowledge Graph.md"),
    },
    NoteSource {
        rel: "Launch Week.md",
        bytes: include_bytes!("../../../vault/Launch Week.md"),
    },
    NoteSource {
        rel: "Local Graph.md",
        bytes: include_bytes!("../../../vault/Local Graph.md"),
    },
    NoteSource {
        rel: "Markdown.md",
        bytes: include_bytes!("../../../vault/Markdown.md"),
    },
    // Not a note: `from_sources` ignores non-`.md` sources, as the filesystem
    // walk would. Kept here so the board's bytes ship with the demo and a later
    // source-backed board seam has its input.
    NoteSource {
        rel: "Muninn board.excalidraw",
        bytes: include_bytes!("../../../vault/Muninn board.excalidraw"),
    },
    NoteSource {
        rel: "Reading List.md",
        bytes: include_bytes!("../../../vault/Reading List.md"),
    },
    NoteSource {
        rel: "Scratchpad.md",
        bytes: include_bytes!("../../../vault/Scratchpad.md"),
    },
    NoteSource {
        rel: "Typography.md",
        bytes: include_bytes!("../../../vault/Typography.md"),
    },
    NoteSource {
        rel: "Visual modes.md",
        bytes: include_bytes!("../../../vault/Visual modes.md"),
    },
    NoteSource {
        rel: "Welcome.md",
        bytes: include_bytes!("../../../vault/Welcome.md"),
    },
    NoteSource {
        rel: "Wikilinks.md",
        bytes: include_bytes!("../../../vault/Wikilinks.md"),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::Vault;

    /// The demo's vault input must be a real index, not just bytes: a few notes
    /// resolve, links and backlinks agree, and the board file is not a note.
    #[test]
    fn the_sample_vault_indexes_and_its_links_resolve() {
        let vault = Vault::from_sources(SAMPLE_ROOT, SAMPLE_SOURCES);
        assert_eq!(vault.root, std::path::Path::new(SAMPLE_ROOT));
        assert_eq!(vault.notes.len(), 17);
        assert_eq!(vault.skipped_count(), 0);
        // Every note lives at the root, so the folder list is just the root.
        assert_eq!(vault.dirs, [String::new()]);

        let welcome = vault.resolve("Welcome").expect("Welcome resolves");
        assert_eq!(vault.notes[welcome].title, "Welcome");
        let graph = vault
            .resolve("Knowledge Graph")
            .expect("Knowledge Graph resolves");
        assert!(
            vault.notes[graph].backlinks.contains(&welcome),
            "Welcome links to Knowledge Graph, so it is a backlink there"
        );
        let getting_started = vault
            .resolve("Getting Started")
            .expect("Welcome's first link resolves");
        assert!(vault.notes[welcome].links.contains(&getting_started));

        // The kanban sample keeps its frontmatter through the source seam, so
        // the Kanban page has something to render.
        let launch = vault.resolve("Launch Week").expect("Launch Week resolves");
        assert!(vault.notes[launch].is_kanban());

        // The embedded board is present as a source but is not a note today.
        assert!(
            SAMPLE_SOURCES
                .iter()
                .any(|source| source.rel == "Muninn board.excalidraw")
        );
        assert!(
            vault
                .notes
                .iter()
                .all(|note| note.rel.ends_with(".md") && note.rel != "Muninn board.excalidraw")
        );
    }

    /// The committed folder and the embedded list must not drift: every sample
    /// file is embedded, and nothing embedded is missing from the repository.
    #[test]
    fn every_committed_sample_file_is_embedded() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vault");
        let mut on_disk: Vec<String> = std::fs::read_dir(&root)
            .expect("the repository's vault/ folder")
            .map(|entry| {
                entry
                    .expect("readable entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        on_disk.sort();
        let mut embedded: Vec<String> = SAMPLE_SOURCES
            .iter()
            .map(|source| source.rel.to_owned())
            .collect();
        embedded.sort();
        assert_eq!(embedded, on_disk);
    }
}

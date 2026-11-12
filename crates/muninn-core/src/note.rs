//! One Markdown file and the links indexed from it.

use std::path::PathBuf;

/// One Markdown file in a vault.
#[derive(Clone, Debug)]
pub struct Note {
    pub id: usize,
    pub path: PathBuf,
    /// The path relative to the vault root, with `/` separators.
    pub rel: String,
    pub title: String,
    pub source: String,
    /// Resolved outgoing links, unique and sorted.
    pub links: Vec<usize>,
    /// Notes that link here, unique and sorted.
    pub backlinks: Vec<usize>,
    /// [`crate::is_kanban`] of `source`, cached by [`crate::Vault::reindex`]
    /// so a frame never re-detects it.
    pub(crate) kanban: bool,
}

impl Note {
    /// Whether this note is a kanban note (its frontmatter has
    /// `kanban-plugin`), as of the last index rebuild.
    #[must_use]
    pub fn is_kanban(&self) -> bool {
        self.kanban
    }

    /// Vault-relative folder (e.g. `projects/alpha`), or
    /// empty when the note is at the vault root.
    #[must_use]
    pub fn folder(&self) -> &str {
        match self.rel.rfind('/') {
            Some(at) => &self.rel[..at],
            None => "",
        }
    }

    /// Vault-relative stem without `.md`, e.g. `projects/alpha/note` or
    /// `Welcome`. This is the unambiguous wikilink target for this note.
    #[must_use]
    pub fn stem(&self) -> &str {
        self.rel.strip_suffix(".md").unwrap_or(&self.rel)
    }

    /// Last folder segment (e.g. `alpha` for `projects/alpha`), or empty at
    /// the vault root. Useful for short disambiguating labels.
    #[must_use]
    pub fn parent(&self) -> &str {
        let folder = self.folder();
        folder.rsplit('/').next().unwrap_or(folder)
    }
}

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
}

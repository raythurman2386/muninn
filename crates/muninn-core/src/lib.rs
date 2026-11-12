//! The vault model behind Muninn.
//!
//! A vault is a plain folder of Markdown files. [`Vault`] scans it, parses the
//! `[[wikilinks]]` in each note (skipping code spans and fenced blocks), and builds the
//! outgoing and backlink indexes the rest of the app reads.
//!
//! Whiteboards are `.excalidraw` files ([`Board`]). They live in the vault
//! but are not notes: [`Vault::boards`] lists them separately.
//!
//! Kanban notes ([`Kanban`]) are notes. A `kanban-plugin` frontmatter key
//! marks one; [`Vault::kanbans`] lists those notes without taking them out of
//! the link index.
//!
//! This crate has no UI or runtime dependencies, so it is cheap to test and reusable from
//! tools, benchmarks and future front ends.

mod board;
mod kanban;
mod note;
mod preview;
mod search;
mod vault;
mod wikilink;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use board::{
    BOARD_EXTENSION, Board, BoardFile, Change, Element, HISTORY_BYTES, HISTORY_LIMIT, History,
    IdSource, MUNINN_SOURCE, SCENE_VERSION, Shape, ShapeKind, TEXT_FONT_FAMILY, TEXT_FONT_SIZE,
    TEXT_LINE_HEIGHT, TextBody, index_after, list_boards,
};
pub use kanban::{Archive, Card, CardOrder, Kanban, KanbanFile, Lane, is_kanban};
pub use note::Note;
pub use preview::Attachment;
pub use vault::{BrokenLink, ScanJob, Vault, atomic_copy, board_name};
pub use wikilink::{WikiLink, heading_offset, headings, parse_links, unclosed_link};

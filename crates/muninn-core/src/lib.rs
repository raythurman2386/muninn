//! The vault model behind Muninn.
//!
//! A vault is a plain folder of Markdown files. [`Vault`] scans it, parses the
//! `[[wikilinks]]` in each note (skipping code spans and fenced blocks), and builds the
//! outgoing and backlink indexes the rest of the app reads.
//!
//! Whiteboards are `.excalidraw` files ([`Board`]). They live in the vault
//! but are not notes: [`Vault::boards`] lists them separately.
//!
//! This crate has no UI or runtime dependencies, so it is cheap to test and reusable from
//! tools, benchmarks and future front ends.

mod board;
mod note;
mod preview;
mod search;
mod vault;
mod wikilink;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use board::{
    BOARD_EXTENSION, Board, BoardFile, Element, MUNINN_SOURCE, SCENE_VERSION, Shape, ShapeKind,
    TextBody, list_boards,
};
pub use note::Note;
pub use preview::Attachment;
pub use vault::{BrokenLink, ScanJob, Vault, atomic_copy};
pub use wikilink::{WikiLink, heading_offset, headings, parse_links, unclosed_link};

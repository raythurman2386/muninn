//! The vault model behind Muninn.
//!
//! A vault is a plain folder of Markdown files. [`Vault`] scans it, parses the
//! `[[wikilinks]]` in each note (skipping code spans and fenced blocks), and builds the
//! outgoing and backlink indexes the rest of the app reads.
//!
//! This crate has no UI or runtime dependencies, so it is cheap to test and reusable from
//! tools, benchmarks and future front ends.

mod note;
mod preview;
mod search;
mod vault;
mod wikilink;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use note::Note;
pub use vault::Vault;
pub use wikilink::{WikiLink, parse_links};

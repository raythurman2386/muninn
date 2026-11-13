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
mod frontmatter;
mod kanban;
mod note;
mod preview;
mod search;
mod tags;
mod vault;
mod wikilink;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use board::{
    BOARD_EXTENSION, Board, BoardFile, Change, Element, HISTORY_BYTES, HISTORY_LIMIT, History,
    IdSource, MUNINN_SOURCE, SCENE_VERSION, Shape, ShapeKind, TEXT_FONT_FAMILY, TEXT_FONT_SIZE,
    TEXT_LINE_HEIGHT, TextBody, index_after, list_boards,
};
pub use frontmatter::{Frontmatter, FrontmatterValue};
pub use kanban::{
    ARCHIVE_DATE_FORMAT, ARCHIVE_DATE_POSITION, ARCHIVE_DATE_SEPARATOR, ARCHIVE_WITH_DATE, Archive,
    ArchiveDatePosition, ArchiveSettings, Card, CardOrder, CardText, ChecklistItem, CivilDate,
    CivilTime, DATE_DISPLAY_FORMAT, DATE_FORMAT, DATE_TRIGGER, DateKind, DateSettings, DateToken,
    HIDE_DATE_IN_TITLE, HIDE_TAGS_IN_TITLE, Kanban, KanbanDefaults, KanbanFile,
    LINK_DATE_TO_DAILY_NOTE, LINKED_PAGE_METADATA, Lane, MAX_ARCHIVE_CARDS, MetadataCache,
    MetadataEntry, MetadataField, MetadataRow, MetadataSettings, SHOW_MISSING_METADATA,
    SHOW_RELATIVE_DATE, SHOW_TAGS_AT_BOTTOM, TAG_COLORS, TIME_FORMAT, TIME_TRIGGER, TagColor,
    TagSettings, TagToken, card_checklist, card_metadata, first_linked_note, format_date,
    format_time, in_code_at, inline_fields, is_kanban, is_tag_char, is_tag_text, metadata_fields,
    metadata_rows, note_metadata, scan_card_text, scan_tags, tag_colors,
};
pub use note::Note;
pub use preview::Attachment;
pub use tags::{note_tags, tag_key, unclosed_tag};
pub use vault::{BrokenLink, ScanJob, TagEntry, Vault, atomic_copy, atomic_write, board_name};
pub use wikilink::{
    WikiLink, block_anchors, block_offset, heading_offset, headings, jump_offset, parse_links,
    unclosed_link,
};

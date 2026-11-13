//! The archive (slice 12): moving cards off the board and back.
//!
//! The archive is the final section of the note: a `***` rule, then a level-2
//! heading with the archive word (`docs/kanban-format.md` sections 6 and 8).
//! An archive card is the same task-list item as a lane card, so moving one is
//! a byte splice: the card's lines leave their lane and arrive in the archive
//! whole, passthrough text stays where it is, and the file's own archive word
//! is kept. Only the first archive creates the rule and heading, and it writes
//! the English word (Q8).
//!
//! Settings live in the footer, then the app-wide `kanban_defaults`, then the
//! built-in value (section 9): `archive-with-date`, `archive-date-format`,
//! `archive-date-position`, `archive-date-separator` and `max-archive-cards`.
//! The timestamp is ordinary card text written through slice 9's formatter,
//! never a `@{…}` token. `max-archive-cards` removes the oldest archive cards
//! after an append and never touches a lane card; absent or `0` is unlimited.

use std::io;

use serde_json::Value;

use super::dates::{CivilDate, KanbanDefaults, format_date};
use super::edit::{insert_block, last_text_line_end, leading_indented_end, splice};
use super::parse::split_block_id;
use super::{Kanban, invalid_data, invalid_input};

/// The footer key: add the date to a card when it is archived.
pub const ARCHIVE_WITH_DATE: &str = "archive-with-date";
/// The footer key: how that date is written (slice 9's token language).
pub const ARCHIVE_DATE_FORMAT: &str = "archive-date-format";
/// The footer key: `start` or `end` of the card text.
pub const ARCHIVE_DATE_POSITION: &str = "archive-date-position";
/// The footer key: text between the card and its date.
pub const ARCHIVE_DATE_SEPARATOR: &str = "archive-date-separator";
/// The footer key: how many archive cards to keep. Absent or `0` is no limit.
pub const MAX_ARCHIVE_CARDS: &str = "max-archive-cards";

/// The archive word a new section is written with. An existing section keeps
/// its own word.
const ENGLISH_ARCHIVE_WORD: &str = "Archive";
/// The built-in `archive-date-format`.
const DEFAULT_ARCHIVE_DATE_FORMAT: &str = "YYYY-MM-DD";
/// The built-in `archive-date-separator`.
const DEFAULT_ARCHIVE_DATE_SEPARATOR: &str = " ";

/// Where the archived card's date goes in its text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ArchiveDatePosition {
    /// Before the card's text.
    Start,
    /// After the card's text (the built-in).
    #[default]
    End,
}

impl ArchiveDatePosition {
    /// `start` / `end`, as written in the footer. An unknown value is not a
    /// position.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.trim().eq_ignore_ascii_case("start") {
            Some(Self::Start)
        } else if text.trim().eq_ignore_ascii_case("end") {
            Some(Self::End)
        } else {
            None
        }
    }

    /// The value written to the footer.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
        }
    }
}

/// The archive settings a board resolves: footer, app defaults, built-in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ArchiveSettings {
    pub with_date: bool,
    pub date_format: String,
    pub date_position: ArchiveDatePosition,
    /// Text between the card and its date; empty writes them together.
    pub date_separator: String,
    /// Oldest archive cards beyond this many go; `None` is unlimited.
    pub max_cards: Option<usize>,
}

impl Default for ArchiveSettings {
    fn default() -> Self {
        Self {
            with_date: false,
            date_format: DEFAULT_ARCHIVE_DATE_FORMAT.to_owned(),
            date_position: ArchiveDatePosition::End,
            date_separator: DEFAULT_ARCHIVE_DATE_SEPARATOR.to_owned(),
            max_cards: None,
        }
    }
}

impl ArchiveSettings {
    /// Footer, then app defaults, then the built-in value.
    #[must_use]
    pub fn resolve(footer_json: Option<&str>, defaults: &KanbanDefaults) -> Self {
        let mut settings = defaults.to_archive_settings();
        if let Some(value) = footer_json.and_then(|json| serde_json::from_str::<Value>(json).ok()) {
            settings.apply_value(&value);
        }
        settings.trim_format();
        settings
    }

    /// Read the keys this type owns from one JSON object. A missing key and a
    /// value of the wrong type leave the current value; an unknown
    /// `archive-date-position` keeps the current position.
    pub fn apply_value(&mut self, value: &Value) {
        if let Some(flag) = value.get(ARCHIVE_WITH_DATE).and_then(Value::as_bool) {
            self.with_date = flag;
        }
        if let Some(format) = value
            .get(ARCHIVE_DATE_FORMAT)
            .and_then(Value::as_str)
            .filter(|format| !format.is_empty())
        {
            self.date_format = format.to_owned();
        }
        if let Some(position) = value
            .get(ARCHIVE_DATE_POSITION)
            .and_then(Value::as_str)
            .and_then(ArchiveDatePosition::parse)
        {
            self.date_position = position;
        }
        // An empty separator is a real value: the date is written next to
        // the text.
        if let Some(separator) = value.get(ARCHIVE_DATE_SEPARATOR).and_then(Value::as_str) {
            self.date_separator = separator.to_owned();
        }
        if let Some(max) = value.get(MAX_ARCHIVE_CARDS).and_then(Value::as_u64) {
            self.max_cards = usize::try_from(max).ok().filter(|n| *n > 0);
        }
    }

    /// The timestamp for `date`, without a separator.
    #[must_use]
    pub fn stamp(&self, date: CivilDate) -> String {
        format_date(date, &self.date_format)
    }

    fn trim_format(&mut self) {
        let trimmed = self.date_format.trim();
        if trimmed.is_empty() {
            self.date_format = DEFAULT_ARCHIVE_DATE_FORMAT.to_owned();
        } else {
            self.date_format = trimmed.to_owned();
        }
    }
}

impl Kanban {
    /// This board's archive settings from its footer and the built-in values.
    /// A caller with app-wide defaults resolves them itself
    /// ([`ArchiveSettings::resolve`]) and uses [`Kanban::archive_card_with`].
    #[must_use]
    pub fn archive_settings(&self) -> ArchiveSettings {
        ArchiveSettings::resolve(self.footer_json(), &KanbanDefaults::default())
    }

    /// Move card `index` of `lane` into the archive, whole.
    ///
    /// The archive is created (a `***` line and `## Archive`) when the note has
    /// none. This method writes no timestamp because core has no clock, even
    /// when `archive-with-date` is set; a caller that knows the local day uses
    /// [`Kanban::archive_card_with`]. `max-archive-cards` then drops the oldest
    /// archive cards. Nothing a lane holds is touched, every line the move does
    /// not own stays where it is, and a note whose archive would land inside an
    /// unclosed code fence is refused.
    pub fn archive_card(&mut self, lane: usize, index: usize) -> io::Result<()> {
        self.archive_card_with(lane, index, &self.archive_settings(), None)
    }

    /// [`Kanban::archive_card`] with the settings the caller resolved and the
    /// local date to stamp (`None` writes no date even when the setting asks
    /// for one).
    pub fn archive_card_with(
        &mut self,
        lane: usize,
        index: usize,
        settings: &ArchiveSettings,
        today: Option<CivilDate>,
    ) -> io::Result<()> {
        if self.archive.is_none() && self.ends_in_fence {
            return Err(invalid_data("the note ends inside an unclosed code fence"));
        }
        let card = self
            .lanes
            .get(lane)
            .and_then(|lane| lane.cards.get(index))
            .ok_or_else(|| invalid_input("no such card"))?;
        let span = card.span.clone();
        let text = card.text.clone();
        let mark = card.mark;
        let block_id = card.block_id.clone();
        let mut block = self.source[span.clone()].to_owned();
        let mut expected = text.clone();
        if let Some(date) = today.filter(|_| settings.with_date) {
            let stamp = archive_stamp(self, card, settings, date);
            block.insert_str(stamp.at - span.start, &stamp.insert);
            expected = stamp.text;
        }
        // Take the card out first, then find the archive in what is left, so
        // the indices of the archive's own cards do not shift with the lane.
        let removed = splice(&self.source, span, "");
        let without = Kanban::parse(&removed)?;
        let next = append_archive_card(&without, block)?;
        let mut parsed = Kanban::parse(&next)?;
        let archived = parsed
            .archive
            .as_ref()
            .and_then(|archive| archive.cards.last())
            .ok_or_else(|| invalid_data("the card did not reach the archive"))?;
        if archived.text != expected || archived.mark != mark || archived.block_id != block_id {
            return Err(invalid_data("the card did not reach the archive whole"));
        }
        trim_archive(&mut parsed, settings.max_cards)?;
        *self = parsed;
        Ok(())
    }

    /// Move archive card `index` to the end of `lane`, whole. Its text, checkbox
    /// character and block id are kept byte for byte. The archive section stays
    /// even when it becomes empty.
    pub fn unarchive_card(&mut self, index: usize, lane: usize) -> io::Result<()> {
        let card = self
            .archive
            .as_ref()
            .and_then(|archive| archive.cards.get(index))
            .ok_or_else(|| invalid_input("no such archived card"))?;
        let span = card.span.clone();
        let text = card.text.clone();
        let mark = card.mark;
        let block_id = card.block_id.clone();
        let block = self.source[span.clone()].to_owned();
        let removed = splice(&self.source, span, "");
        let without = Kanban::parse(&removed)?;
        let len = without
            .lanes
            .get(lane)
            .ok_or_else(|| invalid_input("no such lane"))?
            .cards
            .len();
        let at = without.insert_offset(lane, len)?;
        let next = insert_block(&removed, at, block, without.nl);
        let parsed = Kanban::parse(&next)?;
        let restored = parsed
            .lanes
            .get(lane)
            .and_then(|lane| lane.cards.get(len))
            .ok_or_else(|| invalid_data("the card did not return to the lane"))?;
        if restored.text != text || restored.mark != mark || restored.block_id != block_id {
            return Err(invalid_data("the card did not return to the lane whole"));
        }
        *self = parsed;
        Ok(())
    }

    /// Remove archive card `index` from the note. Its lines are the only ones
    /// that change; the archive section stays even when it becomes empty.
    pub fn delete_archived_card(&mut self, index: usize) -> io::Result<()> {
        let span = self
            .archive
            .as_ref()
            .and_then(|archive| archive.cards.get(index))
            .ok_or_else(|| invalid_input("no such archived card"))?
            .span
            .clone();
        *self = Kanban::parse(&splice(&self.source, span, ""))?;
        Ok(())
    }
}

/// The date text for `card`, where it goes in the note, and the card text the
/// write is expected to read back.
struct Stamp {
    insert: String,
    at: usize,
    text: String,
}

fn archive_stamp(
    board: &Kanban,
    card: &super::Card,
    settings: &ArchiveSettings,
    date: CivilDate,
) -> Stamp {
    let stamp = settings.stamp(date);
    let at = first_text_at(board, card);
    let separator = settings.date_separator.as_str();
    if first_line_text_len(board, card) == 0 {
        // `- [ ]` needs the space after its bracket back before the first
        // text, or the line is no longer a card. A card that already has one
        // (a trailing space, or the space before a block id) keeps it, so the
        // stamp is not given a second. The separator is dropped: there is no
        // text to separate the stamp from, and a continuation line is left
        // as it was.
        let insert = if at > 0 && board.source.as_bytes()[at - 1] == b' ' {
            stamp.clone()
        } else {
            format!(" {stamp}")
        };
        return Stamp {
            insert,
            at,
            text: stamped_text(&card.text, &settings.date_position, &stamp, separator),
        };
    }
    let (insert, at) = match settings.date_position {
        ArchiveDatePosition::Start => (format!("{stamp}{separator}"), at),
        ArchiveDatePosition::End => (
            format!("{separator}{stamp}"),
            at + first_line_text_len(board, card),
        ),
    };
    Stamp {
        insert,
        at,
        text: stamped_text(&card.text, &settings.date_position, &stamp, separator),
    }
}

/// Byte offset of the first line's text (after `- [x] `) in the note.
fn first_text_at(board: &Kanban, card: &super::Card) -> usize {
    let after = card.mark_at + card.mark.len_utf8() + 1; // past `]`
    if board.source.as_bytes().get(after) == Some(&b' ') {
        after + 1
    } else {
        after
    }
}

/// The length of the first line's text, without a block id and without any
/// space before it.
fn first_line_text_len(board: &Kanban, card: &super::Card) -> usize {
    let start = first_text_at(board, card);
    let span = &board.source[card.span.start..card.span.end];
    let line_end = span
        .find('\n')
        .map_or(card.span.end, |at| card.span.start + at);
    let line_end = if line_end > start && board.source.as_bytes()[line_end - 1] == b'\r' {
        line_end - 1
    } else {
        line_end
    };
    let body = &board.source[start..line_end.max(start)];
    split_block_id(body).0.len()
}

/// The card text with the date added, as the byte splice produced it.
fn stamped_text(
    text: &str,
    position: &ArchiveDatePosition,
    stamp: &str,
    separator: &str,
) -> String {
    let (first, rest) = match text.split_once('\n') {
        Some((first, rest)) => (first, Some(rest)),
        None => (text, None),
    };
    // An empty first line has no text for the separator to stand between: the
    // stamp alone is the first line, and a continuation line stays whole.
    let first = if first.is_empty() {
        stamp.to_owned()
    } else {
        match position {
            ArchiveDatePosition::Start => format!("{stamp}{separator}{first}"),
            ArchiveDatePosition::End => format!("{first}{separator}{stamp}"),
        }
    };
    match rest {
        Some(rest) => format!("{first}\n{rest}"),
        None => first,
    }
}

/// Put `block` at the end of the archive, creating the section (and its
/// heading, in English) when the note has none.
fn append_archive_card(board: &Kanban, block: String) -> io::Result<String> {
    if let Some(archive) = &board.archive {
        return Ok(archive_insert_offset(board, archive, block));
    }
    let at = board
        .footer
        .as_ref()
        .map_or(board.source.len(), |footer| footer.span.start);
    let nl = board.nl;
    let mut section = String::new();
    // A blank line separates the last lane from the rule when the file does
    // not already hold one before the footer.
    if !board.source[..at].ends_with(&format!("{nl}{nl}")) {
        section.push_str(nl);
    }
    section.push_str("***");
    section.push_str(nl);
    section.push_str(nl);
    section.push_str("## ");
    section.push_str(ENGLISH_ARCHIVE_WORD);
    section.push_str(nl);
    section.push_str(nl);
    section.push_str(&block);
    if at < board.source.len() {
        section.push_str(nl);
    }
    Ok(insert_block(&board.source, at, section, nl))
}

/// Where a new card goes in an existing archive: after its last card, else
/// under its heading. A blank line after the heading stays between the heading
/// and the card.
///
/// An archive with no cards can still hold passthrough text, as a lane can.
/// Column-0 text (a paragraph) stays above the first card, and indented text
/// stays above it too: a card put directly above either would take it as a
/// continuation line. Both rules are the ones an empty lane follows in
/// `insert_offset`.
fn archive_insert_offset(board: &Kanban, archive: &super::Archive, block: String) -> String {
    if let Some(card) = archive.cards.last() {
        return insert_block(&board.source, card.span.end, block, board.nl);
    }
    let after = archive.heading_line.end;
    let body = &board.source[after..archive.span.end];
    if let Some(end) = last_text_line_end(body) {
        let at = after + end;
        // A newly created archive puts a blank line before its first card;
        // keep that here too, so the card does not run into the paragraph.
        let block = if board.source[..at].ends_with(&format!("{}{}", board.nl, board.nl)) {
            block
        } else {
            format!("{}{block}", board.nl)
        };
        return insert_block(&board.source, at, block, board.nl);
    }
    // Nothing at column 0. A blank line right under the heading comes first,
    // then any indented text.
    let blank = body.starts_with(board.nl);
    let content = after + if blank { board.nl.len() } else { 0 };
    let leading = &board.source[content..archive.span.end];
    if let Some(end) = leading_indented_end(leading) {
        return insert_block(&board.source, content + end, block, board.nl);
    }
    // Only the heading and its blank line are there: keep the blank line
    // between the heading and the card, as a newly created archive has it.
    let block = if blank {
        format!("{}{block}", board.nl)
    } else {
        block
    };
    insert_block(&board.source, after, block, board.nl)
}

/// Drop the oldest archive cards while the archive holds more than `max`.
/// Lane cards are never read or written here.
fn trim_archive(board: &mut Kanban, max: Option<usize>) -> io::Result<()> {
    let Some(max) = max.filter(|n| *n > 0) else {
        return Ok(());
    };
    loop {
        let Some(span) = board
            .archive
            .as_ref()
            .filter(|archive| archive.cards.len() > max)
            .map(|archive| archive.cards[0].span.clone())
        else {
            return Ok(());
        };
        *board = Kanban::parse(&splice(&board.source, span, ""))?;
    }
}

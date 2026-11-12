//! Kanban notes stored as Markdown.
//!
//! A kanban note is still a note: it stays in the vault index, so wikilinks,
//! backlinks, search and rename behave as they do for any other `.md` file.
//! [`is_kanban`] recognises one by the `kanban-plugin` frontmatter key. The
//! format itself is `docs/kanban-format.md`.
//!
//! # Preservation
//!
//! [`Kanban::parse`] keeps the original bytes. Writing the model back without
//! an edit returns those bytes unchanged: line endings, a byte-order mark, a
//! missing final newline, indentation and blank-line runs. An edit splices
//! only the spans it is about and new lines use the file's own line-ending
//! style. Lines the model does not understand (a paragraph between lanes, a
//! heading that is not a lane, a list item with no checkbox, an unknown
//! done-lane word) stay in place as passthrough.
//!
//! A settings footer that is unterminated, or whose JSON does not parse as an
//! object, is [`std::io::ErrorKind::InvalidData`]. The parse does not succeed
//! with an empty board, and nothing here writes the file.
//!
//! # Open points
//!
//! Where the format doc says **Open point**, the behaviour is the safe one
//! stated there: keep the text, and do not invent a meaning. In particular a
//! lane limit of `0` is no limit and is never written, a done mark other than
//! `x` is left as it is until toggled, and unknown footer keys are kept.

mod edit;
mod parse;

#[cfg(test)]
mod tests;

use std::io::{self, ErrorKind};
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::vault::{Vault, atomic_write};

/// A `.md` note whose frontmatter marks it as a kanban.
///
/// True when a leading frontmatter block (optionally behind a UTF-8
/// byte-order mark) contains the key `kanban-plugin`. The value, quoting,
/// other keys and the line ending do not matter. This does not parse the
/// board: a kanban note with a broken settings footer is still detected, and
/// [`Kanban::parse`] then returns an error.
#[must_use]
pub fn is_kanban(source: &str) -> bool {
    parse::has_kanban_key(source)
}

/// One kanban note, parsed from the bytes it was read from.
///
/// The owned source is those bytes. Edits replace spans inside a copy and
/// re-parse, so [`Kanban::source`] is always a faithful serialisation.
#[derive(Clone, Debug)]
pub struct Kanban {
    source: String,
    /// Line ending used for lines an edit adds. `"\n"` or `"\r\n"`.
    nl: &'static str,
    lanes: Vec<Lane>,
    archive: Option<Archive>,
    footer: Option<Footer>,
    /// The file ends inside a column-0 code fence that never closed.
    ends_in_fence: bool,
}

/// A column. `written_limit` is the number in the title suffix, including
/// `0`; [`Lane::limit`] reports `0` as no limit.
#[derive(Clone, Debug)]
pub struct Lane {
    title: String,
    written_limit: Option<u32>,
    /// The word inside the done-lane marker (`Complete`, `完了`, …), when
    /// the lane has one this parser recognises.
    done_word: Option<String>,
    cards: Vec<Card>,
    span: Range<usize>,
    title_line: Range<usize>,
    title_span: Range<usize>,
    /// The digits of a `(N)` suffix, not the parentheses.
    limit_number: Option<Range<usize>>,
    /// The whole ` (N)` suffix, including the leading space.
    limit_suffix: Option<Range<usize>>,
    /// End of the marker line (after its newline), when there is one.
    marker_end: Option<usize>,
}

/// One task-list card. `text` is the markdown after the checkbox, without the
/// block id. Continuation lines are stored without the indent `render_card`
/// adds back. A space run wider than two columns keeps two as that indent, so
/// the extra columns stay in the text. A lead stays in the text only when it
/// is at least as wide as that indent and is not a longer copy of it.
#[derive(Clone, Debug)]
pub struct Card {
    text: String,
    mark: char,
    block_id: Option<String>,
    span: Range<usize>,
    mark_at: usize,
    /// Whitespace `render_card` adds back, in the same columns the reader uses.
    /// A wider different lead is not given another copy.
    indent: Option<String>,
}

/// The archive section opened by `***` and an archive heading.
#[derive(Clone, Debug)]
pub struct Archive {
    word: String,
    cards: Vec<Card>,
    span: Range<usize>,
}

/// The settings footer. `json` is the raw JSON line, not a re-serialisation.
#[derive(Clone, Debug)]
struct Footer {
    span: Range<usize>,
    json_span: Range<usize>,
}

/// A kanban note found in a vault. It is also an ordinary note in
/// [`Vault::notes`](crate::Vault::notes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KanbanFile {
    pub path: PathBuf,
    /// Vault-relative path with `/` separators.
    pub rel: String,
    /// The file stem, the same title the note index uses.
    pub title: String,
}

impl Kanban {
    /// Parse `source`. The returned model writes back the same bytes until an
    /// edit splices them.
    pub fn parse(source: &str) -> io::Result<Self> {
        parse::parse(source)
    }

    /// The note as it would be written, including every passthrough byte.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Lanes in file order. An `Archive` heading with no `***` before it is a
    /// lane here, not an archive.
    #[must_use]
    pub fn lanes(&self) -> &[Lane] {
        &self.lanes
    }

    /// The archive, when the file has a `***` line followed by an archive
    /// heading.
    #[must_use]
    pub fn archive(&self) -> Option<&Archive> {
        self.archive.as_ref()
    }

    /// The settings footer's JSON line, exactly as it appears in the file.
    #[must_use]
    pub fn footer_json(&self) -> Option<&str> {
        self.footer
            .as_ref()
            .map(|footer| &self.source[footer.json_span.clone()])
    }

    /// Write the note atomically through the vault's temp-file rename.
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        atomic_write(path.as_ref(), self.source.as_bytes())
    }
}

impl Lane {
    /// The lane title, without a ` (N)` limit suffix.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The lane limit. A written `0`, and a title with no suffix, are both
    /// no limit.
    #[must_use]
    pub fn limit(&self) -> Option<u32> {
        self.written_limit.filter(|&n| n > 0)
    }

    /// The number in the title's ` (N)` suffix, including `0`. `None` when
    /// the title has no suffix.
    #[must_use]
    pub fn written_limit(&self) -> Option<u32> {
        self.written_limit
    }

    /// The done-lane word (`Complete`, `Fertiggestellt`, …) without the
    /// surrounding `**`.
    #[must_use]
    pub fn done_word(&self) -> Option<&str> {
        self.done_word.as_deref()
    }

    /// Cards in file order. List items without a checkbox are not cards.
    #[must_use]
    pub fn cards(&self) -> &[Card] {
        &self.cards
    }
}

impl Card {
    /// Card markdown. The first line has no checkbox and no block id;
    /// later lines do not repeat the file's continuation indent.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The single character between the checkbox brackets. A space is open.
    #[must_use]
    pub fn mark(&self) -> char {
        self.mark
    }

    /// True when [`Card::mark`] is anything other than a space.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.mark != ' '
    }

    /// The block id (`card-7f2` for a card ending ` ^card-7f2`), without the
    /// caret.
    #[must_use]
    pub fn block_id(&self) -> Option<&str> {
        self.block_id.as_deref()
    }
}

impl Archive {
    /// The archive heading word (`Archive`, `Archiv`, `アーカイブ`, …).
    #[must_use]
    pub fn word(&self) -> &str {
        &self.word
    }

    /// Archived cards, in file order.
    #[must_use]
    pub fn cards(&self) -> &[Card] {
        &self.cards
    }
}

impl Vault {
    /// Kanban notes in this vault, in the note index's path order.
    ///
    /// Detection is [`is_kanban`] on each note's source. A note that detects
    /// but does not parse (a broken footer) is still listed: it remains a
    /// note either way.
    #[must_use]
    pub fn kanbans(&self) -> Vec<KanbanFile> {
        self.notes
            .iter()
            .filter(|note| is_kanban(&note.source))
            .map(|note| KanbanFile {
                path: note.path.clone(),
                rel: note.rel.clone(),
                title: note.title.clone(),
            })
            .collect()
    }
}

fn invalid_data(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

fn invalid_input(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, message)
}

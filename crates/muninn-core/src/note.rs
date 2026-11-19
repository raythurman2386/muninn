//! One Markdown file and the links indexed from it.

use std::fmt;
use std::path::PathBuf;

use zeroize::Zeroizing;

/// A locked note's plaintext, kept in this process only.
///
/// The wrapper exists for its [`fmt::Debug`]: [`Note`] derives `Debug`, and a
/// plain `String` would print the body in logs, panic messages and failing
/// test output. The inner buffer is zeroized when the last copy drops.
#[derive(Clone)]
pub(crate) struct UnlockedSource(Zeroizing<String>);

impl fmt::Debug for UnlockedSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UnlockedSource(<redacted>)")
    }
}

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
    /// Tags in the frontmatter and body, unique by case-insensitive key in
    /// file order. The first spelling seen is kept for display; filled by
    /// [`crate::Vault::reindex`].
    pub tags: Vec<String>,
    /// [`crate::is_kanban`] of [`Note::readable_source`], cached by
    /// [`crate::Vault::reindex`] so a frame never re-detects it.
    pub(crate) kanban: bool,
    /// [`crate::note_lock::is_locked`] of `source`, cached by
    /// [`crate::Vault::reindex`] so a frame never re-checks it.
    pub(crate) locked: bool,
    /// A locked note's plaintext while it is unlocked in this session. Never
    /// serialized and never written to disk: [`Note::source`] stays the
    /// envelope, and [`Note::readable_source`] is the only reader for content
    /// derived from the body. Dropped when the session ends.
    ///
    /// Crate-internal (and deliberately not public API): the vault is the only
    /// writer, through [`Note::set_unlocked_source`], because the index must be
    /// rebuilt in the same step.
    pub(crate) unlocked_source: Option<UnlockedSource>,
}

impl Note {
    /// Whether this note is a kanban note (its frontmatter has
    /// `kanban-plugin`), as of the last index rebuild.
    #[must_use]
    pub fn is_kanban(&self) -> bool {
        self.kanban
    }

    /// Whether this note's source is a locked-note envelope, as of the last
    /// index rebuild. A cheap cached prefix check: it never derives a key and
    /// never decrypts, so a caller may gate on it on every frame.
    #[must_use]
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Tags in this note, as of the last index rebuild.
    #[must_use]
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    /// The text this note's content is read from, when there is any to read.
    ///
    /// An ordinary note is its own file's text. A locked note is the envelope
    /// on disk, so it has no readable content until
    /// [`crate::Vault::set_unlocked_source`] supplies the session overlay;
    /// without one this is `None` and every body-derived reader (search body,
    /// tags, links, snippets, kanban) must act as if the note were empty.
    #[must_use]
    pub fn readable_source(&self) -> Option<&str> {
        if !self.locked {
            return Some(&self.source);
        }
        self.unlocked_source.as_ref().map(|text| text.0.as_str())
    }

    /// Whether this note is locked on disk *and* has its plaintext in this
    /// session's overlay. False for a plain note and for a locked note that
    /// has not been unlocked here.
    #[must_use]
    pub fn is_unlocked(&self) -> bool {
        self.locked && self.unlocked_source.is_some()
    }

    /// Set or drop the session overlay. Crate-internal: the vault owns the
    /// index rebuild that must follow it.
    pub(crate) fn set_unlocked_source(&mut self, text: Option<String>) {
        self.unlocked_source = text.map(|text| UnlockedSource(Zeroizing::new(text)));
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

    /// The note id a locked-note envelope binds: [`self.rel`](Self::rel)
    /// without one trailing `.md` (e.g. `Projects/Trip`). Pass this to
    /// [`crate::lock`], [`crate::unlock`] and [`crate::unlock_session`].
    #[must_use]
    pub fn lock_id(&self) -> &str {
        crate::note_lock::note_id(&self.rel)
    }
}

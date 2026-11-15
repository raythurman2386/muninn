//! The `en-US` catalog.
//!
//! This exhaustive `match` is the only place English copy is pinned. Adding a
//! [`Key`] without a row here is a compile error, so a missing message cannot
//! reach a draw call.

use crate::key::Key;

/// The English text for `key`.
///
/// Private: callers go through [`crate::t`] or [`crate::t_in`], which pick the
/// language. Keeping the only catalog behind those functions is what makes a
/// second language a change to this crate rather than to call sites.
pub(crate) fn text(key: Key) -> &'static str {
    match key {
        Key::AddCard => "Add card",
        Key::Archive => "Archive",
        Key::Today => "Today",
        Key::OverLimit => "Over limit",
        Key::KeyboardShortcuts => "Keyboard shortcuts",
        Key::Close => "Close",
        Key::Save => "Save",
        Key::Cancel => "Cancel",
        Key::LinkedNote => "Linked note",
        Key::NoBoards => "No boards in this vault yet",
    }
}

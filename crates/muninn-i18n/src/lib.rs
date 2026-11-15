//! English UI copy for Muninn, behind one seam.
//!
//! This crate is the catalog **seam only**: it holds the keys, the `en-US`
//! text, the parameterised copy that cannot be a single string, and the date
//! name tables. It ships no translation, changes no call site, and changes no
//! user-visible text. Nothing in the workspace depends on it yet.
//!
//! # Why a typed catalog
//!
//! Text is looked up by [`Key`], a `Copy` enum, not by a string id. The English
//! catalog is an exhaustive `match` (`en_us`), so adding a key without text
//! fails `cargo check`; a runtime string lookup would only fail when the
//! message was drawn. Every function here returns `&'static str` or an owned
//! `String`, so a migrated call site keeps today's signature and allocates
//! exactly as it does now. See `docs/i18n.md` §5.3 for the API and §5.4 for the
//! layering rule.
//!
//! # What is here, and what comes later
//!
//! - [`t`] / [`t_in`] resolve a key to text in the current [`Lang`]. Today the
//!   only language is `en-US`.
//! - [`files_skipped`], [`notes`] and [`relative_day`] are the escape hatch for
//!   parameterised copy: a view must not build a translated sentence with
//!   `format!`, so counts and relative dates get typed functions instead. They
//!   take no [`Key`].
//! - [`DateNames`] and [`EN_US`] carry month/weekday names, the first day of
//!   the week and `AM`/`PM`. The display-side date formatter, English ordinals
//!   and the file-versus-display split move in a later slice.
//!
//! Extraction of the call sites is the follow-up work, area by area, keeping
//! English output byte-identical. This crate is the target those changes land
//! in; it is a leaf with no dependencies so `muninn-core`, `muninn-ui` and
//! `muninn-app` can all point at it without an edge cycle.

mod dates;
mod en_us;
mod key;

pub use dates::{DateNames, EN_US};
pub use key::Key;

/// A language the UI can draw.
///
/// One variant today. This is the seam for a locale setting: when a setting
/// exists, it selects a `Lang`, and the catalogs keyed by it grow. There is no
/// setting, no OS-locale detection and no second catalog yet, and adding a
/// variant must not change what `en-US` renders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    /// English (United States): the only language Muninn ships.
    EnUs,
}

impl Lang {
    /// The language the UI is drawing in.
    ///
    /// Always [`Lang::EnUs`] today, because no locale setting exists. Draw
    /// code should call [`t`] and let this decide, rather than pinning a
    /// language at the call site, so a future setting only changes this
    /// function.
    pub const fn current() -> Lang {
        Lang::EnUs
    }
}

/// The text for `key` in the current UI language ([`Lang::current`]).
///
/// Returns `&'static str` so a migrated draw call keeps its existing
/// signature and does not allocate or handle a lookup failure.
///
/// # Examples
///
/// ```
/// use muninn_i18n::{t, Key};
///
/// assert_eq!(t(Key::AddCard), "Add card");
/// ```
pub fn t(key: Key) -> &'static str {
    t_in(Lang::current(), key)
}

/// The text for `key` in `lang`, ignoring the current UI language.
///
/// Use this in tests and in anything that deliberately renders a language
/// other than the UI's. Draw code should normally call [`t`].
///
/// # Examples
///
/// ```
/// use muninn_i18n::{t_in, Key, Lang};
///
/// assert_eq!(t_in(Lang::EnUs, Key::Save), "Save");
/// ```
pub fn t_in(lang: Lang, key: Key) -> &'static str {
    match lang {
        Lang::EnUs => en_us::text(key),
    }
}

/// `"1 file skipped"`, `"0 files skipped"`, `"2 files skipped"`.
///
/// Pins the phrasing of the status bar's skipped-file count. The count is
/// plain decimal, as today.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::files_skipped(1), "1 file skipped");
/// assert_eq!(muninn_i18n::files_skipped(0), "0 files skipped");
/// ```
pub fn files_skipped(n: usize) -> String {
    match n {
        1 => "1 file skipped".to_owned(),
        n => format!("{n} files skipped"),
    }
}

/// `"1 note"`, `"0 notes"`, `"2 notes"`.
///
/// Pins the noun phrase used to count notes; callers append anything else
/// themselves. The count is plain decimal, as today.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::notes(1), "1 note");
/// assert_eq!(muninn_i18n::notes(3), "3 notes");
/// ```
pub fn notes(n: usize) -> String {
    match n {
        1 => "1 note".to_owned(),
        n => format!("{n} notes"),
    }
}

/// `"today"`, `"tomorrow"`, `"yesterday"`, `"in 3 days"`, `"2 days ago"`.
///
/// `days` is the offset from today in days: positive is in the future,
/// negative is in the past. The wording is the card foot's relative-date
/// phrase, copied from `relative_days` in `muninn-ui`'s date picker so the two
/// cannot drift once the call site migrates.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::relative_day(0), "today");
/// assert_eq!(muninn_i18n::relative_day(-2), "2 days ago");
/// ```
pub fn relative_day(days: i64) -> String {
    match days {
        0 => "today".to_owned(),
        1 => "tomorrow".to_owned(),
        -1 => "yesterday".to_owned(),
        n if n > 0 => format!("in {n} days"),
        n => format!("{} days ago", -n),
    }
}

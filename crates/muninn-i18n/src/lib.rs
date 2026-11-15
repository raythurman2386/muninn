//! English UI copy for Muninn, behind one seam.
//!
//! This crate is the catalog **seam only**: it holds the keys, the `en-US`
//! text, the parameterised copy that cannot be a single string, and the date
//! name tables. It ships no translation and changes no user-visible text:
//! `muninn-core` and `muninn-ui` read it, and they pass [`EN_US`], so every
//! rendered string is byte-identical to the hardcoded one it replaced.
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
//! - [`files_skipped`], [`notes`], [`relative_day`], [`restore_to`],
//!   [`filter_by_tag`], [`tray_error`] and [`login_item_error`] are the escape
//!   hatch for parameterised copy: a view must not build a translated sentence
//!   with `format!`, so counts, relative dates, lane names, tags and OS errors
//!   get typed functions instead. They take no [`Key`].
//! - [`DateNames`] and [`EN_US`] carry month/weekday names, short weekday
//!   labels, the first day of the week and `AM`/`PM`. `muninn-core`'s
//!   `format_date_for_display`/`format_time_for_display` read them; the file
//!   formatters keep their own English table, and `Do` still spells an English
//!   ordinal until a locale pack supplies its own rule.
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

/// `"Restore to <lane>"`.
///
/// The archive page's lane picker names the lane a card goes back to. The
/// lane's title is file data and is kept as written, so the sentence is a
/// typed helper rather than a [`Key`], the same way [`files_skipped`] carries
/// its count.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::restore_to("To do"), "Restore to To do");
/// ```
pub fn restore_to(lane: &str) -> String {
    format!("Restore to {lane}")
}

/// `"Filter by #<tag>"`.
///
/// A card's tag chip is a button that filters the board by that tag. The tag
/// itself is file data and is kept as written, so the sentence is a typed
/// helper rather than a [`Key`].
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::filter_by_tag("x"), "Filter by #x");
/// ```
pub fn filter_by_tag(tag: &str) -> String {
    format!("Filter by #{tag}")
}

/// `"The tray icon could not be added: <error>"`.
///
/// The Appearance tab's tray status line: the sentence is fixed and the OS
/// error is kept as written at its end, so it is a typed helper rather than a
/// [`Key`], the same way [`files_skipped`] carries its count. `error` is never
/// translated.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::tray_error("no dock"),
///     "The tray icon could not be added: no dock"
/// );
/// ```
pub fn tray_error(error: &str) -> String {
    format!("The tray icon could not be added: {error}")
}

/// `"The login item could not be changed: <error>"`.
///
/// The Appearance tab's start-at-login status line, with the OS error kept as
/// written at the end of the sentence.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::login_item_error("permission denied"),
///     "The login item could not be changed: permission denied"
/// );
/// ```
pub fn login_item_error(error: &str) -> String {
    format!("The login item could not be changed: {error}")
}

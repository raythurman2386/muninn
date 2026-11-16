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
//!   [`filter_by_tag`], [`board_name_taken`], [`no_note_named`],
//!   [`linked_note_hint`], [`tray_error`] and [`login_item_error`] are the
//!   escape hatch for parameterised copy: a view must not build a translated
//!   sentence with `format!`, so counts, relative dates, lane names, tags,
//!   board and note names and OS errors get typed functions instead. They take
//!   no [`Key`]. The status, recovery, Broken-links, trash and tray surfaces
//!   add [`deleted`], [`scan_ms`], [`recovered_for`], [`recovered_copies`],
//!   [`more_edits`], [`targets`], [`referrers`], [`status_counts`],
//!   [`trash_summary`], [`trash_empty`] and [`sync_status`] the same way.
//!   The status bar's errors, its drop and link notices and the two conflict
//!   bars (note and page) add the [`could_not_save`]-style sentences,
//!   [`links_not_updated`], [`import_notice`], [`board_quit_refused`] and
//!   [`scan_cancelled`]; their static copy — the vault-path prompts, the
//!   switch/restore refusals, the conflict labels — is keyed (`error.*`,
//!   `conflict.*`, `status.*`).
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

/// `"A board named “<name>” is already in this folder"`.
///
/// The Board rename prompt's conflict line: the sentence is fixed and the
/// board name is file data, kept as written inside the English curly quotes,
/// so it is a typed helper rather than a [`Key`], the same way [`restore_to`]
/// carries its lane.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::board_name_taken("Plan"),
///     "A board named “Plan” is already in this folder"
/// );
/// ```
pub fn board_name_taken(name: &str) -> String {
    format!("A board named \u{201c}{name}\u{201d} is already in this folder")
}

/// `"No note named “<typed>” in this vault"`.
///
/// The Board note-link prompt's refusal: the typed note name or path is file
/// data and stays as written inside the English curly quotes, so the sentence
/// is a typed helper rather than a [`Key`].
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::no_note_named("Plan"),
///     "No note named “Plan” in this vault"
/// );
/// ```
pub fn no_note_named(typed: &str) -> String {
    format!("No note named \u{201c}{typed}\u{201d} in this vault")
}

/// `"Linked note: <label> · ⌘Enter opens"`.
///
/// The hint above the Board canvas while a linked element is selected. The
/// label is the note's title (or its stored target when the note is missing),
/// which is file data kept as written, so the sentence is a typed helper
/// rather than a [`Key`].
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::linked_note_hint("Plan"),
///     "Linked note: Plan · ⌘Enter opens"
/// );
/// ```
pub fn linked_note_hint(label: &str) -> String {
    format!("Linked note: {label} · ⌘Enter opens")
}

/// `"1 <one>"` for one, `"<n> <many>"` otherwise.
///
/// The shared shape behind the counted phrases in this module. Private: a view
/// must pick a helper that names the thing it counts, not build a sentence of
/// its own.
fn counted(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// `"Deleted <label>"`.
///
/// The status bar's undo line after a note or board went to the trash. The
/// label is the deleted item's name, which is file data kept as written, so
/// the sentence is a typed helper rather than a [`Key`].
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::deleted("Plan.md"), "Deleted Plan.md");
/// ```
pub fn deleted(label: &str) -> String {
    format!("Deleted {label}")
}

/// `"scan <ms> ms"`, with the duration rounded to whole milliseconds.
///
/// The status bar's scan-timing readout. The number is data and is formatted
/// as today (`{:.0}`), so the sentence itself is a typed helper rather than a
/// [`Key`].
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::scan_ms(12.4), "scan 12 ms");
/// ```
pub fn scan_ms(ms: f64) -> String {
    format!("scan {ms:.0} ms")
}

/// `"Recovered unsaved edits for <note>"`.
///
/// The recovery banner's first line. `note` is the count phrase built by
/// [`recovered_copies`] (or anything the caller already phrased), so the fixed
/// sentence is a typed helper rather than a [`Key`].
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::recovered_for("1 note"),
///     "Recovered unsaved edits for 1 note"
/// );
/// ```
pub fn recovered_for(note: &str) -> String {
    format!("Recovered unsaved edits for {note}")
}

/// `"1 note"`, `"2 boards"`, `"1 note and 2 boards"`.
///
/// The recovery banner counts its copies by kind: notes only, boards only, or
/// both joined with `and`. The numbers are plain decimal, as today.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::recovered_copies(1, 0), "1 note");
/// assert_eq!(muninn_i18n::recovered_copies(0, 2), "2 boards");
/// assert_eq!(muninn_i18n::recovered_copies(1, 2), "1 note and 2 boards");
/// ```
pub fn recovered_copies(notes: usize, boards: usize) -> String {
    match (notes, boards) {
        (notes, 0) => counted(notes, "note", "notes"),
        (0, boards) => counted(boards, "board", "boards"),
        (notes, boards) => format!(
            "{} and {}",
            counted(notes, "note", "notes"),
            counted(boards, "board", "boards")
        ),
    }
}

/// `"+<n> more"`.
///
/// The recovery banner's overflow chip after its first three rows.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::more_edits(2), "+2 more");
/// ```
pub fn more_edits(n: usize) -> String {
    format!("+{n} more")
}

/// `"1 target"`, `"0 targets"`, `"2 targets"`.
///
/// The Broken-links panel's header count.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::targets(1), "1 target");
/// assert_eq!(muninn_i18n::targets(2), "2 targets");
/// ```
pub fn targets(n: usize) -> String {
    counted(n, "target", "targets")
}

/// `"1 referrer"`, `"0 referrers"`, `"2 referrers"`.
///
/// The Broken-links panel's per-target count.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::referrers(1), "1 referrer");
/// assert_eq!(muninn_i18n::referrers(2), "2 referrers");
/// ```
pub fn referrers(n: usize) -> String {
    counted(n, "referrer", "referrers")
}

/// `"12 words · 2 links · 0 backlinks"`.
///
/// The status bar's right-hand counts for the open note. Each count is plain
/// decimal and gets its own plural, as today.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::status_counts(12, 2, 0),
///     "12 words · 2 links · 0 backlinks"
/// );
/// ```
pub fn status_counts(words: usize, links: usize, backlinks: usize) -> String {
    format!(
        "{} · {} · {}",
        counted(words, "word", "words"),
        counted(links, "link", "links"),
        counted(backlinks, "backlink", "backlinks")
    )
}

/// `"Trash is empty"`.
///
/// The Help trash card's readout when nothing is in the trash. Zero has no
/// size to show, so it is a fixed sentence rather than a [`trash_summary`].
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::trash_empty(), "Trash is empty");
/// ```
pub fn trash_empty() -> &'static str {
    "Trash is empty"
}

/// `"Trash · 1 note · 2.0 KB"` or `"Trash · 3 notes · 1.4 MB"`.
///
/// The Help trash card's readout for a non-empty trash. `size` is the view's
/// already-formatted byte size (`trash_bytes`), so the sentence is a typed
/// helper rather than a [`Key`]. Callers use [`trash_empty`] when there is
/// nothing in the trash.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::trash_summary(1, "2.0 KB"), "Trash · 1 note · 2.0 KB");
/// assert_eq!(
///     muninn_i18n::trash_summary(3, "1.4 MB"),
///     "Trash · 3 notes · 1.4 MB"
/// );
/// ```
pub fn trash_summary(files: usize, size: &str) -> String {
    format!("Trash · {} · {size}", counted(files, "note", "notes"))
}

/// `"Sync: <label>"`.
///
/// The tray menu's disabled status row, and the tray's own label for the
/// current sync status. `label` is the trimmed status line the caller built,
/// so the prefix is a typed helper rather than a [`Key`].
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_status("Not connected."),
///     "Sync: Not connected."
/// );
/// ```
pub fn sync_status(label: &str) -> String {
    format!("Sync: {label}")
}

// ---- Errors, notices and conflict bars ------------------------------------
//
// Every sentence below is drawn by the status bar, the startup page or a
// conflict bar. Its fixed part lives here; the parameters are file data, a
// count or an OS error and are always kept as written. A call site names one
// helper per sentence instead of assembling copy of its own, the same way
// [`tray_error`] carries its OS error.

/// `<sentence>: <error>`, with the OS error kept as written.
fn with_error(sentence: &str, error: impl std::fmt::Display) -> String {
    format!("{sentence}: {error}")
}

/// `"Could not save: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_save("disk is full"),
///     "Could not save: disk is full"
/// );
/// ```
pub fn could_not_save(error: impl std::fmt::Display) -> String {
    with_error("Could not save", error)
}

/// `"Could not open the vault: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_open_vault("permission denied"),
///     "Could not open the vault: permission denied"
/// );
/// ```
pub fn could_not_open_vault(error: impl std::fmt::Display) -> String {
    with_error("Could not open the vault", error)
}

/// `"Could not read the vault: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_read_vault("no such folder"),
///     "Could not read the vault: no such folder"
/// );
/// ```
pub fn could_not_read_vault(error: impl std::fmt::Display) -> String {
    with_error("Could not read the vault", error)
}

/// `"Could not read note state: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_read_note_state("stale handle"),
///     "Could not read note state: stale handle"
/// );
/// ```
pub fn could_not_read_note_state(error: impl std::fmt::Display) -> String {
    with_error("Could not read note state", error)
}

/// `"Could not create the vault: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_create_vault("read-only folder"),
///     "Could not create the vault: read-only folder"
/// );
/// ```
pub fn could_not_create_vault(error: impl std::fmt::Display) -> String {
    with_error("Could not create the vault", error)
}

/// `"Could not persist kanban recovery copy: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_persist_kanban_recovery("no space"),
///     "Could not persist kanban recovery copy: no space"
/// );
/// ```
pub fn could_not_persist_kanban_recovery(error: impl std::fmt::Display) -> String {
    with_error("Could not persist kanban recovery copy", error)
}

/// `"Could not persist recovery copy: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_persist_recovery("no space"),
///     "Could not persist recovery copy: no space"
/// );
/// ```
pub fn could_not_persist_recovery(error: impl std::fmt::Display) -> String {
    with_error("Could not persist recovery copy", error)
}

/// `"Could not persist board recovery copy: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_persist_board_recovery("no space"),
///     "Could not persist board recovery copy: no space"
/// );
/// ```
pub fn could_not_persist_board_recovery(error: impl std::fmt::Display) -> String {
    with_error("Could not persist board recovery copy", error)
}

/// `"Could not read recovery copy: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_read_recovery("bad json"),
///     "Could not read recovery copy: bad json"
/// );
/// ```
pub fn could_not_read_recovery(error: impl std::fmt::Display) -> String {
    with_error("Could not read recovery copy", error)
}

/// `"Could not recreate note folder: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_recreate_note_folder("not a folder"),
///     "Could not recreate note folder: not a folder"
/// );
/// ```
pub fn could_not_recreate_note_folder(error: impl std::fmt::Display) -> String {
    with_error("Could not recreate note folder", error)
}

/// `"Could not recreate note: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_recreate_note("disk full"),
///     "Could not recreate note: disk full"
/// );
/// ```
pub fn could_not_recreate_note(error: impl std::fmt::Display) -> String {
    with_error("Could not recreate note", error)
}

/// `"Could not reload the note: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_reload_note("permission denied"),
///     "Could not reload the note: permission denied"
/// );
/// ```
pub fn could_not_reload_note(error: impl std::fmt::Display) -> String {
    with_error("Could not reload the note", error)
}

/// `"Could not save a copy: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_save_copy("disk full"),
///     "Could not save a copy: disk full"
/// );
/// ```
pub fn could_not_save_copy(error: impl std::fmt::Display) -> String {
    with_error("Could not save a copy", error)
}

/// `"Could not rename note: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_rename_note("name taken"),
///     "Could not rename note: name taken"
/// );
/// ```
pub fn could_not_rename_note(error: impl std::fmt::Display) -> String {
    with_error("Could not rename note", error)
}

/// `"Could not rename folder: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_rename_folder("name taken"),
///     "Could not rename folder: name taken"
/// );
/// ```
pub fn could_not_rename_folder(error: impl std::fmt::Display) -> String {
    with_error("Could not rename folder", error)
}

/// `"Could not delete note: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_delete_note("permission denied"),
///     "Could not delete note: permission denied"
/// );
/// ```
pub fn could_not_delete_note(error: impl std::fmt::Display) -> String {
    with_error("Could not delete note", error)
}

/// `"Could not delete folder: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_delete_folder("not empty"),
///     "Could not delete folder: not empty"
/// );
/// ```
pub fn could_not_delete_folder(error: impl std::fmt::Display) -> String {
    with_error("Could not delete folder", error)
}

/// `"Could not restore <label>: <error>"`.
///
/// The trash Undo's failure; `label` is the deleted item's name, kept as
/// written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_restore("Plan.md", "name taken"),
///     "Could not restore Plan.md: name taken"
/// );
/// ```
pub fn could_not_restore(label: &str, error: impl std::fmt::Display) -> String {
    format!("Could not restore {label}: {error}")
}

/// `"Could not empty trash: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_empty_trash("permission denied"),
///     "Could not empty trash: permission denied"
/// );
/// ```
pub fn could_not_empty_trash(error: impl std::fmt::Display) -> String {
    with_error("Could not empty trash", error)
}

/// `"Could not move note: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_move_note("name taken"),
///     "Could not move note: name taken"
/// );
/// ```
pub fn could_not_move_note(error: impl std::fmt::Display) -> String {
    with_error("Could not move note", error)
}

/// `"Could not create folder: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_create_folder("name taken"),
///     "Could not create folder: name taken"
/// );
/// ```
pub fn could_not_create_folder(error: impl std::fmt::Display) -> String {
    with_error("Could not create folder", error)
}

/// `"Could not create a note: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_create_note("name taken"),
///     "Could not create a note: name taken"
/// );
/// ```
pub fn could_not_create_note(error: impl std::fmt::Display) -> String {
    with_error("Could not create a note", error)
}

/// `"Could not add the note template: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_add_note_template("permission denied"),
///     "Could not add the note template: permission denied"
/// );
/// ```
pub fn could_not_add_note_template(error: impl std::fmt::Display) -> String {
    with_error("Could not add the note template", error)
}

/// `"Refusing unsafe recovery path <rel>"`.
///
/// A recovery record named a path outside the vault (or one with `..`); the
/// path is data and stays as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::refusing_unsafe_recovery_path("../x.md"),
///     "Refusing unsafe recovery path ../x.md"
/// );
/// ```
pub fn refusing_unsafe_recovery_path(rel: &str) -> String {
    format!("Refusing unsafe recovery path {rel}")
}

/// `"Links in <name> were not updated"`.
///
/// A rename or move could not rewrite one kanban's inbound links. `name` is
/// the board's title, kept as written; `None` is the fallback name when the
/// title could not be read.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::links_not_updated(Some("Board")),
///     "Links in Board were not updated"
/// );
/// assert_eq!(
///     muninn_i18n::links_not_updated(None),
///     "Links in the kanban were not updated"
/// );
/// ```
pub fn links_not_updated(name: Option<&str>) -> String {
    format!("Links in {} were not updated", name.unwrap_or("the kanban"))
}

/// `"the <what> could not be read"`.
///
/// The quit refusal's reason when a document's recovery copy could not be
/// read back; `what` is the catalogued document name ([`Key::ErrorDocumentBoard`]
/// or [`Key::ErrorDocumentKanban`]).
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_be_read("board"),
///     "the board could not be read"
/// );
/// ```
pub fn could_not_be_read(what: &str) -> String {
    format!("the {what} could not be read")
}

/// `"Renamed, but <n> inbound link file(s) could not be rewritten: <list>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::renamed_links_failed(1, "Plan.md: denied"),
///     "Renamed, but 1 inbound link file(s) could not be rewritten: Plan.md: denied"
/// );
/// ```
pub fn renamed_links_failed(count: usize, list: &str) -> String {
    format!("Renamed, but {count} inbound link file(s) could not be rewritten: {list}")
}

/// `"Moved, but <n> inbound link file(s) could not be rewritten: <list>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::moved_links_failed(2, "Plan.md: denied"),
///     "Moved, but 2 inbound link file(s) could not be rewritten: Plan.md: denied"
/// );
/// ```
pub fn moved_links_failed(count: usize, list: &str) -> String {
    format!("Moved, but {count} inbound link file(s) could not be rewritten: {list}")
}

/// `"Note <id> is no longer in this vault"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::note_not_in_vault(7),
///     "Note 7 is no longer in this vault"
/// );
/// ```
pub fn note_not_in_vault(id: usize) -> String {
    format!("Note {id} is no longer in this vault")
}

/// `"Cannot open <target>: no note is selected"`.
///
/// `target` is the embed's written path, kept as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::cannot_open_without_note("pic.png"),
///     "Cannot open pic.png: no note is selected"
/// );
/// ```
pub fn cannot_open_without_note(target: &str) -> String {
    format!("Cannot open {target}: no note is selected")
}

/// `"Attachment <target> is not in this vault — drop it in, or fix the embed"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::attachment_not_in_vault("pic.png"),
///     "Attachment pic.png is not in this vault — drop it in, or fix the embed"
/// );
/// ```
pub fn attachment_not_in_vault(target: &str) -> String {
    format!("Attachment {target} is not in this vault \u{2014} drop it in, or fix the embed")
}

/// `"<path> is not a file"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::not_a_file("/vault/assets"),
///     "/vault/assets is not a file"
/// );
/// ```
pub fn not_a_file(path: impl std::fmt::Display) -> String {
    format!("{path} is not a file")
}

/// `"<path> no longer exists"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::path_gone("/vault/assets/pic.png"),
///     "/vault/assets/pic.png no longer exists"
/// );
/// ```
pub fn path_gone(path: impl std::fmt::Display) -> String {
    format!("{path} no longer exists")
}

/// `<sentence> <path>: <error>. <hint>` for an OS file action.
fn file_action_error(
    sentence: &str,
    path: impl std::fmt::Display,
    error: impl std::fmt::Display,
    hint: &str,
) -> String {
    format!("{sentence} {path}: {error}. {hint}")
}

/// `"Could not open <path>: <error>. <hint>"`.
///
/// The status bar's failed Open externally. The hint names the platform's
/// opener and is part of the copy.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_open_file("Plan.md", "denied", "Install xdg-utils."),
///     "Could not open Plan.md: denied. Install xdg-utils."
/// );
/// ```
pub fn could_not_open_file(
    path: impl std::fmt::Display,
    error: impl std::fmt::Display,
    hint: &str,
) -> String {
    file_action_error("Could not open", path, error, hint)
}

/// `"Could not reveal <path>: <error>. <hint>"`.
///
/// The status bar's failed Reveal in file manager.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_reveal_file("Plan.md", "denied", "Install xdg-utils."),
///     "Could not reveal Plan.md: denied. Install xdg-utils."
/// );
/// ```
pub fn could_not_reveal_file(
    path: impl std::fmt::Display,
    error: impl std::fmt::Display,
    hint: &str,
) -> String {
    file_action_error("Could not reveal", path, error, hint)
}

/// `"No file opener is documented for this platform; open the file manually"`.
///
/// The status bar's refusal on a platform with no known opener.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::no_file_opener(),
///     "No file opener is documented for this platform; open the file manually"
/// );
/// ```
pub fn no_file_opener() -> &'static str {
    "No file opener is documented for this platform; open the file manually"
}

/// `"Could not import into <path>: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_import_into("/vault/assets", "denied"),
///     "Could not import into /vault/assets: denied"
/// );
/// ```
pub fn could_not_import_into(
    path: impl std::fmt::Display,
    error: impl std::fmt::Display,
) -> String {
    format!("Could not import into {path}: {error}")
}

/// `"Import refused: <list>"`.
///
/// The drop's refusal when nothing was imported; `list` is the reasons joined
/// by the caller.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_refused("a.png: not a file"),
///     "Import refused: a.png: not a file"
/// );
/// ```
pub fn import_refused(list: &str) -> String {
    format!("Import refused: {list}")
}

/// `"Imported <imported> of <total>; refused: <list>"`.
///
/// The drop's partly-successful line: how many arrived, out of the whole drop,
/// and why the rest stayed behind.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_partly_refused(1, 2, "b.pdf: already exists in this folder"),
///     "Imported 1 of 2; refused: b.pdf: already exists in this folder"
/// );
/// ```
pub fn import_partly_refused(imported: usize, total: usize, list: &str) -> String {
    format!("Imported {imported} of {total}; refused: {list}")
}

/// `"Cannot import <names>: drop .md, png, jpg, jpeg, gif, webp or pdf files"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::cannot_import("a.txt, b.zip"),
///     "Cannot import a.txt, b.zip: drop .md, png, jpg, jpeg, gif, webp or pdf files"
/// );
/// ```
pub fn cannot_import(names: &str) -> String {
    format!("Cannot import {names}: drop .md, png, jpg, jpeg, gif, webp or pdf files")
}

/// `"Scan cancelled after <done> of <total> notes"`.
///
/// The startup page after Escape abandoned the scan.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::scan_cancelled(12, 40),
///     "Scan cancelled after 12 of 40 notes"
/// );
/// ```
pub fn scan_cancelled(done: usize, total: usize) -> String {
    format!("Scan cancelled after {done} of {total} notes")
}

/// `"Board not saved and no recovery copy could be written: <why>. Use Save copy or fix the folder, then quit again."`.
///
/// The status bar's refusal when quitting would lose a dirty board; `why` is
/// the OS error or the read refusal, kept as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::board_quit_refused("disk full"),
///     "Board not saved and no recovery copy could be written: disk full. Use Save copy or fix the folder, then quit again."
/// );
/// ```
pub fn board_quit_refused(why: &str) -> String {
    format!(
        "Board not saved and no recovery copy could be written: {why}. Use Save copy or fix the folder, then quit again."
    )
}

/// `"Imported 2 files · 1 note · non-Markdown files stay attachments"`.
///
/// The status bar's drop summary: the file count always, the note count when
/// any note arrived, and the attachments note when files stayed out of the
/// index. Counts are plain decimal, as today.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::import_notice(2, 1, true), "Imported 2 files · 1 note · non-Markdown files stay attachments");
/// assert_eq!(muninn_i18n::import_notice(1, 1, false), "Imported 1 file · 1 note");
/// ```
pub fn import_notice(files: usize, notes: usize, attachments: bool) -> String {
    let mut text = format!("Imported {}", counted(files, "file", "files"));
    if notes > 0 {
        text.push_str(&format!(" · {}", counted(notes, "note", "notes")));
    }
    if attachments {
        text.push_str(" · non-Markdown files stay attachments");
    }
    text
}

/// `"<name>: not a file"`.
///
/// One refused drop entry: the dropped name and why it stayed behind.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::refused_not_a_file("a.txt"),
///     "a.txt: not a file"
/// );
/// ```
pub fn refused_not_a_file(name: &str) -> String {
    format!("{name}: not a file")
}

/// `"<name>: unsupported file type"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::refused_unsupported_type("a.zip"),
///     "a.zip: unsupported file type"
/// );
/// ```
pub fn refused_unsupported_type(name: &str) -> String {
    format!("{name}: unsupported file type")
}

/// `"<name>: already exists in this folder"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::refused_already_exists("a.md"),
///     "a.md: already exists in this folder"
/// );
/// ```
pub fn refused_already_exists(name: &str) -> String {
    format!("{name}: already exists in this folder")
}

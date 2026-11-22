//! UI copy for Muninn, behind one seam.
//!
//! This crate is the catalog **and locale seam**: it holds the keys, the
//! `en-US`, `es-ES`, `fr-FR`, `de-DE` and `pt-BR` text, the parameterised
//! copy that cannot be a single string, the date name tables, locale identity
//! and detection, and wave-1 plural categories. English, Spanish, French,
//! German and Brazilian Portuguese ship; `en-US` output is byte-identical to
//! the hardcoded string it replaced.
//!
//! # Why a typed catalog
//!
//! Text is looked up by [`Key`], a `Copy` enum, not by a string id. Each
//! language catalog is an exhaustive `match` (`en_us`, `es_es`, `fr_fr`), so
//! adding a key without text fails `cargo check`; a runtime string lookup
//! would only fail when the message was drawn. Every function here returns
//! `&'static str` or an owned `String`, so a migrated call site keeps today's
//! signature and allocates exactly as it does now. See `docs/i18n.md` §5.3 for
//! the API, §5.4 for the layering rule and §5.8 for the locale seam.
//!
//! # What is here
//!
//! - [`t`] / [`t_in`] resolve a key to text in the current [`Lang`]. English,
//!   Spanish and French are shipped; [`Lang::Pseudo`] is a test-only catalog that
//!   brackets the English so a render check can see which drawn strings came
//!   from here.
//! - [`LocaleTag`], [`LocaleTag::parse`], [`system_locale`] and [`locale_from`]
//!   identify the OS locale; [`PluralCategory`] and [`LocaleTag::plural_category`]
//!   carry the wave-1 plural rules. [`Lang::tag`], [`Lang::endonym`],
//!   [`Lang::from_tag`], [`Lang::SHIPPED`] and [`Lang::locale_tag`] are the
//!   bridge a language pack fills in.
//! - [`set_lang`] is the per-thread override, [`set_process_lang`] the
//!   process-wide default, [`clear_lang`] drops the override, and
//!   [`Lang::current`] prefers the override. The app sets both on a switch so
//!   the UI, tray and workers agree without serialising parallel tests.
//! - [`MsgKey`] is the second table, for **parameterised sentences**:
//!   [`phrase`] / [`phrase_current`] substitute positional arguments into a
//!   template and return an owned `String`. The English templates are the
//!   exact `format!` strings the helpers used to hold, the Spanish and French
//!   tables mirror them arm for arm, and [`Lang::Pseudo`] wraps the **rendered**
//!   sentence, so a count or a file name is inside the brackets. Keeping the
//!   templates in their own table preserves the catalog's no-placeholder rule
//!   for [`Key`], and `messages`/`messages_es`/`messages_fr` are exhaustive
//!   matches, so a message without text fails `cargo check`.
//! - [`files_skipped`], [`notes`], [`relative_day`], [`restore_to`],
//!   [`filter_by_tag`], [`board_name_taken`], [`no_note_named`],
//!   [`linked_note_hint`], [`tray_error`] and [`login_item_error`] are the
//!   escape hatch for parameterised copy: a view must not build a translated
//!   sentence with `format!`, so counts, relative dates, lane names, tags,
//!   board and note names and OS errors get typed functions instead. The
//!   helpers in this crate name a [`MsgKey`] or a [`Key`]; a caller never
//!   passes an English frame, and every helper's output follows [`Lang`].
//!   The status, recovery, Broken-links, trash and tray surfaces add
//!   [`deleted`], [`scan_ms`], [`recovered_for`], [`recovered_copies`],
//!   [`more_edits`], [`targets`], [`referrers`], [`status_counts`],
//!   [`trash_summary`], [`trash_empty`] and [`sync_status`] the same way.
//!   The status bar's errors, its drop and link notices and the two conflict
//!   bars (note and page) add the [`could_not_save`]-style sentences,
//!   [`links_not_updated`], [`import_notice`], [`board_quit_refused`] and
//!   [`scan_cancelled`]; their static copy — the vault-path prompts, the
//!   switch/restore refusals, the conflict labels — is keyed (`error.*`,
//!   `conflict.*`, `status.*`).
//!   Settings › Sync adds the status and error sentences: [`connected_as`],
//!   [`last_sync`], [`device_code`], [`sign_in_failed`], [`tailnet_hosting`],
//!   [`shared_key_copied`] and the `sync_offline`-style run failures; its
//!   fixed copy is keyed (`sync.status.*`, `sync.error.*`).
//! - A helper that **composes** a sentence ([`import_notice`],
//!   [`recovered_for`], the lane labels) builds it from named fragments with
//!   the private `render` and wraps the joined line once with `compose`, so
//!   [`Lang::Pseudo`] brackets the sentence rather than each fragment. An
//!   argument that is itself catalogued copy or another helper's sentence is
//!   passed through `strip_pseudo`, so a composed render never nests markers.
//! - [`DateNames`], [`EN_US`], [`ES`] and [`FR`] carry month/weekday names, short
//!   weekday labels, the first day of the week and `AM`/`PM`. [`date_names`]
//!   is the language lookup the UI's five display sites use.
//!   `muninn-core`'s `format_date_for_display`/`format_time_for_display` read
//!   them; the file formatters keep their own English table, and `Do` still
//!   spells an English ordinal until a locale pack supplies its own rule.
//!
//! The only helpers that do not name a key are pure data formatters — the byte
//! size in `file_size`, the count/date strings a caller already formatted — and
//! the substitution engine itself.
//!
//! Extraction of the call sites is done, area by area, keeping English output
//! byte-identical. This crate is the target those changes landed in; it is a
//! leaf with no dependencies so `muninn-core`, `muninn-ui` and `muninn-app`
//! can all point at it without an edge cycle. A language pack adds catalog
//! data and one [`Lang`] variant here; `docs/i18n.md` §5.8 lists the arms.

mod dates;
mod de_de;
mod en_us;
mod es_es;
mod fr_fr;
mod key;
mod locale;
mod messages;
mod messages_de;
mod messages_es;
mod messages_fr;
mod messages_pt_br;
mod msg;
mod pseudo;
mod pt_br;

pub use dates::{DE, DateNames, EN_US, ES, FR, PT_BR, date_names};
pub use key::Key;
pub use locale::{LocaleTag, PluralCategory, locale_from, system_locale};
pub use msg::MsgKey;

use std::cell::Cell;
use std::sync::atomic::{AtomicU8, Ordering};

/// A language the UI can draw.
///
/// A locale pack adds one variant here plus its exhaustive catalog, and lists
/// the variant in [`Lang::SHIPPED`]; the setting, picker, detection, date
/// names and plural rules then reach it without touching their code. Adding a
/// variant must not change what `en-US` renders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    /// English (United States), `en-US`: the default everywhere.
    EnUs,
    /// Spanish (Spain), `es-ES`: the first wave-1 language pack.
    ///
    /// Neutral software Spanish with one/other plurals and Monday-first
    /// dates. Files stay English and ASCII; only the UI copy, the date display
    /// tables and the parameterised sentences follow it.
    Es,
    /// French (France), `fr-FR`: the second wave-1 language pack.
    ///
    /// Neutral software French with the CLDR French plural rule (0 and 1 are
    /// singular, 2 and up are plural), Monday-first dates and French
    /// typography (a no-break space U+00A0 before `:` `;` `!` `?`). Files stay
    /// English and ASCII; only the UI copy, the date display tables and the
    /// parameterised sentences follow it.
    Fr,
    /// German (Germany), `de-DE`: the third wave-1 language pack.
    ///
    /// Neutral software German with one/other plurals and Monday-first dates.
    /// Files stay English and ASCII; only the UI copy, the date display
    /// tables and the parameterised sentences follow it.
    De,
    /// Brazilian Portuguese (Brazil), `pt-BR`: the fourth wave-1 language pack.
    ///
    /// Neutral Brazilian Portuguese with one/other plurals and Sunday-first
    /// dates. Files stay English and ASCII; only the UI copy, the date display
    /// tables and the parameterised sentences follow it.
    PtBr,
    /// Pseudo-localisation: a **test and verification language**, not a
    /// shippable locale.
    ///
    /// Every key resolves to its English text wrapped in [`PSEUDO_OPEN`] and
    /// [`PSEUDO_CLOSE`], so a render check can tell catalog copy from a
    /// hardcoded literal that was built where the lint cannot see it. Nothing
    /// selects it in a release build; a test sets it with [`set_lang`] and
    /// resets it. It follows `en-US` for date names, tags and plural rules.
    Pseudo,
}

impl Lang {
    /// The languages a user can pick in Settings › Appearance.
    ///
    /// This is the picker's data and the completeness tests' loop. All four
    /// wave-1 packs ship: English, Spanish, French, German and Brazilian
    /// Portuguese. `Pseudo` is a test language and is never listed.
    pub const SHIPPED: &'static [Lang] = &[Lang::EnUs, Lang::Es, Lang::Fr, Lang::De, Lang::PtBr];

    /// The language's canonical BCP 47 tag, for persistence and for Rune.
    ///
    /// [`Lang::Pseudo`] names the locale it emulates (`en-US`): its copy is
    /// bracketed English, and Rune's shaping and spellcheck should follow the
    /// same locale as the English it is built from.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Lang::EnUs | Lang::Pseudo => "en-US",
            Lang::Es => "es-ES",
            Lang::Fr => "fr-FR",
            Lang::De => "de-DE",
            Lang::PtBr => "pt-BR",
        }
    }

    /// The language's own name, shown untranslated in the picker.
    ///
    /// An endonym is not catalog copy: a French user looking for French must
    /// see `Français` even while the UI is English, so each pack supplies its
    /// own spelling. [`Lang::Pseudo`] is a test language and is never drawn in
    /// the picker.
    #[must_use]
    pub const fn endonym(self) -> &'static str {
        match self {
            Lang::EnUs => "English",
            Lang::Es => "Español",
            Lang::Fr => "Français",
            Lang::De => "Deutsch",
            Lang::PtBr => "Português (Brasil)",
            Lang::Pseudo => "Pseudo (test)",
        }
    }

    /// The shipped language for an OS or settings tag, or `None` when this
    /// build has no catalog for it.
    ///
    /// Detection and a saved setting both go through this: every wave-1 tag
    /// resolves to its pack, and a tag outside wave 1 stays `None`.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Lang> {
        match LocaleTag::parse(tag)? {
            LocaleTag::EnUs => Some(Lang::EnUs),
            LocaleTag::Es => Some(Lang::Es),
            LocaleTag::Fr => Some(Lang::Fr),
            LocaleTag::De => Some(Lang::De),
            LocaleTag::PtBr => Some(Lang::PtBr),
        }
    }

    /// The locale identity behind this language, for date names and plurals.
    ///
    /// [`Lang::Pseudo`] follows `en-US`, so a pseudo render shows English
    /// dates and plurals exactly as the default does; only catalog copy is
    /// marked.
    #[must_use]
    pub const fn locale_tag(self) -> LocaleTag {
        match self {
            Lang::EnUs | Lang::Pseudo => LocaleTag::EnUs,
            Lang::Es => LocaleTag::Es,
            Lang::Fr => LocaleTag::Fr,
            Lang::De => LocaleTag::De,
            Lang::PtBr => LocaleTag::PtBr,
        }
    }

    /// The CLDR plural category of `n` in this language.
    ///
    /// See [`LocaleTag::plural_category`] for the wave-1 rules.
    #[must_use]
    pub const fn plural_category(self, n: u64) -> PluralCategory {
        self.locale_tag().plural_category(n)
    }

    /// This language's encoded value in [`process_lang`]'s atomic.
    ///
    /// The numbers are a stable encoding for the process atomic (nothing else
    /// reads them): a new pack takes the next free slot so an existing value
    /// never shifts. French takes slot 3, German slot 4 and Portuguese slot
    /// 5, after English (0), Pseudo (1) and Spanish (2).
    const fn as_u8(self) -> u8 {
        match self {
            Lang::EnUs => 0,
            Lang::Pseudo => 1,
            Lang::Es => 2,
            Lang::Fr => 3,
            Lang::De => 4,
            Lang::PtBr => 5,
        }
    }

    /// Decode [`Lang::as_u8`]; an unknown byte is English, the safe default.
    const fn from_u8(value: u8) -> Lang {
        match value {
            1 => Lang::Pseudo,
            2 => Lang::Es,
            3 => Lang::Fr,
            4 => Lang::De,
            5 => Lang::PtBr,
            _ => Lang::EnUs,
        }
    }
}

/// The process-wide default language, for threads with no override.
///
/// The app sets this once at startup and on every language switch, so the UI
/// thread, the tray and any worker agree. It is deliberately separate from
/// the per-thread override: tests in one binary run in parallel, and a
/// process-wide value alone would let them race.
static PROCESS: AtomicU8 = AtomicU8::new(Lang::EnUs.as_u8());

thread_local! {
    /// The language this thread draws in, when it differs from the process
    /// default.
    ///
    /// Tests and a UI thread that renders a language other than the setting
    /// (the pseudo render check) use this. A worker with no override follows
    /// [`process_lang`].
    static OVERRIDE: Cell<Option<Lang>> = const { Cell::new(None) };
}

/// Sets the language [`Lang::current`] returns for **this thread**.
///
/// This is the override tests use to render in [`Lang::Pseudo`], and the
/// thread half of the live switch: the settings page sets the process default
/// with [`set_process_lang`] and this thread's value with `set_lang`, so the
/// next frame draws the new language even if the process default is read by
/// another thread. Call [`clear_lang`] to follow the process default again.
///
/// Set it back to [`Lang::EnUs`] when a test is done: tests in the same binary
/// share a thread pool, not a language.
///
/// # Examples
///
/// ```
/// use muninn_i18n::{set_lang, Lang, Key, t};
///
/// let before = Lang::current();
/// set_lang(Lang::Pseudo);
/// assert_eq!(t(Key::Save), "⟦Save⟧");
/// set_lang(before);
/// assert_eq!(t(Key::Save), "Save");
/// ```
pub fn set_lang(lang: Lang) {
    OVERRIDE.with(|current| current.set(Some(lang)));
}

/// Drops this thread's override, so [`Lang::current`] follows
/// [`process_lang`].
pub fn clear_lang() {
    OVERRIDE.with(|current| current.set(None));
}

/// Sets the process-wide default language, the value every thread without an
/// override follows.
///
/// The app calls this at startup and on a language switch. Tests that change
/// it own the global for their test binary; see `tests/locale.rs`.
pub fn set_process_lang(lang: Lang) {
    PROCESS.store(lang.as_u8(), Ordering::Release);
}

/// The process-wide default language ([`set_process_lang`]).
///
/// [`Lang::EnUs`] until the app starts and applies the saved setting.
#[must_use]
pub fn process_lang() -> Lang {
    Lang::from_u8(PROCESS.load(Ordering::Acquire))
}

impl Lang {
    /// The language the UI draws in on this thread: the override when one is
    /// set, else the process default.
    ///
    /// Draw code should call [`t`] and let this decide, rather than pinning a
    /// language at the call site, so a language switch only changes the seam.
    #[must_use]
    pub fn current() -> Lang {
        OVERRIDE.with(Cell::get).unwrap_or_else(process_lang)
    }
}

/// The opening marker of a [`Lang::Pseudo`] string.
pub const PSEUDO_OPEN: char = '⟦';

/// The closing marker of a [`Lang::Pseudo`] string.
pub const PSEUDO_CLOSE: char = '⟧';

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
/// [`Lang::Pseudo`] returns the English text wrapped in [`PSEUDO_OPEN`] and
/// [`PSEUDO_CLOSE`]; that catalog is built once per process, so a render loop
/// pays no allocation per frame.
///
/// # Examples
///
/// ```
/// use muninn_i18n::{t_in, Key, Lang};
///
/// assert_eq!(t_in(Lang::EnUs, Key::Save), "Save");
/// assert_eq!(t_in(Lang::Pseudo, Key::Save), "⟦Save⟧");
/// ```
pub fn t_in(lang: Lang, key: Key) -> &'static str {
    match lang {
        Lang::EnUs => en_us::text(key),
        Lang::Es => es_es::text(key),
        Lang::Fr => fr_fr::text(key),
        Lang::De => de_de::text(key),
        Lang::PtBr => pt_br::text(key),
        Lang::Pseudo => pseudo::text(key),
    }
}

/// The sentence for `key` in `lang`, with `args` substituted in order.
///
/// A [`MsgKey`] names a sentence **template** whose placeholders are `{0}`,
/// `{1}`, … ([`MsgKey::arity`] is how many). This is the seam for
/// parameterised copy: a helper here picks a key and passes its data, so a
/// language renders the whole sentence and not just the words around it.
///
/// [`Lang::EnUs`] returns today's English bytes exactly. [`Lang::Pseudo`]
/// wraps the **rendered** sentence in [`PSEUDO_OPEN`] and [`PSEUDO_CLOSE`], so
/// the render check sees a phrase whose arguments came from file data too.
/// Draw code normally calls [`phrase_current`]; this form is for tests and
/// anything that deliberately renders another language.
///
/// # Examples
///
/// ```
/// use muninn_i18n::{Lang, MsgKey, phrase};
///
/// assert_eq!(
///     phrase(Lang::EnUs, MsgKey::RestoreTo, &["To do"]),
///     "Restore to To do"
/// );
/// assert_eq!(
///     phrase(Lang::Pseudo, MsgKey::RestoreTo, &["To do"]),
///     "⟦Restore to To do⟧"
/// );
/// ```
pub fn phrase(lang: Lang, key: MsgKey, args: &[&str]) -> String {
    let rendered = render(lang, key, args);
    match lang {
        Lang::EnUs | Lang::Es | Lang::Fr | Lang::De | Lang::PtBr => rendered,
        Lang::Pseudo => format!("{PSEUDO_OPEN}{rendered}{PSEUDO_CLOSE}"),
    }
}

/// The sentence for `key` in `lang` with `args` substituted, without the
/// pseudo marker.
///
/// Private: [`phrase`] adds the marker once to a finished sentence. A helper
/// that **composes** a sentence from fragments ([`import_notice`],
/// [`recovered_for`], the lane labels) renders each fragment here and calls
/// [`compose`] once on the joined line, so a pseudo render has one marker pair
/// around the whole sentence instead of one per fragment. A language pack adds
/// its template table in this function.
fn render(lang: Lang, key: MsgKey, args: &[&str]) -> String {
    match lang {
        Lang::EnUs | Lang::Pseudo => substitute(messages::template(key), args),
        Lang::Es => substitute(messages_es::template(key), args),
        Lang::Fr => substitute(messages_fr::template(key), args),
        Lang::De => substitute(messages_de::template(key), args),
        Lang::PtBr => substitute(messages_pt_br::template(key), args),
    }
}

/// [`render`] in the current UI language ([`Lang::current`]).
fn render_current(key: MsgKey, args: &[&str]) -> String {
    render(Lang::current(), key, args)
}

/// Marks a sentence assembled from [`render_current`] fragments for
/// [`Lang::Pseudo`], exactly once.
///
/// This is [`phrase`]'s marker step without a template: composition renders
/// the pieces and finishes with this call, so the pseudo render brackets the
/// sentence rather than each fragment.
fn compose(text: String) -> String {
    match Lang::current() {
        Lang::EnUs | Lang::Es | Lang::Fr | Lang::De | Lang::PtBr => text,
        Lang::Pseudo => format!("{PSEUDO_OPEN}{text}{PSEUDO_CLOSE}"),
    }
}

/// `text` without the pseudo marker pair [`phrase`]/[`compose`] added, if it
/// is exactly one bracketed sentence.
///
/// A helper that takes another helper's finished sentence as an argument
/// ([`recovered_for`], [`board_canvas`], [`relative_date`]) strips that inner
/// marker so the composed line is bracketed once, not nested. Text with more
/// than one marker pair (a caller-joined list, say) is left as it is: stripping
/// the outer pair would cut into the fragments.
fn strip_pseudo(text: &str) -> &str {
    let Some(inner) = text
        .strip_prefix(PSEUDO_OPEN)
        .and_then(|rest| rest.strip_suffix(PSEUDO_CLOSE))
    else {
        return text;
    };
    if inner.contains(PSEUDO_OPEN) || inner.contains(PSEUDO_CLOSE) {
        return text;
    }
    inner
}

/// The sentence for `key` in the current UI language ([`Lang::current`]).
///
/// This is the `String` sibling of [`t`]: a helper that builds a sentence from
/// arguments calls this, so its wording follows the same language as the
/// static copy. The English output is byte-identical to the `format!` it
/// replaced.
///
/// # Examples
///
/// ```
/// use muninn_i18n::{Lang, MsgKey, phrase_current, set_lang};
///
/// assert_eq!(phrase_current(MsgKey::Deleted, &["Plan.md"]), "Deleted Plan.md");
///
/// let before = Lang::current();
/// set_lang(Lang::Pseudo);
/// assert_eq!(phrase_current(MsgKey::Deleted, &["Plan.md"]), "⟦Deleted Plan.md⟧");
/// set_lang(before);
/// ```
pub fn phrase_current(key: MsgKey, args: &[&str]) -> String {
    phrase(Lang::current(), key, args)
}

/// `{0}`, `{1}`, … in `template` replaced by `args`, in order.
///
/// A placeholder past the end of `args` is left as written; every call site
/// here passes [`MsgKey::arity`] arguments and `tests/messages.rs` checks that
/// the table and the arities agree.
fn substitute(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let index = after[..close].parse::<usize>().ok();
        match index.and_then(|index| args.get(index)) {
            Some(arg) => out.push_str(arg),
            None => {
                out.push('{');
                out.push_str(&after[..close]);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
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
    counted(n, MsgKey::FilesSkippedOne, MsgKey::FilesSkippedOther)
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
    counted(n, MsgKey::NoteOne, MsgKey::NoteOther)
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
        0 => phrase_current(MsgKey::RelativeToday, &[]),
        1 => phrase_current(MsgKey::RelativeTomorrow, &[]),
        -1 => phrase_current(MsgKey::RelativeYesterday, &[]),
        n if n > 0 => phrase_current(MsgKey::RelativeInDays, &[&n.to_string()]),
        n => phrase_current(MsgKey::RelativeDaysAgo, &[&(-n).to_string()]),
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
    phrase_current(MsgKey::RestoreTo, &[lane])
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
    phrase_current(MsgKey::FilterByTag, &[tag])
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
    phrase_current(MsgKey::TrayError, &[error])
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
    phrase_current(MsgKey::LoginItemError, &[error])
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
    phrase_current(MsgKey::BoardNameTaken, &[name])
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
    phrase_current(MsgKey::NoNoteNamed, &[typed])
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
    phrase_current(MsgKey::LinkedNoteHint, &[label])
}

/// `"1 <one>"` for one, `"<n> <many>"` otherwise.
///
/// The shared shape behind the counted phrases in this module. Each branch is
/// its own [`MsgKey`] template, so a language supplies a singular and a plural
/// form instead of inheriting English's `if`. Private: a view must pick a
/// helper that names the thing it counts, not build a sentence of its own.
fn counted(n: usize, one: MsgKey, many: MsgKey) -> String {
    let count = n.to_string();
    let key = if Lang::current().locale_tag().plural_category(n as u64) == PluralCategory::One {
        one
    } else {
        many
    };
    phrase_current(key, &[&count])
}

/// [`counted`] in `lang`, **without** the pseudo marker, for a helper that
/// joins the count into a larger sentence.
///
/// A composed sentence renders its fragments here and wraps once with
/// [`compose`]; using [`counted`] would put a marker pair inside the line.
fn counted_in(lang: Lang, n: usize, one: MsgKey, many: MsgKey) -> String {
    let count = n.to_string();
    let key = if lang.locale_tag().plural_category(n as u64) == PluralCategory::One {
        one
    } else {
        many
    };
    render(lang, key, &[&count])
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
    phrase_current(MsgKey::Deleted, &[label])
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
    phrase_current(MsgKey::ScanMs, &[&format!("{ms:.0}")])
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
    phrase_current(MsgKey::RecoveredFor, &[strip_pseudo(note)])
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
    let lang = Lang::current();
    match (notes, boards) {
        (notes, 0) => counted(notes, MsgKey::NoteOne, MsgKey::NoteOther),
        (0, boards) => counted(boards, MsgKey::BoardOne, MsgKey::BoardOther),
        (notes, boards) => {
            let notes = counted_in(lang, notes, MsgKey::NoteOne, MsgKey::NoteOther);
            let boards = counted_in(lang, boards, MsgKey::BoardOne, MsgKey::BoardOther);
            phrase_current(MsgKey::RecoveredCopies, &[&notes, &boards])
        }
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
    phrase_current(MsgKey::MoreEdits, &[&n.to_string()])
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
    counted(n, MsgKey::TargetOne, MsgKey::TargetOther)
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
    counted(n, MsgKey::ReferrerOne, MsgKey::ReferrerOther)
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
    let lang = Lang::current();
    let words = counted_in(lang, words, MsgKey::WordOne, MsgKey::WordOther);
    let links = counted_in(lang, links, MsgKey::LinkOne, MsgKey::LinkOther);
    let backlinks = counted_in(lang, backlinks, MsgKey::BacklinkOne, MsgKey::BacklinkOther);
    phrase_current(MsgKey::StatusCounts, &[&words, &links, &backlinks])
}

/// `"Trash is empty"`.
///
/// The Help trash card's readout when nothing is in the trash. Zero has no
/// size to show, so it is a fixed sentence rather than a [`trash_summary`];
/// it resolves through [`Key::TrashEmpty`], which keeps the `&'static str`
/// signature and lets [`Lang::Pseudo`] bracket it like any other copy.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::trash_empty(), "Trash is empty");
/// ```
pub fn trash_empty() -> &'static str {
    t(Key::TrashEmpty)
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
    let count = counted_in(Lang::current(), files, MsgKey::NoteOne, MsgKey::NoteOther);
    phrase_current(MsgKey::TrashSummary, &[&count, size])
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
    phrase_current(MsgKey::SyncStatus, &[label])
}

// ---- Sync status and errors ----------------------------------------------
//
// Settings › Sync builds its status line from the sentences below. The fixed
// part lives here; a provider name, account, commit head, port, browser
// address, user code or provider/OS error is data and is kept as written. A
// call site names one helper per sentence instead of assembling copy of its
// own, the same way [`restore_to`] carries its lane.

/// `"Connected as <account>. The token is in the OS credential store. Sync Now when ready."`.
///
/// The Sync tab's status once an OAuth provider is connected and the account
/// name is known. The account is provider data and stays as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::connected_as("octo"),
///     "Connected as octo. The token is in the OS credential store. Sync Now when ready."
/// );
/// ```
pub fn connected_as(account: &str) -> String {
    phrase_current(MsgKey::ConnectedAs, &[account])
}

/// `"Last successful sync <head>."`.
///
/// `head` is the commit or snapshot the run published; the caller appends any
/// exclusion or conflict note after it.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::last_sync("a1b2c3d"),
///     "Last successful sync a1b2c3d."
/// );
/// ```
pub fn last_sync(head: &str) -> String {
    phrase_current(MsgKey::LastSync, &[head])
}

/// `"Enter code <code> at <url>. Your browser is opening it; Muninn finishes when GitHub approves."`.
///
/// GitHub's device-flow prompt: the user code and verification address are
/// provider data and stay as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::device_code("ABCD-1234", "https://github.com/login/device"),
///     "Enter code ABCD-1234 at https://github.com/login/device. Your browser is opening it; Muninn finishes when GitHub approves."
/// );
/// ```
pub fn device_code(code: &str, url: &str) -> String {
    phrase_current(MsgKey::DeviceCode, &[code, url])
}

/// `"Could not open a browser. Open this address yourself to continue: <url>"`.
///
/// The browser opener failed; the address is kept verbatim so it can be
/// copied into whatever browser the person has.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::browser_failed("https://example.test"),
///     "Could not open a browser. Open this address yourself to continue: https://example.test"
/// );
/// ```
pub fn browser_failed(url: &str) -> String {
    phrase_current(MsgKey::BrowserFailed, &[url])
}

/// `"Sign-in did not finish: <error> No token was stored."`.
///
/// `error` is the provider's or credential store's reason, kept as written.
/// When an older binding survived the failure, the caller appends
/// [`Key::SyncStatusCurrentConnectionKept`].
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sign_in_failed("access denied"),
///     "Sign-in did not finish: access denied No token was stored."
/// );
/// ```
pub fn sign_in_failed(error: impl std::fmt::Display) -> String {
    with_error(MsgKey::SignInFailed, error)
}

/// `"<n> path(s) excluded by the sync contract."`.
///
/// The count is plain decimal and the phrase keeps sync's `path(s)` spelling.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_excluded(2),
///     "2 path(s) excluded by the sync contract."
/// );
/// ```
pub fn sync_excluded(n: usize) -> String {
    phrase_current(MsgKey::SyncExcluded, &[&n.to_string()])
}

/// `"A note changed on two devices; both versions were kept: <names>."`.
///
/// `names` is the comma-joined list of conflict copies, kept as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_conflict_copies("Plan (remote).md"),
///     "A note changed on two devices; both versions were kept: Plan (remote).md."
/// );
/// ```
pub fn sync_conflict_copies(names: &str) -> String {
    phrase_current(MsgKey::SyncConflictCopies, &[names])
}

/// `"<n> conflict(s): <names>. Nothing was overwritten."`.
///
/// A run that stopped on conflicts names the files; the count and list are
/// plain data.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_conflicts(1, "Plan.md"),
///     "1 conflict(s): Plan.md. Nothing was overwritten."
/// );
/// ```
pub fn sync_conflicts(n: usize, names: &str) -> String {
    phrase_current(MsgKey::SyncConflicts, &[&n.to_string(), names])
}

/// `"Hosting on 127.0.0.1:<port>. On this device run: <serve>"`.
///
/// The Tailscale host's status once its loopback server is up. `serve` is the
/// exact `tailscale serve` command and stays as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::tailnet_hosting(8080, "tailscale serve --bg 8080"),
///     "Hosting on 127.0.0.1:8080. On this device run: tailscale serve --bg 8080"
/// );
/// ```
pub fn tailnet_hosting(port: u16, serve: &str) -> String {
    phrase_current(MsgKey::TailnetHosting, &[&port.to_string(), serve])
}

/// `"Hosting. Run once on this device: <serve>. Use Copy key to give the shared key to your other devices."`.
///
/// The status after a Connect chose this device as the host. `serve` is the
/// exact `tailscale serve` command.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::tailnet_hosting_ready("tailscale serve --bg 8080"),
///     "Hosting. Run once on this device: tailscale serve --bg 8080. Use Copy key to give the shared key to your other devices."
/// );
/// ```
pub fn tailnet_hosting_ready(serve: &str) -> String {
    phrase_current(MsgKey::TailnetHostingReady, &[serve])
}

/// `"Shared key copied. The clipboard clears in <seconds> seconds; paste it on the joining device."`.
///
/// The host's Copy key confirmation; the timeout is plain decimal.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::shared_key_copied(60),
///     "Shared key copied. The clipboard clears in 60 seconds; paste it on the joining device."
/// );
/// ```
pub fn shared_key_copied(seconds: u64) -> String {
    phrase_current(MsgKey::SharedKeyCopied, &[&seconds.to_string()])
}

/// `"The previous connection's credential could not be removed: <error>"`.
///
/// Appended to the Connect success line when the old binding's credential
/// outlived the swap. `error` is the credential store's reason, kept as
/// written; the caller adds the separating space.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_previous_credential("permission denied"),
///     "The previous connection's credential could not be removed: permission denied"
/// );
/// ```
pub fn sync_previous_credential(error: impl std::fmt::Display) -> String {
    with_error(MsgKey::SyncPreviousCredential, error)
}

/// `"<error> Nothing was saved."`.
///
/// Tailscale Connect failed before the binding replaced anything; `error` is
/// the credential store's reason, kept as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_nothing_saved("the keyring is locked"),
///     "the keyring is locked Nothing was saved."
/// );
/// ```
pub fn sync_nothing_saved(error: impl std::fmt::Display) -> String {
    with_error(MsgKey::SyncNothingSaved, error)
}

/// `"<error> The old key still works."`.
///
/// Rotating the host's shared key failed; the existing key keeps serving.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_key_rotation_failed("the keyring is locked"),
///     "the keyring is locked The old key still works."
/// );
/// ```
pub fn sync_key_rotation_failed(error: impl std::fmt::Display) -> String {
    with_error(MsgKey::SyncKeyRotationFailed, error)
}

/// `"<error> Connect again before Sync Now."`.
///
/// Sync Now repeats the last Connect failure so the reason is not lost when
/// no binding exists.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_now_after_error("access denied"),
///     "access denied Connect again before Sync Now."
/// );
/// ```
pub fn sync_now_after_error(error: impl std::fmt::Display) -> String {
    with_error(MsgKey::SyncNowAfterError, error)
}

/// `"Kanban not saved and no recovery copy could be written: <why>. Use Save copy or fix the folder, then quit again."`.
///
/// The close/quit refusal for the Kanban page; `why` is the recovery copy's
/// reason, kept as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::kanban_quit_refused("disk full"),
///     "Kanban not saved and no recovery copy could be written: disk full. Use Save copy or fix the folder, then quit again."
/// );
/// ```
pub fn kanban_quit_refused(why: &str) -> String {
    phrase_current(MsgKey::KanbanQuitRefused, &[why])
}

/// `"could not save a copy: <error>"`.
///
/// The Kanban page's Save copy failure. The sentence keeps its historical
/// lower-case opening, so it is a separate helper from [`could_not_save_copy`]
/// rather than a call site with a different capital letter.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::kanban_could_not_save_copy("disk full"),
///     "could not save a copy: disk full"
/// );
/// ```
pub fn kanban_could_not_save_copy(error: impl std::fmt::Display) -> String {
    with_error(MsgKey::KanbanCouldNotSaveCopy, error)
}

/// `"Offline: <detail>"`.
///
/// A sync run stopped before the provider answered; `detail` is the network
/// error, kept as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_offline("connection refused"),
///     "Offline: connection refused"
/// );
/// ```
pub fn sync_offline(detail: &str) -> String {
    phrase_current(MsgKey::SyncOffline, &[detail])
}

/// `"Rate limited; retry after <ms> ms"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_rate_limited_after(1500),
///     "Rate limited; retry after 1500 ms"
/// );
/// ```
pub fn sync_rate_limited_after(ms: u64) -> String {
    phrase_current(MsgKey::SyncRateLimitedAfter, &[&ms.to_string()])
}

/// `"Provider error <status>: <detail>"`.
///
/// `detail` is the provider's own text, kept as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_provider_error(500, "internal error"),
///     "Provider error 500: internal error"
/// );
/// ```
pub fn sync_provider_error(status: u16, detail: &str) -> String {
    phrase_current(MsgKey::SyncProviderError, &[&status.to_string(), detail])
}

/// `"Authentication needed: <detail>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_authentication_needed("token expired"),
///     "Authentication needed: token expired"
/// );
/// ```
pub fn sync_authentication_needed(detail: &str) -> String {
    phrase_current(MsgKey::SyncAuthenticationNeeded, &[detail])
}

/// `"Permission denied: <detail>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_permission_denied("no access to the repository"),
///     "Permission denied: no access to the repository"
/// );
/// ```
pub fn sync_permission_denied(detail: &str) -> String {
    phrase_current(MsgKey::SyncPermissionDenied, &[detail])
}

/// `"OAuth scope rejected: <detail>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_oauth_scope_rejected("drive.file required"),
///     "OAuth scope rejected: drive.file required"
/// );
/// ```
pub fn sync_oauth_scope_rejected(detail: &str) -> String {
    phrase_current(MsgKey::SyncOauthScopeRejected, &[detail])
}

/// `"Remote scope not found: <detail>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_remote_scope_not_found("branch main"),
///     "Remote scope not found: branch main"
/// );
/// ```
pub fn sync_remote_scope_not_found(detail: &str) -> String {
    phrase_current(MsgKey::SyncRemoteScopeNotFound, &[detail])
}

/// `"Remote changed: <detail>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_remote_changed("commit moved"),
///     "Remote changed: commit moved"
/// );
/// ```
pub fn sync_remote_changed(detail: &str) -> String {
    phrase_current(MsgKey::SyncRemoteChanged, &[detail])
}

/// `"Outcome unknown, reconcile before retrying: <detail>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_outcome_unknown("connection dropped"),
///     "Outcome unknown, reconcile before retrying: connection dropped"
/// );
/// ```
pub fn sync_outcome_unknown(detail: &str) -> String {
    phrase_current(MsgKey::SyncOutcomeUnknown, &[detail])
}

/// `"Remote listing incomplete; refusing to treat that as deletion: <detail>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_listing_incomplete("page 3 of 5"),
///     "Remote listing incomplete; refusing to treat that as deletion: page 3 of 5"
/// );
/// ```
pub fn sync_listing_incomplete(detail: &str) -> String {
    phrase_current(MsgKey::SyncListingIncomplete, &[detail])
}

/// `"Unsupported <path>: <detail>"`.
///
/// The relative path is file data and stays as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_unsupported("Data.xlsx", "Workspace document"),
///     "Unsupported Data.xlsx: Workspace document"
/// );
/// ```
pub fn sync_unsupported(path: &str, detail: &str) -> String {
    phrase_current(MsgKey::SyncUnsupported, &[path, detail])
}

/// `"Sync limit exceeded: <detail>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_limit_exceeded("file larger than 100 MB"),
///     "Sync limit exceeded: file larger than 100 MB"
/// );
/// ```
pub fn sync_limit_exceeded(detail: &str) -> String {
    phrase_current(MsgKey::SyncLimitExceeded, &[detail])
}

/// `"Credentials unavailable: <detail>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_credentials_unavailable("the keyring is locked"),
///     "Credentials unavailable: the keyring is locked"
/// );
/// ```
pub fn sync_credentials_unavailable(detail: &str) -> String {
    phrase_current(MsgKey::SyncCredentialsUnavailable, &[detail])
}

/// `"Save or discard unsaved edits in <path> before sync can change it"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_dirty_buffer("Plan.md"),
///     "Save or discard unsaved edits in Plan.md before sync can change it"
/// );
/// ```
pub fn sync_dirty_buffer(path: &str) -> String {
    phrase_current(MsgKey::SyncDirtyBuffer, &[path])
}

/// `"<path> changed during sync; run Sync Now again to replan"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_local_changed("Plan.md"),
///     "Plan.md changed during sync; run Sync Now again to replan"
/// );
/// ```
pub fn sync_local_changed(path: &str) -> String {
    phrase_current(MsgKey::SyncLocalChanged, &[path])
}

/// `"Sync is not configured: <detail>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_not_configured("missing repository"),
///     "Sync is not configured: missing repository"
/// );
/// ```
pub fn sync_not_configured(detail: &str) -> String {
    phrase_current(MsgKey::SyncNotConfigured, &[detail])
}

/// `"File error: <detail>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::sync_file_error("read-only folder"),
///     "File error: read-only folder"
/// );
/// ```
pub fn sync_file_error(detail: &str) -> String {
    phrase_current(MsgKey::SyncFileError, &[detail])
}

// ---- Errors, notices and conflict bars ------------------------------------
//
// Every sentence below is drawn by the status bar, the startup page or a
// conflict bar. Its fixed part lives here; the parameters are file data, a
// count or an OS error and are always kept as written. A call site names one
// helper per sentence instead of assembling copy of its own, the same way
// [`tray_error`] carries its OS error.

/// `<sentence>: <error>`, with the OS error kept as written.
///
/// `key` is the sentence's template; the error is data and is never
/// translated.
fn with_error(key: MsgKey, error: impl std::fmt::Display) -> String {
    phrase_current(key, &[&error.to_string()])
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
    with_error(MsgKey::CouldNotSave, error)
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
    with_error(MsgKey::CouldNotOpenVault, error)
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
    with_error(MsgKey::CouldNotReadVault, error)
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
    with_error(MsgKey::CouldNotReadNoteState, error)
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
    with_error(MsgKey::CouldNotCreateVault, error)
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
    with_error(MsgKey::CouldNotPersistKanbanRecovery, error)
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
    with_error(MsgKey::CouldNotPersistRecovery, error)
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
    with_error(MsgKey::CouldNotPersistBoardRecovery, error)
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
    with_error(MsgKey::CouldNotReadRecovery, error)
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
    with_error(MsgKey::CouldNotRecreateNoteFolder, error)
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
    with_error(MsgKey::CouldNotRecreateNote, error)
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
    with_error(MsgKey::CouldNotReloadNote, error)
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
    with_error(MsgKey::CouldNotSaveCopy, error)
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
    with_error(MsgKey::CouldNotRenameNote, error)
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
    with_error(MsgKey::CouldNotRenameFolder, error)
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
    with_error(MsgKey::CouldNotDeleteNote, error)
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
    with_error(MsgKey::CouldNotDeleteFolder, error)
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
    phrase_current(MsgKey::CouldNotRestore, &[label, &error.to_string()])
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
    with_error(MsgKey::CouldNotEmptyTrash, error)
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
    with_error(MsgKey::CouldNotMoveNote, error)
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
    with_error(MsgKey::CouldNotCreateFolder, error)
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
    with_error(MsgKey::CouldNotCreateNote, error)
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
    with_error(MsgKey::CouldNotAddNoteTemplate, error)
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
    phrase_current(MsgKey::RefusingUnsafeRecoveryPath, &[rel])
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
    let name = name.unwrap_or_else(|| t(Key::KanbanUiThis));
    phrase_current(MsgKey::LinksNotUpdated, &[strip_pseudo(name)])
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
    phrase_current(MsgKey::CouldNotBeRead, &[strip_pseudo(what)])
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
    phrase_current(MsgKey::RenamedLinksFailed, &[&count.to_string(), list])
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
    phrase_current(MsgKey::MovedLinksFailed, &[&count.to_string(), list])
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
    phrase_current(MsgKey::NoteNotInVault, &[&id.to_string()])
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
    phrase_current(MsgKey::CannotOpenWithoutNote, &[target])
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
    phrase_current(MsgKey::AttachmentNotInVault, &[target])
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
    phrase_current(MsgKey::NotAFile, &[&path.to_string()])
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
    phrase_current(MsgKey::PathGone, &[&path.to_string()])
}

/// `<sentence> <path>: <error>. <hint>` for an OS file action.
fn file_action_error(
    key: MsgKey,
    path: impl std::fmt::Display,
    error: impl std::fmt::Display,
    hint: &str,
) -> String {
    phrase_current(key, &[&path.to_string(), &error.to_string(), hint])
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
    file_action_error(MsgKey::CouldNotOpenFile, path, error, hint)
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
    file_action_error(MsgKey::CouldNotRevealFile, path, error, hint)
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
    t(Key::ErrorNoFileOpener)
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
    phrase_current(
        MsgKey::CouldNotImportInto,
        &[&path.to_string(), &error.to_string()],
    )
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
    phrase_current(MsgKey::ImportRefused, &[list])
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
    phrase_current(
        MsgKey::ImportPartlyRefused,
        &[&imported.to_string(), &total.to_string(), list],
    )
}

/// `"Cannot import <names>: drop Markdown, image, PDF or office documents"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::cannot_import("a.txt, b.zip"),
///     "Cannot import a.txt, b.zip: drop Markdown, image, PDF or office documents"
/// );
/// ```
pub fn cannot_import(names: &str) -> String {
    phrase_current(MsgKey::CannotImport, &[names])
}

/// `"Importing <name>…"`.
///
/// The status line while a document converts on the worker thread.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_progress("report.docx"),
///     "Importing report.docx…"
/// );
/// ```
pub fn import_progress(name: &str) -> String {
    phrase_current(MsgKey::ImportProgress, &[name])
}

/// `"Imported <title>"`.
///
/// The success line once the note, its images and the source document are
/// inside the vault.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_document_notice("Quarterly Report"),
///     "Imported Quarterly Report"
/// );
/// ```
pub fn import_document_notice(title: &str) -> String {
    phrase_current(MsgKey::ImportDocumentNotice, &[title])
}

/// `"Could not import <name>: this file type cannot be converted."`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_unsupported("notes.bin"),
///     "Could not import notes.bin: this file type cannot be converted."
/// );
/// ```
pub fn import_unsupported(name: &str) -> String {
    phrase_current(MsgKey::ImportUnsupported, &[name])
}

/// Why a scanned PDF could not become a note, naming the OCR pages.
///
/// `pages` are 1-indexed and `page_count` is the document's page count.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_needs_ocr("scan.pdf", &[2, 3], 9),
///     "Could not import scan.pdf: pages 2, 3 of 9 are scanned images and need OCR, which Muninn does not do. Export a text version first."
/// );
/// ```
pub fn import_needs_ocr(name: &str, pages: &[u32], page_count: u32) -> String {
    let list = pages
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let key = if pages.len() == 1 {
        MsgKey::ImportNeedsOcrOne
    } else {
        MsgKey::ImportNeedsOcrOther
    };
    phrase_current(key, &[name, &list, &page_count.to_string()])
}

/// `"Could not import <name>: the document is encrypted. Remove its password and import it again."`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_encrypted("private.docx"),
///     "Could not import private.docx: the document is encrypted. Remove its password and import it again."
/// );
/// ```
pub fn import_encrypted(name: &str) -> String {
    phrase_current(MsgKey::ImportEncrypted, &[name])
}

/// `"Could not import <name>: the file is damaged, or it is not a readable document."`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_malformed("broken.docx"),
///     "Could not import broken.docx: the file is damaged, or it is not a readable document."
/// );
/// ```
pub fn import_malformed(name: &str) -> String {
    phrase_current(MsgKey::ImportMalformed, &[name])
}

/// `"Could not import <name>: the document is too complex to convert."`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_resource_limit("huge.xlsx"),
///     "Could not import huge.xlsx: the document is too complex to convert."
/// );
/// ```
pub fn import_resource_limit(name: &str) -> String {
    phrase_current(MsgKey::ImportResourceLimit, &[name])
}

/// `"Could not import <name>: the document is missing a part it needs."`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_missing_part("report.docx"),
///     "Could not import report.docx: the document is missing a part it needs."
/// );
/// ```
pub fn import_missing_part(name: &str) -> String {
    phrase_current(MsgKey::ImportMissingPart, &[name])
}

/// `"Could not import <name>: the file could not be read."`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_io("gone.docx"),
///     "Could not import gone.docx: the file could not be read."
/// );
/// ```
pub fn import_io(name: &str) -> String {
    phrase_current(MsgKey::ImportIo, &[name])
}

/// `"Could not import <name>: the document could not be converted."`.
///
/// The fallback for a parser failure the catalog does not model yet.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_failed("odd.docx"),
///     "Could not import odd.docx: the document could not be converted."
/// );
/// ```
pub fn import_failed(name: &str) -> String {
    phrase_current(MsgKey::ImportFailed, &[name])
}

/// `"Could not import <name>: the file is <size>, over the <limit> limit."`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_too_large("big.docx", 51 * 1024 * 1024, 50 * 1024 * 1024),
///     "Could not import big.docx: the file is 51 MB, over the 50 MB limit."
/// );
/// ```
pub fn import_too_large(name: &str, size: u64, limit: u64) -> String {
    phrase_current(
        MsgKey::ImportTooLarge,
        &[name, &file_size(size), &file_size(limit)],
    )
}

/// `"Could not import <name>: the converted note is <size>, over the <limit> limit."`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::import_body_too_large("wide.csv", 11 * 1024 * 1024, 10 * 1024 * 1024),
///     "Could not import wide.csv: the converted note is 11 MB, over the 10 MB limit."
/// );
/// ```
pub fn import_body_too_large(name: &str, size: usize, limit: usize) -> String {
    phrase_current(
        MsgKey::ImportBodyTooLarge,
        &[name, &file_size(size as u64), &file_size(limit as u64)],
    )
}

/// A compact byte count for the import limits (`51 MB`, `900 KB`, `12 B`).
fn file_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    if bytes >= MIB {
        format!("{:.0} MB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.0} KB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
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
    phrase_current(
        MsgKey::ScanCancelled,
        &[&done.to_string(), &total.to_string()],
    )
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
    phrase_current(MsgKey::BoardQuitRefused, &[why])
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
    let lang = Lang::current();
    let file_count = files;
    let files = counted_in(lang, files, MsgKey::FileOne, MsgKey::FileOther);
    // `Imported 2 files` has no verb agreement in English; Spanish needs the
    // plural verb (`Se importaron 2 archivos`), so the count picks the template
    // through the same plural category as the noun.
    let imported = match lang.plural_category(file_count as u64) {
        PluralCategory::One => MsgKey::ImportedFilesOne,
        PluralCategory::Other => MsgKey::ImportedFilesOther,
    };
    let mut text = render_current(imported, &[&files]);
    if notes > 0 {
        let notes = counted_in(lang, notes, MsgKey::NoteOne, MsgKey::NoteOther);
        text.push_str(&render_current(MsgKey::ImportJoin, &[&notes]));
    }
    if attachments {
        text.push_str(&render_current(MsgKey::ImportAttachmentsNote, &[]));
    }
    compose(text)
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
    phrase_current(MsgKey::RefusedNotAFile, &[name])
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
    phrase_current(MsgKey::RefusedUnsupportedType, &[name])
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
    phrase_current(MsgKey::RefusedAlreadyExists, &[name])
}

/// `"3 broken"`: the sidebar chip's count of unresolved wikilinks.
///
/// The count is data; the word stays in the catalog so a locale can place it.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::broken_links(3), "3 broken");
/// ```
pub fn broken_links(n: usize) -> String {
    phrase_current(MsgKey::BrokenLinksCount, &[&n.to_string()])
}

/// `"1 link"`, `"0 links"`, `"2 links"`.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::links(1), "1 link");
/// assert_eq!(muninn_i18n::links(2), "2 links");
/// ```
pub fn links(n: usize) -> String {
    counted(n, MsgKey::LinkOne, MsgKey::LinkOther)
}

/// The sidebar row's link summary: `"no links"`, `"2 backlinks"`,
/// `"1 link · 2 backlinks"`.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::note_links(0, 0), "no links");
/// assert_eq!(muninn_i18n::note_links(2, 0), "2 links");
/// assert_eq!(muninn_i18n::note_links(1, 2), "1 link · 2 backlinks");
/// ```
pub fn note_links(links: usize, backlinks: usize) -> String {
    let lang = Lang::current();
    match (links, backlinks) {
        (0, 0) => phrase_current(MsgKey::NoLinks, &[]),
        (0, b) => counted(b, MsgKey::BacklinkOne, MsgKey::BacklinkOther),
        (l, 0) => counted(l, MsgKey::LinkOne, MsgKey::LinkOther),
        (l, b) => {
            let links = counted_in(lang, l, MsgKey::LinkOne, MsgKey::LinkOther);
            let backlinks = counted_in(lang, b, MsgKey::BacklinkOne, MsgKey::BacklinkOther);
            phrase_current(MsgKey::NoteLinks, &[&links, &backlinks])
        }
    }
}

/// A sidebar row's subtitle: the link summary, prefixed by the folder when the
/// title alone would be ambiguous. `folder` is file data kept as written;
/// `""` means the vault root.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::note_links_in_folder("", 1, 0),
///     "vault root · 1 link"
/// );
/// assert_eq!(
///     muninn_i18n::note_links_in_folder("Projects", 2, 0),
///     "Projects · 2 links"
/// );
/// ```
pub fn note_links_in_folder(folder: &str, links: usize, backlinks: usize) -> String {
    let summary = note_links(links, backlinks);
    let summary = strip_pseudo(&summary);
    if folder.is_empty() {
        phrase_current(MsgKey::NoteLinksRoot, &[summary])
    } else {
        phrase_current(MsgKey::NoteLinksFolder, &[folder, summary])
    }
}

/// The sidebar footer: `"12 notes · 3 links"`.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::sidebar_summary(12, 3), "12 notes · 3 links");
/// ```
pub fn sidebar_summary(notes: usize, links: usize) -> String {
    let lang = Lang::current();
    let notes = counted_in(lang, notes, MsgKey::NoteOne, MsgKey::NoteOther);
    let links = counted_in(lang, links, MsgKey::LinkOne, MsgKey::LinkOther);
    phrase_current(MsgKey::SidebarSummary, &[&notes, &links])
}

/// `"New note in <place>"`, where `place` is a folder name or
/// `"the vault root"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::new_note_in("the vault root"),
///     "New note in the vault root"
/// );
/// ```
pub fn new_note_in(place: &str) -> String {
    phrase_current(MsgKey::NewNoteIn, &[strip_pseudo(place)])
}

/// `"New folder in <place>"`, where `place` is a folder name or
/// `"the vault root"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::new_folder_in("Projects"),
///     "New folder in Projects"
/// );
/// ```
pub fn new_folder_in(place: &str) -> String {
    phrase_current(MsgKey::NewFolderIn, &[strip_pseudo(place)])
}

/// The theme button's accessible label: `"Theme: Dark — click to cycle
/// (System, Dark, Light)"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::theme_label("Dark"),
///     "Theme: Dark — click to cycle (System, Dark, Light)"
/// );
/// ```
pub fn theme_label(theme: &str) -> String {
    phrase_current(MsgKey::ThemeLabel, &[strip_pseudo(theme)])
}

/// `"2 notes link here"`: the rename/move confirmation's link-impact count.
/// `place` is catalogued copy such as [`Key::LinkImpactHere`].
///
/// # Examples
///
/// ```
/// use muninn_i18n::{Key, link_impact, t};
///
/// assert_eq!(link_impact(1, t(Key::LinkImpactHere)), "1 note links here");
/// ```
pub fn link_impact(referrers: usize, place: &str) -> String {
    let count = referrers.to_string();
    let key = if referrers == 1 {
        MsgKey::LinkImpactOne
    } else {
        MsgKey::LinkImpactOther
    };
    phrase_current(key, &[&count, strip_pseudo(place)])
}

/// `"Show Muninn with <chord>"`, the global-shortcut switch's label.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::show_with_chord("Ctrl+Alt+M"),
///     "Show Muninn with Ctrl+Alt+M"
/// );
/// ```
pub fn show_with_chord(chord: &str) -> String {
    phrase_current(MsgKey::ShowWithChord, &[chord])
}

/// `"<chord> could not be registered: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::shortcut_failed("Ctrl+Alt+M", "taken"),
///     "Ctrl+Alt+M could not be registered: taken"
/// );
/// ```
pub fn shortcut_failed(chord: &str, error: impl std::fmt::Display) -> String {
    phrase_current(MsgKey::ShortcutFailed, &[chord, &error.to_string()])
}

/// `"On. <chord> shows Muninn from any app."`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::shortcut_on("Ctrl+Alt+M"),
///     "On. Ctrl+Alt+M shows Muninn from any app."
/// );
/// ```
pub fn shortcut_on(chord: &str) -> String {
    phrase_current(MsgKey::ShortcutOn, &[chord])
}

/// `"Autosave writes 1.50 s after you stop typing."`, with the delay kept to
/// two decimals as the Settings page shows it.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::autosave_help(1.5),
///     "Autosave writes 1.50 s after you stop typing."
/// );
/// ```
pub fn autosave_help(delay: f64) -> String {
    phrase_current(MsgKey::AutosaveHelp, &[&format!("{delay:.2}")])
}

/// `"Version 0.8.0."`.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::version("0.8.0"), "Version 0.8.0.");
/// ```
pub fn version(version: &str) -> String {
    phrase_current(MsgKey::Version, &[version])
}

/// `"Muninn 0.10.0 is available"`, the release banner's line.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::update_available("0.10.0"),
///     "Muninn 0.10.0 is available"
/// );
/// ```
pub fn update_available(version: &str) -> String {
    phrase_current(MsgKey::UpdateAvailable, &[version])
}

/// `"Muninn 0.11.0 is installed. Restart Muninn to use it."`, the self-update
/// success notice.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::update_installed("0.11.0"),
///     "Muninn 0.11.0 is installed. Restart Muninn to use it."
/// );
/// ```
pub fn update_installed(version: &str) -> String {
    phrase_current(MsgKey::UpdateInstalled, &[version])
}

/// `"Update failed: <detail>"`, the self-update failure notice.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::update_failed("SHA-256 mismatch"),
///     "Update failed: SHA-256 mismatch"
/// );
/// ```
pub fn update_failed(detail: &str) -> String {
    phrase_current(MsgKey::UpdateFailed, &[detail])
}

/// `"New vault created at <path> — make your first note"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::welcome_vault_created("~/notes"),
///     "New vault created at ~/notes — make your first note"
/// );
/// ```
pub fn welcome_vault_created(path: &str) -> String {
    phrase_current(MsgKey::WelcomeVaultCreated, &[path])
}

/// `"Scanning 3/12 notes — Esc to cancel"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::scan_progress(3, 12),
///     "Scanning 3/12 notes — Esc to cancel"
/// );
/// ```
pub fn scan_progress(done: usize, total: usize) -> String {
    phrase_current(
        MsgKey::ScanProgress,
        &[&done.to_string(), &total.to_string()],
    )
}

/// `"Hide the chrome and keep the caret's line centred (<chord>)"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::focus_help("⌘⇧F"),
///     "Hide the chrome and keep the caret's line centred (⌘⇧F)"
/// );
/// ```
pub fn focus_help(chord: &str) -> String {
    phrase_current(MsgKey::FocusHelp, &[chord])
}

/// `"Back to the note's chrome (<chord>)"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::focus_exit_help("⌘⇧F"),
///     "Back to the note's chrome (⌘⇧F)"
/// );
/// ```
pub fn focus_exit_help(chord: &str) -> String {
    phrase_current(MsgKey::FocusExitHelp, &[chord])
}

/// `"Kanban · <folder>"`, the palette's folder subtitle for a kanban. `folder`
/// is file data kept as written.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::palette_kanban_folder("Projects"),
///     "Kanban · Projects"
/// );
/// ```
pub fn palette_kanban_folder(folder: &str) -> String {
    phrase_current(MsgKey::PaletteKanbanFolder, &[folder])
}

/// `"1 result"`, `"0 results"`, `"2 results"`.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::results(1), "1 result");
/// assert_eq!(muninn_i18n::results(2), "2 results");
/// ```
pub fn results(n: usize) -> String {
    counted(n, MsgKey::ResultOne, MsgKey::ResultOther)
}

/// `"1 element"`, `"0 elements"`, `"12 elements"`.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::elements(1), "1 element");
/// assert_eq!(muninn_i18n::elements(12), "12 elements");
/// ```
pub fn elements(n: usize) -> String {
    counted(n, MsgKey::ElementOne, MsgKey::ElementOther)
}

/// `"1 element · 2 kept as-is"`: a board's summary when the file holds more
/// elements than the model draws. `elements` is [`elements`]' sentence.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::board_kept_as_is("1 element", 2),
///     "1 element · 2 kept as-is"
/// );
/// ```
pub fn board_kept_as_is(elements: &str, raw: usize) -> String {
    phrase_current(
        MsgKey::BoardKeptAsIs,
        &[strip_pseudo(elements), &raw.to_string()],
    )
}

/// `"Could not read: <error>"`, a board list row's detail.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::board_could_not_read("bad json"),
///     "Could not read: bad json"
/// );
/// ```
pub fn board_could_not_read(error: impl std::fmt::Display) -> String {
    with_error(MsgKey::BoardCouldNotRead, error)
}

/// The Board canvas's accessible name: `"Board canvas, 2 elements, rectangle
/// tool"`, plus `", 1 selected"` when something is picked. `elements` is
/// [`elements`]' sentence; `tool` is the tool's stable name.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::board_canvas("2 elements", "rectangle", 0),
///     "Board canvas, 2 elements, rectangle tool"
/// );
/// assert_eq!(
///     muninn_i18n::board_canvas("1 element", "hand", 1),
///     "Board canvas, 1 element, hand tool, 1 selected"
/// );
/// ```
pub fn board_canvas(elements: &str, tool: &str, selected: usize) -> String {
    let elements = strip_pseudo(elements);
    if selected > 0 {
        phrase_current(
            MsgKey::BoardCanvasSelected,
            &[elements, tool, &selected.to_string()],
        )
    } else {
        phrase_current(MsgKey::BoardCanvas, &[elements, tool])
    }
}

/// `"Unsaved · ⌘S saves"`, the Board and Kanban page's unsaved indicator.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::unsaved_saves("⌘S"),
///     "Unsaved · ⌘S saves"
/// );
/// ```
pub fn unsaved_saves(chord: &str) -> String {
    phrase_current(MsgKey::UnsavedSaves, &[chord])
}

/// `"<title> (root)"`: a duplicate title, identified by the vault root.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::note_label_root("Plan"), "Plan (root)");
/// ```
pub fn note_label_root(title: &str) -> String {
    phrase_current(MsgKey::NoteLabelRoot, &[title])
}

/// `"<title> (<parent>)"`: a duplicate-title graph node's folder.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::graph_node_parent("Plan", "Projects"),
///     "Plan (Projects)"
/// );
/// ```
pub fn graph_node_parent(title: &str, parent: &str) -> String {
    phrase_current(MsgKey::GraphNodeParent, &[title, parent])
}

/// `"12 notes · 30 edges"`. The readout keeps the plural words at any count,
/// as the graph's overlay always has.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::graph_stats(12, 30), "12 notes · 30 edges");
/// assert_eq!(muninn_i18n::graph_stats(1, 1), "1 notes · 1 edges");
/// ```
pub fn graph_stats(nodes: usize, edges: usize) -> String {
    phrase_current(
        MsgKey::GraphStats,
        &[&nodes.to_string(), &edges.to_string()],
    )
}

/// `"1 outgoing link · 2 backlinks"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::graph_links(1, 2),
///     "1 outgoing link · 2 backlinks"
/// );
/// ```
pub fn graph_links(links: usize, backlinks: usize) -> String {
    let lang = Lang::current();
    let links = counted_in(
        lang,
        links,
        MsgKey::GraphOutgoingLinkOne,
        MsgKey::GraphOutgoingLinkOther,
    );
    let backlinks = counted_in(lang, backlinks, MsgKey::BacklinkOne, MsgKey::BacklinkOther);
    phrase_current(MsgKey::GraphLinks, &[&links, &backlinks])
}

/// `"from 2026-11-01"`: the kanban filter summary's range start. `day` is the
/// formatted date.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::kanban_filter_from("2026-11-01"),
///     "from 2026-11-01"
/// );
/// ```
pub fn kanban_filter_from(day: &str) -> String {
    phrase_current(MsgKey::KanbanFilterFrom, &[day])
}

/// `"to 2026-11-30"`: the kanban filter summary's range end.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::kanban_filter_to("2026-11-30"),
///     "to 2026-11-30"
/// );
/// ```
pub fn kanban_filter_to(day: &str) -> String {
    phrase_current(MsgKey::KanbanFilterTo, &[day])
}

/// `"2026-11-01 – 2026-11-30"`: the kanban filter summary's full range.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::kanban_filter_range("2026-11-01", "2026-11-30"),
///     "2026-11-01 – 2026-11-30"
/// );
/// ```
pub fn kanban_filter_range(from: &str, to: &str) -> String {
    phrase_current(MsgKey::KanbanFilterRange, &[from, to])
}

/// `"Day 14"`: the date picker's per-day accessible name.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::day_label(14), "Day 14");
/// ```
pub fn day_label(day: u32) -> String {
    phrase_current(MsgKey::DayLabel, &[&day.to_string()])
}

/// `"2026-11-14 · today"`: a card date with its relative day.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::relative_date("2026-11-14", "today"),
///     "2026-11-14 · today"
/// );
/// ```
pub fn relative_date(shown: &str, relative: &str) -> String {
    phrase_current(MsgKey::RelativeDate, &[shown, strip_pseudo(relative)])
}

/// `"1 archived card"`, `"3 archived cards"`.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::archived_cards(1), "1 archived card");
/// assert_eq!(muninn_i18n::archived_cards(3), "3 archived cards");
/// ```
pub fn archived_cards(n: usize) -> String {
    counted(n, MsgKey::ArchivedCardOne, MsgKey::ArchivedCardOther)
}

/// `"Lane actions: To do"`, a lane menu's accessible name.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::lane_actions("To do"), "Lane actions: To do");
/// ```
pub fn lane_actions(title: &str) -> String {
    phrase_current(MsgKey::LaneActions, &[title])
}

/// `"To do controls"`, a lane header's accessible name.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::lane_controls("To do"), "To do controls");
/// ```
pub fn lane_controls(title: &str) -> String {
    phrase_current(MsgKey::LaneControls, &[title])
}

/// `"Collapse lane: To do"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::collapse_lane("To do"),
///     "Collapse lane: To do"
/// );
/// ```
pub fn collapse_lane(title: &str) -> String {
    phrase_current(MsgKey::CollapseLane, &[title])
}

/// `"Expand lane: To do"`.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::expand_lane("To do"), "Expand lane: To do");
/// ```
pub fn expand_lane(title: &str) -> String {
    phrase_current(MsgKey::ExpandLane, &[title])
}

/// `"To do cards"`, a lane's card list's accessible name.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::lane_cards("To do"), "To do cards");
/// ```
pub fn lane_cards(title: &str) -> String {
    phrase_current(MsgKey::LaneCards, &[title])
}

/// `"Add card to To do"`.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::add_card_to_lane("To do"), "Add card to To do");
/// ```
pub fn add_card_to_lane(title: &str) -> String {
    phrase_current(MsgKey::AddCardToLane, &[title])
}

/// A lane region's label: `"To do, 3 cards"`, `", over limit"` and
/// `", done lane"` appended when they apply. `count` is the drawn count.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::lane_label("To do", "3", false, false),
///     "To do, 3 cards"
/// );
/// assert_eq!(
///     muninn_i18n::lane_label("Done", "2", true, true),
///     "Done, 2 cards, over limit, done lane"
/// );
/// ```
pub fn lane_label(title: &str, count: &str, over: bool, done: bool) -> String {
    let mut label = render_current(MsgKey::LaneLabel, &[title, count]);
    push_lane_flags(&mut label, over, done);
    compose(label)
}

/// The List view's lane region label: `"To do lane, 3 cards"`, with the same
/// suffixes as [`lane_label`].
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::lane_region_label("To do", "3", false, false),
///     "To do lane, 3 cards"
/// );
/// ```
pub fn lane_region_label(title: &str, count: &str, over: bool, done: bool) -> String {
    let mut label = render_current(MsgKey::LaneRegionLabel, &[title, count]);
    push_lane_flags(&mut label, over, done);
    compose(label)
}

/// A collapsed lane's label: `"To do, 3 cards, collapsed"`, and
/// `", done lane"` when it is the done lane.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::collapsed_lane_label("To do", "3", false),
///     "To do, 3 cards, collapsed"
/// );
/// ```
pub fn collapsed_lane_label(title: &str, count: &str, done: bool) -> String {
    let mut label = render_current(MsgKey::CollapsedLaneLabel, &[title, count]);
    if done {
        label.push_str(&render_current(MsgKey::LaneDone, &[]));
    }
    compose(label)
}

/// `"Collapsed · 3 cards"`, the List view's collapsed lane line.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::collapsed_cards(3),
///     "Collapsed · 3 cards"
/// );
/// ```
pub fn collapsed_cards(n: usize) -> String {
    phrase_current(MsgKey::CollapsedCards, &[&n.to_string()])
}

fn push_lane_flags(label: &mut String, over: bool, done: bool) {
    if over {
        label.push_str(&render_current(MsgKey::LaneOverLimit, &[]));
    }
    if done {
        label.push_str(&render_current(MsgKey::LaneDone, &[]));
    }
}

/// The delete-lane question when no note text is lost:
/// `"Delete “To do” and its 2 cards? Any note text in it stays in the file."`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::delete_lane("To do", 1),
///     "Delete “To do” and its card? Any note text in it stays in the file."
/// );
/// ```
pub fn delete_lane(title: &str, cards: usize) -> String {
    let cards = delete_cards(cards);
    phrase_current(MsgKey::DeleteLane, &[title, &cards])
}

/// The delete-lane question when note text is lost too:
/// `"Delete “To do” and its card? 1 line of note text the board doesn't show
/// will be deleted too, starting “…”."`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::delete_lane_text("To do", 1, "1 line", "hidden"),
///     "Delete “To do” and its card? 1 line of note text the board doesn't show will be deleted too, starting “hidden”."
/// );
/// ```
pub fn delete_lane_text(title: &str, cards: usize, lines: &str, preview: &str) -> String {
    let cards = delete_cards(cards);
    phrase_current(
        MsgKey::DeleteLaneText,
        &[title, &cards, strip_pseudo(lines), preview],
    )
}

fn delete_cards(cards: usize) -> String {
    match cards {
        0 => String::new(),
        1 => render_current(MsgKey::DeleteCardsOne, &[]),
        n => render_current(MsgKey::DeleteCardsOther, &[&n.to_string()]),
    }
}

/// `"1 line"`, `"0 lines"`, `"3 lines"`.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::lines(1), "1 line");
/// assert_eq!(muninn_i18n::lines(3), "3 lines");
/// ```
pub fn lines(n: usize) -> String {
    counted(n, MsgKey::LineOne, MsgKey::LineOther)
}

/// A card's own accessible label: `"Done: Ship it"`, or `"Open, empty card"`
/// when it has no text. `state` is [`Key::KanbanFilterDone`] or
/// [`Key::KanbanFilterOpen`].
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::card_label("Done", "Ship it"), "Done: Ship it");
/// assert_eq!(muninn_i18n::card_label("Open", ""), "Open, empty card");
/// ```
pub fn card_label(state: &str, first: &str) -> String {
    if first.trim().is_empty() {
        phrase_current(MsgKey::CardLabelEmpty, &[strip_pseudo(state)])
    } else {
        phrase_current(MsgKey::CardLabel, &[strip_pseudo(state), first])
    }
}

/// A card menu's accessible name: `"Card actions: Ship it"`, or `"Card
/// actions: empty card"` when it has no text.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::card_actions("Ship it"),
///     "Card actions: Ship it"
/// );
/// ```
pub fn card_actions(first: &str) -> String {
    if first.trim().is_empty() {
        phrase_current(MsgKey::CardActionsEmpty, &[])
    } else {
        phrase_current(MsgKey::CardActions, &[first])
    }
}

/// `"Card actions: empty card in To do"`, the List view's empty-card menu.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::card_actions_in_lane("To do"),
///     "Card actions: empty card in To do"
/// );
/// ```
pub fn card_actions_in_lane(lane: &str) -> String {
    phrase_current(MsgKey::CardActionsInLane, &[lane])
}

/// `"1 of 2 done"`, a checklist's progress readout.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::checklist_progress(1, 2), "1 of 2 done");
/// ```
pub fn checklist_progress(done: usize, total: usize) -> String {
    phrase_current(
        MsgKey::ChecklistProgress,
        &[&done.to_string(), &total.to_string()],
    )
}

/// `"Checklist, 1 of 2 done"`, the checklist list's accessible name.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::checklist_label(1, 2),
///     "Checklist, 1 of 2 done"
/// );
/// ```
pub fn checklist_label(done: usize, total: usize) -> String {
    phrase_current(
        MsgKey::ChecklistLabel,
        &[&done.to_string(), &total.to_string()],
    )
}

/// `"Not saved: <reason>"`, the Board and Kanban page's failed-save status.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::not_saved("the file was removed"),
///     "Not saved: the file was removed"
/// );
/// ```
pub fn not_saved(reason: &str) -> String {
    phrase_current(MsgKey::NotSaved, &[reason])
}

/// `"Could not <action>: <error>"`, where `action` is catalogued copy such as
/// [`Key::KanbanEditAddCard`].
///
/// # Examples
///
/// ```
/// use muninn_i18n::{Key, could_not_do, t};
///
/// assert_eq!(
///     could_not_do(t(Key::KanbanEditAddCard), "read-only"),
///     "Could not add the card: read-only"
/// );
/// ```
pub fn could_not_do(action: &str, error: impl std::fmt::Display) -> String {
    phrase_current(
        MsgKey::CouldNotDo,
        &[strip_pseudo(action), &error.to_string()],
    )
}

/// `"Could not create a board: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_create_board("denied"),
///     "Could not create a board: denied"
/// );
/// ```
pub fn could_not_create_board(error: impl std::fmt::Display) -> String {
    with_error(MsgKey::CouldNotCreateBoard, error)
}

/// `"Could not create a kanban: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_create_kanban("denied"),
///     "Could not create a kanban: denied"
/// );
/// ```
pub fn could_not_create_kanban(error: impl std::fmt::Display) -> String {
    with_error(MsgKey::CouldNotCreateKanban, error)
}

/// `"Could not convert to a kanban: <error>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::could_not_convert_kanban("has text"),
///     "Could not convert to a kanban: has text"
/// );
/// ```
pub fn could_not_convert_kanban(error: impl std::fmt::Display) -> String {
    with_error(MsgKey::CouldNotConvertKanban, error)
}

/// `"Open <name>"`, a link's tooltip. `name` is the link's own display name.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::link_description("Plan"), "Open Plan");
/// ```
pub fn link_description(name: &str) -> String {
    phrase_current(MsgKey::LinkDescription, &[name])
}

/// `"1 board"`, `"0 boards"`, `"4 boards"`.
///
/// # Examples
///
/// ```
/// assert_eq!(muninn_i18n::boards(1), "1 board");
/// assert_eq!(muninn_i18n::boards(4), "4 boards");
/// ```
pub fn boards(n: usize) -> String {
    counted(n, MsgKey::BoardOne, MsgKey::BoardOther)
}

/// `"To do · Done: Ship it"`: a List or Table row's accessible label, naming
/// the lane first. `state` is [`Key::KanbanFilterDone`] or
/// [`Key::KanbanFilterOpen`].
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::card_label_in_lane("To do", "Done", "Ship it"),
///     "To do · Done: Ship it"
/// );
/// ```
pub fn card_label_in_lane(lane: &str, state: &str, first: &str) -> String {
    phrase_current(MsgKey::CardLabelInLane, &[lane, strip_pseudo(state), first])
}

/// `"<path> is outside the vault"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::outside_vault("/tmp/notes"),
///     "/tmp/notes is outside the vault"
/// );
/// ```
pub fn outside_vault(path: impl std::fmt::Display) -> String {
    phrase_current(MsgKey::OutsideVault, &[&path.to_string()])
}

/// `"<path> is not a board inside the vault"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::not_a_board_in_vault("Plan.excalidraw"),
///     "Plan.excalidraw is not a board inside the vault"
/// );
/// ```
pub fn not_a_board_in_vault(path: impl std::fmt::Display) -> String {
    phrase_current(MsgKey::NotABoardInVault, &[&path.to_string()])
}

/// `"<name> already exists"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::already_exists("Plan.md"),
///     "Plan.md already exists"
/// );
/// ```
pub fn already_exists(name: impl std::fmt::Display) -> String {
    phrase_current(MsgKey::AlreadyExists, &[&name.to_string()])
}

/// `"<path> is not a trash path"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::not_a_trash_path("../Plan.md"),
///     "../Plan.md is not a trash path"
/// );
/// ```
pub fn not_a_trash_path(path: impl std::fmt::Display) -> String {
    phrase_current(MsgKey::NotATrashPath, &[&path.to_string()])
}

/// `"<path> is no longer in the trash"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::no_longer_in_trash("1/Plan.md"),
///     "1/Plan.md is no longer in the trash"
/// );
/// ```
pub fn no_longer_in_trash(path: impl std::fmt::Display) -> String {
    phrase_current(MsgKey::NoLongerInTrash, &[&path.to_string()])
}

/// `"<path> is not a folder"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::not_a_folder("Notes.md"),
///     "Notes.md is not a folder"
/// );
/// ```
pub fn not_a_folder(path: impl std::fmt::Display) -> String {
    phrase_current(MsgKey::NotAFolder, &[&path.to_string()])
}

/// `"a board name <why>"`, where `why` is a catalogued reason such as
/// [`Key::CoreBoardNameEmpty`].
///
/// # Examples
///
/// ```
/// use muninn_i18n::{Key, board_name_invalid, t};
///
/// assert_eq!(
///     board_name_invalid(t(Key::CoreBoardNameEmpty)),
///     "a board name cannot be empty"
/// );
/// ```
pub fn board_name_invalid(why: &str) -> String {
    phrase_current(MsgKey::BoardNameInvalid, &[strip_pseudo(why)])
}

/// `"scene <key> must be an object"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::board_scene_key_object("appState"),
///     "scene appState must be an object"
/// );
/// ```
pub fn board_scene_key_object(key: &str) -> String {
    phrase_current(MsgKey::BoardSceneKeyObject, &[key])
}

/// `"Renamed, but 2 note(s) embedding it could not be updated: a.md, b.md"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::board_renamed_embeds_failed(1, "a.md"),
///     "Renamed, but 1 note(s) embedding it could not be updated: a.md"
/// );
/// ```
pub fn board_renamed_embeds_failed(count: usize, list: &str) -> String {
    phrase_current(
        MsgKey::BoardRenamedEmbedsFailed,
        &[&count.to_string(), list],
    )
}

/// `"<target> (missing)"`, a board note link whose note is gone.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::board_note_link_missing("Plan"),
///     "Plan (missing)"
/// );
/// ```
pub fn board_note_link_missing(target: &str) -> String {
    phrase_current(MsgKey::BoardNoteLinkMissing, &[target])
}

/// `"reg.exe could not run: <error>"`: the Windows login-item command.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::autostart_reg_failed("access denied"),
///     "reg.exe could not run: access denied"
/// );
/// ```
pub fn autostart_reg_failed(error: impl std::fmt::Display) -> String {
    with_error(MsgKey::AutostartRegFailed, error)
}

/// `"reg.exe exited with <status>"`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     muninn_i18n::autostart_reg_exit("exit code: 1"),
///     "reg.exe exited with exit code: 1"
/// );
/// ```
pub fn autostart_reg_exit(status: impl std::fmt::Display) -> String {
    with_error(MsgKey::AutostartRegExit, status)
}

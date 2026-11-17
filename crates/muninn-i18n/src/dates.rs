//! Month, weekday and AM/PM names for the display language.
//!
//! These are the display tables only. Muninn's files stay English and ASCII
//! (`@{2026-11-12}`, `MMMM` in a `date-format`, the archive heading), so a
//! translated UI never rewrites a note. `muninn-core` keeps a separate English
//! table for `format_date_for_file`, `format_time_for_file` and parsing;
//! `format_date_for_display`/`format_time_for_display` read [`DateNames`].
//! `Do` still spells an English ordinal, and will until a locale pack supplies
//! its own rule (#154).
//!
//! The values are byte-identical to `MONTHS`/`WEEKDAYS` in
//! `crates/muninn-core/src/kanban/dates.rs`, so using `EN_US` changes no
//! output.

use crate::Lang;

/// Localised date words and the first day of the week.
///
/// `weekdays` is indexed like `CivilDate::weekday()`: Sunday is 0. A date
/// picker reads `week_start` instead of hardcoding Sunday, and the header row
/// uses [`DateNames::short_weekday`] or, for the picker, the narrower
/// [`DateNames::narrow_weekday`] over a grid column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DateNames {
    /// Full month names, January (index 0) through December.
    pub months: [&'static str; 12],
    /// Full weekday names, Sunday (index 0) through Saturday.
    pub weekdays: [&'static str; 7],
    /// Short weekday names for a calendar header, Sunday (index 0) through
    /// Saturday. Kept separate from `weekdays` because a language abbreviates
    /// by its own rule, not by cutting three characters off the full name.
    pub weekdays_short: [&'static str; 7],
    /// Narrow weekday labels for the date picker's header, Sunday (index 0)
    /// through Saturday. en-US keeps the single initials Muninn has always
    /// drawn; a language whose initials collide supplies its own labels.
    pub weekdays_narrow: [&'static str; 7],
    /// Index into `weekdays` of the first column in a calendar grid. Must be
    /// 0–6; [`DateNames::column`], [`DateNames::short_weekday`] and
    /// [`DateNames::narrow_weekday`] rotate by it.
    pub week_start: u8,
    /// The morning marker used by 12-hour clocks.
    pub am: &'static str,
    /// The afternoon marker used by 12-hour clocks.
    pub pm: &'static str,
}

impl DateNames {
    /// The calendar-grid column of a weekday when `week_start` is the first
    /// column.
    ///
    /// `weekday` is a `CivilDate::weekday()` value: Sunday is 0, Saturday is
    /// 6. For `en-US` (week start Sunday) the column is the weekday itself; a
    /// Monday-first table moves Sunday to column 6.
    ///
    /// # Examples
    ///
    /// ```
    /// use muninn_i18n::EN_US;
    ///
    /// assert_eq!(EN_US.column(0), 0, "Sunday is the first column");
    /// assert_eq!(EN_US.column(6), 6, "Saturday is the last");
    /// ```
    #[must_use]
    pub fn column(&self, weekday: u8) -> usize {
        (usize::from(weekday) + 7 - usize::from(self.week_start)) % 7
    }

    /// The short weekday label for a calendar-grid column `0..7`, starting at
    /// `week_start`.
    ///
    /// # Examples
    ///
    /// ```
    /// use muninn_i18n::EN_US;
    ///
    /// assert_eq!(EN_US.short_weekday(0), "Sun");
    /// assert_eq!(EN_US.short_weekday(6), "Sat");
    /// ```
    #[must_use]
    pub fn short_weekday(&self, column: usize) -> &'static str {
        self.weekdays_short[(column + usize::from(self.week_start)) % 7]
    }

    /// The narrow label for grid column `0..7`: [`DateNames::weekdays_narrow`]
    /// rotated by [`DateNames::week_start`].
    ///
    /// The date picker's header uses this, so en-US keeps Muninn's single
    /// initials (`S M T W T F S`) and only a locale that supplies different
    /// narrow labels changes them.
    ///
    /// # Examples
    ///
    /// ```
    /// use muninn_i18n::EN_US;
    ///
    /// assert_eq!(EN_US.narrow_weekday(0), "S");
    /// assert_eq!(EN_US.narrow_weekday(6), "S");
    /// ```
    #[must_use]
    pub fn narrow_weekday(&self, column: usize) -> &'static str {
        self.weekdays_narrow[(column + usize::from(self.week_start)) % 7]
    }
}

/// English (United States) date names: Sunday-first weeks and `AM`/`PM`.
pub const EN_US: DateNames = DateNames {
    months: [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    weekdays: [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ],
    weekdays_short: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    weekdays_narrow: ["S", "M", "T", "W", "T", "F", "S"],
    week_start: 0,
    am: "AM",
    pm: "PM",
};

/// Spanish (Spain) date names: Monday-first weeks, lower-case months and
/// weekdays, and Spain's dotted `a. m.`/`p. m.` markers.
///
/// Spain writes day and month names in lower case (`12 de noviembre`,
/// `domingo`), so the table keeps them lower case and the display formatter's
/// capitalisation rules stay as they are. `week_start` is Monday, the CLDR
/// first day for `es-ES`. The two-letter weekday labels are the forms a
/// Spanish calendar header uses; the narrow row follows the Spanish one-letter
/// convention (`X` for miércoles, so `M` stays martes and `mié` stays
/// distinct).
pub const ES: DateNames = DateNames {
    months: [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ],
    weekdays: [
        "domingo",
        "lunes",
        "martes",
        "miércoles",
        "jueves",
        "viernes",
        "sábado",
    ],
    weekdays_short: ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"],
    weekdays_narrow: ["D", "L", "M", "X", "J", "V", "S"],
    week_start: 1,
    am: "a. m.",
    pm: "p. m.",
};

/// French (France) date names: Monday-first weeks, lower-case months and
/// weekdays, and the 24-hour `AM`/`PM` markers.
///
/// France writes day and month names in lower case (`12 novembre`,
/// `dimanche`), so the table keeps them lower case and the display
/// formatter's capitalisation rules stay as they are. `week_start` is Monday,
/// the CLDR first day for `fr-FR`. The narrow row is the CLDR French one:
/// `D L M M J V S`, with two `M`s (mardi and mercredi share an initial), so
/// the date picker's narrow header repeats `M`. That is expected French
/// behaviour, not a typo; the two-letter short row keeps `mar` and `mer`
/// distinct. A 24-hour clock ignores `am`/`pm`, but the table still supplies
/// the labels a 12-hour surface would ask for.
pub const FR: DateNames = DateNames {
    months: [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ],
    weekdays: [
        "dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi",
    ],
    weekdays_short: ["dim", "lun", "mar", "mer", "jeu", "ven", "sam"],
    weekdays_narrow: ["D", "L", "M", "M", "J", "V", "S"],
    week_start: 1,
    am: "AM",
    pm: "PM",
};

/// The display date names for `lang`.
///
/// The UI's date sites call this with [`Lang::current`] instead of naming
/// [`EN_US`], so a locale pack adds its `&'static DateNames` here and every
/// picker, card foot and table follows. [`Lang::Pseudo`] follows `en-US`: its
/// job is to mark catalog copy, not to fake a date language.
///
/// File formatting never calls this. `muninn-core`'s file formatters keep
/// their own English table, so a translated UI never rewrites a note.
///
/// # Examples
///
/// ```
/// use muninn_i18n::{EN_US, Lang, date_names};
///
/// assert_eq!(*date_names(Lang::EnUs), EN_US);
/// assert_eq!(*date_names(Lang::Pseudo), EN_US);
/// assert_eq!(date_names(Lang::Es).months[0], "enero");
/// assert_eq!(date_names(Lang::Fr).months[0], "janvier");
/// ```
#[must_use]
pub const fn date_names(lang: Lang) -> &'static DateNames {
    match lang {
        Lang::EnUs | Lang::Pseudo => &EN_US,
        Lang::Es => &ES,
        Lang::Fr => &FR,
    }
}

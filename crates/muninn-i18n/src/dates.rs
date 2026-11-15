//! Month, weekday and AM/PM names for the display language.
//!
//! These are the display tables only. Muninn's files stay English and ASCII
//! (`@{2026-11-12}`, `MMMM` in a `date-format`, the archive heading), so a
//! translated UI never rewrites a note. Splitting the formatter into explicit
//! file and display paths in `muninn-core`, and moving English ordinals
//! (`Do`) into this table, is slice #154.
//!
//! The values are byte-identical to `MONTHS`/`WEEKDAYS` in
//! `crates/muninn-core/src/kanban/dates.rs`, so using `EN_US` changes no
//! output.

/// Localised date words and the first day of the week.
///
/// `weekdays` is indexed like `CivilDate::weekday()`: Sunday is 0. A date
/// picker reads `week_start` instead of hardcoding Sunday, and the header row
/// uses `weekdays`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DateNames {
    /// Full month names, January (index 0) through December.
    pub months: [&'static str; 12],
    /// Full weekday names, Sunday (index 0) through Saturday.
    pub weekdays: [&'static str; 7],
    /// Index into `weekdays` of the first column in a calendar grid.
    pub week_start: u8,
    /// The morning marker used by 12-hour clocks.
    pub am: &'static str,
    /// The afternoon marker used by 12-hour clocks.
    pub pm: &'static str,
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
    week_start: 0,
    am: "AM",
    pm: "PM",
};

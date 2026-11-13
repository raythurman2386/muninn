//! Civil dates and times, the small date/time formatter, and the date
//! settings a board resolves (slice 9).
//!
//! `docs/kanban-format.md` section 7 fixes the tokens and section 9 the
//! setting names. There is no dependency and no locale data: month and
//! weekday names are English, the language of a new file.

use serde_json::{Map, Value};

use super::archive::{
    ARCHIVE_DATE_FORMAT, ARCHIVE_DATE_POSITION, ARCHIVE_DATE_SEPARATOR, ARCHIVE_WITH_DATE,
    MAX_ARCHIVE_CARDS,
};
use super::metadata::{LINKED_PAGE_METADATA, SHOW_MISSING_METADATA};
use super::tags::{HIDE_TAGS_IN_TITLE, SHOW_TAGS_AT_BOTTOM, TAG_COLORS, TagSettings, tag_colors};

/// The footer key: the text that starts a date (`@` by default).
pub const DATE_TRIGGER: &str = "date-trigger";
/// The footer key: the text that starts a time (`@@` by default).
pub const TIME_TRIGGER: &str = "time-trigger";
/// The footer key: how a new date is written inside its braces.
pub const DATE_FORMAT: &str = "date-format";
/// The footer key: how a new time is written inside its braces.
pub const TIME_FORMAT: &str = "time-format";
/// The footer key: how the card foot shows a date.
pub const DATE_DISPLAY_FORMAT: &str = "date-display-format";
/// The footer key: show `today`, `tomorrow`, `in 3 days` beside the date.
pub const SHOW_RELATIVE_DATE: &str = "show-relative-date";
/// The footer key: hide the date and time tokens in the card's text.
pub const HIDE_DATE_IN_TITLE: &str = "hide-date-in-title";
/// The footer key: make the shown date a link to its daily note.
pub const LINK_DATE_TO_DAILY_NOTE: &str = "link-date-to-daily-note";

const MONTHS: [&str; 12] = [
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
];

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// One day in the proleptic Gregorian calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDate {
    year: i64,
    month: u32,
    day: u32,
}

impl CivilDate {
    /// A date, when the month is 1–12 and the day exists in that month.
    #[must_use]
    pub fn new(year: i64, month: u32, day: u32) -> Option<Self> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Self { year, month, day })
    }

    #[must_use]
    pub fn year(self) -> i64 {
        self.year
    }

    #[must_use]
    pub fn month(self) -> u32 {
        self.month
    }

    #[must_use]
    pub fn day(self) -> u32 {
        self.day
    }

    /// Days since 1970-01-01, negative before it.
    #[must_use]
    pub fn days_since_epoch(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    /// The date `days` after this one (negative goes back).
    #[must_use]
    pub fn add_days(self, days: i64) -> Self {
        Self::from_days_since_epoch(self.days_since_epoch() + days)
    }

    #[must_use]
    pub fn from_days_since_epoch(days: i64) -> Self {
        let (year, month, day) = civil_from_days(days);
        Self { year, month, day }
    }

    /// 0 is Sunday, 6 is Saturday.
    #[must_use]
    pub fn weekday(self) -> u32 {
        (self.days_since_epoch() + 4).rem_euclid(7) as u32
    }

    /// The number of days in this month.
    #[must_use]
    pub fn days_in_month(self) -> u32 {
        days_in_month(self.year, self.month)
    }
}

/// A time of day, without a zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilTime {
    hour: u8,
    minute: u8,
    second: u8,
}

impl CivilTime {
    #[must_use]
    pub fn new(hour: u8, minute: u8, second: u8) -> Option<Self> {
        (hour < 24 && minute < 60 && second < 60).then_some(Self {
            hour,
            minute,
            second,
        })
    }

    #[must_use]
    pub fn hour(self) -> u8 {
        self.hour
    }

    #[must_use]
    pub fn minute(self) -> u8 {
        self.minute
    }

    #[must_use]
    pub fn second(self) -> u8 {
        self.second
    }

    /// Seconds since midnight, for ordering.
    #[must_use]
    pub fn seconds(self) -> i64 {
        i64::from(self.hour) * 3_600 + i64::from(self.minute) * 60 + i64::from(self.second)
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 for a civil date. The algorithm is the public-domain
/// one Howard Hinnant published for `days_from_civil`.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The inverse of [`days_from_civil`].
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m as u32, d as u32)
}

/// `YYYY-MM-DD` and the tokens in `docs/kanban-format.md` section 7.
///
/// `[` and `]` wrap literal text (`[at ]HH:mm` shows `at 14:30`). Anything
/// that is not a token is copied as written, so `QQQQ` stays `QQQQ`.
#[must_use]
pub fn format_date(date: CivilDate, format: &str) -> String {
    let mut out = String::new();
    let mut rest = format;
    while let Some(ch) = rest.chars().next() {
        if let Some((literal, used)) = literal_escape(rest) {
            out.push_str(literal);
            rest = &rest[used..];
            continue;
        }
        if let Some((text, used)) = date_token(date, rest) {
            out.push_str(&text);
            rest = &rest[used..];
        } else {
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

/// A time in the same token language: `HH H hh h mm m ss s A a`.
#[must_use]
pub fn format_time(time: CivilTime, format: &str) -> String {
    let mut out = String::new();
    let mut rest = format;
    while let Some(ch) = rest.chars().next() {
        if let Some((literal, used)) = literal_escape(rest) {
            out.push_str(literal);
            rest = &rest[used..];
            continue;
        }
        if let Some((text, used)) = time_token(time, rest) {
            out.push_str(&text);
            rest = &rest[used..];
        } else {
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

/// `[literal]` → the inside and the bytes used, or `None` for a `[` with no
/// closing `]`.
fn literal_escape(rest: &str) -> Option<(&str, usize)> {
    let inside = rest.strip_prefix('[')?;
    let close = inside.find(']')?;
    Some((&inside[..close], 1 + close + 1))
}

fn date_token(date: CivilDate, rest: &str) -> Option<(String, usize)> {
    // Longest first; `Do` before `D`, and every `d` form before `d`.
    for token in [
        "YYYY", "MMMM", "dddd", "MMM", "ddd", "Do", "YY", "MM", "DD", "dd", "M", "D", "d",
    ] {
        if rest.starts_with(token) {
            let text = match token {
                "YYYY" => format!("{:04}", date.year),
                "YY" => format!("{:02}", date.year.rem_euclid(100)),
                "MMMM" => MONTHS[(date.month - 1) as usize].to_owned(),
                "MMM" => MONTHS[(date.month - 1) as usize][..3].to_owned(),
                "MM" => format!("{:02}", date.month),
                "M" => date.month.to_string(),
                "DD" => format!("{:02}", date.day),
                "D" => date.day.to_string(),
                "Do" => ordinal(date.day),
                "dddd" => WEEKDAYS[date.weekday() as usize].to_owned(),
                "ddd" => WEEKDAYS[date.weekday() as usize][..3].to_owned(),
                "dd" => WEEKDAYS[date.weekday() as usize][..2].to_owned(),
                "d" => date.weekday().to_string(),
                _ => return None,
            };
            return Some((text, token.len()));
        }
    }
    None
}

fn time_token(time: CivilTime, rest: &str) -> Option<(String, usize)> {
    for token in ["HH", "hh", "mm", "ss", "H", "h", "m", "s", "A", "a"] {
        if rest.starts_with(token) {
            let hour12 = if time.hour.is_multiple_of(12) {
                12
            } else {
                time.hour % 12
            };
            let text = match token {
                "HH" => format!("{:02}", time.hour),
                "H" => time.hour.to_string(),
                "hh" => format!("{hour12:02}"),
                "h" => hour12.to_string(),
                "mm" => format!("{:02}", time.minute),
                "m" => time.minute.to_string(),
                "ss" => format!("{:02}", time.second),
                "s" => time.second.to_string(),
                "A" => {
                    if time.hour < 12 {
                        "AM".to_owned()
                    } else {
                        "PM".to_owned()
                    }
                }
                "a" => {
                    if time.hour < 12 {
                        "am".to_owned()
                    } else {
                        "pm".to_owned()
                    }
                }
                _ => return None,
            };
            return Some((text, token.len()));
        }
    }
    None
}

fn ordinal(day: u32) -> String {
    let suffix = match (day % 10, day % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{day}{suffix}")
}

/// Parse `text` with `pattern`; falls back to any ISO date in the text so a
/// daily note name like `daily/2026-11-11` still has a date.
#[must_use]
pub fn parse_date_with(text: &str, pattern: &str) -> Option<CivilDate> {
    parse_date_tokens(text, pattern).or_else(|| find_date(text))
}

/// Parse a time written with `pattern`; falls back to the first `H:MM[:SS]`
/// in the text.
#[must_use]
pub fn parse_time_with(text: &str, pattern: &str) -> Option<CivilTime> {
    parse_time_tokens(text, pattern).or_else(|| find_time(text))
}

fn parse_date_tokens(text: &str, pattern: &str) -> Option<CivilDate> {
    let mut cursor = Cursor::new(text);
    let mut year = None;
    let mut month = None;
    let mut day = None;
    let mut rest = pattern;
    while let Some(ch) = rest.chars().next() {
        if let Some((literal, used)) = literal_escape(rest) {
            cursor.expect_literal(literal)?;
            rest = &rest[used..];
            continue;
        }
        let Some((token, used)) = date_pattern_token(rest) else {
            cursor.expect_literal(ch.to_string().as_str())?;
            rest = &rest[ch.len_utf8()..];
            continue;
        };
        match token {
            "YYYY" => year = Some(cursor.digits(4, 4)?),
            "YY" => {
                let value = cursor.digits(1, 2)?;
                year = Some(if value >= 69 {
                    1900 + value
                } else {
                    2000 + value
                });
            }
            "MMMM" => month = Some(cursor.month_name(false)?),
            "MMM" => month = Some(cursor.month_name(true)?),
            "MM" | "M" => month = Some(cursor.digits(1, 2)? as u32),
            "DD" | "D" => day = Some(cursor.digits(1, 2)? as u32),
            "Do" => {
                let value = cursor.digits(1, 2)?;
                cursor.ordinal_suffix()?;
                day = Some(value as u32);
            }
            "dddd" => {
                cursor.word()?;
            }
            "ddd" | "dd" => {
                cursor.word()?;
            }
            "d" => {
                cursor.digits(1, 2)?;
            }
            _ => return None,
        }
        rest = &rest[used..];
    }
    if !cursor.done() {
        return None;
    }
    CivilDate::new(year?, month?, day?)
}

fn parse_time_tokens(text: &str, pattern: &str) -> Option<CivilTime> {
    let mut cursor = Cursor::new(text);
    let mut hour = None;
    let mut minute = None;
    let mut second = None;
    let mut meridiem = None;
    let mut twelve = false;
    let mut rest = pattern;
    while let Some(ch) = rest.chars().next() {
        if let Some((literal, used)) = literal_escape(rest) {
            cursor.expect_literal(literal)?;
            rest = &rest[used..];
            continue;
        }
        let Some((token, used)) = time_pattern_token(rest) else {
            cursor.expect_literal(ch.to_string().as_str())?;
            rest = &rest[ch.len_utf8()..];
            continue;
        };
        match token {
            "HH" | "H" => hour = Some(cursor.digits(1, 2)? as u8),
            "hh" | "h" => {
                twelve = true;
                hour = Some(cursor.digits(1, 2)? as u8);
            }
            "mm" | "m" => minute = Some(cursor.digits(1, 2)? as u8),
            "ss" | "s" => second = Some(cursor.digits(1, 2)? as u8),
            "A" | "a" => meridiem = Some(cursor.meridiem()?),
            _ => return None,
        }
        rest = &rest[used..];
    }
    if !cursor.done() {
        return None;
    }
    let mut hour = hour?;
    if twelve || meridiem.is_some() {
        let pm = meridiem.unwrap_or(false);
        if hour == 12 {
            hour = 0;
        }
        if pm {
            hour += 12;
        }
    }
    CivilTime::new(hour, minute.unwrap_or(0), second.unwrap_or(0))
}

/// The same token order as the formatter's date arm, as a plain matcher.
fn date_pattern_token(rest: &str) -> Option<(&'static str, usize)> {
    for token in [
        "YYYY", "MMMM", "dddd", "MMM", "ddd", "Do", "YY", "MM", "DD", "dd", "M", "D", "d",
    ] {
        if rest.starts_with(token) {
            return Some((token, token.len()));
        }
    }
    None
}

fn time_pattern_token(rest: &str) -> Option<(&'static str, usize)> {
    for token in ["HH", "hh", "mm", "ss", "H", "h", "m", "s", "A", "a"] {
        if rest.starts_with(token) {
            return Some((token, token.len()));
        }
    }
    None
}

struct Cursor<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text: text.trim(),
            at: 0,
        }
    }

    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn done(&self) -> bool {
        self.rest().trim().is_empty()
    }

    fn digits(&mut self, min: usize, max: usize) -> Option<i64> {
        let rest = self.rest();
        let mut end = 0;
        for (at, ch) in rest.char_indices() {
            if !ch.is_ascii_digit() || at >= max {
                break;
            }
            end = at + 1;
        }
        if end < min {
            return None;
        }
        let value = rest[..end].parse().ok()?;
        self.at += end;
        Some(value)
    }

    fn word(&mut self) -> Option<&'a str> {
        let rest = self.rest();
        let end = rest
            .char_indices()
            .take_while(|(_, ch)| ch.is_alphabetic())
            .last()
            .map_or(0, |(at, ch)| at + ch.len_utf8());
        if end == 0 {
            return None;
        }
        let word = &rest[..end];
        self.at += end;
        Some(word)
    }

    fn month_name(&mut self, short: bool) -> Option<u32> {
        let word = self.word()?;
        let index = MONTHS.iter().position(|month| {
            let head: String = word.chars().take(3).collect();
            month.eq_ignore_ascii_case(word)
                || (short || word.chars().count() == 3) && month[..3].eq_ignore_ascii_case(&head)
        })?;
        Some(index as u32 + 1)
    }

    fn ordinal_suffix(&mut self) -> Option<()> {
        let rest = self.rest();
        let end = rest
            .char_indices()
            .take_while(|(_, ch)| ch.is_ascii_alphabetic())
            .take(2)
            .last()
            .map_or(0, |(at, ch)| at + ch.len_utf8());
        let suffix = &rest[..end];
        if matches!(suffix, "st" | "nd" | "rd" | "th") {
            self.at += end;
            Some(())
        } else {
            None
        }
    }

    fn meridiem(&mut self) -> Option<bool> {
        let rest = self.rest();
        let upper = rest.get(..2)?.to_ascii_uppercase();
        let pm = match upper.as_str() {
            "AM" => false,
            "PM" => true,
            _ => return None,
        };
        self.at += 2;
        Some(pm)
    }

    fn expect_literal(&mut self, literal: &str) -> Option<()> {
        let rest = self.rest();
        if rest.starts_with(literal) {
            self.at += literal.len();
            Some(())
        } else if literal.chars().all(char::is_whitespace) {
            let after = rest.trim_start();
            if after.len() == rest.len() {
                return None;
            }
            self.at += rest.len() - after.len();
            Some(())
        } else {
            None
        }
    }
}

/// Find the first ISO-looking date anywhere in `text` (`2026-11-11`).
fn find_date(text: &str) -> Option<CivilDate> {
    let bytes = text.as_bytes();
    for at in 0..bytes.len() {
        if !bytes[at].is_ascii_digit() {
            continue;
        }
        if at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'-') {
            continue;
        }
        if let Some(date) = date_at(bytes, at) {
            return Some(date);
        }
    }
    None
}

fn date_at(bytes: &[u8], at: usize) -> Option<CivilDate> {
    let year = ascii_number(bytes, at, 4, 4)?;
    let dash = at + year.1;
    bytes.get(dash)?;
    if bytes[dash] != b'-' {
        return None;
    }
    let month = ascii_number(bytes, dash + 1, 1, 2)?;
    let dash2 = dash + 1 + month.1;
    bytes.get(dash2)?;
    if bytes[dash2] != b'-' {
        return None;
    }
    let day = ascii_number(bytes, dash2 + 1, 1, 2)?;
    let end = dash2 + 1 + day.1;
    if bytes.get(end).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    CivilDate::new(year.0, month.0 as u32, day.0 as u32)
}

/// Find the first `H:MM[:SS]`, with an optional `AM`/`PM`.
fn find_time(text: &str) -> Option<CivilTime> {
    let bytes = text.as_bytes();
    for at in 0..bytes.len() {
        if !bytes[at].is_ascii_digit() {
            continue;
        }
        if at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b':') {
            continue;
        }
        if let Some(time) = time_at(bytes, at) {
            return Some(time);
        }
    }
    None
}

fn time_at(bytes: &[u8], at: usize) -> Option<CivilTime> {
    let hour = ascii_number(bytes, at, 1, 2)?;
    let colon = at + hour.1;
    bytes.get(colon)?;
    if bytes[colon] != b':' {
        return None;
    }
    let minute = ascii_number(bytes, colon + 1, 2, 2)?;
    let mut end = colon + 1 + minute.1;
    let mut second = 0;
    if bytes.get(end) == Some(&b':') {
        let value = ascii_number(bytes, end + 1, 2, 2)?;
        second = value.0 as u8;
        end += 1 + value.1;
    }
    let mut hour = hour.0 as u8;
    let mut rest_at = end;
    while bytes.get(rest_at) == Some(&b' ') {
        rest_at += 1;
    }
    let rest = &bytes[rest_at..];
    let upper: Vec<u8> = rest
        .iter()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .take(2)
        .map(u8::to_ascii_uppercase)
        .collect();
    if upper == b"AM" || upper == b"PM" {
        if hour == 12 {
            hour = 0;
        }
        if upper == b"PM" {
            hour += 12;
        }
    }
    CivilTime::new(hour, minute.0 as u8, second)
}

/// `(value, bytes used)`, with between `min` and `max` digits.
fn ascii_number(bytes: &[u8], at: usize, min: usize, max: usize) -> Option<(i64, usize)> {
    let mut end = at;
    while end < bytes.len() && end - at < max && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end - at < min {
        return None;
    }
    let text = std::str::from_utf8(&bytes[at..end]).ok()?;
    Some((text.parse().ok()?, end - at))
}

/// The date/time settings a board shows.
///
/// Resolution order is the file's settings footer, then the app-wide
/// `kanban_defaults` in `settings.json`, then the built-in default
/// (`docs/kanban-format.md` section 9).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DateSettings {
    pub date_trigger: String,
    pub time_trigger: String,
    pub date_format: String,
    pub time_format: String,
    pub date_display_format: String,
    pub show_relative_date: bool,
    pub hide_date_in_title: bool,
    pub link_date_to_daily_note: bool,
}

impl Default for DateSettings {
    fn default() -> Self {
        Self {
            date_trigger: "@".to_owned(),
            time_trigger: "@@".to_owned(),
            date_format: "YYYY-MM-DD".to_owned(),
            time_format: "HH:mm".to_owned(),
            date_display_format: "YYYY-MM-DD".to_owned(),
            show_relative_date: false,
            hide_date_in_title: true,
            link_date_to_daily_note: false,
        }
    }
}

impl DateSettings {
    /// Footer, then app defaults, then the built-in value.
    #[must_use]
    pub fn resolve(footer_json: Option<&str>, defaults: &KanbanDefaults) -> Self {
        let mut settings = defaults.to_settings();
        if let Some(value) = footer_json.and_then(|json| serde_json::from_str::<Value>(json).ok()) {
            settings.apply_value(&value);
        }
        settings.trim_formats();
        settings
    }

    fn trim_formats(&mut self) {
        self.date_format = trimmed_format(&self.date_format, "YYYY-MM-DD");
        self.time_format = trimmed_format(&self.time_format, "HH:mm");
        self.date_display_format = trimmed_format(&self.date_display_format, "YYYY-MM-DD");
    }

    /// Read the keys this type owns from one JSON object. Other keys, and a
    /// value of the wrong type, are ignored; the current value stays.
    pub fn apply_value(&mut self, value: &Value) {
        if let Some(text) = text_value(value, DATE_TRIGGER) {
            self.date_trigger = text;
        }
        if let Some(text) = text_value(value, TIME_TRIGGER) {
            self.time_trigger = text;
        }
        if let Some(text) = text_value(value, DATE_FORMAT) {
            self.date_format = text;
        }
        if let Some(text) = text_value(value, TIME_FORMAT) {
            self.time_format = text;
        }
        if let Some(text) = text_value(value, DATE_DISPLAY_FORMAT) {
            self.date_display_format = text;
        }
        if let Some(flag) = value.get(SHOW_RELATIVE_DATE).and_then(Value::as_bool) {
            self.show_relative_date = flag;
        }
        if let Some(flag) = value.get(HIDE_DATE_IN_TITLE).and_then(Value::as_bool) {
            self.hide_date_in_title = flag;
        }
        if let Some(flag) = value.get(LINK_DATE_TO_DAILY_NOTE).and_then(Value::as_bool) {
            self.link_date_to_daily_note = flag;
        }
        self.trim_formats();
    }

    /// A new date token, trigger and braces included.
    #[must_use]
    pub fn date_token(&self, date: CivilDate) -> String {
        format!(
            "{}{{{}}}",
            self.date_trigger,
            format_date(date, &self.date_format)
        )
    }

    /// A new link-form date token (`@[[…]]`), trigger and brackets included.
    #[must_use]
    pub fn date_link_token(&self, date: CivilDate) -> String {
        format!(
            "{}[[{}]]",
            self.date_trigger,
            format_date(date, &self.date_format)
        )
    }

    /// A new time token, trigger and braces included.
    #[must_use]
    pub fn time_token(&self, time: CivilTime) -> String {
        format!(
            "{}{{{}}}",
            self.time_trigger,
            format_time(time, &self.time_format)
        )
    }

    /// The date written inside a token: the board's format, then anywhere in
    /// the text (so a link target with a daily-note name still works).
    #[must_use]
    pub fn parse_date(&self, text: &str) -> Option<CivilDate> {
        parse_date_with(text, &self.date_format)
    }

    #[must_use]
    pub fn parse_time(&self, text: &str) -> Option<CivilTime> {
        parse_time_with(text, &self.time_format)
    }
}

/// A format with surrounding whitespace removed. An all-whitespace format
/// would write tokens Muninn could not read back, so it falls back.
fn trimmed_format(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// A settings key is one string; `None` for a missing or non-string value.
fn text_value(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Whether `key` is one this version reads from `kanban_defaults` with a
/// value of the right type. Anything else is kept in
/// [`KanbanDefaults::other`] byte for byte.
fn handles_default_key(key: &str, defaults: &KanbanDefaults) -> bool {
    match key {
        DATE_TRIGGER => defaults.date_trigger.is_some(),
        TIME_TRIGGER => defaults.time_trigger.is_some(),
        DATE_FORMAT => defaults.date_format.is_some(),
        TIME_FORMAT => defaults.time_format.is_some(),
        DATE_DISPLAY_FORMAT => defaults.date_display_format.is_some(),
        SHOW_RELATIVE_DATE => defaults.show_relative_date.is_some(),
        HIDE_DATE_IN_TITLE => defaults.hide_date_in_title.is_some(),
        LINK_DATE_TO_DAILY_NOTE => defaults.link_date_to_daily_note.is_some(),
        HIDE_TAGS_IN_TITLE => defaults.hide_tags_in_title.is_some(),
        SHOW_TAGS_AT_BOTTOM => defaults.show_tags_at_bottom.is_some(),
        TAG_COLORS => defaults.tag_colors.is_some(),
        LINKED_PAGE_METADATA => defaults.linked_page_metadata.is_some(),
        SHOW_MISSING_METADATA => defaults.show_missing_metadata.is_some(),
        ARCHIVE_WITH_DATE => defaults.archive_with_date.is_some(),
        ARCHIVE_DATE_FORMAT => defaults.archive_date_format.is_some(),
        ARCHIVE_DATE_POSITION => defaults.archive_date_position.is_some(),
        ARCHIVE_DATE_SEPARATOR => defaults.archive_date_separator.is_some(),
        MAX_ARCHIVE_CARDS => defaults.max_archive_cards.is_some(),
        _ => false,
    }
}

/// The archive keys of one `kanban_defaults` object.
fn apply_archive_defaults(value: &Value, defaults: &mut KanbanDefaults) {
    defaults.archive_with_date = value.get(ARCHIVE_WITH_DATE).and_then(Value::as_bool);
    defaults.archive_date_format = text_value(value, ARCHIVE_DATE_FORMAT);
    defaults.archive_date_position = text_value(value, ARCHIVE_DATE_POSITION);
    defaults.archive_date_separator = value
        .get(ARCHIVE_DATE_SEPARATOR)
        .and_then(Value::as_str)
        .map(str::to_owned);
    defaults.max_archive_cards = value.get(MAX_ARCHIVE_CARDS).and_then(Value::as_u64);
}

/// The archive keys of [`KanbanDefaults::to_json`].
fn insert_archive_defaults(object: &mut Map<String, Value>, defaults: &KanbanDefaults) {
    if let Some(value) = defaults.archive_with_date {
        object.insert(ARCHIVE_WITH_DATE.to_owned(), Value::Bool(value));
    }
    if let Some(value) = &defaults.archive_date_format {
        object.insert(ARCHIVE_DATE_FORMAT.to_owned(), Value::String(value.clone()));
    }
    if let Some(value) = &defaults.archive_date_position {
        object.insert(
            ARCHIVE_DATE_POSITION.to_owned(),
            Value::String(value.clone()),
        );
    }
    if let Some(value) = &defaults.archive_date_separator {
        object.insert(
            ARCHIVE_DATE_SEPARATOR.to_owned(),
            Value::String(value.clone()),
        );
    }
    if let Some(value) = defaults.max_archive_cards {
        object.insert(MAX_ARCHIVE_CARDS.to_owned(), Value::from(value));
    }
}

/// The app-wide `kanban_defaults` object in `settings.json`. Every known
/// field is absent until the user sets it; an absent key means the built-in
/// default. Unknown keys, and known keys with the wrong type, stay in
/// [`KanbanDefaults::other`] and are written back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KanbanDefaults {
    pub date_trigger: Option<String>,
    pub time_trigger: Option<String>,
    pub date_format: Option<String>,
    pub time_format: Option<String>,
    pub date_display_format: Option<String>,
    pub show_relative_date: Option<bool>,
    pub hide_date_in_title: Option<bool>,
    pub link_date_to_daily_note: Option<bool>,
    // Tag defaults (slice 10).
    pub hide_tags_in_title: Option<bool>,
    pub show_tags_at_bottom: Option<bool>,
    /// The raw `tag-colors` value. It is kept as written so extra fields and
    /// entries a later editor does not understand survive a settings save.
    /// [`TagSettings`] reads the entries it understands from it.
    pub tag_colors: Option<Value>,
    // Card metadata defaults (slice 11).
    /// The raw `linked-page-metadata` value. It is kept as written so extra
    /// fields, their order, and a wrong-typed `containsMarkdown` survive a
    /// settings save. [`super::metadata::MetadataSettings`] reads the entries
    /// it understands from it.
    pub linked_page_metadata: Option<Value>,
    pub show_missing_metadata: Option<bool>,
    // Archive defaults (slice 12). `archive_date_position` keeps the written
    // string so an unknown value survives a settings save.
    pub archive_with_date: Option<bool>,
    pub archive_date_format: Option<String>,
    pub archive_date_position: Option<String>,
    pub archive_date_separator: Option<String>,
    pub max_archive_cards: Option<u64>,
    /// Keys this version does not know, kept so a newer Muninn's app defaults
    /// are not lost by an older one. A known key with the wrong JSON type is
    /// kept here too.
    pub other: Map<String, Value>,
}

impl KanbanDefaults {
    /// Read `kanban_defaults`. A missing key, a non-object, or a mistyped
    /// value leaves that default unset.
    #[must_use]
    pub fn from_json(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        let mut defaults = Self {
            date_trigger: text_value(value, DATE_TRIGGER),
            time_trigger: text_value(value, TIME_TRIGGER),
            date_format: text_value(value, DATE_FORMAT),
            time_format: text_value(value, TIME_FORMAT),
            date_display_format: text_value(value, DATE_DISPLAY_FORMAT),
            show_relative_date: value.get(SHOW_RELATIVE_DATE).and_then(Value::as_bool),
            hide_date_in_title: value.get(HIDE_DATE_IN_TITLE).and_then(Value::as_bool),
            link_date_to_daily_note: value.get(LINK_DATE_TO_DAILY_NOTE).and_then(Value::as_bool),
            hide_tags_in_title: value.get(HIDE_TAGS_IN_TITLE).and_then(Value::as_bool),
            show_tags_at_bottom: value.get(SHOW_TAGS_AT_BOTTOM).and_then(Value::as_bool),
            tag_colors: value
                .get(TAG_COLORS)
                .filter(|value| value.is_array())
                .cloned(),
            linked_page_metadata: value
                .get(LINKED_PAGE_METADATA)
                .filter(|value| value.is_array())
                .cloned(),
            show_missing_metadata: value.get(SHOW_MISSING_METADATA).and_then(Value::as_bool),
            archive_with_date: None,
            archive_date_format: None,
            archive_date_position: None,
            archive_date_separator: None,
            max_archive_cards: None,
            other: Map::new(),
        };
        apply_archive_defaults(value, &mut defaults);
        if let Some(object) = value.as_object() {
            for (key, item) in object {
                if !handles_default_key(key, &defaults) {
                    defaults.other.insert(key.clone(), item.clone());
                }
            }
        }
        defaults
    }

    /// The object written to `settings.json`, with only the keys that are
    /// set and in the documented order.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = self.other.clone();
        if let Some(value) = &self.date_trigger {
            object.insert(DATE_TRIGGER.to_owned(), Value::String(value.clone()));
        }
        if let Some(value) = &self.time_trigger {
            object.insert(TIME_TRIGGER.to_owned(), Value::String(value.clone()));
        }
        if let Some(value) = &self.date_format {
            object.insert(DATE_FORMAT.to_owned(), Value::String(value.clone()));
        }
        if let Some(value) = &self.time_format {
            object.insert(TIME_FORMAT.to_owned(), Value::String(value.clone()));
        }
        if let Some(value) = &self.date_display_format {
            object.insert(DATE_DISPLAY_FORMAT.to_owned(), Value::String(value.clone()));
        }
        if let Some(value) = self.show_relative_date {
            object.insert(SHOW_RELATIVE_DATE.to_owned(), Value::Bool(value));
        }
        if let Some(value) = self.hide_date_in_title {
            object.insert(HIDE_DATE_IN_TITLE.to_owned(), Value::Bool(value));
        }
        if let Some(value) = self.link_date_to_daily_note {
            object.insert(LINK_DATE_TO_DAILY_NOTE.to_owned(), Value::Bool(value));
        }
        if let Some(value) = self.hide_tags_in_title {
            object.insert(HIDE_TAGS_IN_TITLE.to_owned(), Value::Bool(value));
        }
        if let Some(value) = self.show_tags_at_bottom {
            object.insert(SHOW_TAGS_AT_BOTTOM.to_owned(), Value::Bool(value));
        }
        if let Some(colors) = &self.tag_colors {
            object.insert(TAG_COLORS.to_owned(), colors.clone());
        }
        if let Some(fields) = &self.linked_page_metadata {
            object.insert(LINKED_PAGE_METADATA.to_owned(), fields.clone());
        }
        if let Some(value) = self.show_missing_metadata {
            object.insert(SHOW_MISSING_METADATA.to_owned(), Value::Bool(value));
        }
        insert_archive_defaults(&mut object, self);
        Value::Object(object)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.date_trigger.is_none()
            && self.time_trigger.is_none()
            && self.date_format.is_none()
            && self.time_format.is_none()
            && self.date_display_format.is_none()
            && self.show_relative_date.is_none()
            && self.hide_date_in_title.is_none()
            && self.link_date_to_daily_note.is_none()
            && self.hide_tags_in_title.is_none()
            && self.show_tags_at_bottom.is_none()
            && self.tag_colors.is_none()
            && self.linked_page_metadata.is_none()
            && self.show_missing_metadata.is_none()
            && self.archive_with_date.is_none()
            && self.archive_date_format.is_none()
            && self.archive_date_position.is_none()
            && self.archive_date_separator.is_none()
            && self.max_archive_cards.is_none()
            && self.other.is_empty()
    }

    fn to_settings(&self) -> DateSettings {
        let mut settings = DateSettings::default();
        if let Some(value) = &self.date_trigger {
            settings.date_trigger.clone_from(value);
        }
        if let Some(value) = &self.time_trigger {
            settings.time_trigger.clone_from(value);
        }
        if let Some(value) = &self.date_format {
            settings.date_format.clone_from(value);
        }
        if let Some(value) = &self.time_format {
            settings.time_format.clone_from(value);
        }
        if let Some(value) = &self.date_display_format {
            settings.date_display_format.clone_from(value);
        }
        if let Some(value) = self.show_relative_date {
            settings.show_relative_date = value;
        }
        if let Some(value) = self.hide_date_in_title {
            settings.hide_date_in_title = value;
        }
        if let Some(value) = self.link_date_to_daily_note {
            settings.link_date_to_daily_note = value;
        }
        settings
    }

    /// The archive settings these defaults supply; missing keys stay
    /// built-in.
    #[must_use]
    pub fn to_archive_settings(&self) -> super::ArchiveSettings {
        let mut settings = super::ArchiveSettings::default();
        if let Some(value) = self.archive_with_date {
            settings.with_date = value;
        }
        if let Some(value) = &self.archive_date_format {
            settings.date_format.clone_from(value);
        }
        if let Some(position) = self
            .archive_date_position
            .as_deref()
            .and_then(super::ArchiveDatePosition::parse)
        {
            settings.date_position = position;
        }
        if let Some(value) = &self.archive_date_separator {
            settings.date_separator.clone_from(value);
        }
        settings.max_cards = self
            .max_archive_cards
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n > 0);
        settings
    }

    /// The tag settings these defaults supply; missing keys stay built-in.
    #[must_use]
    pub fn to_tag_settings(&self) -> TagSettings {
        let mut settings = TagSettings::default();
        if let Some(value) = self.hide_tags_in_title {
            settings.hide_tags_in_title = value;
        }
        if let Some(value) = self.show_tags_at_bottom {
            settings.show_tags_at_bottom = value;
        }
        if let Some(colors) = tag_colors(self.tag_colors.as_ref()) {
            settings.tag_colors = colors;
        }
        settings
    }
}

#[cfg(test)]
mod tests {
    use super::super::tags::TagColor;
    use super::*;
    use serde_json::json;

    #[test]
    fn every_date_token_renders() {
        let date = CivilDate::new(2026, 11, 12).expect("date");
        assert_eq!(
            format_date(date, "YYYY YY MMMM MMM MM M DD D dddd ddd dd d"),
            "2026 26 November Nov 11 11 12 12 Thursday Thu Th 4"
        );
        assert_eq!(
            format_date(CivilDate::new(2024, 2, 29).unwrap(), "YYYY-M-D"),
            "2024-2-29"
        );
        assert_eq!(
            format_date(CivilDate::new(5, 1, 2).unwrap(), "YYYY"),
            "0005"
        );
    }

    #[test]
    fn every_time_token_renders() {
        let midnight = CivilTime::new(0, 5, 9).expect("time");
        assert_eq!(
            format_time(midnight, "HH H hh h mm m ss s A a"),
            "00 0 12 12 05 5 09 9 AM am"
        );
        let afternoon = CivilTime::new(14, 30, 0).expect("time");
        assert_eq!(
            format_time(afternoon, "HH H hh h mm m ss s A a"),
            "14 14 02 2 30 30 00 0 PM pm"
        );
        assert_eq!(
            format_time(CivilTime::new(12, 0, 0).unwrap(), "h A"),
            "12 PM"
        );
    }

    #[test]
    fn ordinals_literals_and_unknown_tokens_are_literal() {
        for (day, want) in [
            (1, "1st"),
            (2, "2nd"),
            (3, "3rd"),
            (4, "4th"),
            (11, "11th"),
            (12, "12th"),
            (13, "13th"),
            (21, "21st"),
            (22, "22nd"),
            (23, "23rd"),
            (31, "31st"),
        ] {
            assert_eq!(
                format_date(CivilDate::new(2026, 1, day).unwrap(), "Do"),
                want
            );
        }
        let date = CivilDate::new(2026, 11, 12).unwrap();
        assert_eq!(format_date(date, "[on ]YYYY[ at] HH"), "on 2026 at HH");
        assert_eq!(format_date(date, "[YYYY] QQQQ"), "YYYY QQQQ");
        assert_eq!(
            format_time(CivilTime::new(9, 5, 0).unwrap(), "[at ]h:mm"),
            "at 9:05"
        );
        // An unclosed escape stays literal.
        assert_eq!(format_date(date, "[YYYY"), "[2026");
    }

    #[test]
    fn civil_dates_round_trip_days_and_weekdays() {
        let epoch = CivilDate::new(1970, 1, 1).unwrap();
        assert_eq!(epoch.days_since_epoch(), 0);
        assert_eq!(epoch.weekday(), 4, "Thursday");
        let date = CivilDate::new(2026, 11, 12).unwrap();
        assert_eq!(
            CivilDate::from_days_since_epoch(date.days_since_epoch()),
            date
        );
        assert_eq!(date.add_days(1), CivilDate::new(2026, 11, 13).unwrap());
        assert_eq!(date.add_days(-1), CivilDate::new(2026, 11, 11).unwrap());
        assert_eq!(date.weekday(), 4, "Thursday");
        assert_eq!(
            CivilDate::new(2024, 2, 29).unwrap().add_days(1),
            CivilDate::new(2024, 3, 1).unwrap()
        );
        assert!(CivilDate::new(2023, 2, 29).is_none());
        assert!(
            CivilDate::new(1900, 2, 29).is_none(),
            "1900 is not a leap year"
        );
        assert!(CivilDate::new(2000, 2, 29).is_some(), "2000 is a leap year");
        assert_eq!(CivilDate::new(2026, 4, 1).unwrap().days_in_month(), 30);
        assert_eq!(CivilDate::new(2026, 2, 1).unwrap().days_in_month(), 28);
        for days in [-1_000, -1, 0, 1, 365, 20_000, 100_000] {
            let date = CivilDate::from_days_since_epoch(days);
            assert_eq!(date.days_since_epoch(), days, "{days}");
        }
    }

    #[test]
    fn dates_and_times_parse_with_the_written_format_and_fall_back() {
        let day = CivilDate::new(2026, 11, 12).unwrap();
        assert_eq!(parse_date_with("2026-11-12", "YYYY-MM-DD"), Some(day));
        assert_eq!(parse_date_with("12.11.2026", "DD.MM.YYYY"), Some(day));
        assert_eq!(
            parse_date_with("12 November 2026", "D MMMM YYYY"),
            Some(day)
        );
        assert_eq!(parse_date_with("12 Nov 2026", "D MMM YYYY"), Some(day));
        assert_eq!(parse_date_with("Nov 12, 2026", "MMM D, YYYY"), Some(day));
        assert_eq!(
            parse_date_with("12th November 2026", "Do MMMM YYYY"),
            Some(day)
        );
        assert_eq!(
            parse_date_with("[due] 2026-11-12", "[due] YYYY-MM-DD"),
            Some(day)
        );
        // A daily-note link's target still has a date.
        assert_eq!(parse_date_with("daily/2026-11-12", "YYYY-MM-DD"), Some(day));
        assert_eq!(parse_date_with("tomorrow", "YYYY-MM-DD"), None);
        assert_eq!(parse_date_with("2026-02-30", "YYYY-MM-DD"), None);

        assert_eq!(parse_time_with("14:30", "HH:mm"), CivilTime::new(14, 30, 0));
        assert_eq!(
            parse_time_with("2:05 PM", "h:mm A"),
            CivilTime::new(14, 5, 0)
        );
        assert_eq!(
            parse_time_with("12:00 am", "hh:mm a"),
            CivilTime::new(0, 0, 0)
        );
        assert_eq!(
            parse_time_with("at 09:30:15", "HH:mm"),
            CivilTime::new(9, 30, 15)
        );
        assert_eq!(parse_time_with("half past", "HH:mm"), None);
    }

    #[test]
    fn footer_values_beat_app_defaults_which_beat_builtins() {
        let builtin = DateSettings::default();
        assert_eq!(builtin.date_trigger, "@");
        assert_eq!(builtin.time_trigger, "@@");
        assert!(builtin.hide_date_in_title);
        assert!(!builtin.show_relative_date);

        let defaults = KanbanDefaults::from_json(Some(&json!({
            "date-trigger": "@d",
            "time-trigger": "at",
            "date-format": "DD.MM.YYYY",
            "time-format": "hh:mm A",
            "date-display-format": "D MMMM YYYY",
            "show-relative-date": true,
            "hide-date-in-title": false,
            "link-date-to-daily-note": true,
        })));
        let resolved = DateSettings::resolve(None, &defaults);
        assert_eq!(resolved.date_trigger, "@d");
        assert_eq!(resolved.time_trigger, "at");
        assert_eq!(resolved.date_format, "DD.MM.YYYY");
        assert_eq!(resolved.time_format, "hh:mm A");
        assert_eq!(resolved.date_display_format, "D MMMM YYYY");
        assert!(resolved.show_relative_date && resolved.link_date_to_daily_note);
        assert!(!resolved.hide_date_in_title);

        let footer = r#"{"date-trigger":"due:","show-relative-date":false,"hide-date-in-title":true,"unknown":1}"#;
        let resolved = DateSettings::resolve(Some(footer), &defaults);
        assert_eq!(resolved.date_trigger, "due:");
        assert_eq!(
            resolved.time_trigger, "at",
            "a missing key keeps the app default"
        );
        assert!(!resolved.show_relative_date, "the board wins");
        assert!(resolved.hide_date_in_title);
        assert_eq!(resolved.date_format, "DD.MM.YYYY");
    }

    #[test]
    fn a_mistyped_footer_or_app_value_is_ignored() {
        let defaults = KanbanDefaults::from_json(Some(&json!({
            "date-trigger": 3,
            "hide-date-in-title": "no",
        })));
        assert_eq!(defaults.date_trigger, None);
        assert_eq!(defaults.hide_date_in_title, None);
        assert!(
            !defaults.is_empty(),
            "the raw values are kept for the next write"
        );
        let footer = r#"{"date-trigger":3,"time-trigger":"","show-relative-date":"yes","hide-date-in-title":null}"#;
        let resolved = DateSettings::resolve(Some(footer), &defaults);
        assert_eq!(resolved, DateSettings::default());
        // Malformed JSON keeps every built-in value.
        assert_eq!(
            DateSettings::resolve(Some("{not json"), &defaults),
            DateSettings::default()
        );
    }

    #[test]
    fn app_defaults_serialize_stably_and_round_trip() {
        let empty = KanbanDefaults::default();
        assert!(empty.is_empty());
        assert_eq!(empty.to_json(), json!({}));
        let defaults = KanbanDefaults {
            date_trigger: Some("@d".to_owned()),
            time_trigger: Some("at".to_owned()),
            date_format: Some("DD.MM.YYYY".to_owned()),
            time_format: Some("HH:mm".to_owned()),
            date_display_format: Some("D MMMM YYYY".to_owned()),
            show_relative_date: Some(true),
            hide_date_in_title: Some(false),
            link_date_to_daily_note: Some(true),
            hide_tags_in_title: Some(true),
            show_tags_at_bottom: Some(false),
            tag_colors: Some(json!([{"tagKey": "work", "color": "2"}])),
            ..KanbanDefaults::default()
        };
        let json = defaults.to_json();
        assert_eq!(KanbanDefaults::from_json(Some(&json)), defaults);
        let text = serde_json::to_string(&json).unwrap();
        assert_eq!(
            text,
            r#"{"date-display-format":"D MMMM YYYY","date-format":"DD.MM.YYYY","date-trigger":"@d","hide-date-in-title":false,"hide-tags-in-title":true,"link-date-to-daily-note":true,"show-relative-date":true,"show-tags-at-bottom":false,"tag-colors":[{"color":"2","tagKey":"work"}],"time-format":"HH:mm","time-trigger":"at"}"#
        );
    }

    #[test]
    fn tag_colors_is_kept_as_written_including_unknown_and_malformed_entries() {
        let raw = json!([
            {"tagKey": "work", "color": "1", "extra": "kept"},
            {"tagKey": 2, "color": "2"},
            "not an object"
        ]);
        let defaults = KanbanDefaults {
            tag_colors: Some(raw.clone()),
            ..KanbanDefaults::default()
        };
        let written = defaults.to_json();
        assert_eq!(written[TAG_COLORS], raw, "the raw array survives a save");
        assert_eq!(KanbanDefaults::from_json(Some(&written)), defaults);
        // The typed view understands the entries it can and ignores the rest.
        assert_eq!(
            defaults.to_tag_settings().tag_colors,
            vec![TagColor {
                tag_key: "work".to_owned(),
                color: "1".to_owned()
            }]
        );
        // A value of the wrong type is kept too, through `other`.
        let wrong = json!({"tag-colors": "not an array"});
        let defaults = KanbanDefaults::from_json(Some(&wrong));
        assert!(defaults.tag_colors.is_none());
        assert_eq!(defaults.to_json()["tag-colors"], "not an array");
    }

    #[test]
    fn surrounding_whitespace_in_a_format_is_trimmed() {
        let day = CivilDate::new(2026, 11, 12).unwrap();
        let time = CivilTime::new(14, 5, 0).unwrap();
        for (dates, times, want_date, want_time) in [
            (" DD.MM.YYYY", "HH:mm ", "12.11.2026", "14:05"),
            ("DD.MM.YYYY ", " HH:mm", "12.11.2026", "14:05"),
            ("D MMMM YYYY ", "HH:mm ", "12 November 2026", "14:05"),
        ] {
            let footer = format!("{{\"date-format\":{dates:?},\"time-format\":{times:?}}}");
            let settings = DateSettings::resolve(Some(&footer), &KanbanDefaults::default());
            assert_eq!(settings.date_format, dates.trim(), "{footer}");
            assert_eq!(settings.time_format, times.trim(), "{footer}");
            let date_token = settings.date_token(day);
            let time_token = settings.time_token(time);
            assert_eq!(date_token, format!("@{{{want_date}}}"));
            assert_eq!(time_token, format!("@@{{{want_time}}}"));
            assert_eq!(settings.parse_date(want_date), Some(day), "{footer}");
            assert_eq!(settings.parse_time(want_time), Some(time), "{footer}");
        }
        // The app defaults are trimmed the same way.
        let defaults = KanbanDefaults {
            date_format: Some(" DD.MM.YYYY ".to_owned()),
            ..KanbanDefaults::default()
        };
        let settings = DateSettings::resolve(None, &defaults);
        assert_eq!(settings.date_format, "DD.MM.YYYY");
        assert_eq!(settings.date_token(day), "@{12.11.2026}");
    }

    #[test]
    fn a_space_before_am_or_pm_is_accepted_in_the_fallback() {
        let time = CivilTime::new(14, 5, 0);
        assert_eq!(parse_time_with("2:05 PM", "HH:mm"), time);
        assert_eq!(parse_time_with("2:05PM", "HH:mm"), time);
        assert_eq!(parse_time_with("2:05 pm", "HH:mm"), time);
        // The pattern path still parses a spaced meridiem too.
        assert_eq!(parse_time_with("2:05 PM", "h:mm A"), time);
        assert_eq!(parse_time_with("14:05", "HH:mm"), time);
    }

    #[test]
    fn an_unknown_or_mistyped_app_default_is_kept_for_the_next_write() {
        let input = json!({
            "date-trigger": "@d",
            "date-format": 3,
            "hide-date-in-title": "no",
            "unknown": {"x": 1},
        });
        let defaults = KanbanDefaults::from_json(Some(&input));
        assert_eq!(defaults.date_trigger.as_deref(), Some("@d"));
        assert_eq!(defaults.date_format, None);
        assert_eq!(defaults.hide_date_in_title, None);
        assert!(!defaults.is_empty());
        let out = defaults.to_json();
        assert_eq!(out["date-trigger"], "@d");
        assert_eq!(out["date-format"], 3);
        assert_eq!(out["hide-date-in-title"], "no");
        assert_eq!(out["unknown"]["x"], 1);
        assert_eq!(KanbanDefaults::from_json(Some(&out)), defaults);
    }

    #[test]
    fn a_board_writes_its_own_trigger_and_format() {
        let settings = DateSettings {
            date_trigger: "due:".to_owned(),
            time_trigger: "at:".to_owned(),
            date_format: "DD.MM.YYYY".to_owned(),
            time_format: "hh:mm A".to_owned(),
            ..DateSettings::default()
        };
        let date = CivilDate::new(2026, 11, 12).unwrap();
        assert_eq!(settings.date_token(date), "due:{12.11.2026}");
        let time = CivilTime::new(14, 30, 0).unwrap();
        assert_eq!(settings.time_token(time), "at:{02:30 PM}");
        assert_eq!(settings.date_link_token(date), "due:[[12.11.2026]]");
        assert_eq!(settings.parse_date("12.11.2026"), Some(date));
        assert_eq!(settings.parse_time("02:30 PM"), Some(time));
    }
}

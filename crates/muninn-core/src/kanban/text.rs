//! Scanning a card's text for dates, times and tags (slice 9).
//!
//! The same scanner serves sorting, the card's display and the token the
//! picker inserts. Tokens inside a fenced block or an inline code span are
//! not dates, times or tags, as `docs/kanban-format.md` section 7 says.

use std::ops::Range;

use super::dates::{CivilTime, DateSettings};
use super::parse::{fence_closes, opens_fence};

/// The two card-text tokens with a time value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DateKind {
    Date,
    Time,
}

/// One `@{…}`, `@[[…]]` or `@@{…}` found in card text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateToken {
    pub kind: DateKind,
    /// The text inside the braces, or the target of the link form.
    pub value: String,
    /// Byte range of the whole token in the scanned text.
    pub start: usize,
    pub end: usize,
    /// The token is `@[[…]]`, a wikilink to a daily note.
    pub link: bool,
}

/// One `#tag` found in card text. The `#` is not part of `tag`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagToken {
    pub tag: String,
    pub start: usize,
    pub end: usize,
}

/// What [`scan_card_text`] found: the first date, the first time, and every
/// tag, all outside fences and inline code.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardText {
    pub date: Option<DateToken>,
    pub time: Option<DateToken>,
    pub tags: Vec<TagToken>,
}

impl CardText {
    /// `text` without the first date and first time token. This is what
    /// `hide-date-in-title` shows in the card's main text; the bytes on disk
    /// never change for a display setting.
    #[must_use]
    pub fn without_dates(&self, text: &str) -> String {
        let mut ranges: Vec<Range<usize>> = [&self.date, &self.time]
            .into_iter()
            .flatten()
            .map(|token| token.start..token.end)
            .collect();
        if ranges.is_empty() {
            return text.to_owned();
        }
        ranges.sort_by_key(|range| range.start);
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        for range in ranges {
            if range.start < at {
                continue;
            }
            out.push_str(&text[at..range.start]);
            at = range.end;
        }
        out.push_str(&text[at..]);
        out
    }

    /// The date and time as one sortable number: day × 86,400 plus seconds.
    /// `None` when the card has no date.
    #[must_use]
    pub fn sort_value(&self, settings: &DateSettings) -> Option<i64> {
        let date = self
            .date
            .as_ref()
            .and_then(|token| settings.parse_date(&token.value))?;
        let time = self
            .time
            .as_ref()
            .and_then(|token| settings.parse_time(&token.value))
            .unwrap_or(CivilTime::new(0, 0, 0)?);
        Some(date.days_since_epoch() * 86_400 + time.seconds())
    }
}

/// Scan `text` with the board's triggers and formatting rules.
#[must_use]
pub fn scan_card_text(text: &str, settings: &DateSettings) -> CardText {
    let triggers = triggers(settings);
    let mut found = CardText::default();
    let mut fence: Option<(u8, usize)> = None;
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let body = body.strip_suffix('\r').unwrap_or(body);
        let line_end = line_start + body.len();
        if let Some((marker, run)) = fence {
            if fence_closes(body, marker, run) {
                fence = None;
            }
            line_start += line.len();
            continue;
        }
        if let Some(opened) = opens_fence(body) {
            fence = Some(opened);
            line_start += line.len();
            continue;
        }
        let code = code_ranges(body);
        scan_line(body, line_start, line_end, &triggers, &code, &mut found);
        scan_tags(body, line_start, &code, &mut found.tags);
        line_start += line.len();
    }
    found
}

/// The triggers to look for, longest first so `@@` is not read as `@`.
fn triggers(settings: &DateSettings) -> Vec<(DateKind, &str)> {
    let mut triggers = vec![
        (DateKind::Date, settings.date_trigger.as_str()),
        (DateKind::Time, settings.time_trigger.as_str()),
    ];
    triggers.retain(|(_, trigger)| !trigger.is_empty());
    triggers.sort_by_key(|(_, trigger)| std::cmp::Reverse(trigger.len()));
    triggers
}

fn scan_line(
    body: &str,
    base: usize,
    body_end: usize,
    triggers: &[(DateKind, &str)],
    code: &[Range<usize>],
    found: &mut CardText,
) {
    let mut at = 0;
    while at < body.len() {
        let ch = body[at..]
            .chars()
            .next()
            .expect("a char at a char boundary");
        if !in_code(code, at) {
            let hit = triggers
                .iter()
                .find(|(_, trigger)| body[at..].starts_with(*trigger));
            if let Some((kind, trigger)) = hit
                && let Some(token) = token_at(*kind, body, base, body_end, at, trigger)
            {
                let advance = trigger.len() + token_len_after_trigger(&token);
                let slot = match kind {
                    DateKind::Date => &mut found.date,
                    DateKind::Time => &mut found.time,
                };
                if slot.is_none() {
                    *slot = Some(token);
                }
                at += advance;
                continue;
            }
        }
        at += ch.len_utf8();
    }
}

/// Inline code spans: a run of N backticks up to the next run of exactly N.
/// An unmatched run is literal text, not an open code span.
fn code_ranges(body: &str) -> Vec<Range<usize>> {
    let bytes = body.as_bytes();
    let mut ranges = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'`' {
            at += body[at..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let open = at;
        let mut end = at;
        while bytes.get(end) == Some(&b'`') {
            end += 1;
        }
        let run = end - open;
        let mut close_at = end;
        let mut closed = None;
        while close_at < bytes.len() {
            if bytes[close_at] == b'`' {
                let mut close_end = close_at;
                while bytes.get(close_end) == Some(&b'`') {
                    close_end += 1;
                }
                if close_end - close_at == run {
                    closed = Some(close_end);
                    break;
                }
                close_at = close_end;
            } else {
                close_at += body[close_at..].chars().next().map_or(1, char::len_utf8);
            }
        }
        if let Some(close_end) = closed {
            ranges.push(end..close_at);
            at = close_end;
        } else {
            at = end;
        }
    }
    ranges
}

fn in_code(ranges: &[Range<usize>], at: usize) -> bool {
    ranges.iter().any(|range| range.contains(&at))
}

/// The bytes of a token after its trigger: `{value}` is len + 2, the link
/// form is len + 4.
fn token_len_after_trigger(token: &DateToken) -> usize {
    token.value.len() + if token.link { 4 } else { 2 }
}

fn token_at(
    kind: DateKind,
    body: &str,
    base: usize,
    body_end: usize,
    at: usize,
    trigger: &str,
) -> Option<DateToken> {
    let after = at + trigger.len();
    let rest = &body[after..];
    let (value, inner, close) = if let Some(inside) = rest.strip_prefix('{') {
        let end = inside.find('}')?;
        (&inside[..end], 1, 1)
    } else if kind == DateKind::Date {
        let inside = rest.strip_prefix("[[")?;
        let end = inside.find("]]")?;
        (&inside[..end], 2, 2)
    } else {
        return None;
    };
    if value.is_empty() || value.contains('\n') {
        return None;
    }
    let start = base + at;
    let end = base + after + inner + value.len() + close;
    if end > body_end {
        return None;
    }
    Some(DateToken {
        kind,
        value: value.to_owned(),
        start,
        end,
        link: inner == 2,
    })
}

fn scan_tags(body: &str, base: usize, code: &[Range<usize>], tags: &mut Vec<TagToken>) {
    let mut before = ' ';
    for (at, ch) in body.char_indices() {
        if ch == '#' && !in_code(code, at) && before.is_whitespace() {
            let word = &body[at + 1..];
            let len = word
                .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '/')))
                .unwrap_or(word.len());
            let tag = &word[..len];
            if tag.chars().any(|c| !c.is_ascii_digit()) {
                tags.push(TagToken {
                    tag: tag.to_owned(),
                    start: base + at + 1,
                    end: base + at + 1 + len,
                });
            }
        }
        before = ch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> DateSettings {
        DateSettings::default()
    }

    fn scan(text: &str) -> CardText {
        scan_card_text(text, &settings())
    }

    #[test]
    fn a_date_is_the_first_date_then_the_first_time() {
        let found = scan("ship @{2026-11-12}");
        assert_eq!(
            found.date.as_ref().map(|t| t.value.as_str()),
            Some("2026-11-12")
        );
        assert_eq!(found.date.as_ref().unwrap().start, 5);
        assert_eq!(found.date.as_ref().unwrap().end, 18);
        assert!(found.time.is_none());

        let found = scan("at @@{09:30} on @[[2026-01-02]] @{2027-01-01}");
        assert_eq!(
            found.date.as_ref().map(|t| t.value.as_str()),
            Some("2026-01-02")
        );
        assert!(found.date.as_ref().unwrap().link);
        assert_eq!(found.time.as_ref().map(|t| t.value.as_str()), Some("09:30"));
        // The first time is paired with the first date even when it comes first.
        assert_eq!(
            found.sort_value(&settings()),
            Some(
                super::super::dates::CivilDate::new(2026, 1, 2)
                    .unwrap()
                    .days_since_epoch()
                    * 86_400
                    + 9 * 3_600
                    + 30 * 60
            )
        );
        assert!(scan("only a time @@{14:30}").date.is_none());
        assert_eq!(
            scan("only a time @@{14:30}")
                .time
                .as_ref()
                .map(|t| t.value.as_str()),
            Some("14:30")
        );
        assert!(scan("mail me@{x").date.is_none());
    }

    #[test]
    fn a_fence_and_inline_code_hide_their_dates_and_tags() {
        let found = scan("`@{2026-01-01} #code` @{2026-02-02} #real");
        assert_eq!(
            found.date.as_ref().map(|t| t.value.as_str()),
            Some("2026-02-02")
        );
        assert_eq!(
            found
                .tags
                .iter()
                .map(|t| t.tag.as_str())
                .collect::<Vec<_>>(),
            ["real"]
        );
        let fenced = scan("```\n@{2026-01-01} #inside\n```\n@{2026-02-02} #outside");
        assert_eq!(
            fenced.date.as_ref().map(|t| t.value.as_str()),
            Some("2026-02-02")
        );
        assert_eq!(
            fenced
                .tags
                .iter()
                .map(|t| t.tag.as_str())
                .collect::<Vec<_>>(),
            ["outside"]
        );
        let tilde = scan("~~~md\n@{2026-01-01}\n~~~\n@{2026-03-03}");
        assert_eq!(
            tilde.date.as_ref().map(|t| t.value.as_str()),
            Some("2026-03-03")
        );
    }

    #[test]
    fn a_backtick_run_matches_the_same_run_and_an_unmatched_one_is_literal() {
        let double = scan("``@{2026-01-01} #code`` @{2026-02-02} #real");
        assert_eq!(
            double.date.as_ref().map(|t| t.value.as_str()),
            Some("2026-02-02")
        );
        assert_eq!(
            double
                .tags
                .iter()
                .map(|t| t.tag.as_str())
                .collect::<Vec<_>>(),
            ["real"]
        );
        // A single backtick inside a double-backtick span is literal, not a close.
        let mixed = scan("``a ` @{2026-01-01}`` @{2026-03-03}");
        assert_eq!(
            mixed.date.as_ref().map(|t| t.value.as_str()),
            Some("2026-03-03")
        );
        // An unmatched run does not open a code span.
        let unmatched = scan("`@{2026-04-04} #tag");
        assert_eq!(
            unmatched.date.as_ref().map(|t| t.value.as_str()),
            Some("2026-04-04")
        );
        assert_eq!(
            unmatched
                .tags
                .iter()
                .map(|t| t.tag.as_str())
                .collect::<Vec<_>>(),
            ["tag"]
        );
    }

    #[test]
    fn crlf_and_multibyte_text_keep_byte_offsets() {
        let text = "漢字 @{2026-11-12}\r\né @@{08:05}";
        let found = scan(text);
        let date = found.date.expect("date");
        assert_eq!(date.value, "2026-11-12");
        assert_eq!(&text[date.start..date.end], "@{2026-11-12}");
        let time = found.time.expect("time");
        assert_eq!(time.value, "08:05");
        assert_eq!(&text[time.start..time.end], "@@{08:05}");
    }

    #[test]
    fn a_custom_trigger_and_a_link_form_are_recognised() {
        let custom = DateSettings {
            date_trigger: "due:".to_owned(),
            time_trigger: "at:".to_owned(),
            ..DateSettings::default()
        };
        let found = scan_card_text("due:[[2026-11-11]] at:{14:30}", &custom);
        assert_eq!(
            found.date.as_ref().map(|t| t.value.as_str()),
            Some("2026-11-11")
        );
        assert!(found.date.as_ref().unwrap().link);
        assert_eq!(found.time.as_ref().map(|t| t.value.as_str()), Some("14:30"));
        // The default triggers are ordinary text under this board.
        assert!(scan_card_text("@{2026-11-11}", &custom).date.is_none());
    }

    #[test]
    fn without_dates_removes_only_the_first_tokens() {
        let text = "a @{2026-01-01} @@{09:30} @{2027-02-02}";
        let found = scan(text);
        assert_eq!(found.without_dates(text), "a   @{2027-02-02}");
        let plain = scan("no dates here");
        assert_eq!(plain.without_dates("no dates here"), "no dates here");
    }

    #[test]
    fn tags_skip_code_headings_and_numbers() {
        let found = scan("#Ops fix `#not` #12 a#b #ui/menu\n```\n#fenced\n```\n#last");
        assert_eq!(
            found
                .tags
                .iter()
                .map(|t| t.tag.as_str())
                .collect::<Vec<_>>(),
            ["Ops", "ui/menu", "last"]
        );
        assert!(scan("## not a tag").tags.is_empty());
    }
}

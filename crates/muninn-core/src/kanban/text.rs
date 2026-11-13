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

/// One `#tag` found in card text. `tag` is the word without its `#`; `start`
/// is the byte of the `#`, so `text[start..end]` is the whole token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagToken {
    pub tag: String,
    pub start: usize,
    pub end: usize,
}

/// The characters a tag may hold after its `#`.
#[must_use]
pub fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}

/// Whether `tag` (without its `#`) is a tag: non-empty, its characters are
/// [`is_tag_char`], and it is not only digits.
#[must_use]
pub fn is_tag_text(tag: &str) -> bool {
    !tag.is_empty() && tag.chars().all(is_tag_char) && !tag.chars().all(char::is_numeric)
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
        self.without(text, true, false)
    }

    /// `text` without every tag token. This is what `hide-tags-in-title`
    /// shows; like [`CardText::without_dates`] it only changes the display.
    #[must_use]
    pub fn without_tags(&self, text: &str) -> String {
        self.without(text, false, true)
    }

    /// `text` without the tokens the named display settings hide. Ranges come
    /// from one scan of `text`, so hiding both kinds in one pass stays
    /// correct; hiding dates and tags in two passes would use stale offsets.
    #[must_use]
    pub fn without(&self, text: &str, dates: bool, tags: bool) -> String {
        let mut ranges: Vec<Range<usize>> = Vec::new();
        if dates {
            ranges.extend(
                [&self.date, &self.time]
                    .into_iter()
                    .flatten()
                    .map(|token| token.start..token.end),
            );
        }
        if tags {
            ranges.extend(self.tags.iter().map(|token| token.start..token.end));
        }
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
        scan_tags_in_line(body, line_start, &code, &mut found.tags);
        line_start += line.len();
    }
    found
}

/// Every `#tag` in `text`, outside fenced blocks and inline code. This is the
/// card scanner's tag half and the note index's body reader
/// ([`crate::tags::note_tags`]).
#[must_use]
pub fn scan_tags(text: &str) -> Vec<TagToken> {
    let mut tags = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let body = body.strip_suffix('\r').unwrap_or(body);
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
        scan_tags_in_line(body, line_start, &code, &mut tags);
        line_start += line.len();
    }
    tags
}

/// Whether byte `caret` sits inside a fenced block or an inline code span.
/// The fence and inline rules are the ones [`scan_card_text`] uses; the card
/// editor's `#` completion asks this so it does not offer tags inside code.
#[must_use]
pub fn in_code_at(text: &str, caret: usize) -> bool {
    let caret = caret.min(text.len());
    let mut fence: Option<(u8, usize)> = None;
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let body = body.strip_suffix('\r').unwrap_or(body);
        let line_end = line_start + body.len();
        if let Some((marker, run)) = fence {
            if (line_start..=line_end).contains(&caret) {
                return true;
            }
            if fence_closes(body, marker, run) {
                fence = None;
            }
            line_start += line.len();
            continue;
        }
        if let Some(opened) = opens_fence(body) {
            if (line_start..=line_end).contains(&caret) {
                return true;
            }
            fence = Some(opened);
            line_start += line.len();
            continue;
        }
        if (line_start..=line_end).contains(&caret) {
            return in_code(&code_ranges(body), caret - line_start);
        }
        line_start += line.len();
    }
    false
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
pub(super) fn code_ranges(body: &str) -> Vec<Range<usize>> {
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

pub(super) fn in_code(ranges: &[Range<usize>], at: usize) -> bool {
    // Inclusive at both ends: a caret just after the opening run and just
    // before the closing run is still inside the span.
    ranges
        .iter()
        .any(|range| range.start <= at && at <= range.end)
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

fn scan_tags_in_line(body: &str, base: usize, code: &[Range<usize>], tags: &mut Vec<TagToken>) {
    let mut before = ' ';
    for (at, ch) in body.char_indices() {
        if ch == '#' && !in_code(code, at) && before.is_whitespace() {
            let word = &body[at + 1..];
            let len = word.find(|c: char| !is_tag_char(c)).unwrap_or(word.len());
            let tag = &word[..len];
            if is_tag_text(tag) {
                tags.push(TagToken {
                    tag: tag.to_owned(),
                    start: base + at,
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
        assert!(scan("# Heading").tags.is_empty());
        assert!(scan("### also a heading").tags.is_empty());
        assert!(scan("[[Note#heading]]").tags.is_empty());
        assert!(scan("`#inline`").tags.is_empty());
        assert!(scan("#123").tags.is_empty());
        assert!(scan("#١٢٣").tags.is_empty(), "Unicode digits are digits");
    }

    #[test]
    fn tags_are_unicode_nested_and_keep_byte_offsets_in_crlf_text() {
        let text = "#日本語 #café/résumé\r\nsee #parent/child";
        let found = scan(text);
        assert_eq!(
            found
                .tags
                .iter()
                .map(|t| t.tag.as_str())
                .collect::<Vec<_>>(),
            ["日本語", "café/résumé", "parent/child"]
        );
        for token in &found.tags {
            let whole = format!("#{}", token.tag);
            assert_eq!(&text[token.start..token.end], whole);
        }
    }

    #[test]
    fn without_tags_and_without_both_kinds_use_the_same_scan() {
        let text = "ship #one @{2026-01-01} then #two";
        let found = scan(text);
        assert_eq!(found.without_tags(text), "ship  @{2026-01-01} then ");
        assert_eq!(found.without(text, true, true), "ship   then ");
        assert_eq!(found.without(text, false, false), text);
    }

    #[test]
    fn in_code_at_follows_spans_fences_and_crlf() {
        let inline = "`see #wo`";
        assert!(in_code_at(inline, 6), "inside the span");
        assert!(!in_code_at(inline, 0), "on the opening backtick");
        assert!(!in_code_at(inline, 9), "after the closing backtick");

        let fenced = "```md\r\n#wo\r\n```\r\n#tag";
        assert!(in_code_at(fenced, 4), "on the opening fence");
        assert!(in_code_at(fenced, 12), "inside the fence");
        assert!(!in_code_at(fenced, 21), "after the closing fence");

        assert!(!in_code_at("plain #tag", 10));
        assert!(!in_code_at("multi ``a ` b`` #tag", 20), "after a span");
    }
}

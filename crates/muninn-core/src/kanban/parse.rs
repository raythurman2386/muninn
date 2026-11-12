//! Line-based kanban parser.
//!
//! The file is split into lines with their byte ranges. Frontmatter, lane
//! titles, done-lane markers, task-list cards, the archive and the settings
//! footer are classified from those lines. Everything else stays in the
//! original string and is never copied into a normalised form.

use std::io;
use std::ops::Range;

use serde_json::Value;

use super::{Archive, Card, Footer, Kanban, Lane, invalid_data};

const BOM: char = '\u{feff}';

/// Done-lane marker line → the word inside the asterisks.
const DONE_MARKERS: &[(&str, &str)] = &[
    ("**Complete**", "Complete"),
    ("**Fertiggestellt**", "Fertiggestellt"),
    ("**Completato**", "Completato"),
    ("**完了**", "完了"),
    ("**완료됨**", "완료됨"),
    ("**Concluído**", "Concluído"),
    ("**Выполнено**", "Выполнено"),
    ("**完成**", "完成"),
];

/// Archive heading word after `## `.
const ARCHIVE_WORDS: &[&str] = &[
    "Archive",
    "Archiv",
    "Archivio",
    "アーカイブ",
    "보관됨",
    "Arquivado",
    "Архивировать",
    "归档",
];

struct Line {
    /// First byte of the line, including a leading byte-order mark.
    start: usize,
    /// First byte after the line's text, before `\r` / `\n`.
    end: usize,
    /// First byte of the next line, or `source.len()` after the last line.
    next: usize,
}

pub(super) fn has_kanban_key(source: &str) -> bool {
    // Same line breaks as [`split_lines`], and only through the closing `---`.
    // A nested or indented key is not the marker.
    let bytes = source.as_bytes();
    let Some(first) = next_line(bytes, 0) else {
        return false;
    };
    if content(source, &first) != "---" {
        return false;
    }
    let mut at = first.next;
    let mut found = false;
    while at < bytes.len() {
        let Some(line) = next_line(bytes, at) else {
            break;
        };
        let body = content(source, &line);
        if body == "---" {
            return found;
        }
        if frontmatter_key(body) == Some("kanban-plugin") {
            found = true;
        }
        at = line.next;
    }
    false
}

pub(super) fn parse(source: &str) -> io::Result<Kanban> {
    let lines = split_lines(source);
    let Some(front_end) = frontmatter_end(&lines, source) else {
        return Err(invalid_data("not a kanban note"));
    };
    let marked = (1..front_end)
        .any(|index| frontmatter_key(content(source, &lines[index])) == Some("kanban-plugin"));
    if !marked {
        return Err(invalid_data("not a kanban note"));
    }
    let mut i = front_end + 1;
    let mut lanes: Vec<Lane> = Vec::new();
    let mut archive: Option<Archive> = None;
    let mut footer: Option<Footer> = None;
    let mut fence: Option<(u8, usize)> = None;

    while i < lines.len() {
        let line = &lines[i];
        let text = content(source, line);
        if let Some((marker, run)) = fence {
            if fence_closes(text, marker, run) {
                fence = None;
            }
            i += 1;
            continue;
        }
        if let Some(opened) = opens_fence(text) {
            fence = Some(opened);
            i += 1;
            continue;
        }
        if text == "%% kanban:settings" {
            let start = line.start;
            let (parsed, next) = parse_footer(source, &lines, i)?;
            close_open(&mut lanes, &mut archive, start);
            footer = Some(parsed);
            i = next;
            break;
        }
        if archive.is_none()
            && text == "***"
            && let Some(heading) = archive_heading_line(source, &lines, i)
        {
            close_open(&mut lanes, &mut archive, line.start);
            let word = archive_word(content(source, &lines[heading]))
                .expect("heading line was just recognised")
                .to_owned();
            archive = Some(Archive {
                word,
                cards: Vec::new(),
                span: line.start..source.len(),
            });
            i = heading + 1;
            continue;
        }
        if let Some(section) = archive.as_mut() {
            if card_mark(source, line).is_some() {
                let (card, next) = parse_card(source, &lines, i);
                section.cards.push(card);
                i = next;
            } else {
                i += 1;
            }
            continue;
        }
        if text.starts_with("## ") {
            close_open(&mut lanes, &mut archive, line.start);
            let heading = parse_heading(source, line);
            let marker = marker_line(source, &lines, i + 1);
            let marker_end = marker.map(|index| lines[index].next);
            let done_word = marker.map(|index| {
                marker_word(content(source, &lines[index]))
                    .expect("marker line was just recognised")
                    .to_owned()
            });
            let next = marker.map_or(i + 1, |index| index + 1);
            lanes.push(Lane {
                title: heading.title,
                written_limit: heading.written_limit,
                done_word,
                cards: Vec::new(),
                span: line.start..source.len(),
                title_line: line.start..line.next,
                title_span: heading.title_span,
                limit_number: heading.limit_number,
                limit_suffix: heading.limit_suffix,
                marker_end,
            });
            i = next;
            continue;
        }
        if !lanes.is_empty() && card_mark(source, line).is_some() {
            let (card, next) = parse_card(source, &lines, i);
            lanes
                .last_mut()
                .expect("lane was just checked")
                .cards
                .push(card);
            i = next;
            continue;
        }
        i += 1;
    }

    if footer.is_none() {
        close_open(&mut lanes, &mut archive, source.len());
    }
    // A second settings marker outside a fence is a malformed footer. The same
    // line inside a fence was already skipped above, and is skipped here too.
    let mut trailing_fence = None;
    for line in &lines[i..] {
        let text = content(source, line);
        if let Some((marker, run)) = trailing_fence {
            if fence_closes(text, marker, run) {
                trailing_fence = None;
            }
            continue;
        }
        if let Some(opened) = opens_fence(text) {
            trailing_fence = Some(opened);
            continue;
        }
        if text == "%% kanban:settings" {
            return Err(invalid_data("malformed settings footer"));
        }
    }

    Ok(Kanban {
        nl: if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        },
        source: source.to_owned(),
        lanes,
        archive,
        footer,
        ends_in_fence: fence.is_some(),
    })
}

fn close_open(lanes: &mut [Lane], archive: &mut Option<Archive>, end: usize) {
    if let Some(section) = archive.as_mut() {
        section.span.end = end;
    } else if let Some(lane) = lanes.last_mut() {
        lane.span.end = end;
    }
}

struct Heading {
    title: String,
    title_span: Range<usize>,
    written_limit: Option<u32>,
    limit_number: Option<Range<usize>>,
    limit_suffix: Option<Range<usize>>,
}

fn parse_heading(source: &str, line: &Line) -> Heading {
    let start = content_start(source, line);
    let text = &source[start..line.end];
    // The caller checks the `## ` prefix, so this slice is the title body.
    let body = text
        .strip_prefix("## ")
        .expect("lane heading starts with '## '");
    let body_at = start + 3;
    let core_len = body.trim_end().len();
    let core = &body[..core_len];
    if let Some((title, number)) = limit_suffix(core) {
        let title_span = body_at..body_at + title.len();
        let suffix_at = title_span.end;
        let suffix_end = body_at + core_len;
        // ` (123)`: digits sit between the space-paren and the closing paren.
        let limit_number = suffix_at + 2..suffix_end - 1;
        return Heading {
            title: title.to_owned(),
            title_span,
            written_limit: Some(number),
            limit_number: Some(limit_number),
            limit_suffix: Some(suffix_at..suffix_end),
        };
    }
    Heading {
        title: core.to_owned(),
        title_span: body_at..body_at + core_len,
        written_limit: None,
        limit_number: None,
        limit_suffix: None,
    }
}

/// `Doing (3)` → `("Doing", 3)`. The space before `(` is required. `0` is
/// returned as `0`; callers treat it as no limit.
fn limit_suffix(core: &str) -> Option<(&str, u32)> {
    let open = core.rfind(" (")?;
    let inside = core[open + 2..].strip_suffix(')')?;
    if inside.is_empty() || !inside.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((&core[..open], inside.parse().ok()?))
}

fn marker_line(source: &str, lines: &[Line], from: usize) -> Option<usize> {
    let mut index = from;
    while index < lines.len() && content(source, &lines[index]).trim().is_empty() {
        index += 1;
    }
    marker_word(content(source, lines.get(index)?)).map(|_| index)
}

fn marker_word(text: &str) -> Option<&str> {
    DONE_MARKERS
        .iter()
        .find(|(line, _)| *line == text)
        .map(|(_, word)| *word)
}

fn archive_heading_line(source: &str, lines: &[Line], rule: usize) -> Option<usize> {
    let mut index = rule + 1;
    while index < lines.len() && content(source, &lines[index]).trim().is_empty() {
        index += 1;
    }
    archive_word(content(source, lines.get(index)?)).map(|_| index)
}

fn archive_word(text: &str) -> Option<&str> {
    let body = text.strip_prefix("## ")?.trim_end();
    ARCHIVE_WORDS.iter().copied().find(|word| *word == body)
}

fn parse_card(source: &str, lines: &[Line], index: usize) -> (Card, usize) {
    let (mark, mark_at) = card_mark(source, &lines[index]).expect("caller checked the card line");
    let line = &lines[index];
    let after_bracket = mark_at + mark.len_utf8() + 1; // past `]`
    let body_at = if source.as_bytes().get(after_bracket) == Some(&b' ') {
        after_bracket + 1
    } else {
        after_bracket
    };
    let (first, block_id) = split_block_id(&source[body_at..line.end]);
    let mut end_line = index;
    let mut next = index + 1;
    let mut raw: Vec<&str> = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    while next < lines.len() {
        let continued = content(source, &lines[next]);
        if let Some((marker, run)) = fence {
            // The card ends at the first non-blank column-0 line. That line
            // is not part of the card, even when it would close or open a fence.
            if !continued.trim().is_empty() && !is_indented(continued) {
                break;
            }
            raw.push(continued);
            if fence_closes(continued, marker, run) {
                fence = None;
            }
            end_line = next;
            next += 1;
            continue;
        }
        if continued.trim().is_empty() {
            // A blank line stays with the card when an indented continuation
            // follows it. Otherwise it ends the card.
            let mut look = next + 1;
            while look < lines.len() && content(source, &lines[look]).trim().is_empty() {
                look += 1;
            }
            let resumes = look < lines.len() && is_indented(content(source, &lines[look]));
            if !resumes {
                break;
            }
            while next < look {
                raw.push(content(source, &lines[next]));
                end_line = next;
                next += 1;
            }
            continue;
        }
        if !is_indented(continued) {
            break;
        }
        if let Some(opened) = opens_fence(continued) {
            fence = Some(opened);
        }
        raw.push(continued);
        end_line = next;
        next += 1;
    }
    let (text, indent) = logical_continuations(&first, &raw);
    let card = Card {
        text,
        mark,
        block_id,
        span: lines[index].start..lines[end_line].next,
        mark_at,
        indent,
    };
    (card, next)
}

fn is_indented(text: &str) -> bool {
    !text.trim().is_empty() && text.starts_with(char::is_whitespace)
}

/// The indent `render` adds back. Lines that share a whitespace prefix use
/// the shortest one, except a single run of more than two spaces keeps two
/// as the prefix so extra spaces stay in the text. Lines that do not share a
/// prefix keep their own indent (`Some("")` means "do not add another").
fn logical_continuations(first: &str, lines: &[&str]) -> (String, Option<String>) {
    let leads: Vec<&str> = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| leading_ws(line))
        .collect();
    let indent = structural_indent(&leads);
    let mut text = first.to_owned();
    for line in lines {
        text.push('\n');
        if line.trim().is_empty() {
            continue;
        }
        text.push_str(match &indent {
            Some(indent) => line.strip_prefix(indent.as_str()).unwrap_or(line),
            None => line,
        });
    }
    (text, indent)
}

fn structural_indent(leads: &[&str]) -> Option<String> {
    let shortest = leads
        .iter()
        .copied()
        .min_by_key(|lead| lead.chars().count())?;
    if !leads.iter().all(|lead| lead.starts_with(shortest)) {
        return Some(String::new());
    }
    if shortest.chars().all(|ch| ch == ' ') && shortest.len() > 2 {
        return Some("  ".to_owned());
    }
    Some(shortest.to_owned())
}

fn leading_ws(line: &str) -> &str {
    let bytes = line
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .map(char::len_utf8)
        .sum();
    &line[..bytes]
}

/// A fence opener: optional indent, then three or more `` ` `` or `~`.
fn opens_fence(text: &str) -> Option<(u8, usize)> {
    let start = text.trim_start();
    let bytes = start.as_bytes();
    let marker = *bytes.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let run = bytes.iter().take_while(|&&byte| byte == marker).count();
    (run >= 3).then_some((marker, run))
}

fn fence_closes(text: &str, marker: u8, run: usize) -> bool {
    let trimmed = text.trim();
    let bytes = trimmed.as_bytes();
    let closer = bytes.iter().take_while(|&&byte| byte == marker).count();
    closer >= run && bytes[closer..].iter().all(u8::is_ascii_whitespace)
}

/// A task item at column 0: `- [`, one character, `]`, then the end or a space.
fn card_mark(source: &str, line: &Line) -> Option<(char, usize)> {
    let start = content_start(source, line);
    let text = &source[start..line.end];
    let bytes = text.as_bytes();
    if bytes.len() < 4 || &bytes[..3] != b"- [" {
        return None;
    }
    let mark = text[3..].chars().next()?;
    let after = 3 + mark.len_utf8();
    if bytes.get(after) != Some(&b']') {
        return None;
    }
    let rest = after + 1;
    if rest < bytes.len() && bytes[rest] != b' ' {
        return None;
    }
    Some((mark, start + 3))
}

/// Split a trailing ` ^identifier` off a card's first line. The identifier is
/// letters, digits and dashes, which is the block-id shape Obsidian's help
/// describes for block references.
fn split_block_id(body: &str) -> (String, Option<String>) {
    let trimmed_len = body.trim_end().len();
    let trimmed = &body[..trimmed_len];
    if let Some(split) = trimmed.rfind(" ^") {
        let id = &trimmed[split + 2..];
        let ok = !id.is_empty()
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
        if ok {
            return (trimmed[..split].to_owned(), Some(id.to_owned()));
        }
    }
    (body.to_owned(), None)
}

fn parse_footer(source: &str, lines: &[Line], index: usize) -> io::Result<(Footer, usize)> {
    if index + 4 >= lines.len() {
        return Err(invalid_data("unterminated settings footer"));
    }
    let shape_ok = content(source, &lines[index]) == "%% kanban:settings"
        && content(source, &lines[index + 1]) == "```"
        && content(source, &lines[index + 3]) == "```"
        && content(source, &lines[index + 4]) == "%%";
    if !shape_ok {
        return Err(invalid_data("malformed settings footer"));
    }
    let json_line = &lines[index + 2];
    let json = content(source, json_line);
    match serde_json::from_str::<Value>(json) {
        Ok(Value::Object(_)) => {}
        Ok(_) => return Err(invalid_data("settings footer JSON must be an object")),
        Err(_) => return Err(invalid_data("settings footer JSON does not parse")),
    }
    let footer = Footer {
        span: lines[index].start..lines[index + 4].next,
        json_span: json_line.start..json_line.end,
    };
    Ok((footer, index + 5))
}

fn frontmatter_end(lines: &[Line], source: &str) -> Option<usize> {
    if lines.is_empty() || content(source, &lines[0]) != "---" {
        return None;
    }
    (1..lines.len()).find(|&index| content(source, &lines[index]) == "---")
}

/// A column-0 `key: value` line. Indented keys, list items and comments are not keys.
fn frontmatter_key(line: &str) -> Option<&str> {
    if line.is_empty()
        || line.starts_with(char::is_whitespace)
        || line.starts_with('#')
        || line.starts_with('-')
    {
        return None;
    }
    let (key, _) = line.split_once(':')?;
    let key = key.trim_end();
    if key.is_empty() || key.chars().any(char::is_whitespace) {
        return None;
    }
    Some(key)
}

fn split_lines(source: &str) -> Vec<Line> {
    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let Some(line) = next_line(bytes, at) else {
            break;
        };
        at = line.next;
        lines.push(line);
    }
    lines
}

/// One line, using the same ending for detection and parsing: `\n`, an
/// optional `\r` before that `\n`, or a final `\r` when the file has no
/// newline at the end.
fn next_line(bytes: &[u8], start: usize) -> Option<Line> {
    if start >= bytes.len() {
        return None;
    }
    let mut index = start;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            let mut end = index;
            if end > start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
            return Some(Line {
                start,
                end,
                next: index + 1,
            });
        }
        index += 1;
    }
    let mut end = bytes.len();
    if end > start && bytes[end - 1] == b'\r' {
        end -= 1;
    }
    Some(Line {
        start,
        end,
        next: bytes.len(),
    })
}

fn content<'a>(source: &'a str, line: &Line) -> &'a str {
    &source[content_start(source, line)..line.end]
}

fn content_start(source: &str, line: &Line) -> usize {
    if line.start == 0 && source.starts_with(BOM) {
        line.start + BOM.len_utf8()
    } else {
        line.start
    }
}

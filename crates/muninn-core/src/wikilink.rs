//! Parsing for `[[target|alias]]` and `[[target#heading]]` links.
//!
//! Links inside inline code and fenced blocks are left alone, as Obsidian leaves them.

/// A `[[target|alias]]` or `[[target#heading]]` occurrence in a note's source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WikiLink {
    /// Byte offset of the opening `[[`.
    pub start: usize,
    /// Byte offset just past the closing `]]`.
    pub end: usize,
    /// The note it names, without a heading or `.md`.
    pub target: String,
    pub heading: Option<String>,
    pub alias: Option<String>,
}

impl WikiLink {
    fn parse(inner: &str, start: usize, end: usize) -> Option<WikiLink> {
        let (left, alias) = match inner.split_once('|') {
            Some((left, alias)) => (
                left,
                Some(alias.trim().to_owned()).filter(|s| !s.is_empty()),
            ),
            None => (inner, None),
        };
        let (target, heading) = match left.split_once('#') {
            Some((target, heading)) => (
                target,
                Some(heading.trim().to_owned()).filter(|s| !s.is_empty()),
            ),
            None => (left, None),
        };
        let mut target = target.trim().to_owned();
        if target.to_ascii_lowercase().ends_with(".md") {
            target.truncate(target.len() - 3);
        }
        if target.is_empty() && heading.is_none() {
            return None;
        }
        Some(WikiLink {
            start,
            end,
            target,
            heading,
            alias,
        })
    }

    /// The text a link shows when it has no alias.
    #[must_use]
    pub fn label(&self) -> String {
        if let Some(alias) = &self.alias {
            return alias.clone();
        }
        if self.target.is_empty() {
            return self
                .heading
                .clone()
                .unwrap_or_else(|| "This note".to_owned());
        }
        self.target.clone()
    }
}

/// Parse the wikilinks in one note's source, skipping fenced code blocks and inline code.
#[must_use]
pub fn parse_links(source: &str) -> Vec<WikiLink> {
    let mut links = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut offset = 0;
    for raw in source.split_inclusive('\n') {
        let line = raw
            .strip_suffix('\n')
            .unwrap_or(raw)
            .strip_suffix('\r')
            .unwrap_or(raw);
        let trimmed = line.trim_start();
        if let Some((marker, run)) = fence {
            let bytes = trimmed.as_bytes();
            let closes = bytes.iter().take_while(|&&b| b == marker).count() >= run
                && trimmed[run..].trim().is_empty();
            if closes {
                fence = None;
            }
        } else if let Some(opened) = fence_open(trimmed) {
            fence = Some(opened);
        } else {
            parse_inline(line, offset, &mut links);
        }
        offset += raw.len();
    }
    links
}

/// Find the `[[` opening an unterminated wikilink that contains `caret`, outside
/// fenced code blocks and inline code.
///
/// Returns the byte offset of the opening `[[` and the text between it and the
/// caret (the autocomplete query). This is the detection half of link
/// autocomplete: `[[ind` yields `(offset, "ind")`, a closed `[[Done]]` or a link
/// inside a fence yields `None`. A query that already contains a heading (`#`),
/// an alias separator (`|`) or a closing bracket is not a candidate, because
/// the caret is no longer in the note-name part.
#[must_use]
pub fn unclosed_link(source: &str, caret: usize) -> Option<(usize, String)> {
    let caret = caret.min(source.len());
    if !source.is_char_boundary(caret) {
        return None;
    }
    let line_start = source[..caret].rfind('\n').map_or(0, |at| at + 1);
    // Fence state is line-scoped, so only the lines above the caret matter.
    let mut fence: Option<(u8, usize)> = None;
    for raw in source[..line_start].split_inclusive('\n') {
        let line = raw
            .strip_suffix('\n')
            .unwrap_or(raw)
            .strip_suffix('\r')
            .unwrap_or(raw);
        fence_step(&mut fence, line);
    }
    if fence.is_some() {
        return None;
    }
    let prefix = &source[line_start..caret];
    // The caret may be on the fence opener itself (` ```[[x `).
    if fence_open(prefix.trim_start()).is_some() {
        return None;
    }
    let start = last_open_bracket(prefix)?;
    let query = &prefix[start + 2..];
    if query.contains(['#', '|', ']', '\n']) {
        return None;
    }
    Some((line_start + start, query.to_owned()))
}

/// Offset of the first line whose ATX heading (`#`, `##`, …) matches `heading`
/// case-insensitively, outside fenced code blocks. Used by heading deep links:
/// the offset is where the Read view scrolls to and where the Write view puts
/// the caret. `None` means the note has no such heading.
#[must_use]
pub fn heading_offset(source: &str, heading: &str) -> Option<usize> {
    let target = heading.trim().trim_start_matches('#').trim();
    if target.is_empty() {
        return None;
    }
    headings(source)
        .into_iter()
        .find(|(_, text)| text.eq_ignore_ascii_case(target))
        .map(|(offset, _)| offset)
}

/// Every top-level ATX heading in `source` as `(line offset, heading text)`,
/// skipping fenced code blocks. The text has its leading hashes and any closing
/// hashes removed; it is not case-folded.
#[must_use]
pub fn headings(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut offset = 0;
    for raw in source.split_inclusive('\n') {
        let line = raw
            .strip_suffix('\n')
            .unwrap_or(raw)
            .strip_suffix('\r')
            .unwrap_or(raw);
        let before = fence;
        fence_step(&mut fence, line);
        if before.is_none()
            && fence.is_none()
            && let Some(text) = atx_heading(line)
        {
            out.push((offset, text.to_owned()));
        }
        offset += raw.len();
    }
    out
}

/// Update `fence` for one line: close the current fence or open a new one.
fn fence_step(fence: &mut Option<(u8, usize)>, line: &str) {
    let trimmed = line.trim_start();
    if let Some((marker, run)) = *fence {
        let bytes = trimmed.as_bytes();
        let closes = bytes.iter().take_while(|&&b| b == marker).count() >= run
            && trimmed[run..].trim().is_empty();
        if closes {
            *fence = None;
        }
    } else if let Some(opened) = fence_open(trimmed) {
        *fence = Some(opened);
    }
}

/// The heading text of an ATX line (`## Title ##` → `Title`), or `None`.
fn atx_heading(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let text = rest.trim();
    // A closing hash sequence is only a delimiter when whitespace precedes it
    // (CommonMark): `## Title ##` is `Title`, but `## C#` keeps its `#`.
    let text = match text.trim_end_matches('#') {
        rest if rest.len() != text.len()
            && (rest.is_empty() || rest.ends_with(char::is_whitespace)) =>
        {
            rest.trim_end()
        }
        _ => text,
    };
    (!text.is_empty()).then_some(text)
}

/// The last unclosed `[[` in `prefix`, skipping escaped characters, inline code
/// spans and complete `[[links]]`.
fn last_open_bracket(prefix: &str) -> Option<usize> {
    let bytes = prefix.as_bytes();
    let mut open = None;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 1;
                if let Some(c) = prefix[i..].chars().next() {
                    i += c.len_utf8();
                }
            }
            b'`' => {
                let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
                if let Some(at) = find_run(&prefix[i + run..], b'`', run) {
                    i += run + at + run;
                } else {
                    i += run;
                }
            }
            b'[' if bytes.get(i + 1) == Some(&b'[') => {
                if let Some(at) = prefix[i + 2..].find("]]") {
                    i += 2 + at + 2;
                    open = None;
                } else {
                    open = Some(i);
                    i += 2;
                }
            }
            _ => i += prefix[i..].chars().next().map_or(1, char::len_utf8),
        }
    }
    open
}

fn fence_open(line: &str) -> Option<(u8, usize)> {
    let marker = *line.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let run = line.as_bytes().iter().take_while(|&&b| b == marker).count();
    (run >= 3).then_some((marker, run))
}

fn parse_inline(line: &str, base: usize, out: &mut Vec<WikiLink>) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 1;
                if let Some(c) = line[i..].chars().next() {
                    i += c.len_utf8();
                }
            }
            b'`' => {
                let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
                if let Some(at) = find_run(&line[i + run..], b'`', run) {
                    i += run + at + run;
                } else {
                    i += run;
                }
            }
            b'[' if bytes.get(i + 1) == Some(&b'[') => {
                if let Some(at) = line[i + 2..].find("]]") {
                    let end = i + 2 + at + 2;
                    if let Some(link) =
                        WikiLink::parse(&line[i + 2..i + 2 + at], base + i, base + end)
                    {
                        out.push(link);
                    }
                    i = end;
                } else {
                    i += 1;
                }
            }
            _ => {
                i += line[i..].chars().next().map_or(1, char::len_utf8);
            }
        }
    }
}

fn find_run(hay: &str, marker: u8, run: usize) -> Option<usize> {
    let bytes = hay.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == marker {
            let start = i;
            while i < bytes.len() && bytes[i] == marker {
                i += 1;
            }
            if i - start == run {
                return Some(start);
            }
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_spans_and_fences_keep_their_brackets() {
        let src = "a [[Real]] link\n\n`[[Not]]` and ``[[AlsoNot]]``\n\n```\n[[Fenced]]\n```\n\nend [[Last]]\n";
        let links = parse_links(src);
        let targets: Vec<&str> = links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, ["Real", "Last"]);
    }

    #[test]
    fn aliases_headings_and_self_links_are_parsed() {
        let links = parse_links("[[Target#Part|the part]] and [[#Local]] and [[Target.md]]");
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].target, "Target");
        assert_eq!(links[0].heading.as_deref(), Some("Part"));
        assert_eq!(links[0].alias.as_deref(), Some("the part"));
        assert_eq!(links[0].label(), "the part");
        assert!(links[1].target.is_empty());
        assert_eq!(links[1].heading.as_deref(), Some("Local"));
        assert_eq!(links[1].label(), "Local");
        assert_eq!(links[2].target, "Target");
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    #[test]
    fn unclosed_link_detects_the_query_and_ignores_codes_and_closed_links() {
        let src = "text [[ind";
        assert_eq!(unclosed_link(src, src.len()), Some((5, "ind".to_owned())));
        // A closed link is not a candidate, but a later open one is.
        let src = "[[Done]] then [[in";
        assert_eq!(unclosed_link(src, src.len()), Some((14, "in".to_owned())));
        // Inline code and fenced blocks never trigger.
        let src = "`[[Not` and [[Yes";
        assert_eq!(unclosed_link(src, src.len()), Some((12, "Yes".to_owned())));
        let src = "```\n[[Fenced\n```\n[[Open";
        assert_eq!(unclosed_link(src, src.len()), Some((17, "Open".to_owned())));
        let src = "```\n[[Fenced";
        assert_eq!(unclosed_link(src, src.len()), None);
        let src = "```[[Fence opener";
        assert_eq!(unclosed_link(src, src.len()), None);
        // Headings and aliases are not note-name queries.
        let src = "[[Note#Head";
        assert_eq!(unclosed_link(src, src.len()), None);
        let src = "[[Note|Alias";
        assert_eq!(unclosed_link(src, src.len()), None);
        // A newline before the caret starts a fresh line.
        let src = "[[Open\nplain";
        assert_eq!(unclosed_link(src, src.len()), None);
    }

    #[test]
    fn headings_find_atx_lines_outside_fences_case_insensitively() {
        let src = "# Top\n\ntext\n\n## Second ##\n\n```\n# Not\n```\n";
        assert_eq!(heading_offset(src, "top"), Some(0));
        assert_eq!(heading_offset(src, "Second"), Some(13));
        assert_eq!(
            heading_offset(src, "not"),
            None,
            "fenced lines are not headings"
        );
        assert_eq!(heading_offset(src, "absent"), None);
        let all = headings(src);
        assert_eq!(all.len(), 2, "{all:?}");
        assert_eq!(all[1], (13, "Second".to_owned()));
        // A trailing hash that is not a closing sequence is part of the title.
        assert_eq!(headings("## C#\n")[0].1, "C#");
        assert_eq!(headings("#\n"), Vec::new());
    }
}

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

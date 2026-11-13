//! A minimal, read-only reader for a note's leading frontmatter.
//!
//! `docs/kanban-format.md` section 9 and the Kanban slices need a few values
//! from the YAML block at the top of a note (a board's `kanban-plugin`, a
//! note's `tags`, a linked note's metadata). Muninn has no YAML dependency,
//! so this reader understands only the flat shape both Obsidian and people
//! write: `key: value`, quoted values, `- item` lists, `[a, b]` flow lists,
//! comments, CRLF and a leading byte-order mark.
//!
//! Anything nested (an indented `key:`, a map inside a list) is ignored; it is
//! not rewritten and no caller writes through this reader. The reader keeps
//! the byte offset where the block ends so a body scanner can skip it.

/// One frontmatter value, as far as the flat shape goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrontmatterValue {
    /// A scalar, unquoted and with its comment removed.
    Text(String),
    /// The items of a `- item` or `[a, b]` list, in order.
    List(Vec<String>),
}

/// The frontmatter block of a note, or an empty one when the note has none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frontmatter {
    entries: Vec<(String, FrontmatterValue)>,
    body_start: usize,
}

impl Frontmatter {
    /// Read the block at the start of `source`. A source without a leading
    /// `---` line, or whose block never closes, has no frontmatter.
    #[must_use]
    pub fn parse(source: &str) -> Self {
        let start = if source.starts_with('\u{feff}') {
            '\u{feff}'.len_utf8()
        } else {
            0
        };
        let Some((first, used)) = next_line(source, start) else {
            return Self::default();
        };
        if first.trim_end() != "---" {
            return Self::default();
        }
        let mut at = start + used;
        let mut entries: Vec<(String, Option<FrontmatterValue>)> = Vec::new();
        // A leading `---` is also a thematic break. The block is frontmatter
        // only when at least one line reads as `key: value`; otherwise the
        // whole file is body (section 7's the-break case).
        let mut saw_key = false;
        while let Some((body, used)) = next_line(source, at) {
            at += used;
            let line = body;
            if line.trim_end() == "---" || line.trim_end() == "..." {
                if !saw_key {
                    return Self::default();
                }
                return Self {
                    entries: entries
                        .into_iter()
                        .filter_map(|(key, value)| value.map(|value| (key, value)))
                        .collect(),
                    body_start: at,
                };
            }
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            // A list item belongs to the previous key whether it is indented
            // (`  - work`) or not (`- work`); both are common in frontmatter.
            let trimmed = line.trim_start();
            if trimmed.starts_with('-') {
                add_list_item(&mut entries, trimmed);
                continue;
            }
            if line.starts_with(' ') || line.starts_with('\t') {
                // A nested map or other indented line: flat reading ignores it.
                continue;
            }
            let Some(colon) = line.find(':') else {
                continue;
            };
            let key = line[..colon].trim();
            if key.is_empty() || key.contains(char::is_whitespace) {
                continue;
            }
            saw_key = true;
            let raw = line[colon + 1..].trim();
            let value = (!raw.is_empty()).then(|| parse_value(raw));
            entries.push((key.to_owned(), value));
        }
        // An unterminated block is ordinary text, like a line `---` that
        // never closes: the note is read as a body, not as frontmatter.
        Self::default()
    }

    /// Byte offset where the note's body starts (after the closing line).
    /// Zero when there is no frontmatter.
    #[must_use]
    pub fn body_start(&self) -> usize {
        self.body_start
    }

    /// The value for `key`. A repeated key keeps its first value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&FrontmatterValue> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// A scalar value for `key`; a list is not a scalar.
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(FrontmatterValue::Text(text)) => Some(text),
            _ => None,
        }
    }

    /// A list value for `key`. A scalar is not a list.
    #[must_use]
    pub fn list(&self, key: &str) -> Option<&[String]> {
        match self.get(key) {
            Some(FrontmatterValue::List(items)) => Some(items),
            _ => None,
        }
    }

    /// True when the note has no frontmatter or an empty block.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// One line without its `\n`/`\r\n`, and the bytes it used including the
/// ending.
fn next_line(source: &str, at: usize) -> Option<(&str, usize)> {
    let rest = source.get(at..)?;
    if rest.is_empty() {
        return None;
    }
    match rest.find('\n') {
        Some(end) => {
            let line = &rest[..end];
            Some((line.strip_suffix('\r').unwrap_or(line), end + 1))
        }
        None => Some((rest.strip_suffix('\r').unwrap_or(rest), rest.len())),
    }
}

fn add_list_item(entries: &mut [(String, Option<FrontmatterValue>)], item: &str) {
    let Some(item) = item.strip_prefix('-') else {
        return;
    };
    if !item.is_empty() && !item.starts_with(char::is_whitespace) {
        return;
    }
    let Some((_, slot)) = entries.last_mut() else {
        return;
    };
    let text = parse_scalar(item);
    match slot {
        Some(FrontmatterValue::List(items)) => items.push(text),
        Some(FrontmatterValue::Text(_)) => {
            // A scalar followed by list items is malformed; keep the scalar.
        }
        None => *slot = Some(FrontmatterValue::List(vec![text])),
    }
}

fn parse_value(raw: &str) -> FrontmatterValue {
    let raw = strip_comment(raw).trim();
    if let Some(inner) = raw
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        return FrontmatterValue::List(flow_items(inner));
    }
    FrontmatterValue::Text(unquote(raw))
}

/// Split a flow list on commas that are not inside quotes.
fn flow_items(inner: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut item = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in inner.chars() {
        if escaped {
            item.push(c);
            escaped = false;
            continue;
        }
        match quote {
            Some(mark) => {
                if c == '\\' && mark == '"' {
                    escaped = true;
                } else {
                    if c == mark {
                        quote = None;
                    }
                    item.push(c);
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    item.push(c);
                }
                ',' => {
                    items.push(parse_scalar(&item));
                    item.clear();
                }
                _ => item.push(c),
            },
        }
    }
    items.push(parse_scalar(&item));
    items.retain(|item| !item.is_empty());
    items
}

/// Remove a YAML comment: `#` at the start or after whitespace, outside
/// quotes.
fn strip_comment(raw: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (at, c) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(mark) => {
                if c == '\\' && mark == '"' {
                    escaped = true;
                } else if c == mark {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' => quote = Some(c),
                '#' if at == 0 || raw[..at].ends_with(char::is_whitespace) => {
                    return raw[..at].trim_end();
                }
                _ => {}
            },
        }
    }
    raw.trim_end()
}

fn unquote(raw: &str) -> String {
    let raw = raw.trim();
    let Some(first) = raw.chars().next() else {
        return String::new();
    };
    if raw.len() < 2 || !raw.ends_with(first) || !matches!(first, '"' | '\'') {
        return raw.to_owned();
    }
    let inner = &raw[1..raw.len() - 1];
    if first == '\'' {
        return inner.replace("''", "'");
    }
    let mut out = String::with_capacity(inner.len());
    let mut escaped = false;
    for c in inner.chars() {
        if escaped {
            out.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_scalar(raw: &str) -> String {
    unquote(strip_comment(raw.trim()).trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scalar_quoted_list_and_absent_key() {
        let source = "---\ntitle: Board\nkanban-plugin: \"board\"\ntags: [alpha, \"beta gamma\"]\nlist:\n  - one\n  - 'two'\n---\nbody\n";
        let front = Frontmatter::parse(source);
        assert_eq!(front.text("title"), Some("Board"));
        assert_eq!(front.text("kanban-plugin"), Some("board"));
        assert_eq!(
            front.list("tags"),
            Some(&["alpha".to_owned(), "beta gamma".to_owned()][..])
        );
        assert_eq!(
            front.list("list"),
            Some(&["one".to_owned(), "two".to_owned()][..])
        );
        assert!(front.get("absent").is_none());
        assert_eq!(&source[front.body_start()..], "body\n");
    }

    #[test]
    fn comments_blank_lines_crlf_and_a_bom() {
        let source = "\u{feff}---\r\n# a comment\r\n\r\ntags: one, two # trailing\r\nother: \"x # y\"\r\n---\r\n";
        let front = Frontmatter::parse(source);
        assert_eq!(front.text("tags"), Some("one, two"));
        assert_eq!(front.text("other"), Some("x # y"));
        assert_eq!(&source[front.body_start()..], "");
    }

    #[test]
    fn list_items_belong_to_the_previous_key_with_or_without_indent() {
        let flat = "---\ntags:\n- work\n- home/garden\n---\n";
        let front = Frontmatter::parse(flat);
        assert_eq!(
            front.list("tags"),
            Some(&["work".to_owned(), "home/garden".to_owned()][..])
        );
        let indented = "---\ntags:\n  - work\n  - home/garden\n---\n";
        let front = Frontmatter::parse(indented);
        assert_eq!(
            front.list("tags"),
            Some(&["work".to_owned(), "home/garden".to_owned()][..])
        );
    }

    #[test]
    fn a_leading_thematic_break_is_not_frontmatter_without_a_key() {
        let source = "---\nfirst #one\n---\nsecond #two\n";
        let front = Frontmatter::parse(source);
        assert!(front.is_empty());
        assert_eq!(front.body_start(), 0, "the whole file is body");
    }

    #[test]
    fn nested_maps_are_ignored_and_never_rewritten() {
        let source = "---\nouter:\n  inner: value\n  - kept\ntags: work\n---\n";
        let front = Frontmatter::parse(source);
        assert_eq!(front.text("outer"), None);
        assert_eq!(
            front.list("outer"),
            Some(&["kept".to_owned()][..]),
            "a list item under an empty key is the flat reading"
        );
        assert_eq!(front.text("tags"), Some("work"));
    }

    #[test]
    fn no_frontmatter_or_an_unterminated_one_is_read_as_a_body() {
        assert!(Frontmatter::parse("just a body\n").is_empty());
        assert_eq!(Frontmatter::parse("# Heading\n").body_start(), 0);
        let open = "---\ntitle: x\nno close\n";
        assert!(Frontmatter::parse(open).is_empty());
        assert_eq!(Frontmatter::parse(open).body_start(), 0);
    }
}

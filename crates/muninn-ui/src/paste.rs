//! #111: pasting into the Write editor.
//!
//! Rune hands a focused field's `on_input_paste` hook every form the clipboard
//! holds (`ClipboardContent`): plain text, and sanitised HTML when the source
//! offered it. [`paste_into`] turns that into one Markdown insertion:
//!
//! - a single http(s) URL over a non-empty selection becomes
//!   `[selection](url)`; with no selection it stays plain text;
//! - otherwise HTML becomes Markdown (headings, lists, links, emphasis,
//!   paragraphs and line breaks) through [`html_to_markdown`];
//! - everything else — no HTML, blank HTML, or HTML that would produce the
//!   plain text already there — returns `false`, leaving Rune's ordinary
//!   plain-text paste (and its already-Markdown text) untouched.
//!
//! Every insertion goes through [`TextEdit::insert`], which replaces the
//! selection and records one undo step in the field's history, so the normal
//! autosave/`⌘S` path sees it like any other edit.

use rune::{ClipboardContent, TextEdit};

/// The longest clipboard text still treated as a single pasted URL.
const MAX_URL: usize = 2048;

/// Paste the clipboard into the note editor at the selection.
///
/// Returns `true` when this took the paste (one undo step), and `false` when
/// the caller should let Rune paste the plain text as it does today.
pub(crate) fn paste_into(edit: &mut TextEdit, content: &ClipboardContent) -> bool {
    let text = content.text();
    if let Some(url) = single_url(text) {
        if edit.range().0 != edit.range().1 {
            let label = link_label(edit.selected());
            if !label.is_empty() {
                edit.insert(&format!("[{label}]({})", escape_url(url)));
                return true;
            }
        }
        // A URL with no selection keeps today's plain-text paste. Declining
        // also stops a copied link's HTML form wrapping it in Markdown.
        return false;
    }
    let Some(html) = content.html() else {
        return false;
    };
    let Some(markdown) = html_to_markdown(html) else {
        return false;
    };
    // HTML that says no more than the plain item keeps the plain paste, so a
    // styled code editor's copy is not reflowed or re-escaped.
    if markdown == text {
        return false;
    }
    edit.insert(&markdown);
    true
}

/// The single http(s) URL `text` holds, trimmed, or `None`.
///
/// Requires one token: no whitespace or control characters anywhere, a
/// `http://` or `https://` scheme, and a host. A sentence that merely contains
/// a link is not a URL.
pub(crate) fn single_url(text: &str) -> Option<&str> {
    let url = text.trim();
    if url.is_empty() || url.len() > MAX_URL {
        return None;
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() || host.starts_with('.') {
        return None;
    }
    Some(url)
}

/// Convert an HTML clipboard fragment to Markdown.
///
/// Keeps headings (`h1`–`h6`), unordered and ordered lists (nested included),
/// links (`<a href>`), emphasis (`b`/`strong`, `i`/`em`, `s`/`del`), paragraphs,
/// line breaks, blockquotes, rules and `<pre>` blocks. Other tags are dropped
/// and their text kept; literal Markdown punctuation in the text is escaped so
/// a page's characters are not read back as formatting. Images and inline
/// `<code>` stay plain text.
///
/// Returns `None` for blank or text-less HTML, so the caller keeps the
/// clipboard's plain item unchanged.
pub(crate) fn html_to_markdown(html: &str) -> Option<String> {
    if html.trim().is_empty() {
        return None;
    }
    let mut out = Markdown::default();
    out.run(tokenize(html));
    out.finish()
}

/// Called for an `<li>`'s start: the marker its first line gets.
fn item_marker(ordered: bool, next: &mut usize) -> String {
    if ordered {
        let number = *next;
        *next += 1;
        format!("{number}. ")
    } else {
        "- ".to_owned()
    }
}

/// One link label: whitespace runs collapse to a space, and the characters
/// that would end the label early are escaped.
fn link_label(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        if matches!(ch, '\\' | '[' | ']') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// A Markdown link destination. A URL holding whitespace or parentheses uses
/// the `<...>` form so the link does not end early.
fn escape_url(url: &str) -> String {
    let url = url.trim();
    if url.contains([' ', '(', ')', '<', '>']) {
        format!("<{url}>")
    } else {
        url.to_owned()
    }
}

// ---------------------------------------------------------------------------
// HTML tokenizer
// ---------------------------------------------------------------------------

/// A tag kept whole, or text with its entities decoded.
enum Token {
    Text(String),
    Open {
        name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
    },
    Close(String),
}

/// Elements whose whole content is dropped (they cannot carry note text, and
/// their contents are code or styling, not prose).
const SKIP_WITH_CONTENT: &[&str] = &[
    "script", "style", "template", "noscript", "title", "textarea",
];

fn tokenize(html: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < html.len() {
        let rest = &html[i..];
        let Some(lt) = rest.find('<') else {
            text.push_str(rest);
            break;
        };
        if lt > 0 {
            text.push_str(&rest[..lt]);
            i += lt;
            continue;
        }
        // Text before a tag belongs before it in the output, not after every tag.
        if !text.is_empty() {
            tokens.push(Token::Text(decode_entities(&text)));
            text.clear();
        }
        if rest.starts_with("<!--") {
            let Some(end) = rest.find("-->") else { break };
            i += end + 3;
        } else if rest.starts_with("<![CDATA[") {
            let Some(end) = rest.find("]]>") else { break };
            text.push_str(&rest[9..end]);
            i += end + 3;
        } else if rest.starts_with("<!") || rest.starts_with("<?") {
            let Some(end) = rest.find('>') else { break };
            i += end + 1;
        } else if rest.starts_with("</") {
            let Some(end) = rest.find('>') else { break };
            let name = rest[2..end].trim().to_ascii_lowercase();
            if !name.is_empty() {
                tokens.push(Token::Close(name));
            }
            i += end + 1;
        } else if rest.as_bytes().get(1).is_some_and(u8::is_ascii_alphabetic) {
            // `parse_open` gets the tag from its name, without the `<`.
            let Some(tag) = parse_open(&rest[1..]) else {
                text.push('<');
                i += 1;
                continue;
            };
            i += 1 + tag.len;
            if SKIP_WITH_CONTENT.contains(&tag.name.as_str()) {
                i += skip_element(&html[i..], &tag.name);
            } else {
                tokens.push(Token::Open {
                    name: tag.name,
                    attrs: tag.attrs,
                    self_closing: tag.self_closing,
                });
            }
        } else {
            text.push('<');
            i += 1;
        }
    }
    if !text.is_empty() {
        tokens.push(Token::Text(decode_entities(&text)));
    }
    tokens
}

/// Bytes from `html`'s start through the closing tag of `name`, so a skipped
/// element's contents never reach the Markdown.
fn skip_element(html: &str, name: &str) -> usize {
    let needle = format!("</{name}");
    let lower = html.to_ascii_lowercase();
    let Some(at) = lower.find(&needle) else {
        return html.len();
    };
    let Some(close) = lower[at..].find('>') else {
        return html.len();
    };
    at + close + 1
}

/// An opening tag from just after its `<`.
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    self_closing: bool,
    /// Byte length consumed through `>`.
    len: usize,
}

/// Parse an opening tag from just after its `<`.
fn parse_open(s: &str) -> Option<Tag> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b':')
    {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let name = s[..i].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut self_closing = false;
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        match bytes[i] {
            b'>' => {
                i += 1;
                break;
            }
            b'/' => {
                self_closing = true;
                i += 1;
                continue;
            }
            _ => {}
        }
        let start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'>'
        {
            i += 1;
        }
        let attr = s[start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                i += 1;
                let from = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                value = decode_entities(&s[from..i]);
                if i < bytes.len() {
                    i += 1;
                }
            } else {
                let from = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                    i += 1;
                }
                value = decode_entities(&s[from..i]);
            }
        }
        if !attr.is_empty() {
            attrs.push((attr, value));
        }
    }
    Some(Tag {
        name,
        attrs,
        self_closing,
        len: i,
    })
}

/// Decode the HTML entities a copied passage is likely to carry. Unknown
/// entities stay as written.
fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(end) = tail.find(';').filter(|end| *end > 1 && *end <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..end];
        match entity(name) {
            Some(decoded) => {
                out.push_str(&decoded);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<String> {
    if let Some(digits) = name.strip_prefix('#') {
        let code = if let Some(hex) = digits.strip_prefix(['x', 'X']) {
            u32::from_str_radix(hex, 16).ok()
        } else {
            digits.parse::<u32>().ok()
        }?;
        return char::from_u32(code).map(|c| c.to_string());
    }
    let named = match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" | "ensp" | "emsp" | "thinsp" => " ",
        "ndash" => "–",
        "mdash" => "—",
        "hellip" => "…",
        "lsquo" => "‘",
        "rsquo" => "’",
        "ldquo" => "“",
        "rdquo" => "”",
        "laquo" => "«",
        "raquo" => "»",
        "bull" => "•",
        "middot" => "·",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "times" => "×",
        "divide" => "÷",
        "plusmn" => "±",
        "deg" => "°",
        "sect" => "§",
        "para" => "¶",
        "dagger" => "†",
        "euro" => "€",
        "pound" => "£",
        "yen" => "¥",
        "cent" => "¢",
        "frac12" => "½",
        "frac14" => "¼",
        "frac34" => "¾",
        "larr" => "←",
        "rarr" => "→",
        "harr" => "↔",
        "uarr" => "↑",
        "darr" => "↓",
        "hearts" => "♥",
        _ => return None,
    };
    Some(named.to_owned())
}

// ---------------------------------------------------------------------------
// Markdown builder
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Inline {
    Link,
    Strong,
    Em,
    Strike,
}

struct Open {
    inline: Inline,
    /// The opening characters, empty when the tag opens nothing (a link with
    /// no usable `href`, say).
    open: &'static str,
    close: String,
    /// Whether `open` has been written on the current line. An empty element
    /// (`<b></b>`) writes neither half.
    opened: bool,
}

struct List {
    ordered: bool,
    next: usize,
}

#[derive(Default)]
struct Markdown {
    lines: Vec<String>,
    /// The line being built, prefix included.
    line: String,
    /// The first index of `line` that is content rather than prefix.
    content_start: usize,
    started: bool,
    has_content: bool,
    pending_space: bool,
    lists: Vec<List>,
    quotes: usize,
    /// The marker for a list item's next line.
    line_marker: Option<String>,
    /// A heading's `#`s for its next line.
    block_prefix: Option<String>,
    opens: Vec<Open>,
    /// `<pre>` text, kept verbatim until the block closes.
    code: Option<String>,
}

impl Markdown {
    fn run(&mut self, tokens: Vec<Token>) {
        for token in tokens {
            match token {
                Token::Text(text) => self.text(&text),
                Token::Open {
                    name,
                    attrs,
                    self_closing,
                } => {
                    self.open(&name, &attrs);
                    if self_closing {
                        self.close(&name);
                    }
                }
                Token::Close(name) => self.close(&name),
            }
        }
    }

    fn attr(attrs: &[(String, String)], name: &str) -> Option<String> {
        attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }

    fn text(&mut self, text: &str) {
        if let Some(code) = &mut self.code {
            // HTML drops one newline straight after `<pre>`; do the same so the
            // fence's first line is the code's own.
            let text = if code.is_empty() {
                text.strip_prefix('\n').unwrap_or(text)
            } else {
                text
            };
            code.push_str(text);
            return;
        }
        for ch in text.chars() {
            if ch.is_whitespace() {
                if self.has_content {
                    self.pending_space = true;
                }
                continue;
            }
            self.ensure_started();
            if self.pending_space && self.has_content {
                self.line.push(' ');
            }
            self.pending_space = false;
            self.ensure_inline_open();
            self.push_char(ch);
            self.has_content = true;
        }
    }

    fn push_char(&mut self, ch: char) {
        let leading = !self.has_content;
        match ch {
            '\\' | '*' | '_' | '`' | '[' | ']' | '<' => {
                self.line.push('\\');
                self.line.push(ch);
            }
            _ => {
                // A text block that starts like a heading, quote, rule or list
                // marker would be read back as one; escape the first character.
                if leading && matches!(ch, '#' | '>' | '-' | '+' | '=' | '~' | '|') {
                    self.line.push('\\');
                }
                // "1. " at the start of a block is an ordered item.
                if ch == '.' && self.digits_only() {
                    self.line.push('\\');
                }
                self.line.push(ch);
            }
        }
    }

    /// Whether the line's content so far is only ASCII digits.
    fn digits_only(&self) -> bool {
        let content = &self.line[self.content_start.min(self.line.len())..];
        !content.is_empty() && content.bytes().all(|b| b.is_ascii_digit())
    }

    fn ensure_started(&mut self) {
        if self.started {
            return;
        }
        let mut prefix = String::new();
        prefix.push_str(&"> ".repeat(self.quotes));
        if let Some(marker) = self.line_marker.take() {
            prefix.push_str(&"  ".repeat(self.lists.len().saturating_sub(1)));
            prefix.push_str(&marker);
        }
        if let Some(block) = self.block_prefix.take() {
            prefix.push_str(&block);
        }
        self.line.push_str(&prefix);
        self.content_start = self.line.len();
        self.started = true;
    }

    fn ensure_inline_open(&mut self) {
        let line = &mut self.line;
        for open in &mut self.opens {
            if !open.opened && !open.open.is_empty() {
                line.push_str(open.open);
                open.opened = true;
            }
        }
    }

    fn open_inline(&mut self, inline: Inline, open: &'static str, close: String) {
        self.opens.push(Open {
            inline,
            open,
            close,
            opened: false,
        });
    }

    fn close_inline(&mut self, inline: Inline) {
        let Some(at) = self.opens.iter().rposition(|open| open.inline == inline) else {
            return;
        };
        while self.opens.len() > at {
            let Some(open) = self.opens.pop() else { break };
            if !open.opened {
                continue;
            }
            while self.line.ends_with(' ') {
                self.line.pop();
            }
            self.line.push_str(&open.close);
            self.pending_space = false;
        }
    }

    fn finish_line(&mut self, hard_break: bool) {
        self.pending_space = false;
        if !self.started {
            return;
        }
        if !self.has_content {
            self.line.clear();
            self.content_start = 0;
            self.started = false;
            return;
        }
        let end = self.line.trim_end().len();
        self.line.truncate(end);
        if hard_break {
            self.line.push_str("  ");
        }
        self.lines.push(std::mem::take(&mut self.line));
        self.content_start = 0;
        self.started = false;
        self.has_content = false;
    }

    /// Finish the line and, outside lists and quotes, leave one blank line
    /// between blocks.
    fn separate(&mut self) {
        self.finish_line(false);
        self.blank();
    }

    fn blank(&mut self) {
        if self.lists.is_empty()
            && self.quotes == 0
            && self.lines.last().is_some_and(|line| !line.is_empty())
        {
            self.lines.push(String::new());
        }
    }

    fn heading(&mut self, level: usize) {
        self.separate();
        self.block_prefix = Some(format!("{} ", "#".repeat(level.clamp(1, 6))));
    }

    fn start_list(&mut self, ordered: bool, start: usize) {
        self.separate();
        self.lists.push(List {
            ordered,
            next: start.max(1),
        });
    }

    fn end_list(&mut self) {
        self.finish_line(false);
        self.lists.pop();
        if self.lists.is_empty() {
            self.blank();
        }
    }

    fn start_item(&mut self) {
        self.finish_line(false);
        let marker = match self.lists.last_mut() {
            Some(list) => item_marker(list.ordered, &mut list.next),
            None => String::new(),
        };
        self.line_marker = Some(marker);
    }

    fn end_item(&mut self) {
        self.finish_line(false);
        self.line_marker = None;
    }

    fn start_pre(&mut self) {
        self.separate();
        self.code = Some(String::new());
    }

    fn end_pre(&mut self) {
        let Some(code) = self.code.take() else {
            return;
        };
        let body = code.trim_end_matches('\n');
        if body.is_empty() {
            self.blank();
            return;
        }
        let fence = if body.contains("```") { "````" } else { "```" };
        self.lines.push(fence.to_owned());
        for line in body.split('\n') {
            self.lines.push(line.trim_end().to_owned());
        }
        self.lines.push(fence.to_owned());
        self.blank();
    }

    fn line_break(&mut self) {
        self.finish_line(true);
    }

    fn open(&mut self, name: &str, attrs: &[(String, String)]) {
        match name {
            "br" => {
                if let Some(code) = &mut self.code {
                    code.push('\n');
                } else {
                    self.line_break();
                }
            }
            "hr" => {
                if self.code.is_none() {
                    self.separate();
                    self.lines.push("---".to_owned());
                    self.blank();
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = name[1..].parse::<usize>().unwrap_or(1);
                self.heading(level);
            }
            "p" | "div" | "section" | "article" | "header" | "footer" | "main" | "aside"
            | "nav" | "figure" | "figcaption" | "address" | "table" | "thead" | "tbody" | "tr"
            | "dl" | "dt" | "dd" | "form" | "fieldset" => self.separate(),
            "ul" => self.start_list(false, 1),
            "ol" => {
                let start = Self::attr(attrs, "start")
                    .and_then(|s| s.trim().parse::<usize>().ok())
                    .unwrap_or(1);
                self.start_list(true, start);
            }
            "li" => self.start_item(),
            "blockquote" => {
                self.separate();
                self.quotes += 1;
            }
            "pre" => self.start_pre(),
            "a" => {
                let href = Self::attr(attrs, "href").unwrap_or_default();
                if href.trim().is_empty() || !safe_url(&href) {
                    self.open_inline(Inline::Link, "", String::new());
                } else {
                    self.open_inline(Inline::Link, "[", format!("]({})", escape_url(&href)));
                }
            }
            "strong" | "b" => self.open_inline(Inline::Strong, "**", "**".to_owned()),
            "em" | "i" => self.open_inline(Inline::Em, "*", "*".to_owned()),
            "del" | "s" | "strike" => self.open_inline(Inline::Strike, "~~", "~~".to_owned()),
            _ => {}
        }
    }

    fn close(&mut self, name: &str) {
        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.separate();
                // An empty heading still took its prefix; don't hand it to the
                // next block.
                self.block_prefix = None;
            }
            "p" | "div" | "section" | "article" | "header" | "footer" | "main" | "aside"
            | "nav" | "figure" | "figcaption" | "address" | "table" | "thead" | "tbody" | "tr"
            | "dl" | "dt" | "dd" | "form" | "fieldset" => self.separate(),
            "ul" | "ol" => self.end_list(),
            "li" => self.end_item(),
            "blockquote" => {
                self.separate();
                self.quotes = self.quotes.saturating_sub(1);
                self.blank();
            }
            "pre" => self.end_pre(),
            "a" => self.close_inline(Inline::Link),
            "strong" | "b" => self.close_inline(Inline::Strong),
            "em" | "i" => self.close_inline(Inline::Em),
            "del" | "s" | "strike" => self.close_inline(Inline::Strike),
            _ => {}
        }
    }

    fn finish(mut self) -> Option<String> {
        self.finish_line(false);
        while self.lines.last().is_some_and(|line| line.is_empty()) {
            self.lines.pop();
        }
        let text = self.lines.join("\n");
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_owned())
    }
}

/// Whether a link may keep its target: no active-content scheme.
fn safe_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    let scheme = lower.split(':').next().unwrap_or_default();
    let has_scheme = lower.contains(':')
        && !lower.starts_with('/')
        && !lower.starts_with('#')
        && !lower.starts_with('.');
    !has_scheme || !matches!(scheme, "javascript" | "vbscript" | "data" | "blob" | "file")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(html: &str) -> String {
        html_to_markdown(html).unwrap_or_else(|| panic!("no Markdown for {html:?}"))
    }

    #[test]
    fn headings_keep_their_level() {
        assert_eq!(md("<h1>One</h1>"), "# One");
        assert_eq!(
            md("<h1>One</h1><h2>Two</h2><h3>Three</h3><h4>Four</h4><h5>Five</h5><h6>Six</h6>"),
            "# One\n\n## Two\n\n### Three\n\n#### Four\n\n##### Five\n\n###### Six"
        );
        assert_eq!(md("<h2>Empty</h2><p>after</p>"), "## Empty\n\nafter");
        // An empty heading does not lend its prefix to the next paragraph.
        assert_eq!(md("<h2></h2><p>after</p>"), "after");
    }

    #[test]
    fn lists_keep_their_markers_and_nesting() {
        assert_eq!(md("<ul><li>One</li><li>Two</li></ul>"), "- One\n- Two");
        assert_eq!(md("<ol><li>One</li><li>Two</li></ol>"), "1. One\n2. Two");
        assert_eq!(
            md("<ol start=\"3\"><li>Three</li><li>Four</li></ol>"),
            "3. Three\n4. Four"
        );
        assert_eq!(
            md("<ul><li>One<ul><li>Nested</li></ul></li><li>Two</li></ul>"),
            "- One\n  - Nested\n- Two"
        );
        assert_eq!(
            md("<ul><li><p>Para one</p><p>Para two</p></li></ul>"),
            "- Para one\nPara two"
        );
    }

    #[test]
    fn links_keep_their_text_and_target() {
        assert_eq!(
            md("<p>See <a href=\"https://example.com\">the site</a>.</p>"),
            "See [the site](https://example.com)."
        );
        assert_eq!(
            md("<a href=\"https://example.com/a(b)\">brackets</a>"),
            "[brackets](<https://example.com/a(b)>)"
        );
        assert_eq!(
            md("<a href=\"https://example.com\"><strong>bold link</strong></a>"),
            "[**bold link**](https://example.com)"
        );
        // No target, and active content, are just text.
        assert_eq!(md("<a>plain</a>"), "plain");
        assert_eq!(md("<a href=\"javascript:steal()\">x</a>"), "x");
    }

    #[test]
    fn emphasis_maps_to_markdown_markers() {
        assert_eq!(
            md("<p><strong>Bold</strong> and <em>italic</em></p>"),
            "**Bold** and *italic*"
        );
        assert_eq!(md("<p><b>b</b> <i>i</i></p>"), "**b** *i*");
        assert_eq!(md("<p><strong><em>both</em></strong></p>"), "***both***");
        // Space inside the tag stays outside the markers.
        assert_eq!(md("<b> padded </b>"), "**padded**");
        // An empty tag writes nothing at all.
        assert_eq!(md("<p>a<b></b>b</p>"), "ab");
    }

    #[test]
    fn paragraphs_and_breaks() {
        assert_eq!(md("<p>one<br>two</p><p>three</p>"), "one  \ntwo\n\nthree");
        assert_eq!(md("<div>one</div><div>two</div>"), "one\n\ntwo");
        assert_eq!(
            md("<blockquote><p>quoted</p></blockquote><p>after</p>"),
            "> quoted\n\nafter"
        );
        assert_eq!(md("<p>a</p><hr><p>b</p>"), "a\n\n---\n\nb");
    }

    #[test]
    fn entities_and_whitespace_collapse() {
        assert_eq!(md("<p>a &amp; b   &mdash;   c&nbsp;d</p>"), "a & b — c d");
        assert_eq!(md("<p>&lt;tag&gt; &#65;&#x42;</p>"), "\\<tag> AB");
        // Source newlines and indentation are HTML whitespace, not Markdown.
        assert_eq!(md("<p>\n  wrapped\n  words\n</p>"), "wrapped words");
    }

    #[test]
    fn literal_markdown_in_text_is_escaped() {
        assert_eq!(md("<p>*not bold*</p>"), "\\*not bold\\*");
        assert_eq!(md("<p>[not a link]</p>"), "\\[not a link\\]");
        assert_eq!(md("<p># not a heading</p>"), "\\# not a heading");
        assert_eq!(md("<p>- not a list</p>"), "\\- not a list");
        assert_eq!(md("<p>1. not a list</p>"), "1\\. not a list");
        assert_eq!(md("<p>snake_case_name</p>"), "snake\\_case\\_name");
    }

    #[test]
    fn pre_blocks_stay_preformatted() {
        assert_eq!(
            md("<pre><code>let a = 1;\n  let b = 2;\n</code></pre>"),
            "```\nlet a = 1;\n  let b = 2;\n```"
        );
    }

    #[test]
    fn blank_html_has_no_markdown() {
        assert_eq!(html_to_markdown(""), None);
        assert_eq!(html_to_markdown("   \n\t "), None);
        assert_eq!(html_to_markdown("<p></p><span>  </span>"), None);
        assert_eq!(html_to_markdown("<script>alert(1)</script>"), None);
    }

    #[test]
    fn a_lone_http_url_is_recognised() {
        assert_eq!(
            single_url("https://example.com"),
            Some("https://example.com")
        );
        assert_eq!(
            single_url("  http://localhost:3000/x?y=1  "),
            Some("http://localhost:3000/x?y=1")
        );
        assert_eq!(
            single_url("HTTPS://Example.COM"),
            Some("HTTPS://Example.COM")
        );
        for not_url in [
            "",
            "example.com",
            "ftp://example.com",
            "https://example.com and more",
            "https://",
            "https:///path",
            "see https://example.com",
        ] {
            assert_eq!(single_url(not_url), None, "{not_url:?}");
        }
    }

    #[test]
    fn url_over_a_selection_becomes_one_undoable_link() {
        let mut edit = TextEdit::new("Select me here");
        edit.set_selection(0, 9);
        let content = ClipboardContent::new("  https://example.com/a  ");
        assert!(paste_into(&mut edit, &content));
        assert_eq!(edit.text, "[Select me](https://example.com/a) here");
        edit.undo();
        assert_eq!(edit.text, "Select me here", "the link was one undo step");
        assert_eq!(edit.range(), (0, 9), "undo restores the selection");

        // A label that would end early is escaped, and a multi-line selection
        // folds to one line.
        let mut edit = TextEdit::new("a ] b\nsecond");
        edit.set_selection(0, edit.text.len());
        assert!(paste_into(
            &mut edit,
            &ClipboardContent::new("https://x.dev")
        ));
        assert_eq!(edit.text, "[a \\] b second](https://x.dev)");

        // Whitespace only is no selection; the URL pastes as plain text.
        let mut edit = TextEdit::new("  ");
        edit.set_selection(0, 2);
        assert!(!paste_into(
            &mut edit,
            &ClipboardContent::new("https://x.dev")
        ));
        assert_eq!(edit.text, "  ");

        // A URL with no selection is Rune's plain paste, not a link — even when
        // the clipboard also carries the copied link's HTML form.
        let mut edit = TextEdit::new("done");
        edit.set_caret(4);
        assert!(!paste_into(
            &mut edit,
            &ClipboardContent::new("https://x.dev")
        ));
        assert_eq!(edit.text, "done");
        let linked = ClipboardContent::new("https://x.dev")
            .with_html("<a href=\"https://x.dev\">https://x.dev</a>");
        assert!(!paste_into(&mut edit, &linked));
        assert_eq!(edit.text, "done");
    }

    #[test]
    fn rich_clipboard_text_becomes_one_undoable_markdown_insert() {
        let mut edit = TextEdit::new("intro\n\n");
        edit.set_caret(edit.text.len());
        let content = ClipboardContent::new("Title\nBody with weight.").with_html(
            "<h2>Title</h2><p>Body with <strong>weight</strong>.</p><ul><li>one</li></ul>",
        );
        assert!(paste_into(&mut edit, &content));
        assert_eq!(
            edit.text,
            "intro\n\n## Title\n\nBody with **weight**.\n\n- one"
        );
        edit.undo();
        assert_eq!(edit.text, "intro\n\n", "the rich paste was one undo step");
    }

    #[test]
    fn plain_and_already_markdown_clipboards_decline() {
        for plain in [
            "**already Markdown** and [a link](https://x.dev)",
            "# heading\n- item",
            "just some words",
        ] {
            let mut edit = TextEdit::new("");
            edit.set_caret(0);
            assert!(
                !paste_into(&mut edit, &ClipboardContent::new(plain)),
                "{plain:?} must keep the plain paste"
            );
            assert_eq!(edit.text, "");
        }
        // HTML that says no more than the plain item declines too.
        let mut edit = TextEdit::new("");
        let content = ClipboardContent::new("Hello").with_html("<p>Hello</p>");
        assert!(!paste_into(&mut edit, &content));
        assert_eq!(edit.text, "");
        // Blank HTML falls back to the plain item.
        let mut edit = TextEdit::new("");
        let content = ClipboardContent::new("Hello").with_html("   ");
        assert!(!paste_into(&mut edit, &content));
        assert_eq!(edit.text, "");
    }

    #[test]
    fn html_only_clipboards_paste_their_text() {
        let mut edit = TextEdit::new("");
        edit.set_caret(0);
        let content = ClipboardContent::new("").with_html("<p><em>only</em> rich</p>");
        assert!(paste_into(&mut edit, &content));
        assert_eq!(edit.text, "*only* rich");
    }
}
